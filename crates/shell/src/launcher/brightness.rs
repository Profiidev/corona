//! Brightness of the focused monitor, any monitor or all of them, and night light

use std::rc::Rc;

use corona_brightness::{BrightnessExt, Display};
use corona_ipc::{IpcCommand, Switch};
use corona_launcher::{
  Accessory, Action, ArgSpec, ArgValue, Args, Command, CommandSource, Hud, Icon, Meta, Outcome,
  Query,
  kit::{Group, Notify, level_target, on_off, percent, tag, watch},
  simple,
};
use gpui_kit::{App, SharedString, Subscription, assets::IconName};
use rust_i18n::t;

use crate::{
  dashboard::sliders::brightness::display,
  ipc::brightness::{
    display::{Action as BrightnessAction, SetBrightness},
    nightlight::{Nightlight, Request as NightlightRequest},
  },
};

fn label(d: &Display) -> String {
  d.output
    .clone()
    .or(d.name.clone())
    .unwrap_or_else(|| d.id.clone())
}

/// `brightness 70`, `brightness +10 DP-1`, `brightness 50 *`, or one display's row
struct Brightness {
  /// None: the focused monitor, or the one named in the arguments
  display: Option<Display>,
}

impl Brightness {
  fn target(&self, args: &Args, cx: &App) -> Option<(Option<String>, f32)> {
    let monitor = match &self.display {
      Some(d) => Some(d.id.clone()),
      None => args.text(1).map(str::to_string),
    };
    let current = match &self.display {
      Some(d) => d.percent() / 100.,
      None => display(cx)?.percent() / 100.,
    };
    Some((monitor, level_target(args.get(0)?, current)?))
  }
}

impl Command for Brightness {
  fn id(&self) -> SharedString {
    match &self.display {
      Some(d) => format!("brightness:{}", d.id).into(),
      None => "brightness".into(),
    }
  }

  fn meta(&self, cx: &App) -> Meta {
    let shown = self.display.clone().or_else(|| display(cx).cloned());
    let mut args = vec![ArgSpec::level(t!("app.launcher.display.level"))];
    if self.display.is_none() {
      args.push(ArgSpec::text(t!("app.launcher.display.monitor")).optional());
    }
    Meta {
      title: match &self.display {
        Some(d) => t!("app.launcher.display.brightness_of", monitor = label(d)).into(),
        None => t!("app.launcher.display.brightness").into(),
      },
      subtitle: shown.as_ref().map(|d| percent(d.percent() / 100.).into()),
      icon: Icon::Tile(IconName::Sun, 45.),
      keywords: vec!["brightness".into(), "screen".into(), "backlight".into()],
      accessories: shown
        .map(|d| Accessory::Progress(d.percent() / 100.))
        .into_iter()
        .collect(),
      args,
      ..Default::default()
    }
  }

  fn describe(&self, args: &Args, cx: &App) -> Option<SharedString> {
    let (monitor, level) = self.target(args, cx)?;
    Some(
      t!(
        "app.launcher.display.set_to",
        monitor = monitor.unwrap_or_else(|| t!("app.launcher.display.focused").into()),
        level = percent(level)
      )
      .into(),
    )
  }

  fn actions(&self, _: &App) -> Vec<Action> {
    let fixed = self.display.clone();
    vec![Action::sync(
      "set",
      t!("app.launcher.display.set"),
      move |args, _, cx| {
        let this = Brightness {
          display: fixed.clone(),
        };
        let Some((monitor, level)) = this.target(args, cx) else {
          anyhow::bail!("{}", t!("app.launcher.display.no_display"));
        };
        SetBrightness::handle((monitor, BrightnessAction::Set(level)), cx)?;
        Ok(Outcome::Close(Some(
          Hud::new(percent(level)).icon(IconName::Sun),
        )))
      },
    )]
  }
}

struct Displays;

impl CommandSource for Displays {
  fn id(&self) -> SharedString {
    "brightness".into()
  }

  fn commands(&self, _: &Query, cx: &App) -> Vec<Rc<dyn Command>> {
    let mut commands: Vec<Rc<dyn Command>> = vec![Rc::new(Brightness { display: None })];
    commands.extend(
      cx.brightness()
        .list_displays(cx)
        .iter()
        .filter(|d| d.unavailable.is_none())
        .map(|d| {
          Rc::new(Brightness {
            display: Some(d.clone()),
          }) as Rc<dyn Command>
        }),
    );
    commands
  }

  fn observe(&self, notify: Notify, cx: &mut App) -> Vec<Subscription> {
    let displays = cx.brightness().displays.clone();
    vec![watch(&displays, &notify, cx)]
  }
}

pub(crate) fn register(cx: &mut App) {
  nightlight(cx);
  corona_launcher::registry::register_source(Rc::new(Displays), cx);
}

/// Night light as a toggle with a temperature
fn nightlight(cx: &mut App) {
  Group::new("system:nightlight", vec![nightlight_command()])
    .observe(|notify, cx| {
      let nightlight = cx.brightness().nightlight.clone();
      vec![watch(&nightlight, &notify, cx)]
    })
    .register(cx);
}

/// `nightlight`, `nightlight off`, `nightlight 3500` for that temperature
fn nightlight_command() -> Rc<dyn Command> {
  fn request(args: &Args, current: bool) -> NightlightRequest {
    let switch = match args.get(0) {
      Some(ArgValue::Word(w)) if w == "on" => Switch::On,
      Some(ArgValue::Word(w)) if w == "off" => Switch::Off,
      // a temperature turns it on
      Some(ArgValue::Number(_)) => Switch::On,
      _ => (!current).into(),
    };
    NightlightRequest {
      switch: Some(switch),
      temperature: match args.get(0) {
        Some(ArgValue::Number(kelvin)) => Some(kelvin.clamp(0., u32::MAX as f64) as u32),
        _ => None,
      },
    }
  }
  simple(
    "nightlight",
    |cx| {
      let state = *cx.brightness().nightlight(cx);
      let name = t!("app.launcher.system.nightlight");
      Meta {
        title: match (state.available, state.on) {
          (false, _) => name.clone(),
          (true, true) => t!("app.launcher.disable", name = name.clone()),
          (true, false) => t!("app.launcher.enable", name = name.clone()),
        }
        .into(),
        subtitle: Some(match state.available {
          true => t!(
            "app.launcher.system.nightlight_state",
            state = on_off(state.on),
            kelvin = state.temperature
          )
          .into(),
          false => t!("app.launcher.unavailable").into(),
        }),
        icon: Icon::Tile(IconName::Moon, 30.),
        keywords: vec!["night".into(), "light".into(), "warm".into(), "blue".into()],
        accessories: state.available.then(|| tag(state.on)).into_iter().collect(),
        args: vec![
          ArgSpec::number(t!("app.launcher.system.kelvin"))
            .words(["on", "off", "toggle"])
            .optional(),
        ],
        ..Default::default()
      }
    },
    |_| {
      vec![Action::sync(
        "switch",
        t!("app.launcher.switch"),
        |args, _, cx| {
          let current = cx.brightness().nightlight(cx).on;
          let status = Nightlight::handle(request(args, current), cx)?;
          let text = match status.on {
            true => t!(
              "app.launcher.system.nightlight_state",
              state = on_off(true),
              kelvin = status.temperature
            ),
            false => t!(
              "app.launcher.state_is",
              name = t!("app.launcher.system.nightlight"),
              state = on_off(false)
            ),
          };
          Ok(Outcome::Close(Some(Hud::new(text))))
        },
      )]
    },
  )
}

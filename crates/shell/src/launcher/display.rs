//! Brightness of the focused monitor, any monitor or all of them

use std::rc::Rc;

use corona_brightness::{BrightnessExt, Display};
use corona_ipc::IpcCommand;
use corona_launcher::{
  Accessory, Action, ArgSpec, Args, Command, CommandSource, Hud, Icon, Meta, Outcome, Query,
  kit::{Notify, level_target, percent, watch},
};
use gpui_kit::{App, SharedString, Subscription, assets::IconName};
use rust_i18n::t;

use crate::{
  commands::brightness::{Action as BrightnessAction, SetBrightness},
  control_center::dashboard::sliders::display,
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
      Some(d) => Some(d.output.clone().unwrap_or_else(|| d.id.clone())),
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

pub fn init(cx: &mut App) {
  corona_launcher::registry::register_source(Rc::new(Displays), cx);
}

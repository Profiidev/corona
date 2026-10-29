//! System toggles: radios, do not disturb, stay awake, theme, power profile, displays

use std::rc::Rc;

use corona_components::assets::{preview_theme, restore_theme, set_mode, set_theme, theme_names};
use corona_config::ConfigProvider;
use corona_idle::IdleExt;
use corona_ipc::{IpcCommand, Switch};
use corona_launcher::{
  Action, ArgSpec, ArgValue, Args, Command, Hud, Icon, Meta, Outcome,
  kit::{Choices, Group, on_off, pick_or_list, tag, watch},
  simple,
};
use gpui_kit::{App, assets::IconName, component::ActiveTheme};
use rust_i18n::t;

use crate::commands::{
  caffeine::{Request, StayAwake},
  dpms::{Dpms, Power},
};

pub fn init(cx: &mut App) {
  Group::new("system:stay_awake", vec![caffeine()])
    .observe(|notify, cx| {
      let idle = cx.idle().stay_awake.clone();
      vec![watch(&idle, &notify, cx)]
    })
    .register(cx);
  Group::new("system", vec![dark_mode(), theme()]).register(cx);
  dpms(cx);
}

fn theme() -> Rc<dyn Command> {
  pick_or_list(
    "theme",
    t!("app.launcher.system.theme").into(),
    Icon::Tile(IconName::Palette, 300.),
    &["colors", "appearance"],
    theme_names,
    |name, cx| set_theme(name.to_string(), cx),
    themes,
  )
}

fn dpms(cx: &mut App) {
  let off = simple(
    "dpms:off",
    |_| Meta {
      title: t!("app.launcher.system.displays_off").into(),
      icon: Icon::Tile(IconName::MonitorOff, 250.),
      keywords: vec!["dpms".into(), "screen".into(), "monitor".into()],
      ..Default::default()
    },
    |_| {
      vec![Action::sync(
        "off",
        t!("app.launcher.system.displays_off"),
        |_, _, cx| {
          Dpms::handle(Power::Off, cx)?;
          Ok(Outcome::close())
        },
      )]
    },
  );
  Group::new("system:dpms", vec![off]).register(cx);
}

/// `caffeine`, `caffeine off`, `caffeine 30` for half an hour
fn caffeine() -> Rc<dyn Command> {
  fn request(args: &Args, current: bool) -> Request {
    match args.get(0) {
      Some(ArgValue::Duration(d)) => Request {
        switch: Switch::On,
        minutes: Some((d.as_secs() / 60).max(1) as u32),
      },
      Some(ArgValue::Word(w)) if w == "on" => Request {
        switch: Switch::On,
        minutes: None,
      },
      Some(ArgValue::Word(w)) if w == "off" => Request {
        switch: Switch::Off,
        minutes: None,
      },
      _ => Request {
        switch: (!current).into(),
        minutes: None,
      },
    }
  }
  simple(
    "stay_awake",
    |cx| {
      let on = cx.idle().stay_awake(cx);
      Meta {
        title: match on {
          true => t!("app.launcher.system.caffeine_off"),
          false => t!("app.launcher.system.caffeine_on"),
        }
        .into(),
        subtitle: Some(
          t!(
            "app.launcher.state_is",
            name = t!("app.launcher.system.caffeine"),
            state = on_off(on)
          )
          .into(),
        ),
        icon: Icon::Tile(IconName::Coffee, 30.),
        keywords: vec!["caffeine".into(), "awake".into(), "inhibit".into()],
        accessories: vec![tag(on)],
        args: vec![
          ArgSpec::duration(t!("app.launcher.system.minutes"))
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
          let current = cx.idle().stay_awake(cx);
          let request = request(args, current);
          let minutes = request.minutes;
          let on = StayAwake::handle(request, cx)?;
          let text = match (on, minutes) {
            (true, Some(minutes)) => t!("app.launcher.system.awake_for", minutes = minutes),
            _ => t!(
              "app.launcher.state_is",
              name = t!("app.launcher.system.caffeine"),
              state = on_off(on)
            ),
          };
          Ok(Outcome::Close(Some(Hud::new(text))))
        },
      )]
    },
  )
}

fn dark_mode() -> Rc<dyn Command> {
  simple(
    "theme:mode",
    |cx| {
      let dark = cx.theme().is_dark();
      Meta {
        title: match dark {
          true => t!("app.launcher.system.light_mode"),
          false => t!("app.launcher.system.dark_mode"),
        }
        .into(),
        icon: Icon::Tile(if dark { IconName::Sun } else { IconName::Moon }, 260.),
        keywords: vec!["dark".into(), "light".into(), "theme".into()],
        args: vec![
          ArgSpec::choice(t!("app.launcher.system.mode"), ["dark", "light", "toggle"]).optional(),
        ],
        ..Default::default()
      }
    },
    |_| {
      vec![Action::sync(
        "mode",
        t!("app.launcher.switch"),
        |args, _, cx| {
          let dark = match args.text(0) {
            Some("dark") => true,
            Some("light") => false,
            _ => !cx.theme().is_dark(),
          };
          set_mode(dark, cx)?;
          Ok(Outcome::Close(Some(Hud::new(match dark {
            true => t!("app.launcher.system.dark_mode"),
            false => t!("app.launcher.system.light_mode"),
          }))))
        },
      )]
    },
  )
}

fn themes() -> corona_launcher::View {
  Choices {
    id: "theme",
    items: Rc::new(|cx| {
      theme_names(cx)
        .into_iter()
        .map(|n| (n.clone().into(), n.into()))
        .collect()
    }),
    current: Rc::new(|cx| Some(cx.config().theme.name.clone().into())),
    apply: Rc::new(|name, cx| {
      if name == cx.config().theme.name {
        return restore_theme(cx);
      }
      set_theme(name.to_string(), cx)
    }),
    preview: Some(Rc::new(|name, cx| {
      let _ = preview_theme(name, cx);
    })),
    icon: Icon::Named(IconName::Palette),
  }
  .view(
    t!("app.launcher.system.theme"),
    t!("app.launcher.system.search_themes"),
  )
}

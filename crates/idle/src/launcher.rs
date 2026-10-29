//! Stay awake as a launcher command

use std::rc::Rc;

use corona_ipc::{IpcCommand, Switch};
use corona_launcher::{
  Action, ArgSpec, ArgValue, Args, Command, Hud, Icon, Meta, Outcome,
  kit::{Group, on_off, tag, watch},
  simple,
};
use gpui_kit::{App, assets::IconName};
use rust_i18n::t;

use crate::{
  IdleExt,
  commands::{Request, StayAwake},
};

pub(crate) fn register(cx: &mut App) {
  Group::new("system:stay_awake", vec![caffeine()])
    .observe(|notify, cx| {
      let idle = cx.idle().stay_awake.clone();
      vec![watch(&idle, &notify, cx)]
    })
    .register(cx);
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

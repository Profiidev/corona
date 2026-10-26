//! System toggles: radios, do not disturb, stay awake, theme, power profile, displays

use std::rc::Rc;

use corona_bluez::BluetoothExt;
use corona_brightness::BrightnessExt;
use corona_components::assets::{preview_theme, restore_theme, set_mode, set_theme, theme_names};
use corona_config::ConfigProvider;
use corona_idle::IdleExt;
use corona_ipc::IpcCommand;
use corona_launcher::{
  Action, ArgSpec, ArgValue, Args, Command, Hud, Icon, Meta, Outcome,
  kit::{Choices, Group, Toggle, on_off, tag, watch},
  simple,
};
use corona_network_manager::NetworkManagerExt;
use corona_notifications::NotificationsExt;
use corona_power::PowerExt;
use gpui_kit::{App, SharedString, assets::IconName, component::ActiveTheme};
use rust_i18n::t;

use crate::{
  commands::{
    caffeine::{Request, StayAwake},
    dpms::{Dpms, Power},
    nightlight::{Nightlight, Request as NightlightRequest},
    notification::{Dnd, DoNotDisturb},
    power_profile::{CycleProfile, SetProfile},
    radio::{Bluetooth, Switch, Wifi},
  },
  control_center::dashboard::toggles::{airplane_state, set_airplane},
};

fn switch(on: bool) -> Switch {
  match on {
    true => Switch::On,
    false => Switch::Off,
  }
}

pub fn init(cx: &mut App) {
  let commands: Vec<Rc<dyn Command>> = vec![
    Toggle::command(
      "wifi",
      t!("app.launcher.system.wifi"),
      IconName::Wifi,
      210.,
      |cx| {
        let network = cx.network_manager();
        network.wifi_supported(cx).then(|| network.wifi_enabled(cx))
      },
      |on, cx| Wifi::handle(switch(on), cx).map(|_| ()),
    ),
    Toggle::command(
      "bluetooth",
      t!("app.launcher.system.bluetooth"),
      IconName::Bluetooth,
      230.,
      |cx| cx.bluetooth().adapter(cx).map(|a| a.powered),
      |on, cx| Bluetooth::handle(switch(on), cx).map(|_| ()),
    ),
    Toggle::command(
      "dnd",
      t!("app.launcher.system.dnd"),
      IconName::BellOff,
      280.,
      |cx| Some(cx.notifications().do_not_disturb(cx)),
      |on, cx| {
        let dnd = match on {
          true => Dnd::On,
          false => Dnd::Off,
        };
        DoNotDisturb::handle(dnd, cx).map(|_| ())
      },
    ),
    Toggle::command(
      "airplane",
      t!("app.launcher.system.airplane"),
      IconName::Plane,
      190.,
      |cx| Some(airplane_state(cx)),
      |on, cx| {
        set_airplane(on, cx);
        Ok(())
      },
    ),
    caffeine(),
    nightlight(),
    dark_mode(),
    pick_or_list(
      "theme",
      t!("app.launcher.system.theme").into(),
      Icon::Tile(IconName::Palette, 300.),
      &["colors", "appearance"],
      theme_names,
      |name, cx| set_theme(name.to_string(), cx),
      themes,
    ),
    pick_or_list(
      "power_profile",
      t!("app.launcher.system.power_profile").into(),
      Icon::Tile(IconName::Gauge, 40.),
      &["performance", "battery", "balanced"],
      |cx| {
        cx.power()
          .profiles(cx)
          .map(|p| p.available.clone())
          .unwrap_or_default()
      },
      |name, cx| SetProfile::handle(name.to_string(), cx),
      profiles,
    ),
    simple(
      "power_profile:cycle",
      |cx| Meta {
        title: t!("app.launcher.system.cycle_profile").into(),
        subtitle: cx
          .power()
          .profiles(cx)
          .map(|p| SharedString::from(p.active.clone())),
        icon: Icon::Tile(IconName::Gauge, 40.),
        keywords: vec!["power profile".into()],
        ..Default::default()
      },
      |_| {
        vec![Action::sync(
          "cycle",
          t!("app.launcher.system.cycle_profile"),
          |_, _, cx| {
            CycleProfile::handle((), cx)?;
            Ok(Outcome::close())
          },
        )]
      },
    ),
    simple(
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
    ),
  ];
  Group::new("system", commands)
    .observe(|notify, cx| {
      let network = cx.network_manager().clone();
      let bluetooth = cx.bluetooth().clone();
      let notifications = cx.notifications().clone();
      let idle = cx.idle().stay_awake.clone();
      let profiles = cx.power().profiles.clone();
      let nightlight = cx.brightness().nightlight.clone();
      vec![
        watch(&nightlight, &notify, cx),
        watch(&network.wifi_enabled, &notify, cx),
        watch(&bluetooth.adapter, &notify, cx),
        watch(&notifications.do_not_disturb, &notify, cx),
        watch(&idle, &notify, cx),
        watch(&profiles, &notify, cx),
      ]
    })
    .register(cx);
}

/// A list to pick from, or with the name typed after it (`theme nord`) the pick itself
fn pick_or_list(
  id: &'static str,
  title: SharedString,
  icon: Icon,
  keywords: &'static [&'static str],
  names: fn(&App) -> Vec<String>,
  apply: fn(&str, &mut App) -> anyhow::Result<()>,
  list: fn() -> corona_launcher::View,
) -> Rc<dyn Command> {
  let label = title.clone();
  simple(
    id,
    move |cx| Meta {
      title: title.clone(),
      icon: icon.clone(),
      keywords: keywords.iter().map(|k| SharedString::from(*k)).collect(),
      args: vec![ArgSpec::choice(t!("app.launcher.system.name"), names(cx)).optional()],
      ..Default::default()
    },
    move |_| {
      let label = label.clone();
      vec![Action::sync("open", label, move |args, _, cx| {
        match args.text(0) {
          Some(name) => {
            apply(name, cx)?;
            Ok(Outcome::Close(Some(Hud::new(name.to_string()))))
          }
          None => Ok(Outcome::Push(list())),
        }
      })]
    },
  )
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
        switch: switch(!current),
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

/// `nightlight`, `nightlight off`, `nightlight 3500` for that temperature
fn nightlight() -> Rc<dyn Command> {
  fn request(args: &Args, current: bool) -> NightlightRequest {
    let switch = match args.get(0) {
      Some(ArgValue::Word(w)) if w == "on" => Switch::On,
      Some(ArgValue::Word(w)) if w == "off" => Switch::Off,
      // a temperature turns it on
      Some(ArgValue::Number(_)) => Switch::On,
      _ => switch(!current),
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

fn profiles() -> corona_launcher::View {
  Choices {
    id: "power_profile",
    items: Rc::new(|cx| {
      cx.power()
        .profiles(cx)
        .map(|p| {
          p.available
            .iter()
            .map(|n| (n.clone().into(), n.clone().into()))
            .collect()
        })
        .unwrap_or_default()
    }),
    current: Rc::new(|cx| cx.power().profiles(cx).map(|p| p.active.clone().into())),
    apply: Rc::new(|name, cx| SetProfile::handle(name.to_string(), cx)),
    preview: None,
    icon: Icon::Named(IconName::Gauge),
  }
  .view(
    t!("app.launcher.system.power_profile"),
    t!("app.launcher.system.search_profiles"),
  )
}

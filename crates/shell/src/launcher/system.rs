//! System toggles: radios, do not disturb, stay awake, theme, power profile, displays

use std::rc::Rc;

use corona_bluez::BluetoothExt;
use corona_brightness::BrightnessExt;
use corona_components::assets::{preview_theme, restore_theme, set_mode, set_theme, theme_names};
use corona_config::ConfigProvider;
use corona_idle::IdleExt;
use corona_ipc::{IpcCommand, Switch};
use corona_launcher::{
  Action, ArgSpec, ArgValue, Args, Command, Hud, Icon, Meta, Outcome,
  kit::{Choices, Group, Toggle, on_off, pick_or_list, tag, watch},
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
    radio::Wifi,
  },
  control_center::dashboard::toggles::{airplane_state, set_airplane},
};

pub fn init(cx: &mut App) {
  network(cx);
  dnd(cx);
  Group::new("system:stay_awake", vec![caffeine()])
    .observe(|notify, cx| {
      let idle = cx.idle().stay_awake.clone();
      vec![watch(&idle, &notify, cx)]
    })
    .register(cx);
  Group::new("system:nightlight", vec![nightlight()])
    .observe(|notify, cx| {
      let nightlight = cx.brightness().nightlight.clone();
      vec![watch(&nightlight, &notify, cx)]
    })
    .register(cx);
  Group::new("system", vec![dark_mode(), theme()]).register(cx);
  power_profile(cx);
  dpms(cx);
}

/// Wi-Fi and airplane mode
fn network(cx: &mut App) {
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
      |on, cx| Wifi::handle(on.into(), cx).map(|_| ()),
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
  ];
  Group::new("system:network", commands)
    .observe(|notify, cx| {
      let network = cx.network_manager().clone();
      let bluetooth = cx.bluetooth().clone();
      vec![
        watch(&network.wifi_enabled, &notify, cx),
        watch(&bluetooth.adapter, &notify, cx),
      ]
    })
    .register(cx);
}

fn dnd(cx: &mut App) {
  let toggle = Toggle::command(
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
  );
  Group::new("system:dnd", vec![toggle])
    .observe(|notify, cx| {
      let notifications = cx.notifications().clone();
      vec![watch(&notifications.do_not_disturb, &notify, cx)]
    })
    .register(cx);
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

fn power_profile(cx: &mut App) {
  let commands = vec![
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
  ];
  Group::new("system:power_profile", commands)
    .observe(|notify, cx| {
      let profiles = cx.power().profiles.clone();
      vec![watch(&profiles, &notify, cx)]
    })
    .register(cx);
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

/// `nightlight`, `nightlight off`, `nightlight 3500` for that temperature
fn nightlight() -> Rc<dyn Command> {
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

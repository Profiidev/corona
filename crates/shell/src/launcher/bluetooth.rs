//! Bluetooth devices: connect, disconnect, pair and forget

use std::rc::Rc;

use corona_bluez::{BluetoothExt, Device};
use corona_ipc::IpcCommand;
use corona_launcher::{
  Accessory, Action, Command, CommandSource, Confirm, Hud, Icon, ListView, Meta, Outcome, Query,
  Section, Tone,
  kit::{ListSource, Notify, opener, watch},
  registry, simple,
};
use gpui_kit::{App, SharedString, Subscription, assets::IconName};
use rust_i18n::t;

use crate::commands::bluetooth::{Action as DeviceAction, DeviceAction as Run, Discover};

fn run(action: DeviceAction, address: String, title: SharedString, done: SharedString) -> Action {
  let id = format!("{action:?}").to_lowercase();
  Action::spawn(id, title, move |_, cx| {
    let task = Run::reply((action, address.clone()), cx);
    let done = done.clone();
    async move {
      task.await?;
      Ok(Outcome::Close(Some(
        Hud::new(done).icon(IconName::Bluetooth),
      )))
    }
  })
}

fn device_row(device: &Device, pairing: bool) -> Rc<dyn Command> {
  let d = device.clone();
  simple(
    format!("bluetooth:{}", device.address),
    move |_| {
      let mut accessories = Vec::new();
      if d.connected {
        accessories.push(Accessory::Tag(
          t!("app.launcher.bluetooth.connected").into(),
          Tone::Ok,
        ));
      }
      Meta {
        title: match (pairing, d.connected) {
          (true, _) => t!("app.launcher.bluetooth.pair", name = d.name.clone()),
          (false, true) => t!("app.launcher.bluetooth.disconnect", name = d.name.clone()),
          (false, false) => t!("app.launcher.bluetooth.connect", name = d.name.clone()),
        }
        .into(),
        subtitle: d
          .battery
          .map(|b| SharedString::from(t!("app.launcher.bluetooth.battery", level = b).to_string())),
        icon: Icon::Tile(IconName::Bluetooth, 230.),
        keywords: vec![
          "bluetooth".into(),
          d.name.clone().into(),
          d.address.clone().into(),
        ],
        section: Section::Results,
        accessories,
        ..Default::default()
      }
    },
    {
      let d = device.clone();
      move |_| {
        let name: SharedString = d.name.clone().into();
        let address = d.address.clone();
        if pairing {
          return vec![run(
            DeviceAction::Pair,
            address,
            t!("app.launcher.bluetooth.pair", name = name.clone()).into(),
            t!("app.launcher.bluetooth.paired", name = name).into(),
          )];
        }
        let toggle = match d.connected {
          true => run(
            DeviceAction::Disconnect,
            address.clone(),
            t!("app.launcher.bluetooth.disconnect", name = name.clone()).into(),
            t!("app.launcher.bluetooth.disconnected", name = name.clone()).into(),
          ),
          false => run(
            DeviceAction::Connect,
            address.clone(),
            t!("app.launcher.bluetooth.connect", name = name.clone()).into(),
            t!("app.launcher.bluetooth.connected_to", name = name.clone()).into(),
          ),
        };
        let forget = run(
          DeviceAction::Forget,
          address,
          t!("app.launcher.bluetooth.forget").into(),
          t!("app.launcher.bluetooth.forgotten", name = name.clone()).into(),
        )
        .destructive()
        .confirm(Confirm {
          title: t!("app.launcher.bluetooth.forget_title", name = name).into(),
          message: t!("app.launcher.bluetooth.forget_message").into(),
          button: t!("app.launcher.bluetooth.forget").into(),
          skippable: false,
        });
        vec![toggle, forget]
      }
    },
  )
}

fn observe(notify: Notify, cx: &mut App) -> Vec<Subscription> {
  let (devices, adapter) = (
    cx.bluetooth().devices.clone(),
    cx.bluetooth().adapter.clone(),
  );
  vec![watch(&devices, &notify, cx), watch(&adapter, &notify, cx)]
}

/// A row per paired device
struct Paired;

impl CommandSource for Paired {
  fn id(&self) -> SharedString {
    "bluetooth:devices".into()
  }

  fn commands(&self, _: &Query, cx: &App) -> Vec<Rc<dyn Command>> {
    cx.bluetooth()
      .list_devices(cx)
      .iter()
      .filter(|d| d.paired)
      .map(|d| device_row(d, false))
      .collect()
  }

  fn observe(&self, notify: Notify, cx: &mut App) -> Vec<Subscription> {
    observe(notify, cx)
  }
}

fn discovery(on: bool, cx: &mut App) {
  let task = Discover::reply(on, cx);
  cx.spawn(async move |_| {
    let _ = task.await;
  })
  .detach();
}

pub fn init(cx: &mut App) {
  registry::register_source(Rc::new(Paired), cx);
  registry::register(
    opener(
      "bluetooth:pair",
      t!("app.launcher.bluetooth.pair_new"),
      Icon::Tile(IconName::BluetoothSearching, 230.),
      &["bluetooth", "pair", "headphones"],
      |_| {
        ListView::new(
          t!("app.launcher.bluetooth.pair_new"),
          t!("app.launcher.bluetooth.search"),
          Rc::new(
            ListSource::new("bluetooth:scan", |_, cx| {
              cx.bluetooth()
                .list_devices(cx)
                .iter()
                .filter(|d| !d.paired)
                .map(|d| device_row(d, true))
                .collect()
            })
            .observe(observe)
            .loading(|cx| cx.bluetooth().adapter(cx).is_some_and(|a| a.discovering))
            .on_open(|cx| discovery(true, cx))
            .on_close(|cx| discovery(false, cx)),
          ),
        )
        .empty(t!("app.launcher.bluetooth.scanning"))
        .into_view()
      },
    ),
    cx,
  );
}

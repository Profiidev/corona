//! Wi-Fi networks, VPNs, hidden networks, hotspot and the captive portal

use std::{rc::Rc, time::Duration};

use corona_ipc::IpcCommand;
use corona_launcher::{
  Accessory, Action, Command, CommandSource, Confirm, Field, FieldKind, FormView, Hud, Icon,
  ListView, Meta, Outcome, Query, Section, Tone, View,
  kit::{Group, ListSource, Notify, Toggle, on_off, opener, switch_arg, switch_target, tag, watch},
  registry, simple,
  ui::{self, Open},
};
use corona_network_manager::{
  NetworkManagerExt, NmConnectivityState, Secret, SecretKind, WifiNetwork, WifiStatus,
};
use gpui_kit::{App, Global, SharedString, Subscription, Task, assets::IconName};
use rust_i18n::t;

use crate::commands::{
  network::{
    ConnectWifi, DisconnectWifi, ForgetWifi, HiddenRequest, Hotspot, JoinHidden, Portal, Rescan,
    Security, SwitchVpn,
  },
  radio::Switch,
};

/// Shows NetworkManager's next password request in the launcher, for a connect started there
#[derive(Default)]
struct SecretWatch(Option<(Subscription, Task<()>)>);

impl Global for SecretWatch {}

fn watch_secret(cx: &mut App) {
  let request = cx.network_manager().secret_request.clone();
  let subscription = cx.observe(&request, |request, cx| {
    if request.read(cx).is_none() {
      return;
    }
    cx.default_global::<SecretWatch>().0 = None;
    if let Some(form) = secret_form(cx) {
      let _ = ui::open(Open::View(form), cx);
    }
  });
  // nobody asked within a minute: the connect needed no password
  let expire = cx.spawn(async |cx| {
    cx.background_executor()
      .timer(Duration::from_secs(60))
      .await;
    cx.update(|cx| cx.default_global::<SecretWatch>().0 = None);
  });
  cx.default_global::<SecretWatch>().0 = Some((subscription, expire));
}

fn secret_form(cx: &App) -> Option<View> {
  let request = cx.network_manager().secret_request(cx)?;
  let mut fields = Vec::new();
  if request.kind == SecretKind::Enterprise && request.identity.is_none() {
    fields.push(Field::new("identity", t!("app.network.username"), FieldKind::Text).required());
  }
  fields.push(Field::new("password", t!("app.network.password"), FieldKind::Password).required());
  let message = match request.retry {
    true => t!(
      "app.launcher.network.wrong_password",
      ssid = request.name.clone()
    ),
    false => t!(
      "app.launcher.network.password_for",
      ssid = request.name.clone()
    ),
  };
  Some(View::Form(FormView {
    title: t!("app.launcher.network.password").into(),
    fields,
    submit_label: t!("app.launcher.network.connect").into(),
    submit: Rc::new(|values, _, cx| {
      let secret = Secret {
        password: values.get("password").cloned().unwrap_or_default(),
        identity: values.get("identity").cloned(),
      };
      cx.network_manager().clone().answer_secret(cx, Some(secret));
      Task::ready(Ok(Outcome::Close(Some(Hud::new(t!(
        "app.launcher.network.connecting"
      ))))))
    }),
    cancel: Some(Rc::new(|cx| {
      cx.network_manager().clone().answer_secret(cx, None);
    })),
    message: Some(message.into()),
  }))
}

fn connect(ssid: String) -> Action {
  Action::spawn(
    "connect",
    t!("app.launcher.network.connect"),
    move |_, cx| {
      watch_secret(cx);
      let connect = ConnectWifi::reply(ssid.clone(), cx);
      let ssid = ssid.clone();
      async move {
        connect.await?;
        Ok(Outcome::Close(Some(
          Hud::new(t!("app.launcher.network.connected", ssid = ssid)).icon(IconName::Wifi),
        )))
      }
    },
  )
}

fn network_row(network: &WifiNetwork) -> Rc<dyn Command> {
  let n = network.clone();
  simple(
    format!("wifi:{}", network.ssid),
    move |_| {
      let status = match n.status {
        WifiStatus::Connected => Some((t!("app.launcher.network.connected_tag"), Tone::Ok)),
        WifiStatus::Connecting | WifiStatus::NeedAuth => {
          Some((t!("app.launcher.network.connecting_tag"), Tone::Warn))
        }
        WifiStatus::Saved => Some((t!("app.launcher.network.saved"), Tone::Off)),
        WifiStatus::New => None,
      };
      let mut accessories: Vec<Accessory> = status
        .map(|(text, tone)| Accessory::Tag(text.into(), tone))
        .into_iter()
        .collect();
      if n.secured {
        accessories.push(Accessory::Icon(IconName::Lock));
      }
      accessories.push(Accessory::Text(format!("{}%", n.strength).into()));
      Meta {
        title: t!("app.launcher.network.connect_to", ssid = n.ssid.clone()).into(),
        icon: Icon::Tile(IconName::Wifi, 210.),
        keywords: vec!["wifi".into(), "wlan".into(), n.ssid.clone().into()],
        section: Section::Results,
        accessories,
        ..Default::default()
      }
    },
    {
      let n = network.clone();
      move |_| {
        let mut actions = vec![connect(n.ssid.clone())];
        if matches!(n.status, WifiStatus::Saved | WifiStatus::Connected) {
          let ssid = n.ssid.clone();
          actions.push(
            Action::spawn("forget", t!("app.launcher.network.forget"), move |_, cx| {
              let forget = ForgetWifi::reply(ssid.clone(), cx);
              async move {
                forget.await?;
                Ok(Outcome::KeepOpen)
              }
            })
            .destructive()
            .keep_open()
            .confirm(Confirm {
              title: t!("app.launcher.network.forget_title", ssid = n.ssid.clone()).into(),
              message: t!("app.launcher.network.forget_message").into(),
              button: t!("app.launcher.network.forget").into(),
              skippable: false,
            }),
          );
        }
        actions
      }
    },
  )
}

fn networks(cx: &App) -> Vec<Rc<dyn Command>> {
  let mut networks = cx.network_manager().list_wifi_networks(cx).to_vec();
  networks.sort_by_key(|n| {
    (
      n.status != WifiStatus::Connected,
      std::cmp::Reverse(n.strength),
    )
  });
  networks.iter().map(network_row).collect()
}

fn observe_network(notify: Notify, cx: &mut App) -> Vec<Subscription> {
  let nm = cx.network_manager().clone();
  vec![
    watch(&nm.wifi_networks, &notify, cx),
    watch(&nm.vpns, &notify, cx),
    watch(&nm.hotspot, &notify, cx),
    watch(&nm.connectivity, &notify, cx),
    watch(&nm.wifi_enabled, &notify, cx),
  ]
}

/// One row per network in range and per VPN, the portal while there is one
struct NetworkSource;

impl CommandSource for NetworkSource {
  fn id(&self) -> SharedString {
    "network".into()
  }

  fn commands(&self, query: &Query, cx: &App) -> Vec<Rc<dyn Command>> {
    let mut commands = Vec::new();
    // networks only once typing, they would crowd the empty list
    if !query.is_empty() {
      commands.extend(networks(cx));
    }
    for vpn in cx.network_manager().list_vpns(cx) {
      let (name, uuid) = (vpn.name.clone(), vpn.uuid.clone());
      let up = matches!(
        vpn.state,
        corona_network_manager::ActiveConnectionState::Activated
          | corona_network_manager::ActiveConnectionState::Activating
      );
      commands.push(simple(
        format!("vpn:{uuid}"),
        move |_| Meta {
          title: t!("app.launcher.network.vpn", name = name.clone()).into(),
          subtitle: Some(on_off(up)),
          icon: Icon::Tile(IconName::ShieldCheck, 150.),
          keywords: vec!["vpn".into(), name.clone().into()],
          accessories: vec![tag(up)],
          args: vec![switch_arg()],
          ..Default::default()
        },
        {
          let uuid = uuid.clone();
          move |_| {
            let uuid = uuid.clone();
            vec![Action::spawn(
              "switch",
              t!("app.launcher.switch"),
              move |args, cx| {
                let on = switch_target(args, up);
                let switch = if on { Switch::On } else { Switch::Off };
                let task = SwitchVpn::reply((uuid.clone(), switch), cx);
                async move {
                  let on = task.await?;
                  Ok(Outcome::hud(format!("VPN {}", on_off(on))))
                }
              },
            )]
          }
        },
      ));
    }
    if cx.network_manager().connectivity(cx) == NmConnectivityState::Portal {
      commands.push(simple(
        "network:portal",
        |_| Meta {
          title: t!("app.launcher.network.portal").into(),
          icon: Icon::Tile(IconName::Globe, 40.),
          keywords: vec!["captive".into(), "login".into(), "hotel".into()],
          ..Default::default()
        },
        |_| {
          vec![Action::sync("open", t!("app.launcher.open"), |_, _, cx| {
            Portal::handle((), cx)?;
            Ok(Outcome::close())
          })]
        },
      ));
    }
    commands
  }

  fn observe(&self, notify: Notify, cx: &mut App) -> Vec<Subscription> {
    observe_network(notify, cx)
  }
}

fn hidden_form() -> View {
  let options = vec![
    ("wpa".into(), t!("app.launcher.network.wpa").into()),
    ("wpa3".into(), t!("app.launcher.network.wpa3").into()),
    ("open".into(), t!("app.launcher.network.open").into()),
  ];
  View::Form(FormView {
    title: t!("app.launcher.network.join_hidden").into(),
    fields: vec![
      Field::new("ssid", t!("app.launcher.network.ssid"), FieldKind::Text).required(),
      Field::new(
        "security",
        t!("app.launcher.network.security"),
        FieldKind::Dropdown(options),
      )
      .value("wpa"),
      Field::new("password", t!("app.network.password"), FieldKind::Password),
    ],
    submit_label: t!("app.launcher.network.connect").into(),
    submit: Rc::new(|values, _, cx| {
      let security = match values.get("security").map(String::as_str) {
        Some("open") => Security::Open,
        Some("wpa3") => Security::Wpa3,
        _ => Security::Wpa,
      };
      let password = values.get("password").filter(|p| !p.is_empty()).cloned();
      let request = HiddenRequest {
        ssid: values.get("ssid").cloned().unwrap_or_default(),
        security,
        password,
      };
      let join = JoinHidden::reply(request, cx);
      cx.spawn(async move |_| {
        join.await?;
        Ok(Outcome::hud(t!("app.launcher.network.connecting")))
      })
    }),
    cancel: None,
    message: None,
  })
}

pub fn init(cx: &mut App) {
  registry::register_source(Rc::new(NetworkSource), cx);
  Group::new(
    "network:fixed",
    vec![
      opener(
        "wifi:networks",
        t!("app.launcher.network.networks"),
        Icon::Tile(IconName::Wifi, 210.),
        &["wifi", "wlan", "connect"],
        |_| {
          ListView::new(
            t!("app.launcher.network.networks"),
            t!("app.launcher.network.search"),
            Rc::new(
              ListSource::new("wifi:list", |_, cx| networks(cx))
                .observe(observe_network)
                .on_open(|cx| {
                  let scan = Rescan::reply((), cx);
                  cx.spawn(async move |_| {
                    let _ = scan.await;
                  })
                  .detach();
                }),
            ),
          )
          .into_view()
        },
      ),
      simple(
        "wifi:disconnect",
        |_| Meta {
          title: t!("app.launcher.network.disconnect").into(),
          icon: Icon::Tile(IconName::WifiOff, 210.),
          keywords: vec!["wifi".into()],
          ..Default::default()
        },
        |_| {
          vec![Action::spawn(
            "disconnect",
            t!("app.launcher.network.disconnect"),
            |_, cx| {
              let task = DisconnectWifi::reply((), cx);
              async move {
                task.await?;
                Ok(Outcome::hud(t!("app.launcher.network.disconnected")))
              }
            },
          )]
        },
      ),
      simple(
        "wifi:rescan",
        |_| Meta {
          title: t!("app.launcher.network.rescan").into(),
          icon: Icon::Tile(IconName::RefreshCw, 210.),
          keywords: vec!["wifi".into(), "scan".into()],
          ..Default::default()
        },
        |_| {
          vec![Action::spawn(
            "rescan",
            t!("app.launcher.network.rescan"),
            |_, cx| {
              let task = Rescan::reply((), cx);
              async move {
                task.await?;
                Ok(Outcome::hud(t!("app.launcher.network.scanned")))
              }
            },
          )]
        },
      ),
      corona_launcher::kit::opener(
        "wifi:hidden",
        t!("app.launcher.network.join_hidden"),
        Icon::Tile(IconName::EyeOff, 210.),
        &["wifi", "ssid"],
        |_| hidden_form(),
      ),
      Toggle::command(
        "hotspot",
        t!("app.launcher.network.hotspot"),
        IconName::RadioTower,
        180.,
        |cx| {
          let nm = cx.network_manager();
          nm.wifi_supported(cx).then(|| nm.hotspot(cx).is_some())
        },
        |on, cx| {
          let task = Hotspot::reply(if on { Switch::On } else { Switch::Off }, cx);
          cx.spawn(async move |_| {
            if let Err(e) = task.await {
              tracing::warn!("hotspot: {e:#}");
            }
          })
          .detach();
          Ok(())
        },
      ),
    ],
  )
  .observe(observe_network)
  .register(cx);
}

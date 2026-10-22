use std::{pin::Pin, time::Duration};

use anyhow::Result;
use corona_components::async_listener::AsyncListenerExt;
use corona_components::components::card::CardExt;
use corona_network_manager::{
  DeviceState, HotspotBand, HotspotClient, HotspotConfig, NetworkManagerExt,
};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  App, AppContext, Context, Entity, InteractiveElement, IntoElement, ParentElement, Styled, Task,
  TestSupportExt, Window,
  assets::IconName,
  base::{Disableable, IndexPath, StyledExt, input::InputState},
  black,
  component::{
    Icon, Sizable, Theme,
    button::Button,
    input::Input,
    select::{Select, SelectState},
    spinner::Spinner,
    switch::Switch,
  },
  div,
  prelude::FluentBuilder,
  px, white,
};
use qrcode::{Color, QrCode};

use crate::control_center::network::NetworkPanel;
use rust_i18n::t;

pub struct HotspotForm {
  ssid: Entity<InputState>,
  password: Entity<InputState>,
  band: Entity<SelectState<Vec<String>>>,
  /// WPA3 or WPA2
  security: Entity<SelectState<Vec<String>>>,
  auto_off: Entity<SelectState<Vec<String>>>,
  /// whether the clients are kept from each other
  isolate: Entity<SelectState<Vec<String>>>,
  expanded: bool,
  /// starting or stopping, which can take a password and a wifi reconnect
  pending: bool,
  /// the running hotspot as Wi-Fi QR code text
  qr: Option<String>,
  clients: Vec<HotspotClient>,
  _poll_clients: Option<Task<()>>,
}

const BANDS: [HotspotBand; 2] = [HotspotBand::Bg, HotspotBand::A];
/// whether it runs WPA3
const SECURITY: [bool; 2] = [false, true];
/// minutes without clients, None: never
const AUTO_OFF: [Option<u32>; 5] = [None, Some(5), Some(10), Some(30), Some(60)];
/// whether the clients are kept from each other
const ISOLATE: [bool; 2] = [true, false];
const CLIENTS_POLL: Duration = Duration::from_secs(3);

fn band_label(band: HotspotBand) -> String {
  match band {
    HotspotBand::Bg => "2.4 GHz",
    HotspotBand::A => "5 GHz",
  }
  .into()
}

fn security_label(wpa3: bool) -> String {
  if wpa3 { "WPA3" } else { "WPA2" }.into()
}

fn isolate_label(isolate: bool) -> String {
  if isolate {
    t!("app.network.hotspot.isolate")
  } else {
    t!("app.network.hotspot.shared")
  }
  .into()
}

fn auto_off_label(minutes: Option<u32>) -> String {
  match minutes {
    Some(minutes) => t!("app.network.hotspot.auto_off", minutes = minutes),
    None => t!("app.network.hotspot.auto_off_never"),
  }
  .into()
}

/// the option of `options` selected in `select`, labelled by `label`
fn selected<T: Copy>(
  options: &[T],
  label: impl Fn(T) -> String,
  select: &Entity<SelectState<Vec<String>>>,
  cx: &App,
) -> Option<T> {
  let selected = select.read(cx).selected_value()?;
  options.iter().copied().find(|o| label(*o) == *selected)
}

/// what phone cameras read to join a network
fn wifi_qr_text(ssid: &str, password: Option<&str>) -> String {
  let escape = |text: &str| {
    text.chars().fold(String::new(), |mut out, c| {
      if matches!(c, '\\' | ';' | ',' | ':' | '"') {
        out.push('\\');
      }
      out.push(c);
      out
    })
  };
  match password {
    Some(password) => format!("WIFI:T:WPA;S:{};P:{};;", escape(ssid), escape(password)),
    None => format!("WIFI:T:nopass;S:{};;", escape(ssid)),
  }
}

/// dark modules on white with a quiet zone, whatever the theme
fn qr_code(text: &str) -> Option<impl IntoElement> {
  let code = QrCode::new(text).log_err().ok()?;
  let rows = code.to_colors();
  Some(
    div().flex().justify_center().child(
      div()
        .flex()
        .flex_col()
        .p_4()
        .rounded_md()
        .bg(white())
        .children(rows.chunks(code.width()).map(|row| {
          div().flex().children(row.iter().map(|module| {
            div()
              .size(px(4.))
              .when(*module == Color::Dark, |d| d.bg(black()))
          }))
        })),
    ),
  )
}

impl HotspotForm {
  pub fn new(window: &mut Window, cx: &mut App) -> Self {
    Self {
      ssid: cx.new(|cx| InputState::new(window, cx).placeholder(t!("app.network.hidden.ssid"))),
      password: cx.new(|cx| {
        InputState::new(window, cx)
          .masked(true)
          .placeholder(t!("app.network.hotspot.password"))
      }),
      band: cx.new(|cx| {
        SelectState::new(
          BANDS.map(band_label).to_vec(),
          Some(IndexPath::default()),
          window,
          cx,
        )
      }),
      security: cx.new(|cx| {
        SelectState::new(
          SECURITY.map(security_label).to_vec(),
          Some(IndexPath::default()),
          window,
          cx,
        )
      }),
      // 10 minutes, like Android
      auto_off: cx.new(|cx| {
        SelectState::new(
          AUTO_OFF.map(auto_off_label).to_vec(),
          Some(IndexPath::new(2)),
          window,
          cx,
        )
      }),
      isolate: cx.new(|cx| {
        SelectState::new(
          ISOLATE.map(isolate_label).to_vec(),
          Some(IndexPath::default()),
          window,
          cx,
        )
      }),
      expanded: false,
      pending: false,
      qr: None,
      clients: Vec::new(),
      _poll_clients: None,
    }
  }

  fn config(&self, cx: &App) -> HotspotConfig {
    let password = self.password.read(cx).value().to_string();
    HotspotConfig {
      ssid: self.ssid.read(cx).value().trim().to_string(),
      password: Some(password).filter(|password| !password.is_empty()),
      band: selected(&BANDS, band_label, &self.band, cx).unwrap_or(HotspotBand::Bg),
      wpa3: selected(&SECURITY, security_label, &self.security, cx).unwrap_or_default(),
      auto_off: selected(&AUTO_OFF, auto_off_label, &self.auto_off, cx).unwrap_or(Some(10)),
      isolate: selected(&ISOLATE, isolate_label, &self.isolate, cx).unwrap_or(true),
    }
  }
}

impl NetworkPanel {
  /// fills the form and the QR code from the running or the saved hotspot
  pub fn load_hotspot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    self.poll_hotspot_clients(cx);
    let load = cx.network_manager().hotspot_config(cx);
    cx.spawn_in(window, async move |this, cx| {
      let Ok(config) = load.await.log_err() else {
        return;
      };
      this
        .update_in(cx, |this, window, cx| this.fill_hotspot(config, window, cx))
        .ok();
    })
    .detach();
  }

  /// NM signals no client joining, so the running hotspot's are asked for every few seconds
  fn poll_hotspot_clients(&mut self, cx: &mut Context<Self>) {
    let running = cx.network_manager().hotspot(cx).is_some();
    let form = &mut self.hotspot_form;
    form.clients.clear();
    form._poll_clients = running.then(|| {
      cx.spawn(async move |this, cx| {
        loop {
          let clients = cx.update(|cx| cx.network_manager().hotspot_clients(cx));
          // a radio without nl80211 stays so, no point in asking again
          let Ok(clients) = clients.await.log_err() else {
            break;
          };
          let updated = this.update(cx, |this, cx| {
            this.hotspot_form.clients = clients;
            cx.notify();
          });
          if updated.is_err() {
            break;
          }
          cx.background_executor().timer(CLIENTS_POLL).await;
        }
      })
    });
  }

  fn fill_hotspot(
    &mut self,
    config: Option<HotspotConfig>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let running = cx.network_manager().hotspot(cx).is_some();
    let form = &mut self.hotspot_form;
    form.qr = config
      .as_ref()
      .filter(|_| running)
      .map(|c| wifi_qr_text(&c.ssid, c.password.as_deref()));
    // the first start shares a password, not an open network
    if let Some(config) = config.or_else(|| HotspotConfig::generated().log_err().ok()) {
      form
        .ssid
        .update(cx, |input, cx| input.set_value(config.ssid, window, cx));
      form.password.update(cx, |input, cx| {
        input.set_value(config.password.unwrap_or_default(), window, cx)
      });
      form.band.update(cx, |select, cx| {
        select.set_selected_value(&band_label(config.band), window, cx)
      });
      form.security.update(cx, |select, cx| {
        select.set_selected_value(&security_label(config.wpa3), window, cx)
      });
      form.auto_off.update(cx, |select, cx| {
        select.set_selected_value(&auto_off_label(config.auto_off), window, cx)
      });
      form.isolate.update(cx, |select, cx| {
        select.set_selected_value(&isolate_label(config.isolate), window, cx)
      });
    }
    cx.notify();
  }

  fn toggle_hotspot(
    &mut self,
    on: bool,
    cx: &mut Context<Self>,
  ) -> Pin<Box<dyn Future<Output = Result<()>>>> {
    self.hotspot_form.pending = true;
    if !on {
      return Box::pin(cx.network_manager().stop_hotspot(cx));
    }
    let config = self.hotspot_form.config(cx);
    // show what has to be fixed
    if config.validate().is_err() {
      self.hotspot_form.expanded = true;
    }
    Box::pin(cx.network_manager().start_hotspot(config, cx))
  }

  pub fn hotspot(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let nm = cx.network_manager();
    if !nm.wifi_supported(cx) {
      return None;
    }
    let hotspot = nm.hotspot(cx);
    let form = &self.hotspot_form;
    let running = hotspot.is_some();
    // without a second interface the hotspot takes over the connected one, where the radio
    // allows corona adds one, asking for root
    let connected = nm
      .primary_wifi(cx)
      .is_some_and(|wifi| wifi.state == DeviceState::Activated);
    let beside = nm.spare_wifi(cx).is_some() || (nm.hotspot_beside_wifi(cx) && !running);
    // every start and stop runs as root, a password unless the polkit policy is installed
    let hint = match (connected, beside) {
      (false, _) => None,
      (true, false) => Some(t!("app.network.hotspot.takes_wifi")),
      (true, true) => (!running).then(|| t!("app.network.hotspot.asks_admin")),
    };

    Some(
      div()
        .flex()
        .flex_col()
        .w_full()
        .flex_shrink_0()
        .gap_2()
        .p_2()
        .card(cx)
        .child(
          div()
            .flex()
            .gap_2()
            .items_center()
            .child(Icon::new(IconName::RadioTower).small())
            .child(
              div()
                .font_bold()
                .text_sm()
                .child(t!("app.network.hotspot.title")),
            )
            .when(form.pending || hotspot.is_some_and(|h| !h.active), |d| {
              d.child(Spinner::new().small())
            })
            .child(
              Button::new("hotspot-settings")
                .icon(if form.expanded {
                  IconName::ChevronUp
                } else {
                  IconName::ChevronDown
                })
                .small()
                .ml_auto()
                .cursor_pointer()
                .tooltip(t!("app.network.hotspot.settings"))
                .on_click(cx.listener(|this, _, _, cx| {
                  this.hotspot_form.expanded = !this.hotspot_form.expanded;
                  cx.notify();
                })),
            )
            .child(
              Switch::new("hotspot-enabled")
                .checked(running)
                // a second start while the first waits would take over the wifi
                .disabled(form.pending || nm.primary_wifi(cx).is_none())
                .on_change(cx.async_listener(
                  |this, checked, _, cx| this.toggle_hotspot(*checked, cx),
                  |this, result, _| {
                    this.hotspot_form.pending = false;
                    if let Err(e) = result.log_err() {
                      this.error = Some(e.to_string());
                    }
                  },
                )),
            ),
        )
        .when_some(hint, |d, hint| {
          d.child(
            div()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .child(hint),
          )
        })
        // a running hotspot keeps its settings until it is restarted
        .when(form.expanded, |d| {
          d.child(Input::new(&form.ssid).small().disabled(running))
            .child(
              Input::new(&form.password)
                .mask_toggle()
                .small()
                .disabled(running),
            )
            .child(Select::new(&form.band).small().disabled(running))
            .child(Select::new(&form.security).small().disabled(running))
            .child(Select::new(&form.auto_off).small().disabled(running))
            .child(Select::new(&form.isolate).small().disabled(running))
            .when(form.password.read(cx).value().is_empty(), |d| {
              d.child(
                div()
                  .text_xs()
                  .text_color(theme.colors.warning)
                  .child(t!("app.network.hotspot.open_warning")),
              )
            })
        })
        .when_some(
          hotspot.zip(form.qr.as_deref().and_then(qr_code)),
          |d, (hotspot, qr)| {
            d.child(qr).child(
              div()
                .flex()
                .justify_center()
                .text_xs()
                .text_color(theme.colors.muted_foreground)
                .child(t!("app.network.hotspot.scan", ssid = hotspot.ssid)),
            )
          },
        )
        .when(running && !form.clients.is_empty(), |d| {
          d.child(div().text_xs().font_bold().child(t!(
            "app.network.hotspot.clients",
            count = form.clients.len()
          )))
          .children(form.clients.iter().enumerate().map(|(i, client)| {
            div()
              .id(("hotspot-client", i))
              .flex()
              .justify_between()
              .gap_2()
              .text_xs()
              .child(
                client
                  .hostname
                  .clone()
                  .unwrap_or_else(|| client.mac.clone()),
              )
              .when_some(client.ip, |d, ip| {
                d.child(
                  div()
                    .text_color(theme.colors.muted_foreground)
                    .child(ip.to_string()),
                )
              })
              .test_support()
          }))
        }),
    )
  }
}

#[cfg(test)]
mod tests {
  use corona_network_manager::{
    HotspotClient, HotspotInfo, Interface, InterfaceType, NetworkManager,
  };
  use corona_utils::test_bus::TestBus;
  use futures::executor::block_on;
  use gpui_kit::{
    self as gpui, ScrollDelta, TestAppContext, VisualTestContext, point, size, test::TestWindowExt,
  };

  use super::*;
  use crate::control_center::ControlCenterPanel;

  #[test]
  fn wifi_qr_texts() {
    assert_eq!(
      wifi_qr_text("home", Some("pass word")),
      "WIFI:T:WPA;S:home;P:pass word;;"
    );
    assert_eq!(wifi_qr_text("café", None), "WIFI:T:nopass;S:café;;");
    assert_eq!(
      wifi_qr_text(r#"a;b,c:d"e\f"#, Some(";")),
      r#"WIFI:T:WPA;S:a\;b\,c\:d\"e\\f;P:\;;;"#
    );
    assert!(QrCode::new(wifi_qr_text("corona", Some(&"x".repeat(63)))).is_ok());
  }

  #[test]
  fn band_labels_round_trip() {
    for band in BANDS {
      let found: Vec<_> = BANDS
        .iter()
        .filter(|b| band_label(**b) == band_label(band))
        .collect();
      assert_eq!(found, [&band]);
    }
  }

  /// a panel over a fake NetworkManager with a wifi device, and the bus it sits on
  fn panel(cx: &mut TestAppContext) -> (TestBus, Entity<NetworkPanel>, &mut VisualTestContext) {
    cx.executor().allow_parking();
    let bus = TestBus::new();
    let conn = block_on(bus.conn());
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      let nm = NetworkManager::fake(&conn, cx);
      nm.wifi_supported
        .update(cx, |supported, _| *supported = true);
      nm.primary_wifi.update(cx, |wifi, _| {
        *wifi = Some(Interface {
          path: "/org/freedesktop/NetworkManager/Devices/2"
            .try_into()
            .unwrap(),
          name: "wlan0".into(),
          ip: None,
          kind: InterfaceType::Wireless,
          state: DeviceState::Disconnected,
        })
      });
      cx.set_global(nm);
    });
    let (panel, cx) = cx.add_window_view(NetworkPanel::init);
    (bus, panel, cx)
  }

  #[gpui::test]
  fn shows_a_running_hotspot(cx: &mut TestAppContext) {
    let (_bus, panel, cx) = panel(cx);
    cx.update(|window, cx| {
      window.render_frame(cx);
      assert_eq!(window.find("hotspot-enabled").checked(), Some(false));
      let nm = cx.network_manager().clone();
      nm.hotspot.update(cx, |hotspot, cx| {
        *hotspot = Some(HotspotInfo {
          active: true,
          ssid: "corona".into(),
          band: HotspotBand::A,
          connection_path: "/org/freedesktop/NetworkManager/Settings/5"
            .try_into()
            .unwrap(),
          // no interface of this machine, the panel finds no clients of its own
          interface: "corona-test0".into(),
          auto_off: None,
          corona: false,
          channel: None,
        });
        cx.notify();
      });
    });
    panel.update(cx, |panel, cx| {
      panel.hotspot_form.qr = Some(wifi_qr_text("corona", Some("hunter22")));
      panel.hotspot_form.expanded = true;
      panel.hotspot_form.clients = vec![HotspotClient {
        mac: "5e:0b:2a:11:42:07".into(),
        ip: Some([10, 42, 0, 65].into()),
        hostname: Some("phone".into()),
      }];
      cx.notify();
    });
    cx.update(|window, cx| {
      window.render_frame(cx);
      assert_eq!(window.find("hotspot-enabled").checked(), Some(true));
      assert!(window.try_find(("hotspot-client", 0usize)).is_some());
      cx.network_manager()
        .hotspot
        .clone()
        .update(cx, |hotspot, cx| {
          *hotspot = None;
          cx.notify();
        });
    });
    // a stopped hotspot has no clients
    cx.update(|window, cx| {
      window.render_frame(cx);
      assert!(window.try_find(("hotspot-client", 0usize)).is_none());
    });
  }

  #[gpui::test]
  fn a_tall_panel_scrolls(cx: &mut TestAppContext) {
    let (_bus, panel, cx) = panel(cx);
    cx.simulate_resize(size(px(320.), px(240.)));
    panel.update(cx, |panel, cx| {
      panel.hotspot_form.expanded = true;
      cx.notify();
    });
    let top = |cx: &mut VisualTestContext| {
      cx.update(|window, cx| {
        window.render_frame(cx);
        window.find("hotspot-enabled").bounds().origin.y
      })
    };
    let before = top(cx);
    cx.update(|window, cx| {
      let delta = ScrollDelta::Pixels(point(px(0.), px(-200.)));
      window.scroll("hotspot-settings", delta, cx);
    });
    assert!(top(cx) < before);
  }

  #[gpui::test]
  fn short_passwords_open_the_form(cx: &mut TestAppContext) {
    let (_bus, panel, cx) = panel(cx);
    let start = cx.update(|window, cx| {
      panel.update(cx, |panel, cx| {
        let form = &panel.hotspot_form;
        form
          .ssid
          .update(cx, |input, cx| input.set_value("corona", window, cx));
        form
          .password
          .update(cx, |input, cx| input.set_value("short", window, cx));
        assert!(!form.expanded);
        panel.toggle_hotspot(true, cx)
      })
    });
    // the switch is locked against a second start while this one runs
    panel.read_with(cx, |panel, _| assert!(panel.hotspot_form.pending));
    assert_eq!(
      block_on(start).unwrap_err().to_string(),
      "the hotspot password needs 8 to 63 characters"
    );
    panel.read_with(cx, |panel, _| assert!(panel.hotspot_form.expanded));
    cx.update(|window, cx| window.render_frame(cx));
  }
}

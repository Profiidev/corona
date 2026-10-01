use corona_components::async_listener::AsyncListenerExt;
use corona_network_manager::{
  ActiveConnectionState, DeviceState, FailReason, HiddenSecurity, Interface, InterfaceType,
  NetworkManagerExt, NmConnectivityState, Secret, SecretKind, Vpn, VpnKind, WifiNetwork,
  WifiStatus,
};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  App, AppContext, Context, Div, Entity, InteractiveElement, IntoElement, MouseButton,
  MouseDownEvent, ParentElement, Render, StatefulInteractiveElement, Styled, Subscription, Window,
  assets::IconName,
  base::{Disableable, StyledExt},
  component::{
    ActiveTheme, Icon, IndexPath, Sizable, Theme,
    button::{Button, ButtonVariant, ButtonVariants},
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
    select::{Select, SelectEvent, SelectState},
    spinner::Spinner,
    switch::Switch,
    tag::{Tag, TagVariant},
  },
  div,
  prelude::FluentBuilder,
  px,
};

use crate::control_center::ControlCenterPanel;

fn address(i: &Interface) -> String {
  if let Some(addr) = i.ip {
    format!("{}/{}", addr.address, addr.prefix)
  } else {
    "No address".to_string()
  }
}

#[derive(Debug, PartialEq, Eq)]
enum LoadingState {
  Idle,
  Loading,
  Error,
}

impl<T, E> From<Result<T, E>> for LoadingState {
  fn from(result: Result<T, E>) -> Self {
    if result.is_ok() {
      Self::Idle
    } else {
      Self::Error
    }
  }
}

pub struct NetworkPanel {
  connectivity_checking: LoadingState,
  wifi_scanning: LoadingState,
  error: Option<String>,
  secret_prompt: Option<SecretPrompt>,
  hidden_prompt: Option<HiddenPrompt>,
  _secret_request: Subscription,
}

struct SecretPrompt {
  password: Entity<InputState>,
  identity: Option<Entity<InputState>>,
  _submit: Vec<Subscription>,
}

struct HiddenPrompt {
  ssid: Entity<InputState>,
  password: Entity<InputState>,
  security_select: Entity<SelectState<Vec<&'static str>>>,
  /// mirrors the select, so the password field can hide for open networks
  security: HiddenSecurity,
  _security_changed: Subscription,
  _submit: Vec<Subscription>,
}

const SECURITY_OPTIONS: [(&str, HiddenSecurity); 3] = [
  ("Open", HiddenSecurity::Open),
  ("WPA/WPA2", HiddenSecurity::Wpa),
  ("WPA3", HiddenSecurity::Wpa3),
];

fn on_enter(
  input: &Entity<InputState>,
  window: &mut Window,
  cx: &mut Context<NetworkPanel>,
  submit: fn(&mut NetworkPanel, &mut Window, &mut Context<NetworkPanel>),
) -> Subscription {
  cx.subscribe_in(
    input,
    window,
    move |this, _, event: &InputEvent, window, cx| {
      if let InputEvent::PressEnter { .. } = event {
        submit(this, window, cx);
      }
    },
  )
}

fn overlay(
  theme: &Theme,
  card: Div,
  on_close: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
  div()
    .on_mouse_down(MouseButton::Left, on_close)
    .absolute()
    .inset_0()
    .flex()
    .items_center()
    .justify_center()
    .rounded_xl()
    .bg(theme.colors.background.opacity(0.8))
    .occlude()
    .child(
      card
        .flex()
        .flex_col()
        .w_3_4()
        .gap_2()
        .p_4()
        .rounded_xl()
        .border_1()
        .border_color(theme.colors.border)
        .bg(theme.colors.background)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
    )
}

impl ControlCenterPanel for NetworkPanel {
  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let secret_request = cx.network_manager().secret_request.clone();
    let subscription = cx.observe_in(&secret_request, window, |this, _, window, cx| {
      this.sync_secret_prompt(window, cx)
    });
    cx.defer_in(window, |this, window, cx| {
      this.sync_secret_prompt(window, cx)
    });

    Self {
      connectivity_checking: LoadingState::Idle,
      wifi_scanning: LoadingState::Idle,
      error: None,
      secret_prompt: None,
      hidden_prompt: None,
      _secret_request: subscription,
    }
  }
}

impl NetworkPanel {
  fn sync_secret_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(request) = cx.network_manager().secret_request(cx) else {
      self.secret_prompt = None;
      return;
    };
    if self.secret_prompt.is_some() {
      return;
    }
    let identity = (request.kind == SecretKind::Enterprise && request.identity.is_none())
      .then(|| cx.new(|cx| InputState::new(window, cx).placeholder("Username")));
    let password = cx.new(|cx| {
      InputState::new(window, cx)
        .masked(true)
        .placeholder("Password")
    });
    password.update(cx, |input, cx| input.focus(window, cx));
    let submit = identity
      .iter()
      .chain([&password])
      .map(|input| on_enter(input, window, cx, Self::submit_secret))
      .collect();
    self.secret_prompt = Some(SecretPrompt {
      password,
      identity,
      _submit: submit,
    });
  }

  fn submit_secret(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
    let Some(prompt) = &self.secret_prompt else {
      return;
    };
    let secret = Secret {
      password: prompt.password.read(cx).value().to_string(),
      identity: prompt
        .identity
        .as_ref()
        .map(|i| i.read(cx).value().to_string()),
    };
    cx.network_manager().clone().answer_secret(cx, Some(secret));
  }

  fn secret_prompt(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let prompt = self.secret_prompt.as_ref()?;
    let request = cx.network_manager().secret_request(cx)?;

    Some(overlay(
      theme,
      div()
        .child(
          div()
            .font_bold()
            .text_sm()
            .truncate()
            .child(format!("Password for {}", request.name)),
        )
        .when(request.retry, |d| {
          d.child(
            div()
              .text_xs()
              .text_color(theme.colors.danger)
              .child("Wrong password, try again"),
          )
        })
        .when_some(prompt.identity.as_ref(), |d, identity| {
          d.child(Input::new(identity).small())
        })
        .child(Input::new(&prompt.password).mask_toggle().small())
        .child(
          div()
            .flex()
            .gap_2()
            .justify_end()
            .child(
              Button::new("secret-cancel")
                .label("Cancel")
                .cursor_pointer()
                .small()
                .on_click(cx.listener(|_, _, _, cx| {
                  cx.network_manager().clone().answer_secret(cx, None);
                })),
            )
            .child(
              Button::new("secret-connect")
                .primary()
                .label("Connect")
                .cursor_pointer()
                .small()
                .on_click(cx.listener(|this, _, window, cx| this.submit_secret(window, cx))),
            ),
        ),
      cx.listener(|_, _, _, cx| cx.network_manager().clone().answer_secret(cx, None)),
    ))
  }

  fn open_hidden_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let ssid = cx.new(|cx| InputState::new(window, cx).placeholder("Network name"));
    let password = cx.new(|cx| {
      InputState::new(window, cx)
        .masked(true)
        .placeholder("Password")
    });
    let labels = SECURITY_OPTIONS.iter().map(|(label, _)| *label).collect();
    let security_select =
      cx.new(|cx| SelectState::new(labels, Some(IndexPath::default().row(1)), window, cx));
    let security_changed = cx.subscribe(
      &security_select,
      |this, _, event: &SelectEvent<Vec<&'static str>>, cx| {
        let SelectEvent::Confirm(Some(label)) = event else {
          return;
        };
        if let (Some(prompt), Some((_, security))) = (
          &mut this.hidden_prompt,
          SECURITY_OPTIONS.iter().find(|(l, _)| l == label),
        ) {
          prompt.security = *security;
          cx.notify();
        }
      },
    );
    ssid.update(cx, |input, cx| input.focus(window, cx));
    let submit = vec![
      on_enter(&ssid, window, cx, Self::join_hidden),
      on_enter(&password, window, cx, Self::join_hidden),
    ];
    self.hidden_prompt = Some(HiddenPrompt {
      ssid,
      password,
      security_select,
      security: HiddenSecurity::Wpa,
      _security_changed: security_changed,
      _submit: submit,
    });
    cx.notify();
  }

  fn join_hidden(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let Some(prompt) = &self.hidden_prompt else {
      return;
    };
    let ssid = prompt.ssid.read(cx).value().trim().to_string();
    if ssid.is_empty() {
      prompt.ssid.update(cx, |input, cx| input.focus(window, cx));
      return;
    }
    let password = Some(prompt.password.read(cx).value().to_string())
      .filter(|password| prompt.security != HiddenSecurity::Open && !password.is_empty());
    let join = cx
      .network_manager()
      .join_hidden_wifi(ssid, prompt.security, password, cx);
    self.hidden_prompt = None;
    cx.spawn(async move |this, cx| {
      if let Err(e) = join.await.log_err() {
        this
          .update(cx, |this, cx| {
            this.error = Some(e.to_string());
            cx.notify();
          })
          .ok();
      }
    })
    .detach();
    cx.notify();
  }

  fn hidden_prompt(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let prompt = self.hidden_prompt.as_ref()?;

    Some(overlay(
      theme,
      div()
        .child(div().font_bold().text_sm().child("Join hidden network"))
        .child(Input::new(&prompt.ssid).small())
        .child(Select::new(&prompt.security_select).small())
        .when(prompt.security != HiddenSecurity::Open, |d| {
          d.child(Input::new(&prompt.password).mask_toggle().small())
        })
        .child(
          div()
            .flex()
            .gap_2()
            .justify_end()
            .child(
              Button::new("hidden-cancel")
                .label("Cancel")
                .cursor_pointer()
                .small()
                .on_click(cx.listener(|this, _, _, cx| {
                  this.hidden_prompt = None;
                  cx.notify();
                })),
            )
            .child(
              Button::new("hidden-connect")
                .primary()
                .label("Connect")
                .cursor_pointer()
                .small()
                .on_click(cx.listener(|this, _, window, cx| this.join_hidden(window, cx))),
            ),
        ),
      cx.listener(|this, _, _, cx| {
        this.hidden_prompt = None;
        cx.notify();
      }),
    ))
  }

  fn status(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let primary = cx.network_manager().primary_interface(cx);
    let state = cx.network_manager().connectivity(cx);
    let connectivity_check_enabled = cx.network_manager().connectivity_check(cx).is_some();

    let icon = match primary {
      None => IconName::GlobeOff,
      Some(i) if i.kind == InterfaceType::Wired => IconName::EthernetPort,
      Some(_) => IconName::Wifi,
    };

    let (label, variant) = match state {
      NmConnectivityState::Full => ("Online", TagVariant::Success),
      NmConnectivityState::Portal => ("Sign-in required", TagVariant::Warning),
      NmConnectivityState::Loss => ("Limited", TagVariant::Warning),
      NmConnectivityState::None => ("Offline", TagVariant::Danger),
      NmConnectivityState::Unknown => ("Unknown", TagVariant::Secondary),
    };

    div()
      .flex()
      .flex_col()
      .w_full()
      .gap_2()
      .p_2()
      .rounded_xl()
      .bg(theme.colors.accent)
      .child(
        div()
          .flex()
          .gap_2()
          .items_center()
          .child(Icon::new(icon))
          .child(
            div()
              .flex()
              .text_sm()
              .font_bold()
              .child(primary.map_or("Disconnected".to_string(), |i| i.name.clone())),
          )
          .child(
            div()
              .flex()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .child(primary.map_or("No active connection".to_string(), address)),
          )
          .child(
            Button::new("connectivity-check")
              .small()
              .ml_auto()
              .cursor_pointer()
              .tooltip("Recheck")
              .disabled(!connectivity_check_enabled)
              .loading(self.connectivity_checking == LoadingState::Loading)
              .when_else(
                self.connectivity_checking == LoadingState::Error,
                |b| {
                  b.with_variant(ButtonVariant::Danger)
                    .icon(IconName::RotateCw)
                },
                |b| b.icon(IconName::RefreshCw),
              )
              .on_click(cx.async_listener(
                |this, _, _, cx| {
                  this.connectivity_checking = LoadingState::Loading;
                  cx.network_manager().check_connectivity()
                },
                |this, result, _| this.connectivity_checking = result.log_err().into(),
              )),
          )
          .child(Tag::new().small().with_variant(variant).child(label)),
      )
      .when(!connectivity_check_enabled, |d| {
        d.child(
          div()
            .flex()
            .text_xs()
            .text_color(theme.colors.muted_foreground)
            .child("Connectivity check is off, captive portals aren't detected"),
        )
      })
      .when(state == NmConnectivityState::Portal, |d| {
        d.child(
          div()
            .flex()
            .child(
              div()
                .text_xs()
                .text_color(theme.colors.muted_foreground)
                .child("Sign in to the network to get online"),
            )
            .child(
              Button::new("open-portal")
                .small()
                .label("Open")
                .cursor_pointer()
                .ml_auto()
                .icon(IconName::ExternalLink)
                .on_click(cx.listener(|_, _, _, cx| {
                  cx.network_manager().open_portal(cx);
                  cx.notify();
                })),
            ),
        )
      })
  }

  fn wifi(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let device = cx.network_manager().primary_wifi(cx);
    let enabled = cx.network_manager().wifi_enabled(cx);
    let supported = cx.network_manager().wifi_supported(cx);
    let networks = cx.network_manager().list_wifi_networks(cx);

    let placeholder_text = if !supported {
      Some("Wi-Fi not supported")
    } else if device.is_none() {
      Some("Wi-Fi is disabled")
    } else if networks.is_empty() {
      Some("No networks found")
    } else {
      None
    };

    div()
      .flex()
      .flex_col()
      .w_full()
      .flex_1()
      .min_h(px(160.))
      .gap_2()
      .p_2()
      .rounded_xl()
      .bg(theme.colors.accent)
      .child(
        div()
          .flex()
          .gap_2()
          .items_center()
          .child(div().font_bold().text_sm().child("Wi-Fi"))
          .child(
            Button::new("join-hidden-network")
              .icon(IconName::Plus)
              .disabled(device.is_none())
              .small()
              .ml_auto()
              .cursor_pointer()
              .tooltip("Join hidden network")
              .on_click(cx.listener(|this, _, window, cx| this.open_hidden_prompt(window, cx))),
          )
          .child(
            Button::new("wifi-rescan")
              .icon(IconName::RefreshCw)
              .disabled(device.is_none())
              .small()
              .cursor_pointer()
              .tooltip("Scan for networks")
              .loading(self.wifi_scanning == LoadingState::Loading)
              .when_else(
                self.wifi_scanning == LoadingState::Error,
                |b| {
                  b.with_variant(ButtonVariant::Danger)
                    .icon(IconName::RotateCw)
                },
                |b| b.icon(IconName::RefreshCw),
              )
              .on_click(cx.async_listener(
                |this, _, _, cx| {
                  this.wifi_scanning = LoadingState::Loading;
                  cx.network_manager().rescan(cx)
                },
                |this, result, _| this.wifi_scanning = result.log_err().into(),
              )),
          )
          .child(
            Switch::new("wifi-enabled")
              .checked(enabled)
              .disabled(!supported)
              .on_change(cx.async_listener(
                |_, checked, _, cx| cx.network_manager().set_wifi_enabled(*checked),
                |this, result, _| {
                  if let Err(e) = result.log_err() {
                    this.error = Some(e.to_string());
                  }
                },
              )),
          ),
      )
      .when_some(placeholder_text, |d, text| {
        d.child(
          div()
            .flex()
            .justify_center()
            .p_2()
            .text_xs()
            .text_color(theme.colors.muted_foreground)
            .child(text),
        )
      })
      .when_none(&placeholder_text, |d| {
        d.child(
          div()
            .flex()
            .flex_col()
            .gap_1()
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .children(networks.iter().map(|n| self.network(theme, n, device, cx))),
        )
      })
  }

  fn network(
    &self,
    theme: &Theme,
    network: &WifiNetwork,
    device: Option<&Interface>,
    cx: &Context<'_, Self>,
  ) -> impl IntoElement {
    let interface = device.map(|d| d.name.clone()).unwrap_or_default();
    let signal_icon = match network.strength {
      0..25 => IconName::WifiZero,
      25..50 => IconName::WifiLow,
      50..75 => IconName::WifiHigh,
      75.. => IconName::Wifi,
    };

    div()
      .id(format!("wifi-network-{}", network.ssid))
      .flex()
      .gap_2()
      .p_2()
      .w_full()
      .items_center()
      .rounded_xl()
      .bg(theme.colors.background)
      .child(Icon::new(signal_icon).small())
      .child(
        div()
          .flex_1()
          .min_w_0()
          .text_sm()
          .truncate()
          .child(network.ssid.clone()),
      )
      .when(network.secured, |d| {
        d.child(Icon::new(IconName::Lock).small().mr_1())
      })
      .when(network.status == WifiStatus::Connected, |d| {
        d.child(
          Tag::new()
            .small()
            .with_variant(TagVariant::Success)
            .child("Connected"),
        )
      })
      .when(network.status == WifiStatus::Connecting, |d| {
        d.child(Spinner::new().small())
      })
      .when(network.status == WifiStatus::NeedAuth, |d| {
        d.child(
          Tag::new()
            .small()
            .with_variant(TagVariant::Warning)
            .child("Password"),
        )
      })
      .when(network.status == WifiStatus::Saved, |d| {
        d.child(
          Tag::new()
            .small()
            .with_variant(TagVariant::Secondary)
            .child("Saved"),
        )
      })
      .when_else(
        network.status != WifiStatus::New && network.status != WifiStatus::Saved,
        |d| {
          d.child(
            Button::new(format!("wifi-disconnect-{}", network.ssid))
              .icon(IconName::Unplug)
              .small()
              .tooltip("Disconnect")
              .cursor_pointer()
              .on_click(cx.async_listener(
                move |_, _, _, cx| cx.network_manager().disconnect(&interface, cx),
                |this, result, _| {
                  if let Err(e) = result.log_err() {
                    this.error = Some(e.to_string());
                  }
                },
              )),
          )
        },
        |d| {
          d.cursor_pointer().on_click(cx.async_listener(
            {
              let ssid = network.ssid.clone();
              move |_, _, _, cx| cx.network_manager().connect_wifi(ssid.clone(), cx)
            },
            |this, result, _| {
              if let Err(e) = result.log_err() {
                this.error = Some(e.to_string());
              }
            },
          ))
        },
      )
      .when(network.status != WifiStatus::New, |d| {
        d.child(
          Button::new(format!("wifi-forget-{}", network.ssid))
            .icon(IconName::Trash)
            .small()
            .tooltip("Forget")
            .cursor_pointer()
            .on_click(cx.async_listener(
              {
                let ssid = network.ssid.clone();
                move |_, _, _, cx| {
                  cx.stop_propagation();
                  cx.network_manager().forget_wifi(ssid.clone(), cx)
                }
              },
              |this, result, _| {
                if let Err(e) = result.log_err() {
                  this.error = Some(e.to_string());
                }
              },
            )),
        )
      })
  }

  fn vpns(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    let vpns = cx.network_manager().list_vpns(cx);

    if vpns.is_empty() {
      return None;
    }

    Some(
      div()
        .flex()
        .flex_col()
        .w_full()
        .when_else(vpns.len() > 2, |d| d.min_h(px(128.)), |d| d.flex_shrink_0())
        .gap_2()
        .p_2()
        .rounded_xl()
        .bg(theme.colors.accent)
        .child(
          div()
            .flex()
            .gap_2()
            .items_center()
            .child(div().font_bold().text_sm().child("VPNs")),
        )
        .child(
          div()
            .flex()
            .flex_col()
            .gap_1()
            .h_auto()
            .overflow_y_scrollbar()
            .children(vpns.iter().map(|v| self.vpn(theme, v, cx))),
        ),
    )
  }

  fn vpn(&self, theme: &Theme, vpn: &Vpn, cx: &Context<'_, Self>) -> impl IntoElement {
    let up = vpn.state == ActiveConnectionState::Activated
      || vpn.state == ActiveConnectionState::Activating;

    div()
      .flex()
      .w_full()
      .gap_2()
      .p_2()
      .items_center()
      .rounded_xl()
      .bg(theme.colors.background)
      .child(
        Icon::new(if up {
          IconName::ShieldCheck
        } else {
          IconName::Shield
        })
        .small(),
      )
      .child(div().text_sm().truncate().child(vpn.name.clone()))
      .child(
        div()
          .text_xs()
          .text_color(theme.colors.muted_foreground)
          .child(if vpn.kind == VpnKind::WireGuard {
            "WireGuard"
          } else {
            "VPN"
          }),
      )
      .when(vpn.state == ActiveConnectionState::Activated, |d| {
        d.child(
          Tag::new()
            .small()
            .with_variant(TagVariant::Success)
            .child("Connected"),
        )
      })
      .child(
        Button::new(format!("vpn-{}", vpn.name))
          .small()
          .cursor_pointer()
          .ml_auto()
          .loading(
            vpn.state == ActiveConnectionState::Activating
              || vpn.state == ActiveConnectionState::Deactivating,
          )
          .icon(if up { IconName::Unplug } else { IconName::Plug })
          .on_click(cx.async_listener(
            {
              let uuid = vpn.uuid.clone();
              move |_, _, _, cx| {
                let disconnect = cx.network_manager().disconnect_vpn(uuid.clone());
                let connect = cx.network_manager().connect_vpn(uuid.clone());

                async move { if up { disconnect.await } else { connect.await } }
              }
            },
            |this, result, _| {
              if let Err(e) = result.log_err() {
                this.error = Some(e.to_string());
              }
            },
          )),
      )
  }

  fn interfaces(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let interfaces = cx.network_manager().list_interfaces(cx);

    div()
      .flex()
      .flex_col()
      .w_full()
      .when_else(
        interfaces.len() > 2,
        |d| d.min_h(px(128.)),
        |d| d.flex_shrink_0(),
      )
      .gap_2()
      .p_2()
      .rounded_xl()
      .bg(theme.colors.accent)
      .child(
        div()
          .flex()
          .gap_2()
          .items_center()
          .child(div().font_bold().text_sm().child("Interfaces")),
      )
      .child(
        div()
          .flex()
          .flex_col()
          .gap_1()
          .h_auto()
          .overflow_y_scrollbar()
          .children(interfaces.iter().map(|i| self.interface(theme, i, cx))),
      )
      .into_any_element()
  }

  fn interface(
    &self,
    theme: &Theme,
    interface: &Interface,
    cx: &Context<'_, Self>,
  ) -> impl IntoElement {
    let status = match interface.state {
      DeviceState::Activated => address(interface),
      DeviceState::Unmanaged => "Unmanaged".into(),
      DeviceState::Unavailable => "Unavailable".into(),
      DeviceState::Disconnected => "Disconnected".into(),
      DeviceState::Prepare => "Preparing".into(),
      DeviceState::Config => "Configuring".into(),
      DeviceState::NeedAuth => "Needs authentication".into(),
      DeviceState::IpConfig => "Getting address".into(),
      DeviceState::IpCheck => "Checking connection".into(),
      DeviceState::Secondaries => "Starting dependencies".into(),
      DeviceState::Deactivating => "Disconnecting".into(),
      DeviceState::Failed => "Failed".into(),
      DeviceState::Unknown => "Unknown".into(),
    };
    // Some(true): disconnect, also cancels an attempt still in progress, Some(false): connect,
    // None: NM can't act on the device right now
    let disconnect = match interface.state {
      DeviceState::Prepare
      | DeviceState::Config
      | DeviceState::NeedAuth
      | DeviceState::IpConfig
      | DeviceState::IpCheck
      | DeviceState::Secondaries
      | DeviceState::Activated => Some(true),
      DeviceState::Disconnected | DeviceState::Failed => Some(false),
      DeviceState::Unmanaged
      | DeviceState::Unavailable
      | DeviceState::Deactivating
      | DeviceState::Unknown => None,
    };

    div()
      .flex()
      .w_full()
      .gap_2()
      .p_2()
      .items_center()
      .rounded_xl()
      .bg(theme.colors.background)
      .child(
        Icon::new(match interface.kind {
          InterfaceType::Wired => IconName::EthernetPort,
          InterfaceType::Wireless => IconName::Wifi,
        })
        .small(),
      )
      .child(div().text_sm().truncate().child(interface.name.clone()))
      .child(
        div()
          .text_xs()
          .text_color(theme.colors.muted_foreground)
          .child(status),
      )
      .when_some(disconnect, |d, disconnect| {
        d.child(
          Button::new(format!("interface-{}", interface.name))
            .small()
            .icon(if disconnect {
              IconName::Unplug
            } else {
              IconName::Plug
            })
            .tooltip(if disconnect { "Disconnect" } else { "Connect" })
            .cursor_pointer()
            .ml_auto()
            .on_click(cx.async_listener(
              {
                let name = interface.name.clone();
                move |_, _, _, cx| {
                  let disconnect_task = cx.network_manager().disconnect(&name, cx);
                  let connect_task = cx.network_manager().connect(&name, cx);

                  async move {
                    if disconnect {
                      disconnect_task.await
                    } else {
                      connect_task.await
                    }
                  }
                }
              },
              |this, result, _| {
                if let Err(e) = result.log_err() {
                  this.error = Some(e.to_string());
                }
              },
            )),
        )
      })
  }

  fn error(&self, theme: &Theme, cx: &Context<'_, Self>) -> Option<impl IntoElement> {
    if let Some(error) = &self.error {
      return Some(
        div()
          .flex()
          .gap_2()
          .p_2()
          .rounded_xl()
          .bg(theme.colors.accent)
          .child(
            div()
              .text_sm()
              .text_color(theme.colors.danger)
              .truncate()
              .child(error.clone()),
          )
          .child(
            Button::new("error-dismiss")
              .small()
              .ml_auto()
              .icon(IconName::X)
              .cursor_pointer()
              .on_click(cx.listener(|this, _, _, cx| {
                this.error = None;
                cx.notify();
              })),
          ),
      );
    }

    let failure = cx.network_manager().wifi_failure(cx)?;

    let error = match failure.reason {
      FailReason::SsidNotFound => "Network not found".to_string(),
      FailReason::NoSecrets => "No password provided".to_string(),
      FailReason::Other(code) => format!("Connection failed: {}", code),
    };

    Some(
      div()
        .flex()
        .gap_2()
        .p_2()
        .rounded_xl()
        .bg(theme.colors.accent)
        .child(
          div()
            .text_sm()
            .text_color(theme.colors.danger)
            .truncate()
            .child(error),
        ),
    )
  }
}

impl Render for NetworkPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();

    div()
      .relative()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .child(self.status(theme, cx))
      .when_some(self.error(theme, cx), |d, error| d.child(error))
      .child(self.wifi(theme, cx))
      .when_some(self.vpns(theme, cx), |d, vpns| d.child(vpns))
      .child(self.interfaces(theme, cx))
      .when_some(self.hidden_prompt(theme, cx), |d, prompt| d.child(prompt))
      .when_some(self.secret_prompt(theme, cx), |d, prompt| d.child(prompt))
  }
}

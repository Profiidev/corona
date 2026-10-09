use std::{path::Path, rc::Rc, time::Duration};

use anyhow::{Context as _, Result, anyhow};
use corona_auth_screen::{AuthScreen, Check, MenuItem, Purpose, User, corner_menu};
use gpui_kit::{
  Anchor, App, AppContext, Bounds, Context, Entity, IntoElement, ParentElement, Pixels, Render,
  SharedString, Size, Styled, Task, Window, WindowBackgroundAppearance, WindowDecorations,
  WindowOptions,
  assets::IconName,
  base::Root,
  black,
  component::{ActiveTheme, button::Button},
  div, point,
  prelude::FluentBuilder,
};
use rust_i18n::t;

use corona_utils::error::ErrorLogExt;
use greetd::Session;

mod config;
mod greetd;
mod outputs;

rust_i18n::i18n!("../../assets/locales", fallback = "en");

const AVATARS: &str = "/var/lib/AccountsService/icons";
const LAST_SESSION: &str = "last-session";
const LAST_USER: &str = "last-user";

struct Greeter {
  screen: Entity<AuthScreen>,
  monitor: Option<String>,
  /// The outputs' names and places
  outputs: Vec<(String, Bounds<Pixels>)>,
  /// The window size `outputs` were read at
  outputs_for: Option<Size<Pixels>>,
  sessions: Vec<Session>,
  selected: usize,
}

impl Greeter {
  fn new(monitor: Option<String>, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let names = std::fs::read_to_string("/etc/passwd")
      .map(|passwd| greetd::users(&passwd))
      .unwrap_or_default();
    let last_user = config::state_file(LAST_USER).and_then(|file| config::read_state(&file));
    let user = names
      .iter()
      .position(|name| Some(name) == last_user.as_ref())
      .unwrap_or(0);
    let users = names
      .into_iter()
      .map(|name| {
        let avatar = Path::new(AVATARS).join(&name);
        User {
          avatar: avatar.exists().then(|| avatar.into()),
          name: name.into(),
        }
      })
      .collect();
    let this = cx.weak_entity();
    let check: Check = Rc::new(move |user, password, cx: &mut App| {
      let session = this
        .upgrade()
        .and_then(|this| this.read(cx).session().cloned());
      let Some(session) = session else {
        return Task::ready(Err(anyhow!("no Wayland session to start")));
      };
      let id = session.id.clone();
      let name = user.clone();
      let login = cx
        .background_executor()
        .spawn(async move { greetd::login(user, password, session) });
      cx.spawn(async move |cx| {
        let right = login.await?;
        if right {
          for (state, value) in [(LAST_SESSION, &id), (LAST_USER, &name)] {
            if let Some(file) = config::state_file(state) {
              let _ = config::save_state(&file, value).log_err();
            }
          }
          // greetd starts the session once we are gone
          cx.update(|cx| cx.quit());
        }
        Ok(right)
      })
    });
    let sessions = greetd::sessions();
    let last = config::state_file(LAST_SESSION).and_then(|file| config::read_state(&file));
    Self {
      screen: cx.new(|cx| AuthScreen::new(Purpose::Login, users, user, check, window, cx)),
      monitor,
      outputs: Vec::new(),
      outputs_for: None,
      selected: (sessions.iter())
        .position(|s| Some(&s.id) == last.as_ref())
        .unwrap_or(0),
      sessions,
    }
  }

  fn session(&self) -> Option<&Session> {
    self.sessions.get(self.selected)
  }

  fn session_menu(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
    let this = cx.entity();
    let items = (self.sessions.iter().enumerate())
      .map(|(i, session)| {
        let this = this.clone();
        MenuItem {
          id: format!("session-{i}").into(),
          icon: (i == self.selected).then_some(IconName::Check),
          label: session.name.clone().into(),
          enabled: true,
          on_click: Rc::new(move |_, cx| {
            this.update(cx, |this, cx| {
              this.selected = i;
              cx.notify();
            })
          }),
        }
      })
      .collect();
    let label: SharedString = match self.session() {
      Some(session) => session.name.clone().into(),
      None => t!("app.lock.no_session").into(),
    };
    let trigger = Button::new("session-trigger")
      .icon(IconName::Monitor)
      .label(label);
    corner_menu(
      "session-menu",
      Anchor::BottomLeft,
      trigger,
      vec![items],
      |_| {},
    )
  }
}

impl Render for Greeter {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    // cage resizes the window when outputs come, go or move
    let viewport = window.viewport_size();
    if self.outputs_for != Some(viewport) {
      self.outputs_for = Some(viewport);
      self.outputs = outputs::layout()
        .inspect_err(|e| tracing::error!("greeter: no output layout: {e:#}"))
        .unwrap_or_default();
      tracing::info!("greeter: outputs {:?} in {viewport:?}", self.outputs);
    }
    let displays = marked(&self.outputs, self.monitor.as_deref());
    let theme = cx.theme();
    div()
      .size_full()
      .relative()
      .text_sm()
      .bg(black())
      .text_color(theme.foreground)
      .child(
        div()
          .absolute()
          .map(|d| match login_area(&displays) {
            Some(area) => d
              .left(area.origin.x)
              .top(area.origin.y)
              .w(area.size.width)
              .h(area.size.height),
            None => d.size_full(),
          })
          .child(self.screen.clone())
          .child(self.session_menu(cx)),
      )
  }
}

/// Each output's place, flagged when it is `monitor`
fn marked(
  outputs: &[(String, Bounds<Pixels>)],
  monitor: Option<&str>,
) -> Vec<(bool, Bounds<Pixels>)> {
  (outputs.iter())
    .map(|(name, bounds)| (monitor == Some(name.as_str()), *bounds))
    .collect()
}

/// Where the login goes in the window: cage stretches it over every output,
/// its origin at the layout's top left. The wanted display, else the leftmost.
fn login_area(displays: &[(bool, Bounds<Pixels>)]) -> Option<Bounds<Pixels>> {
  let left = displays.iter().map(|(_, b)| b.origin.x).min()?;
  let top = displays.iter().map(|(_, b)| b.origin.y).min()?;
  let (_, area) = (displays.iter().find(|(wanted, _)| *wanted))
    .or_else(|| (displays.iter()).min_by_key(|(_, b)| (b.origin.x, b.origin.y)))?;
  Some(Bounds {
    origin: area.origin - point(left, top),
    size: area.size,
  })
}

async fn init_dbus(cx: &mut App) -> Result<()> {
  let system = zbus::connection::Builder::system()?
    .method_timeout(Duration::from_secs(5))
    .build()
    .await?;
  corona_power::init(cx, &system).await
}

fn main() {
  tracing_subscriber::fmt::init();

  gpui_kit::application()
    .with_assets(corona_components::assets::Assets)
    .run(|cx| {
      gpui_kit::init(cx);
      let config = (config::config_file().context("no home directory"))
        .and_then(|file| config::read(&file))
        .inspect_err(|e| tracing::error!("greeter: default settings: {e:#}"))
        .unwrap_or_default();
      let language = (config.language)
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_default();
      let monitor = config.monitor;
      // the theme loader reads the shell's settings
      cx.set_global(corona_config::Config {
        theme: config.theme,
        ..Default::default()
      });
      rust_i18n::set_locale(language.split(['_', '.', '-']).next().unwrap_or_default());
      corona_components::assets::load(cx).expect("Failed to load assets");
      cx.foreground_executor()
        .clone()
        .block_on(init_dbus(cx))
        .expect("Failed to init dbus");

      cx.open_window(
        WindowOptions {
          window_background: WindowBackgroundAppearance::Opaque,
          window_decorations: Some(WindowDecorations::Client),
          app_id: Some("corona-greeter".into()),
          titlebar: None,
          ..Default::default()
        },
        |window, cx| {
          let view = cx.new(|cx| Greeter::new(monitor, window, cx));
          let focus = view.read(cx).screen.read(cx).focus_handle(cx);
          window.focus(&focus, cx);
          cx.new(|cx| Root::new(view, window, cx))
        },
      )
      .expect("Failed to open the greeter window");
    });
}

#[cfg(test)]
mod tests {
  use gpui_kit::{Size, px};

  use super::*;

  fn at(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
    Bounds {
      origin: point(px(x), px(y)),
      size: Size::new(px(w), px(h)),
    }
  }

  #[test]
  fn monitor_is_found_by_name() {
    let outputs = [
      ("DP-4".to_string(), at(1920., 0., 2560., 1440.)),
      ("eDP-1".to_string(), at(0., 0., 1920., 1200.)),
    ];
    assert_eq!(
      marked(&outputs, Some("eDP-1")),
      [(false, outputs[0].1), (true, outputs[1].1)]
    );
    // unset or disconnected: none, so the leftmost
    for monitor in [None, Some("HDMI-A-1"), Some("edp-1")] {
      let displays = marked(&outputs, monitor);
      assert!(displays.iter().all(|(wanted, _)| !wanted));
      assert_eq!(login_area(&displays), Some(outputs[1].1));
    }
    // the login lands on eDP-1 wherever it sits
    let swapped = [
      ("DP-4".to_string(), at(0., 0., 2560., 1440.)),
      ("eDP-1".to_string(), at(2560., 0., 1920., 1200.)),
    ];
    assert_eq!(
      login_area(&marked(&swapped, Some("eDP-1"))),
      Some(at(2560., 0., 1920., 1200.))
    );
  }

  #[test]
  fn login_on_one_display() {
    assert_eq!(login_area(&[]), None);
    let hdmi = at(0., 0., 1920., 1080.);
    let dp = at(1920., 0., 2560., 1440.);
    // the leftmost without a wanted one
    assert_eq!(login_area(&[(false, dp), (false, hdmi)]), Some(hdmi));
    assert_eq!(
      login_area(&[(false, hdmi), (true, dp)]),
      Some(at(1920., 0., 2560., 1440.))
    );
    // relative to the layout's top left
    let above = at(-1000., -500., 1000., 800.);
    assert_eq!(
      login_area(&[(false, above), (true, hdmi)]),
      Some(at(1000., 500., 1920., 1080.))
    );
  }
}

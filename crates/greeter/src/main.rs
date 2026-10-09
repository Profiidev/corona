use std::{path::Path, rc::Rc, time::Duration};

use anyhow::{Context as _, Result, anyhow};
use corona_auth_screen::{AuthScreen, Check, MenuItem, Purpose, User, corner_menu};
use gpui_kit::{
  Anchor, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString,
  Styled, Task, Window, WindowBackgroundAppearance, WindowDecorations, WindowOptions,
  assets::IconName,
  base::Root,
  component::{ActiveTheme, button::Button},
  div,
};
use rust_i18n::t;

use corona_utils::error::ErrorLogExt;
use greetd::Session;

mod config;
mod greetd;

rust_i18n::i18n!("../../assets/locales", fallback = "en");

const AVATARS: &str = "/var/lib/AccountsService/icons";

struct Greeter {
  screen: Entity<AuthScreen>,
  sessions: Vec<Session>,
  selected: usize,
}

impl Greeter {
  fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    let name = std::fs::read_to_string("/etc/passwd")
      .ok()
      .and_then(|passwd| greetd::first_user(&passwd))
      .unwrap_or_default();
    let avatar = Path::new(AVATARS).join(&name);
    let user = User {
      avatar: avatar.exists().then(|| avatar.into()),
      name: name.into(),
    };
    let this = cx.weak_entity();
    let check: Check = Rc::new(move |user, password, cx: &mut App| {
      let session = this
        .upgrade()
        .and_then(|this| this.read(cx).session().cloned());
      let Some(session) = session else {
        return Task::ready(Err(anyhow!("no Wayland session to start")));
      };
      let id = session.id.clone();
      let login = cx
        .background_executor()
        .spawn(async move { greetd::login(user, password, session) });
      cx.spawn(async move |cx| {
        let right = login.await?;
        if right {
          if let Some(file) = config::last_session_file() {
            let _ = config::save_session(&file, &id).log_err();
          }
          // greetd starts the session once we are gone
          cx.update(|cx| cx.quit());
        }
        Ok(right)
      })
    });
    let sessions = greetd::sessions();
    let last = config::last_session_file().and_then(|file| config::last_session(&file));
    Self {
      screen: cx.new(|cx| AuthScreen::new(Purpose::Login, user, check, window, cx)),
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
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    div()
      .size_full()
      .relative()
      .text_sm()
      .bg(theme.background)
      .text_color(theme.foreground)
      .child(self.screen.clone())
      .child(self.session_menu(cx))
  }
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
          let view = cx.new(|cx| Greeter::new(window, cx));
          let focus = view.read(cx).screen.read(cx).focus_handle(cx);
          window.focus(&focus, cx);
          cx.new(|cx| Root::new(view, window, cx))
        },
      )
      .expect("Failed to open the greeter window");
    });
}

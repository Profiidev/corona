use std::time::Duration;

use anyhow::Result;
use corona_config::{ConfigProvider, observe_section};
use corona_notifications::NotificationsExt;
use corona_surface::bar::BarState;
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;
use rust_i18n::t;

rust_i18n::i18n!("../../assets/locales", fallback = "en");

pub mod commands;
mod control_center;
pub mod i18n;
mod icons;
mod idle;
mod lock;
mod osds;
pub mod overlays;
mod plugins;
pub mod settings;
mod widgets;

pub fn init(cx: &mut App) {
  init_ipc(cx);
  init_integrations(cx);
  register_variants(cx);
  lock::init(cx);
  osds::init(cx);
  overlays::notification::init(cx);
  overlays::switcher::init(cx);
  overlays::taskbar::init(cx);
  overlays::wallpaper::init(cx);
  overlays::screen_corners::init(cx);

  BarState::spawn_bars(cx);
  watch_config(cx);
}

/// Reloads the settings as their files change; a broken file is shown, the
/// settings before it stay.
fn watch_config(cx: &mut App) {
  let watched = corona_config::watch(cx, config_failed);
  if let Err(e) = watched {
    tracing::error!("Failed to watch the config: {e:#}");
  }

  observe_section(
    cx,
    |c| &c.shell.language,
    |language, cx| {
      i18n::apply(language.as_deref());
      cx.refresh_windows();
    },
  );

  // read once at startup
  fn later<T>(name: &'static str) -> impl FnMut(&T, &mut App) {
    move |_, _| tracing::warn!("{name} applies after a restart")
  }
  observe_section(
    cx,
    |c| &c.brightness.enable_ddcutil,
    later("brightness.enable_ddcutil"),
  );
  observe_section(
    cx,
    |c| &c.notification.enabled,
    later("notification.enabled"),
  );
}

/// Shows why the settings did not load; the shell keeps the ones it had
fn config_failed(error: String, cx: &mut App) {
  let send = cx
    .notifications()
    .send(t!("app.config_not_loaded").into(), error);
  cx.spawn(async move |_| {
    let _ = send.await.log_err();
  })
  .detach();
}

fn init_ipc(cx: &mut App) {
  let Some(mut server) = corona_ipc::IpcServer::new().expect("Failed to create IPC server") else {
    tracing::error!("Failed to create IPC server: socket already in use");
    std::process::exit(1);
  };

  corona_surface::commands::register_commands(&mut server);
  overlays::register_commands(&mut server);
  commands::register_commands(&mut server);

  cx.spawn(async move |cx| server.run(cx).await).detach();
}

fn init_integrations(cx: &mut App) {
  let config_error = corona_config::load(cx).err();
  i18n::apply(cx.config().shell.language.as_deref());
  corona_script::init(cx).expect("Failed to init script manager");
  corona_compositor::init(cx).expect("Failed to init compositor");
  corona_pipewire::init(cx).expect("Failed to init pipewire");
  corona_sysinfo::init(cx);
  cx.foreground_executor()
    .clone()
    .block_on(init_dbus(cx))
    .expect("Failed to init dbus");
  corona_components::assets::load(cx).expect("Failed to load assets");
  corona_surface::init(cx).expect("Failed to init ui");

  if let Some(e) = config_error {
    tracing::error!("Failed to load config, using the defaults: {e:#}");
    config_failed(format!("{e:#}"), cx);
  }
}

fn register_variants(cx: &mut App) {
  control_center::register_panels(cx);
  widgets::register_widgets(cx);
  plugins::init(cx);
}

async fn init_dbus(cx: &mut App) -> Result<()> {
  let system = zbus::connection::Builder::system()?
    .method_timeout(Duration::from_secs(5))
    .build()
    .await?;

  corona_network_manager::init(cx, &system).await?;
  corona_bluez::init(cx, &system).await?;
  corona_power::init(cx, &system).await?;
  corona_weather::init(cx, &system);
  corona_brightness::init(cx, &system)?;
  corona_auth::init(cx, &system);

  let session = zbus::connection::Builder::session()?
    .method_timeout(Duration::from_secs(5))
    .build()
    .await?;
  corona_mpris::init(cx, &session).await?;
  let serve = cx.config().notification.enabled;
  corona_notifications::init(cx, &session, serve).await?;
  corona_tray::init(cx, &session).await?;
  idle::init(cx, &session);
  corona_script::init_dbus(cx, &system, &session);

  Ok(())
}

/// Shared test helpers
#[cfg(test)]
pub(crate) mod test_support {
  use std::{cell::RefCell, rc::Rc};

  use anyhow::Result;
  use corona_compositor::{Compositor, CompositorImpl, types};
  use gpui_kit::TestAppContext;

  pub fn workspace(id: &str, monitor: &str) -> types::Workspace {
    types::Workspace {
      id: id.into(),
      name: id.into(),
      monitor: monitor.into(),
      monitor_id: 0,
    }
  }

  pub fn monitor(name: &str) -> types::Monitor {
    types::Monitor {
      id: 0,
      name: name.into(),
      width: 1920,
      height: 1080,
      refresh_rate: 60.,
      x: 0,
      y: 0,
      active_scratchpad: None,
      active_workspace: workspace("1", name),
      scale: 1.,
      focused: true,
      disabled: false,
      mirror_of: "none".into(),
    }
  }

  /// A compositor answering from its fields and recording what it was asked
  #[derive(Default)]
  pub struct FakeCompositor {
    pub workspaces: Vec<types::Workspace>,
    pub windows: Vec<types::Window>,
    pub monitors: Vec<types::Monitor>,
    pub calls: RefCell<Vec<String>>,
  }

  impl CompositorImpl for FakeCompositor {
    fn list_workspaces(&self) -> Result<Vec<types::Workspace>> {
      Ok(self.workspaces.clone())
    }
    fn active_workspace(&self) -> Result<types::Workspace> {
      Ok(workspace("1", "DP-1"))
    }
    fn list_monitors(&self) -> Result<Vec<types::Monitor>> {
      Ok(self.monitors.clone())
    }
    fn active_monitor(&self) -> Result<types::Monitor> {
      Ok(monitor("DP-1"))
    }
    fn list_windows(&self) -> Result<Vec<types::Window>> {
      Ok(self.windows.clone())
    }
    fn active_window(&self) -> Result<Option<types::Window>> {
      Ok(self.windows.first().cloned())
    }
    fn focus_workspace(&self, workspace: &str) -> Result<()> {
      self
        .calls
        .borrow_mut()
        .push(format!("workspace {workspace}"));
      Ok(())
    }
    fn focus_window(&self, address: &str) -> Result<()> {
      self.calls.borrow_mut().push(format!("window {address}"));
      Ok(())
    }
    fn close_window(&self, address: &str) -> Result<()> {
      self.calls.borrow_mut().push(format!("close {address}"));
      Ok(())
    }
    fn cursor_position(&self) -> Result<(i32, i32)> {
      Ok((0, 0))
    }
    fn keyboard_layout(&self) -> Result<Option<String>> {
      Ok(None)
    }
    fn set_dpms(&self, on: bool) -> Result<()> {
      self.calls.borrow_mut().push(format!("dpms {on}"));
      Ok(())
    }
    fn configure_monitor(&self, name: &str, change: types::MonitorChange) -> Result<()> {
      self
        .calls
        .borrow_mut()
        .push(format!("monitor {name} {change:?}"));
      Ok(())
    }
  }

  /// Keeps `corona_config::update` inside a temp dir
  pub fn temp_config() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    unsafe {
      std::env::set_var("XDG_CONFIG_HOME", dir.path().join("config"));
      std::env::set_var("XDG_STATE_HOME", dir.path().join("state"));
    }
    dir
  }

  /// gpui-kit, the default config and `fake` as the compositor
  pub fn setup(fake: FakeCompositor, cx: &mut TestAppContext) -> Rc<FakeCompositor> {
    let fake = Rc::new(fake);
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
      let compositor = Compositor::new(cx, fake.clone()).unwrap();
      cx.set_global(compositor);
    });
    fake
  }
}

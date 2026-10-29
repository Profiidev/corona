use anyhow::Result;
use corona_config::{ConfigProvider, observe_section};
use corona_notifications::NotificationsExt;
use corona_surface::bar::BarState;
use corona_utils::{Buses, error::ErrorLogExt};
use gpui_kit::App;
use rust_i18n::t;

rust_i18n::i18n!("../../assets/locales", fallback = "en");

pub mod commands;
mod control_center;
mod idle;
mod launcher;
mod locale_keys;
mod lock;
mod osds;
pub mod overlays;
pub mod settings;
mod widgets;

pub fn init(cx: &mut App) {
  init_ipc(cx);
  init_integrations(cx);
  register_variants(cx);
  launcher::init(cx);
  lock::init(cx);
  osds::init(cx);
  overlays::osk::init(cx);
  overlays::polkit::init(cx);
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
      corona_components::i18n::apply(language.as_deref());
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
  // binds first: a second instance exits before it starts anything
  corona_ipc::serve(cx);
  corona_surface::commands::register_commands(cx);
  overlays::register_commands(cx);
  commands::register_commands(cx);
}

fn init_integrations(cx: &mut App) {
  let config_error = corona_config::load(cx).err();
  corona_components::i18n::apply(cx.config().shell.language.as_deref());
  corona_compositor::init(cx).expect("Failed to init compositor");
  corona_components::assets::load(cx).expect("Failed to load assets");
  corona_surface::init(cx).expect("Failed to init ui");
  // the registries the integrations register into
  corona_settings::init(cx);
  corona_launcher::init(cx);
  cx.foreground_executor()
    .clone()
    .block_on(init_dbus(cx))
    .expect("Failed to init dbus");

  if let Some(e) = config_error {
    tracing::error!("Failed to load config, using the defaults: {e:#}");
    config_failed(format!("{e:#}"), cx);
  }
}

fn register_variants(cx: &mut App) {
  settings::init(cx);
  control_center::register_panels(cx);
  widgets::register_widgets(cx);
  corona_script::init(cx).expect("Failed to init plugins");
}

async fn init_dbus(cx: &mut App) -> Result<()> {
  let buses = Buses::connect().await?;
  let Buses { system, session } = &buses;

  // first: the integrations after it notify
  corona_notifications::init(cx, &buses).await?;
  corona_pipewire::init(cx, &buses)?;
  corona_capture::init(cx, &buses);
  corona_clipboard::init(cx, &buses);
  corona_sysinfo::init(cx, &buses);
  corona_bluez::init(cx, &buses).await?;
  corona_network_manager::init(cx, &buses).await?;
  corona_printing::init(cx, &buses).await?;
  corona_power::init(cx, &buses).await?;
  corona_udisks::init(cx, &buses).await?;
  corona_location::init(cx, system);
  corona_weather::init(cx, &buses);
  corona_brightness::init(cx, &buses)?;
  corona_auth::init(cx, system);
  corona_mpris::init(cx, &buses).await?;
  corona_tray::init(cx, &buses).await?;
  idle::init(cx, session);
  corona_script::init_dbus(cx, system, session);

  Ok(())
}

/// Shared test helpers
#[cfg(test)]
pub(crate) use corona_surface::testing as test_support;

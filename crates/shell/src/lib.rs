use anyhow::Result;
use corona_config::{ConfigProvider, observe_section};
use corona_js::ScriptExt;
use corona_notifications::NotificationsExt;
use corona_surface::bar::BarState;
use corona_utils::{Buses, error::ErrorLogExt};
use gpui_kit::App;
use rust_i18n::t;

rust_i18n::i18n!("../../assets/locales", fallback = "en");

mod bar;
mod dashboard;
pub mod ipc;
mod launcher;
mod locale_keys;
mod osd;
mod overlay;
mod panel;
mod settings;
mod settings_pages;
mod wiring;

/// What has to happen before any thread starts
pub fn pre_start() {
  corona_notifications::pre_start();
}

pub fn init(cx: &mut App) {
  init_ipc(cx);
  init_integrations(cx);
  register_ui(cx);
  register_variants(cx);
  // made once, from the settings and control center pages registered by now
  corona_launcher::commands::init(cx);
  corona_surface::overlay::wallpaper::init(cx);

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
}

fn init_integrations(cx: &mut App) {
  let config_error = corona_config::load(cx).err();
  corona_components::i18n::apply(cx.config().shell.language.as_deref());
  corona_compositor::init(cx).expect("Failed to init compositor");
  corona_components::assets::load(cx).expect("Failed to load assets");
  corona_surface::init(cx).expect("Failed to init ui");
  corona_surface::register_builtins(cx);
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

/// The integrations' bar widgets, panels, IPC commands and launcher rows; their state is set
fn register_ui(cx: &mut App) {
  ipc::register_all(cx);
  bar::register_all(cx);
  panel::register_all(cx);
  dashboard::register_all(cx);
  osd::register_all(cx);
  launcher::register_all(cx);
  settings_pages::register_all(cx);
  wiring::wire_all(cx);
}

fn register_variants(cx: &mut App) {
  corona_surface::control_center::warn_bad_ids(cx);
  corona_surface::dashboard::warn_bad_ids(cx);
  corona_desktop::register_script(cx.script_mut());
  corona_script::init(cx);
}

async fn init_dbus(cx: &mut App) -> Result<()> {
  let buses = Buses::connect().await?;
  let Buses { system, session } = &buses;

  // first: the integrations after it notify
  corona_notifications::init(cx, &buses).await?;
  if let Err(err) = corona_pipewire::init(cx, &buses) {
    tracing::error!("Failed to start pipewire: {err:#}");
  }
  corona_capture::init(cx, &buses);
  corona_clipboard::init(cx, &buses);
  corona_osk::init(cx, &buses);
  corona_windows::init(cx, &buses);
  corona_sysinfo::init(cx, &buses)?;
  corona_bluez::init(cx, &buses).await?;
  corona_network_manager::init(cx, &buses).await?;
  corona_printing::init(cx, &buses).await?;
  corona_power::init(cx, &buses).await?;
  corona_udisks::init(cx, &buses).await?;
  corona_location::init(cx, system);
  corona_weather::init(cx, &buses);
  corona_brightness::init(cx, &buses)?;
  corona_auth::init(cx, &buses);
  corona_mpris::init(cx, &buses).await?;
  corona_tray::init(cx, &buses).await?;
  corona_idle::init(cx, &buses);
  corona_script::init_dbus(cx, system, session);

  Ok(())
}

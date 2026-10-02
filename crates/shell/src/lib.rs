use std::time::Duration;

use anyhow::Result;
use corona_surface::{
  bar::{BarExt, BarState},
  panel::AppPanelExt,
};
use gpui_kit::App;

use crate::{
  control_center::ControlCenter,
  widgets::{ActiveWindow, ControlCenterButton, Workspaces},
};

mod control_center;
pub mod overlays;
mod widgets;

pub fn init(cx: &mut App) {
  init_ipc(cx);
  init_integrations(cx);
  register_variants(cx);

  BarState::spawn_bars(cx);
}

fn init_ipc(cx: &mut App) {
  let Some(mut server) = corona_ipc::IpcServer::new().expect("Failed to create IPC server") else {
    tracing::error!("Failed to create IPC server: socket already in use");
    std::process::exit(1);
  };

  corona_surface::commands::register_commands(&mut server);
  overlays::screenshot::commands::register_commands(&mut server);

  cx.spawn(async move |cx| server.run(cx).await).detach();
}

fn init_integrations(cx: &mut App) {
  corona_config::load(cx).expect("Failed to load config");
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
}

fn register_variants(cx: &mut App) {
  cx.panel().register::<ControlCenter>();

  cx.bar_mut()
    .register::<ControlCenterButton>()
    .register::<Workspaces>()
    .register::<ActiveWindow>();
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

  let session = zbus::connection::Builder::session()?
    .method_timeout(Duration::from_secs(5))
    .build()
    .await?;
  corona_mpris::init(cx, &session).await?;
  corona_notifications::init(cx, &session).await?;

  Ok(())
}

use std::time::Duration;

use anyhow::Result;
use gpui_kit::App;

mod control_center;
mod widgets;

pub fn init(cx: &mut App) {
  corona_config::load(cx).expect("Failed to load config");
  corona_script::init(cx).expect("Failed to init script manager");
  corona_compositor::init(cx).expect("Failed to init compositor");
  corona_pipewire::init(cx).expect("Failed to init pipewire");
  cx.foreground_executor()
    .clone()
    .block_on(init_dbus(cx))
    .expect("Failed to init dbus");
  corona_components::assets::load(cx).expect("Failed to load assets");
  corona_surface::init(cx, widgets::view).expect("Failed to init ui");
}

async fn init_dbus(cx: &mut App) -> Result<()> {
  let system = zbus::connection::Builder::system()?
    .method_timeout(Duration::from_secs(5))
    .build()
    .await?;

  corona_network_manager::init(cx, &system).await?;
  corona_bluez::init(cx, &system).await?;
  corona_power::init(cx, &system).await?;
  corona_brightness::init(cx, &system)?;

  let session = zbus::connection::Builder::session()?
    .method_timeout(Duration::from_secs(5))
    .build()
    .await?;
  corona_mpris::init(cx, &session).await?;
  corona_notifications::init(cx, &session).await?;

  Ok(())
}

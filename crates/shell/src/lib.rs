use anyhow::Result;
use gpui_kit::App;
use zbus::Connection;

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

async fn init_dbus(_cx: &mut App) -> Result<()> {
  let _system = Connection::system().await?;

  Ok(())
}

use anyhow::Result;
use gpui_kit::App;
use zbus::Connection;

pub async fn init(_cx: &mut App) -> Result<()> {
  let _system = Connection::system().await?;
  Ok(())
}

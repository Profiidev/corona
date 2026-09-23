use anyhow::Result;
use gpui_kit::App;
use zbus::Connection;

pub async fn init(cx: &mut App) -> Result<()> {
  let system = Connection::system().await?;
  Ok(())
}

use std::time::Duration;

use anyhow::Result;

/// The D-Bus connections every integration's `init` gets
pub struct Buses {
  pub system: zbus::Connection,
  pub session: zbus::Connection,
}

impl Buses {
  pub async fn connect() -> Result<Self> {
    Ok(Self {
      system: connect(zbus::connection::Builder::system()?).await?,
      session: connect(zbus::connection::Builder::session()?).await?,
    })
  }
}

async fn connect(builder: zbus::connection::Builder<'_>) -> Result<zbus::Connection> {
  Ok(
    builder
      .method_timeout(Duration::from_secs(5))
      .build()
      .await?,
  )
}

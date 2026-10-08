//! GeoClue2: the system location service, off unless enabled (NixOS: `services.geoclue2.enable`).

use anyhow::{Context, Result};
use futures_lite::StreamExt;
use geoclue2::{LocationProxy, ManagerProxy};
use zbus::Connection;

use crate::state::Location;

const DESKTOP_ID: &str = "corona";
const CITY_ACCURACY: u32 = 4;

pub(crate) async fn locate(
  conn: &Connection,
  timeout: impl Future<Output = ()>,
) -> Result<Location> {
  let manager = ManagerProxy::new(conn).await?;
  let client = manager.get_client().await?;
  client.set_desktop_id(DESKTOP_ID).await?;
  client.set_requested_accuracy_level(CITY_ACCURACY).await?;
  let mut updates = client.receive_location_updated().await?;
  client.start().await?;

  let fix = futures_lite::future::or(
    async { updates.next().await.context("GeoClue stopped") },
    async {
      timeout.await;
      anyhow::bail!("GeoClue found no location in time")
    },
  )
  .await;
  let fix = fix.and_then(|signal| Ok(signal.args()?.new.to_owned()));
  client.stop().await.ok();

  let location = LocationProxy::builder(conn).path(fix?)?.build().await?;
  Ok(Location {
    name: String::new(),
    latitude: location.latitude().await?,
    longitude: location.longitude().await?,
    query: None,
  })
}

use std::time::Duration;

use anyhow::{Context, Result};
use gpui_kit::{App, AppContext, Entity, Global};
use zbus::{Connection, names::OwnedBusName, zvariant::OwnedObjectPath};

use crate::{
  listener::{listener, subscribe},
  snapshot::player_proxy,
};

pub use crate::state::{LoopStatus, PlaybackStatus, Player};

mod listener;
mod snapshot;
mod state;

#[derive(Clone)]
pub struct Mpris {
  pub players: Entity<Vec<Player>>,
  pub active: Entity<Option<String>>,
  conn: Connection,
}

impl Global for Mpris {}

pub trait MprisExt {
  fn mpris(&self) -> &Mpris;
}

impl MprisExt for App {
  fn mpris(&self) -> &Mpris {
    self.global::<Mpris>()
  }
}

impl Mpris {
  pub fn list_players<'c>(&self, cx: &'c App) -> &'c [Player] {
    self.players.read(cx)
  }

  pub fn player<'c>(&self, name: &str, cx: &'c App) -> Option<&'c Player> {
    self.list_players(cx).iter().find(|p| p.name == name)
  }

  pub fn active_player<'c>(&self, cx: &'c App) -> Option<&'c Player> {
    self.player(self.active.read(cx).as_deref()?, cx)
  }

  pub fn set_active(&self, name: &str, cx: &mut App) {
    if self.player(name, cx).is_some() {
      self.active.write(cx, Some(name.to_string()));
    }
  }

  pub fn play_pause(&self, name: &str) -> impl Future<Output = Result<()>> + use<> {
    let (conn, name) = (self.conn.clone(), bus_name(name));
    async move { Ok(player_proxy(&conn, name?).await?.play_pause().await?) }
  }

  pub fn next(&self, name: &str) -> impl Future<Output = Result<()>> + use<> {
    let (conn, name) = (self.conn.clone(), bus_name(name));
    async move { Ok(player_proxy(&conn, name?).await?.next().await?) }
  }

  pub fn previous(&self, name: &str) -> impl Future<Output = Result<()>> + use<> {
    let (conn, name) = (self.conn.clone(), bus_name(name));
    async move { Ok(player_proxy(&conn, name?).await?.previous().await?) }
  }

  pub fn set_position(
    &self,
    name: &str,
    position: Duration,
    cx: &App,
  ) -> impl Future<Output = Result<()>> + use<> {
    let conn = self.conn.clone();
    let name = bus_name(name);
    // the player ignores the call when the track changed in between
    let track: Result<OwnedObjectPath> = name
      .as_ref()
      .map_err(|e| anyhow::anyhow!("{e}"))
      .and_then(|n| {
        self
          .player(n, cx)
          .and_then(|p| p.track_id.clone())
          .context("the player has no track to seek in")
      });
    async move {
      let player = player_proxy(&conn, name?).await?;
      let position = i64::try_from(position.as_micros())?;
      player
        .inner()
        .call_method("SetPosition", &(track?, position))
        .await?;
      Ok(())
    }
  }

  pub fn set_volume(&self, name: &str, volume: f64) -> impl Future<Output = Result<()>> + use<> {
    let (conn, name) = (self.conn.clone(), bus_name(name));
    async move {
      let volume = volume.clamp(0.0, 1.0);
      Ok(player_proxy(&conn, name?).await?.set_volume(volume).await?)
    }
  }

  pub fn set_shuffle(&self, name: &str, shuffle: bool) -> impl Future<Output = Result<()>> + use<> {
    let (conn, name) = (self.conn.clone(), bus_name(name));
    async move {
      Ok(
        player_proxy(&conn, name?)
          .await?
          .set_shuffle(shuffle)
          .await?,
      )
    }
  }

  pub fn set_loop_status(
    &self,
    name: &str,
    status: LoopStatus,
  ) -> impl Future<Output = Result<()>> + use<> {
    let (conn, name) = (self.conn.clone(), bus_name(name));
    async move {
      Ok(
        player_proxy(&conn, name?)
          .await?
          .set_loop_status(status)
          .await?,
      )
    }
  }
}

fn bus_name(name: &str) -> Result<OwnedBusName> {
  Ok(OwnedBusName::try_from(name.to_string())?)
}

pub async fn init(cx: &mut App, conn: &Connection) -> Result<()> {
  let state = Mpris {
    players: cx.new(|_| Vec::new()),
    active: cx.new(|_| None),
    conn: conn.clone(),
  };

  // subscribe before the first snapshot so no change can slip in between
  let changes = subscribe(conn).await?;
  listener(cx, conn.clone(), changes, state.clone());
  cx.set_global(state);

  Ok(())
}

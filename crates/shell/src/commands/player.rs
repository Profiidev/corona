//! The active player beyond play and pause: seek, shuffle, loop, which one is active

use std::time::Duration;

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use corona_ipc::{IpcCommand, IpcServer, Reply};
use corona_mpris::{LoopStatus, MprisExt};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

use crate::commands::radio::Switch;

pub fn register_commands(server: &mut IpcServer) {
  server
    .register::<Seek>()
    .register::<Shuffle>()
    .register::<Loop>()
    .register::<SetPlayer>()
    .register::<ListPlayers>();
}

fn active(cx: &App) -> Result<corona_mpris::Player> {
  cx.mpris()
    .active_player(cx)
    .cloned()
    .context("no active player")
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum SeekTo {
  /// Seconds from the start
  At(f64),
  /// Seconds from now, negative back
  By(f64),
}

pub struct Seek;

impl IpcCommand for Seek {
  const COMMAND: &'static str = "media:seek";

  type Payload = SeekTo;
  type Response = ();

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    bail!("media:seek answers asynchronously")
  }

  fn reply(to: Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    let player = match active(cx) {
      Ok(p) => p,
      Err(e) => return Box::pin(async move { Err(e) }),
    };
    let seconds = match to {
      SeekTo::At(s) => s,
      SeekTo::By(s) => player.position().as_secs_f64() + s,
    };
    let max = player.length.map_or(f64::MAX, |l| l.as_secs_f64());
    let position = Duration::from_secs_f64(seconds.clamp(0., max));
    Box::pin(cx.mpris().set_position(&player.name, position, cx))
  }
}

pub struct Shuffle;

impl IpcCommand for Shuffle {
  const COMMAND: &'static str = "media:shuffle";

  type Payload = Switch;
  type Response = bool;

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    bail!("media:shuffle answers asynchronously")
  }

  fn reply(switch: Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    let player = match active(cx) {
      Ok(p) => p,
      Err(e) => return Box::pin(async move { Err(e) }),
    };
    let current = player.shuffle.unwrap_or(false);
    let Some(on) = switch.target(current) else {
      return Box::pin(async move { Ok(current) });
    };
    let task = cx.mpris().set_shuffle(&player.name, on);
    Box::pin(async move {
      task.await?;
      Ok(on)
    })
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum LoopMode {
  None,
  Track,
  Playlist,
}

pub struct Loop;

impl IpcCommand for Loop {
  const COMMAND: &'static str = "media:loop";

  type Payload = LoopMode;
  type Response = ();

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    bail!("media:loop answers asynchronously")
  }

  fn reply(mode: Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    let player = match active(cx) {
      Ok(p) => p,
      Err(e) => return Box::pin(async move { Err(e) }),
    };
    let status = match mode {
      LoopMode::None => LoopStatus::None,
      LoopMode::Track => LoopStatus::Track,
      LoopMode::Playlist => LoopStatus::Playlist,
    };
    Box::pin(cx.mpris().set_loop_status(&player.name, status))
  }
}

/// Makes a player active, by bus name or identity like `Spotify`
pub struct SetPlayer;

impl IpcCommand for SetPlayer {
  const COMMAND: &'static str = "media:player";

  type Payload = String;
  type Response = ();

  fn handle(which: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let mpris = cx.mpris().clone();
    let name = mpris
      .list_players(cx)
      .iter()
      .find(|p| p.name == which || p.identity.eq_ignore_ascii_case(&which))
      .map(|p| p.name.clone())
      .with_context(|| format!("no player {which}"))?;
    mpris.set_active(&name, cx);
    Ok(())
  }
}

pub struct ListPlayers;

impl IpcCommand for ListPlayers {
  const COMMAND: &'static str = "media:players";

  type Payload = ();
  type Response = Vec<String>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Ok(
      cx.mpris()
        .list_players(cx)
        .iter()
        .map(|p| p.identity.clone())
        .collect(),
    )
  }
}

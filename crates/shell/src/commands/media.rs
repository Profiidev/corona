use anyhow::{Context, Result};
use clap::ValueEnum;
use corona_ipc::{IpcCommand, IpcServer};
use corona_mpris::{MprisExt, PlaybackStatus};
use corona_utils::error::ErrorLogExt;
use futures::FutureExt;
use gpui_kit::App;
use serde::{Deserialize, Serialize};

pub fn register_commands(server: &mut IpcServer) {
  server.register::<Media>();
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ValueEnum)]
pub enum Action {
  /// Go to the previous item
  Previous,
  /// Go to the next item
  Next,
  /// Toggle play/pause
  Toggle,
  /// Resume playback
  Play,
  /// Pause, no-op when nothing is playing
  Pause,
}

pub struct Media;

impl IpcCommand for Media {
  const COMMAND: &'static str = "media:action";

  type Payload = Action;
  type Response = ();

  fn handle(action: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let mpris = cx.mpris().clone();
    let active = mpris
      .active_player(cx)
      .map(|p| (p.name.clone(), p.status == PlaybackStatus::Playing));
    let active = || active.clone().context("no active player");
    let task = match action {
      Action::Previous => mpris.previous(&active()?.0).boxed_local(),
      Action::Next => mpris.next(&active()?.0).boxed_local(),
      Action::Toggle => mpris.play_pause(&active()?.0).boxed_local(),
      Action::Play => mpris.play(&active()?.0).boxed_local(),
      Action::Pause => match active()? {
        (name, true) => mpris.pause(&name).boxed_local(),
        (_, false) => return Ok(()),
      },
    };
    cx.spawn(async move |_| {
      let _ = task.await.log_err();
    })
    .detach();
    Ok(())
  }
}

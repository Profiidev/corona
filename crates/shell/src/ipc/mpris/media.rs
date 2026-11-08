use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use corona_ipc::{IpcCommand, IpcExt, Reply};
use corona_mpris::{MprisExt, PlaybackStatus};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

pub(crate) fn register(cx: &mut App) {
  cx.ipc_mut().register::<Media>();
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

/// The player `action` goes to, from the active player's name and whether it plays;
/// none when there is nothing to do
fn target(action: Action, active: Option<(String, bool)>) -> Result<Option<String>> {
  let (name, playing) = active.context("no active player")?;
  Ok((playing || !matches!(action, Action::Pause)).then_some(name))
}

pub struct Media;

impl IpcCommand for Media {
  const COMMAND: &'static str = "media:action";

  type Payload = Action;
  type Response = ();

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    bail!("media:action answers asynchronously")
  }

  fn reply(action: Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    match Self::dispatch(action, cx) {
      Ok(task) => task,
      Err(e) => Box::pin(async move { Err(e) }),
    }
  }
}

impl Media {
  /// Start `action` on the active player, the future answers once the player did
  pub(crate) fn dispatch(action: Action, cx: &mut App) -> Result<Reply<()>> {
    let mpris = cx.mpris().clone();
    let active = mpris
      .active_player(cx)
      .map(|p| (p.name.clone(), p.status == PlaybackStatus::Playing));
    let Some(name) = target(action, active)? else {
      return Ok(Box::pin(async { Ok(()) }));
    };
    Ok(match action {
      Action::Previous => Box::pin(mpris.previous(&name)),
      Action::Next => Box::pin(mpris.next(&name)),
      Action::Toggle => Box::pin(mpris.play_pause(&name)),
      Action::Play => Box::pin(mpris.play(&name)),
      Action::Pause => Box::pin(mpris.pause(&name)),
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn no_player_is_an_error() {
    for action in Action::value_variants() {
      assert!(target(*action, None).is_err(), "{action:?}");
    }
  }

  #[test]
  fn pause_only_when_playing() {
    let player = |playing| Some(("spotify".to_string(), playing));
    assert_eq!(target(Action::Pause, player(false)).unwrap(), None);
    assert_eq!(
      target(Action::Pause, player(true)).unwrap().as_deref(),
      Some("spotify")
    );
  }

  #[test]
  fn other_actions_ignore_playback_state() {
    for action in [Action::Previous, Action::Next, Action::Toggle, Action::Play] {
      for playing in [false, true] {
        let name = target(action, Some(("vlc".into(), playing))).unwrap();
        assert_eq!(name.as_deref(), Some("vlc"), "{action:?}");
      }
    }
  }
}

//! Play, pause, next and previous on the active player

use corona_ipc::IpcCommand;
use corona_launcher::{
  Action, Command, Icon, Meta, Outcome,
  kit::{Group, watch},
  simple,
};
use corona_mpris::{MprisExt, PlaybackStatus};
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;
use std::rc::Rc;

use crate::commands::media::{Action as MediaAction, Media};

/// "Title – Artist" of the active player
fn track(cx: &App) -> Option<SharedString> {
  let player = cx.mpris().active_player(cx)?;
  let title = player.title.clone()?;
  let artists = player.artists.join(", ");
  Some(match artists.is_empty() {
    true => title.into(),
    false => format!("{title} – {artists}").into(),
  })
}

fn command(
  id: &'static str,
  title: fn(&App) -> SharedString,
  icon: IconName,
  action: MediaAction,
) -> Rc<dyn Command> {
  simple(
    id,
    move |cx| Meta {
      title: title(cx),
      subtitle: track(cx),
      icon: Icon::Tile(icon, 330.),
      keywords: vec!["media".into(), "music".into(), "player".into()],
      ..Default::default()
    },
    move |cx| {
      vec![Action::sync(id, title(cx), move |_, _, cx| {
        Media::handle(action, cx)?;
        Ok(Outcome::close())
      })]
    },
  )
}

pub fn init(cx: &mut App) {
  Group::new(
    "media",
    vec![
      command(
        "media:toggle",
        |cx| {
          let playing = cx
            .mpris()
            .active_player(cx)
            .is_some_and(|p| p.status == PlaybackStatus::Playing);
          match playing {
            true => t!("app.launcher.media.pause").into(),
            false => t!("app.launcher.media.play").into(),
          }
        },
        IconName::Play,
        MediaAction::Toggle,
      ),
      command(
        "media:next",
        |_| t!("app.launcher.media.next").into(),
        IconName::SkipForward,
        MediaAction::Next,
      ),
      command(
        "media:previous",
        |_| t!("app.launcher.media.previous").into(),
        IconName::SkipBack,
        MediaAction::Previous,
      ),
    ],
  )
  .observe(|notify, cx| {
    let mpris = cx.mpris().clone();
    vec![
      watch(&mpris.players, &notify, cx),
      watch(&mpris.active, &notify, cx),
    ]
  })
  .register(cx);
}

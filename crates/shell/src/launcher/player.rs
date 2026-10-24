//! Now playing, seek, shuffle and loop of the active player

use std::{rc::Rc, time::Duration};

use corona_ipc::IpcCommand;
use corona_launcher::{
  Action, ArgSpec, CustomView, Icon, Meta, Outcome, View,
  kit::{switch_arg, switch_target},
  registry, simple,
};
use corona_mpris::{LoopStatus, MprisExt, PlaybackStatus, Player};
use corona_utils::{error::ErrorLogExt, ticker::TickerExt};
use gpui_kit::{
  App, AppContext, Context, InteractiveElement, IntoElement, ObjectFit, ParentElement, Render,
  SharedString, StatefulInteractiveElement, Styled, StyledImage, Subscription, Task, Window,
  assets::IconName,
  component::{ActiveTheme, Icon as KitIcon, Sizable},
  div, img,
  prelude::FluentBuilder,
  px, relative,
};
use rust_i18n::t;

use crate::commands::{
  player::{Loop, LoopMode, Seek, SeekTo, SetPlayer, Shuffle},
  radio::Switch,
};

fn time(d: Duration) -> String {
  let s = d.as_secs();
  format!("{}:{:02}", s / 60, s % 60)
}

/// Spawns an action's future, logging what fails
fn spawn(task: impl Future<Output = anyhow::Result<()>> + 'static, cx: &mut App) {
  cx.spawn(async move |_| {
    let _ = task.await.log_err();
  })
  .detach();
}

struct NowPlaying {
  _subscriptions: Vec<Subscription>,
  _tick: Task<()>,
}

impl NowPlaying {
  fn new(cx: &mut Context<Self>) -> Self {
    let mpris = cx.mpris().clone();
    Self {
      _subscriptions: vec![
        cx.observe(&mpris.players, |_, _, cx| cx.notify()),
        cx.observe(&mpris.active, |_, _, cx| cx.notify()),
      ],
      // the position moves on its own
      _tick: cx.ticker(Duration::from_millis(500), |_, cx| cx.notify()),
    }
  }

  fn control(
    &self,
    id: &'static str,
    icon: IconName,
    active: bool,
    cx: &mut Context<Self>,
    on_click: impl Fn(&Player, &mut App) + 'static,
  ) -> impl IntoElement {
    let theme = cx.theme();
    div()
      .id(id)
      .size(px(40.))
      .flex()
      .items_center()
      .justify_center()
      .rounded_full()
      .cursor_pointer()
      .when(active, |el| el.text_color(theme.colors.primary))
      .hover(|s| s.bg(theme.foreground.opacity(0.06)))
      .on_click(cx.listener(move |_, _, _, cx| {
        if let Some(player) = cx.mpris().active_player(cx).cloned() {
          on_click(&player, cx);
        }
      }))
      .child(KitIcon::new(icon).with_size(px(18.)))
  }
}

impl Render for NowPlaying {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme().clone();
    let mpris = cx.mpris().clone();
    let Some(player) = mpris.active_player(cx).cloned() else {
      return div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_color(theme.colors.muted_foreground)
        .child(t!("app.launcher.player.nothing").to_string())
        .into_any_element();
    };
    let position = player.position();
    let length = player.length.unwrap_or_default();
    let progress = match length.is_zero() {
      true => 0.,
      false => (position.as_secs_f32() / length.as_secs_f32()).clamp(0., 1.),
    };
    let playing = player.status == PlaybackStatus::Playing;
    let shuffle = player.shuffle.unwrap_or(false);
    let looping = player.loop_status.is_some_and(|l| l != LoopStatus::None);
    let players = mpris.list_players(cx).to_vec();
    let cover = match player.art_source() {
      Some(source) => img(source)
        .size(px(220.))
        .rounded(px(14.))
        .object_fit(ObjectFit::Cover)
        .into_any_element(),
      None => div()
        .size(px(220.))
        .rounded(px(14.))
        .bg(theme.foreground.opacity(0.06))
        .flex()
        .items_center()
        .justify_center()
        .child(KitIcon::new(IconName::Music).with_size(px(64.)))
        .into_any_element(),
    };
    div()
      .size_full()
      .p(px(24.))
      .flex()
      .gap(px(24.))
      .child(cover)
      .child(
        div()
          .flex_1()
          .min_w_0()
          .flex()
          .flex_col()
          .gap_2()
          .child(
            div()
              .text_size(px(20.))
              .font_weight(gpui_kit::FontWeight::SEMIBOLD)
              .text_ellipsis()
              .child(player.title.clone().unwrap_or_default()),
          )
          .child(
            div()
              .text_size(px(14.))
              .text_color(theme.colors.muted_foreground)
              .child(player.artists.join(", ")),
          )
          .when_some(player.album.clone(), |el, album| {
            el.child(
              div()
                .text_size(px(13.))
                .text_color(theme.colors.muted_foreground)
                .child(album),
            )
          })
          .child(div().flex_1())
          .child(
            div()
              .h(px(4.))
              .w_full()
              .rounded_full()
              .bg(theme.foreground.opacity(0.1))
              .child(
                div()
                  .h_full()
                  .w(relative(progress))
                  .rounded_full()
                  .bg(theme.colors.primary),
              ),
          )
          .child(
            div()
              .flex()
              .justify_between()
              .text_size(px(12.))
              .text_color(theme.colors.muted_foreground)
              .child(time(position))
              .child(time(length)),
          )
          .child(
            div()
              .flex()
              .items_center()
              .justify_center()
              .gap_2()
              .child(self.control(
                "np-shuffle",
                IconName::Shuffle,
                shuffle,
                cx,
                move |p, cx| {
                  let task = cx.mpris().set_shuffle(&p.name, !shuffle);
                  spawn(task, cx);
                },
              ))
              .child(
                self.control("np-prev", IconName::SkipBack, false, cx, |p, cx| {
                  let task = cx.mpris().previous(&p.name);
                  spawn(task, cx);
                }),
              )
              .child(self.control(
                "np-play",
                if playing {
                  IconName::Pause
                } else {
                  IconName::Play
                },
                false,
                cx,
                |p, cx| {
                  let task = cx.mpris().play_pause(&p.name);
                  spawn(task, cx);
                },
              ))
              .child(
                self.control("np-next", IconName::SkipForward, false, cx, |p, cx| {
                  let task = cx.mpris().next(&p.name);
                  spawn(task, cx);
                }),
              )
              .child(
                self.control("np-loop", IconName::Repeat, looping, cx, move |p, cx| {
                  let status = match looping {
                    true => LoopStatus::None,
                    false => LoopStatus::Playlist,
                  };
                  let task = cx.mpris().set_loop_status(&p.name, status);
                  spawn(task, cx);
                }),
              ),
          )
          .when(players.len() > 1, |el| {
            el.child(
              div()
                .flex()
                .gap_1()
                .justify_center()
                .children(players.into_iter().map(|p| {
                  let active = p.name == player.name;
                  let name = p.name.clone();
                  div()
                    .id(SharedString::from(format!("np-player-{}", p.name)))
                    .px_3()
                    .h(px(26.))
                    .flex()
                    .items_center()
                    .rounded(px(7.))
                    .text_size(px(12.))
                    .cursor_pointer()
                    .when(active, |el| el.bg(theme.foreground.opacity(0.1)))
                    .on_click(move |_, _, cx| {
                      let _ = SetPlayer::handle(name.clone(), cx).log_err();
                    })
                    .child(p.identity.clone())
                })),
            )
          }),
      )
      .into_any_element()
  }
}

fn now_playing() -> View {
  View::Custom(CustomView {
    title: t!("app.launcher.player.now_playing").into(),
    search: false,
    build: Rc::new(|_, cx| cx.new(NowPlaying::new).into()),
    on_query: None,
  })
}

pub fn init(cx: &mut App) {
  registry::register_all(
    [
      simple(
        "media:now_playing",
        |cx| Meta {
          title: t!("app.launcher.player.now_playing").into(),
          subtitle: cx
            .mpris()
            .active_player(cx)
            .and_then(|p| p.title.clone())
            .map(Into::into),
          icon: Icon::Tile(IconName::Music, 330.),
          keywords: vec!["music".into(), "player".into(), "media".into()],
          ..Default::default()
        },
        |_| {
          vec![Action::sync("open", t!("app.launcher.open"), |_, _, _| {
            Ok(Outcome::Push(now_playing()))
          })]
        },
      ),
      simple(
        "media:seek",
        |_| Meta {
          title: t!("app.launcher.player.seek").into(),
          icon: Icon::Tile(IconName::FastForward, 330.),
          keywords: vec!["media".into(), "position".into()],
          args: vec![ArgSpec::text(t!("app.launcher.player.seconds"))],
          ..Default::default()
        },
        |_| {
          vec![Action::spawn(
            "seek",
            t!("app.launcher.player.seek"),
            |args, cx| {
              let text = args.text(0).unwrap_or_default().to_string();
              let seconds: Result<f64, _> = text.parse();
              let to = match (seconds, text.starts_with(['+', '-'])) {
                (Ok(s), true) => Ok(SeekTo::By(s)),
                (Ok(s), false) => Ok(SeekTo::At(s)),
                (Err(_), _) => Err(anyhow::anyhow!("{}", t!("app.launcher.player.seconds"))),
              };
              let task = to.map(|to| Seek::reply(to, cx));
              async move {
                task?.await?;
                Ok(Outcome::close())
              }
            },
          )]
        },
      ),
      simple(
        "media:shuffle",
        |_| Meta {
          title: t!("app.launcher.player.shuffle").into(),
          icon: Icon::Tile(IconName::Shuffle, 330.),
          keywords: vec!["media".into()],
          args: vec![switch_arg()],
          ..Default::default()
        },
        |cx| {
          let current = cx
            .mpris()
            .active_player(cx)
            .and_then(|p| p.shuffle)
            .unwrap_or(false);
          vec![Action::spawn(
            "shuffle",
            t!("app.launcher.switch"),
            move |args, cx| {
              let on = switch_target(args, current);
              let task = Shuffle::reply(if on { Switch::On } else { Switch::Off }, cx);
              async move {
                task.await?;
                Ok(Outcome::close())
              }
            },
          )]
        },
      ),
      simple(
        "media:loop",
        |_| Meta {
          title: t!("app.launcher.player.loop").into(),
          icon: Icon::Tile(IconName::Repeat, 330.),
          keywords: vec!["media".into(), "repeat".into()],
          args: vec![ArgSpec::choice(
            t!("app.launcher.player.mode"),
            ["none", "track", "playlist"],
          )],
          ..Default::default()
        },
        |_| {
          vec![Action::spawn(
            "loop",
            t!("app.launcher.switch"),
            |args, cx| {
              let mode = match args.text(0) {
                Some("track") => LoopMode::Track,
                Some("playlist") => LoopMode::Playlist,
                _ => LoopMode::None,
              };
              let task = Loop::reply(mode, cx);
              async move {
                task.await?;
                Ok(Outcome::close())
              }
            },
          )]
        },
      ),
    ],
    cx,
  );
}

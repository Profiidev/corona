use std::{path::PathBuf, time::Duration};

use corona_components::async_listener::AsyncListenerExt;
use corona_mpris::{LoopStatus, MprisExt, PlaybackStatus, Player};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Context, ImageSource, IntoElement, ObjectFit, ParentElement, Styled, StyledImage,
  assets::IconName,
  base::{Disableable, StyledExt},
  component::{
    Icon, Sizable, Theme,
    button::{Button, ButtonVariant, ButtonVariants},
    slider::Slider,
  },
  div, img,
  prelude::FluentBuilder,
  px,
};

use crate::control_center::media::MediaPanel;

fn time(duration: Duration) -> String {
  let seconds = duration.as_secs();
  format!("{}:{:02}", seconds / 60, seconds % 60)
}

pub fn art_source(player: &Player) -> Option<ImageSource> {
  let url = player.art_url.as_deref()?;
  if let Some(path) = url.strip_prefix("file://") {
    Some(PathBuf::from(path).into())
  } else if url.starts_with("https://") || url.starts_with("http://") {
    Some(url.into())
  } else {
    None
  }
}

fn art(theme: &Theme, player: &Player) -> impl IntoElement {
  let placeholder = || {
    div()
      .size_full()
      .flex()
      .items_center()
      .justify_center()
      .child(Icon::new(IconName::Music).large())
      .into_any_element()
  };
  let source = art_source(player);

  div()
    .size(px(96.))
    .flex_none()
    .rounded_xl()
    .overflow_hidden()
    .bg(theme.colors.background)
    .text_color(theme.colors.muted_foreground)
    .child(match source {
      Some(source) => img(source)
        .size_full()
        .rounded_xl()
        .object_fit(ObjectFit::Cover)
        .with_fallback(placeholder)
        .into_any_element(),
      None => placeholder(),
    })
}

impl MediaPanel {
  pub fn player(&self, theme: &Theme, cx: &Context<'_, Self>) -> impl IntoElement {
    let player = self.shown(cx);

    div()
      .flex()
      .flex_col()
      .w_full()
      .gap_2()
      .p_2()
      .rounded_xl()
      .bg(theme.colors.accent)
      .border_color(theme.border)
      .border_1()
      .when_none(&player, |d| {
        d.child(
          div()
            .flex()
            .justify_center()
            .p_2()
            .text_sm()
            .text_color(theme.colors.muted_foreground)
            .child("Nothing is playing"),
        )
      })
      .when_some(player, |d, player| {
        d.child(self.track(theme, player))
          .when_some(player.length, |d, length| {
            let position = self
              .seeking
              .map_or_else(|| player.position(), |fraction| length.mul_f32(fraction));
            d.child(self.progress(theme, position, length))
          })
          .child(self.controls(player, cx))
      })
  }

  fn track(&self, theme: &Theme, player: &Player) -> impl IntoElement {
    let artists = if player.artists.is_empty() {
      player.identity.clone()
    } else {
      player.artists.join(", ")
    };

    div()
      .flex()
      .gap_2()
      .items_center()
      .child(art(theme, player))
      .child(
        div()
          .flex()
          .flex_col()
          .flex_1()
          .min_w_0()
          .gap_0p5()
          .child(
            div()
              .font_bold()
              .truncate()
              .child(player.title.clone().unwrap_or("Unknown title".into())),
          )
          .child(
            div()
              .text_sm()
              .text_color(theme.colors.muted_foreground)
              .truncate()
              .child(artists),
          )
          .when_some(player.album.clone(), |d, album| {
            d.child(
              div()
                .text_xs()
                .text_color(theme.colors.muted_foreground)
                .truncate()
                .child(album),
            )
          }),
      )
  }

  fn progress(&self, theme: &Theme, position: Duration, length: Duration) -> impl IntoElement {
    div()
      .flex()
      .flex_col()
      .child(Slider::new(&self.progress).cursor_pointer())
      .child(
        div()
          .flex()
          .justify_between()
          .text_xs()
          .text_color(theme.colors.muted_foreground)
          .child(time(position))
          .child(time(length)),
      )
  }

  fn controls(&self, player: &Player, cx: &Context<'_, Self>) -> impl IntoElement {
    let playing = player.status == PlaybackStatus::Playing;
    let loop_status = player.loop_status;
    let shuffle = player.shuffle;
    let name = player.name.clone();

    let action = |id: &'static str, icon: IconName, active: bool| {
      Button::new(id)
        .icon(icon)
        .cursor_pointer()
        .with_variant(if active {
          ButtonVariant::Primary
        } else {
          ButtonVariant::Default
        })
    };
    let log = |_: &mut Self, result: anyhow::Result<()>, _: &mut Context<Self>| {
      result.log_err().ok();
    };

    div()
      .flex()
      .gap_2()
      .items_center()
      .justify_center()
      .child(
        action(
          "media-loop",
          if loop_status == Some(LoopStatus::Track) {
            IconName::Repeat1
          } else {
            IconName::Repeat
          },
          loop_status.is_some_and(|s| s != LoopStatus::None),
        )
        .tooltip("Repeat")
        .disabled(!player.can_control || loop_status.is_none())
        .on_click(cx.async_listener(
          {
            let name = name.clone();
            move |_, _, _, cx| {
              let next = match loop_status {
                Some(LoopStatus::None) | None => LoopStatus::Playlist,
                Some(LoopStatus::Playlist) => LoopStatus::Track,
                Some(LoopStatus::Track) => LoopStatus::None,
              };
              cx.mpris().set_loop_status(&name, next)
            }
          },
          log,
        )),
      )
      .child(
        action("media-previous", IconName::SkipBack, false)
          .tooltip("Previous")
          .disabled(!player.can_go_previous)
          .on_click(cx.async_listener(
            {
              let name = name.clone();
              move |_, _, _, cx| cx.mpris().previous(&name)
            },
            log,
          )),
      )
      .child(
        action(
          "media-play",
          if playing {
            IconName::Pause
          } else {
            IconName::Play
          },
          true,
        )
        .disabled(!if playing {
          player.can_pause
        } else {
          player.can_play
        })
        .on_click(cx.async_listener(
          {
            let name = name.clone();
            move |_, _, _, cx| cx.mpris().play_pause(&name)
          },
          log,
        )),
      )
      .child(
        action("media-next", IconName::SkipForward, false)
          .tooltip("Next")
          .disabled(!player.can_go_next)
          .on_click(cx.async_listener(
            {
              let name = name.clone();
              move |_, _, _, cx| cx.mpris().next(&name)
            },
            log,
          )),
      )
      .child(
        action("media-shuffle", IconName::Shuffle, shuffle == Some(true))
          .tooltip("Shuffle")
          .disabled(!player.can_control || shuffle.is_none())
          .on_click(cx.async_listener(
            move |_, _, _, cx| cx.mpris().set_shuffle(&name, shuffle != Some(true)),
            log,
          )),
      )
  }
}

use std::{env, time::SystemTime};

use corona_components::components::card::CardExt;
use corona_mpris::{MprisExt, PlaybackStatus};
use corona_sysinfo::SystemMonitorExt;
use gpui_kit::{
  App, Context, Div, InteractiveElement, IntoElement, ObjectFit, ParentElement, Render,
  SharedString, Stateful, StatefulInteractiveElement, Styled, StyledImage, Subscription, Window,
  assets::IconName,
  base::StyledExt,
  component::{
    ActiveTheme, Icon, Sizable,
    button::{Button, ButtonVariants},
  },
  div, img,
  prelude::FluentBuilder,
  px,
};

use corona_config::ConfigProvider;

use crate::control_center::{MediaPanel, SysinfoPanel};
use crate::{
  control_center::dashboard::{open, spawn_logged},
  overlays::wallpaper,
};
use corona_surface::{
  control_center::ControlCenterPage,
  dashboard::{DashboardCard, DashboardExt},
};
use rust_i18n::t;

const ART: f32 = 64.;

fn card(id: &'static str, cx: &App) -> Stateful<Div> {
  div().id(id).flex().p_2().gap_2().card(cx)
}

fn muted(text: impl Into<String>, cx: &App) -> Div {
  div()
    .text_sm()
    .truncate()
    .text_color(cx.theme().muted_foreground)
    .child(text.into())
}

fn uptime(booted: SystemTime) -> String {
  uptime_at(booted, SystemTime::now())
}

fn uptime_at(booted: SystemTime, now: SystemTime) -> String {
  let minutes = now.duration_since(booted).unwrap_or_default().as_secs() / 60;
  match (minutes / 1440, minutes / 60 % 24, minutes % 60) {
    (0, 0, m) => t!("app.dashboard.uptime.minutes", minutes = m).into(),
    (0, h, m) => t!("app.dashboard.uptime.hours", hours = h, minutes = m).into(),
    (d, h, _) => t!("app.dashboard.uptime.days", days = d, hours = h).into(),
  }
}

pub(super) fn profile(cx: &App) -> Stateful<Div> {
  let theme = cx.theme();
  let user = env::var("USER").unwrap_or_default();
  let info = cx.system_monitor().info(cx);
  let host = info.map_or_else(String::new, |i| format!("{user}@{}", i.hostname));
  let details = info
    .map(|i| format!("{} · {}", i.os, uptime(i.booted)))
    .unwrap_or_default();
  let initials: String = user.chars().take(2).collect();
  let avatar = cx.config().shell.avatar.as_deref().map(wallpaper::source);

  card("dashboard-profile", cx)
    .items_center()
    .child(
      div()
        .size(px(44.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_2()
        .border_color(theme.colors.primary)
        .font_semibold()
        .child(match avatar {
          Some(source) => img(source)
            .size_full()
            .rounded_full()
            .object_fit(ObjectFit::Cover)
            .with_fallback(move || initials.clone().into_any_element())
            .into_any_element(),
          None => initials.into_any_element(),
        }),
    )
    .child(
      div()
        .flex()
        .flex_col()
        .min_w_0()
        .child(div().font_semibold().truncate().child(host))
        .child(muted(details, cx)),
    )
    .cursor_pointer()
    .on_click(open(SysinfoPanel::ID))
}

fn media(cx: &App) -> Option<Stateful<Div>> {
  let theme = cx.theme();
  let player = cx.mpris().active_player(cx)?;
  let name = player.name.clone();
  let playing = player.status == PlaybackStatus::Playing;
  let art = player.art_source();
  let control = |id: &'static str, icon: IconName, enabled: bool, name: &str| {
    let name = name.to_string();
    Button::new(id)
      .icon(icon)
      .ghost()
      .small()
      .cursor_pointer()
      .when(!enabled, |b| b.opacity(0.5))
      .on_click(move |_, _, cx| {
        cx.stop_propagation();
        let mpris = cx.mpris();
        match icon {
          IconName::SkipBack => spawn_logged(cx, mpris.previous(&name)),
          IconName::SkipForward => spawn_logged(cx, mpris.next(&name)),
          _ => spawn_logged(cx, mpris.play_pause(&name)),
        }
      })
  };

  Some(
    card("dashboard-media", cx)
      .flex_1()
      .min_w_0()
      .items_center()
      .cursor_pointer()
      .on_click(open(MediaPanel::ID))
      .child(
        div()
          .size(px(ART))
          .flex_none()
          .rounded_lg()
          .overflow_hidden()
          .bg(theme.colors.background)
          .flex()
          .items_center()
          .justify_center()
          .child(match art {
            Some(source) => img(source)
              .size_full()
              .rounded_lg()
              .object_fit(ObjectFit::Cover)
              .into_any_element(),
            None => Icon::new(IconName::Music).into_any_element(),
          }),
      )
      .child(
        div()
          .flex()
          .flex_col()
          .min_w_0()
          .flex_1()
          .child(
            div().text_sm().font_semibold().truncate().child(
              player
                .title
                .clone()
                .unwrap_or_else(|| player.identity.clone()),
            ),
          )
          .child(muted(player.artists.join(", "), cx))
          .child(
            div()
              .flex()
              .child(control(
                "dashboard-previous",
                IconName::SkipBack,
                player.can_go_previous,
                &name,
              ))
              .child(control(
                "dashboard-play",
                if playing {
                  IconName::Pause
                } else {
                  IconName::Play
                },
                player.can_play || player.can_pause,
                &name,
              ))
              .child(control(
                "dashboard-next",
                IconName::SkipForward,
                player.can_go_next,
                &name,
              )),
          ),
      ),
  )
}

/// The active player, nothing without one
pub struct MediaCard {
  _subscriptions: [Subscription; 2],
}

impl DashboardCard for MediaCard {
  const ID: &'static str = "media";

  fn title() -> SharedString {
    t!("app.control_center.media").into()
  }

  fn init(_: &mut Window, cx: &mut Context<Self>) -> Self {
    let players = cx.mpris().players.clone();
    let active = cx.mpris().active.clone();
    Self {
      _subscriptions: [
        cx.observe(&players, |_, _, cx| cx.notify()),
        cx.observe(&active, |_, _, cx| cx.notify()),
      ],
    }
  }
}

impl Render for MediaCard {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    media(cx).map_or_else(|| div().into_any_element(), |d| d.into_any_element())
  }
}

pub(crate) fn register(cx: &mut App) {
  cx.dashboard_mut().card::<MediaCard>();
}

#[cfg(test)]
mod tests {
  use std::time::Duration;

  use super::*;

  fn after(minutes: u64) -> String {
    uptime_at(
      SystemTime::UNIX_EPOCH,
      SystemTime::UNIX_EPOCH + Duration::from_secs(minutes * 60 + 59),
    )
  }

  #[test]
  fn uptime() {
    assert_eq!(after(0), "up 0m");
    assert_eq!(after(59), "up 59m");
    assert_eq!(after(60), "up 1h 0m");
    assert_eq!(after(23 * 60 + 5), "up 23h 5m");
    // minutes are dropped once it runs for days
    assert_eq!(after(2 * 1440 + 3 * 60 + 7), "up 2d 3h");
  }

  #[test]
  fn uptime_future_boot() {
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(3600);
    assert_eq!(
      uptime_at(now + Duration::from_secs(600), now),
      uptime_at(now, now)
    );
  }
}

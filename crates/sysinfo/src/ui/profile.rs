//! The dashboard's header: the user, host, system and uptime

use std::{
  env,
  time::{Duration, SystemTime},
};

use corona_components::{assets::image_source, components::card::CardExt};
use corona_config::ConfigProvider;
use corona_surface::{
  control_center::{ControlCenter, ControlCenterPage},
  dashboard::DashboardCard,
};
use corona_utils::ticker::TickerExt;
use gpui_kit::{
  App, Context, Div, InteractiveElement, IntoElement, ObjectFit, ParentElement, Render,
  SharedString, Stateful, StatefulInteractiveElement, Styled, StyledImage, Subscription, Task,
  Window, base::StyledExt, component::ActiveTheme, div, img, px,
};
use rust_i18n::t;

use crate::{SystemMonitorExt, ui::page::SysinfoPanel};

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

fn profile(cx: &App) -> Stateful<Div> {
  let theme = cx.theme();
  let user = env::var("USER").unwrap_or_default();
  let info = cx.system_monitor().info(cx);
  let host = info.map_or_else(String::new, |i| format!("{user}@{}", i.hostname));
  let details = info
    .map(|i| format!("{} · {}", i.os, uptime(i.booted)))
    .unwrap_or_default();
  let initials: String = user.chars().take(2).collect();
  let avatar = cx.config().shell.avatar.as_deref().map(image_source);

  div()
    .id("dashboard-profile")
    .flex()
    .p_2()
    .gap_2()
    .card(cx)
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
    .on_click(|_, window, cx| ControlCenter::navigate_id(SysinfoPanel::ID, window, cx))
}

/// Shown above the dashboard's tiles
pub struct ProfileCard {
  _subscription: Subscription,
  /// The uptime counts minutes
  _ticker: Task<()>,
}

impl DashboardCard for ProfileCard {
  const ID: &'static str = "profile";

  fn title() -> SharedString {
    t!("app.settings.system.groups.profile").into()
  }

  fn init(_: &mut Window, cx: &mut Context<Self>) -> Self {
    let info = cx.system_monitor().info.clone();
    Self {
      _subscription: cx.observe(&info, |_, _, cx| cx.notify()),
      _ticker: cx.ticker(Duration::from_secs(60), |_, cx| cx.notify()),
    }
  }
}

impl Render for ProfileCard {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    profile(cx)
  }
}

#[cfg(test)]
mod tests {
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

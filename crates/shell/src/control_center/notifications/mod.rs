use std::borrow::Cow;
use std::time::{Duration, SystemTime};

use corona_components::components::card::CardExt;
use corona_notifications::{Notification, NotificationsExt};
use corona_utils::ticker::TickerExt;
use gpui_kit::{
  AnyElement, App, Context, Div, IntoElement, ParentElement, Render, Styled, Subscription, Task,
  Window,
  assets::IconName,
  base::Disableable,
  component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariant, ButtonVariants},
    scroll::ScrollableElement,
  },
  div,
  prelude::FluentBuilder,
};

use crate::control_center::{ControlCenterPanel, variants::ControlCenterType};
use rust_i18n::t;

mod notification;

pub use notification::ReplyInputs;

const TICK: Duration = Duration::from_secs(30);
const DAY: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Filter {
  All,
  Today,
  Earlier,
}

pub struct NotificationsPanel {
  filter: Filter,
  replies: ReplyInputs,
  _subscriptions: [Subscription; 3],
  _ticker: Task<()>,
}

impl ControlCenterPanel for NotificationsPanel {
  const TYPE: ControlCenterType = ControlCenterType::Notifications;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let notifications = cx.notifications().clone();

    let subscriptions = [
      cx.observe_in(&notifications.notifications, window, |_, _, window, cx| {
        cx.notifications().clone().mark_all_read(cx);
        window.refresh();
        cx.notify();
      }),
      cx.observe_in(&notifications.do_not_disturb, window, |_, _, window, cx| {
        window.refresh();
        cx.notify();
      }),
      cx.observe(&notifications.active, |_, _, cx| cx.notify()),
    ];

    notifications.mark_all_read(cx);

    let ticker = cx.ticker(TICK, |_, cx| cx.notify());

    Self {
      filter: Filter::All,
      replies: ReplyInputs::default(),
      _subscriptions: subscriptions,
      _ticker: ticker,
    }
  }

  fn buttons(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
    let notifications = cx.notifications();
    let dnd = notifications.do_not_disturb(cx);
    let empty = notifications.list(cx).is_empty();

    vec![
      Button::new("notifications-dnd")
        .icon(if dnd {
          IconName::BellOff
        } else {
          IconName::Moon
        })
        .tooltip(t!("app.notifications.dnd"))
        .cursor_pointer()
        .when(dnd, |b| b.primary())
        .on_click(move |_, _, cx| {
          cx.notifications().clone().set_do_not_disturb(!dnd, cx);
        })
        .into_any_element(),
      Button::new("notifications-clear")
        .icon(IconName::Trash)
        .tooltip(t!("app.notifications.clear_all"))
        .cursor_pointer()
        .with_variant(ButtonVariant::Danger)
        .disabled(empty)
        .on_click(|_, _, cx| cx.notifications().clone().clear_all(cx))
        .into_any_element(),
    ]
  }
}

fn card(cx: &App) -> Div {
  div().flex().flex_col().w_full().gap_2().p_2().card(cx)
}

fn today(notification: &Notification) -> bool {
  today_at(notification.time, SystemTime::now())
}

/// Within the last 24 hours; a clock that jumped back counts as today too
fn today_at(time: SystemTime, now: SystemTime) -> bool {
  now.duration_since(time).is_ok_and(|elapsed| elapsed < DAY) || time > now
}

impl NotificationsPanel {
  fn filters(&self, total: usize, cx: &Context<'_, Self>) -> impl IntoElement {
    let filters = [
      (
        Filter::All,
        "notifications-all",
        t!("app.notifications.all", count = total),
      ),
      (
        Filter::Today,
        "notifications-today",
        t!("app.notifications.today"),
      ),
      (
        Filter::Earlier,
        "notifications-earlier",
        t!("app.notifications.earlier"),
      ),
    ];
    div()
      .flex()
      .gap_1()
      .children(filters.into_iter().map(|(filter, id, label)| {
        Button::new(id)
          .label(label)
          .small()
          .cursor_pointer()
          .when(filter == self.filter, |b| b.primary())
          .on_click(cx.listener(move |this, _, _, cx| {
            this.filter = filter;
            cx.notify();
          }))
      }))
  }
}

impl Render for NotificationsPanel {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let all = cx.notifications().list(cx).to_vec();
    self.replies.sync(&all, window, cx);
    let theme = cx.theme();
    let notifications = cx.notifications();
    let shown: Vec<&Notification> = all
      .iter()
      .filter(|n| match self.filter {
        Filter::All => true,
        Filter::Today => today(n),
        Filter::Earlier => !today(n),
      })
      .collect();
    let muted = |text: Cow<'static, str>| {
      div()
        .text_xs()
        .text_color(theme.colors.muted_foreground)
        .child(text)
    };

    div()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .child(self.filters(all.len(), cx))
      .when(!notifications.active(cx), |d| {
        d.child(card(cx).child(muted(t!("app.notifications.other_daemon"))))
      })
      .when_else(
        shown.is_empty(),
        |d| {
          d.child(
            card(cx).child(
              div()
                .flex()
                .justify_center()
                .p_2()
                .child(muted(t!("app.notifications.empty"))),
            ),
          )
        },
        |d| {
          d.child(
            div()
              .flex()
              .flex_col()
              .flex_1()
              .min_h_0()
              .gap_2()
              .overflow_y_scrollbar()
              .children(
                shown
                  .iter()
                  .map(|n| Self::notification(cx, n, self.replies.get(n.id))),
              ),
          )
        },
      )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn today_at() {
    let now = SystemTime::UNIX_EPOCH + DAY * 10;
    let secs = Duration::from_secs;
    assert!(super::today_at(now, now));
    assert!(super::today_at(now - secs(3600), now));
    assert!(super::today_at(now - DAY + secs(1), now));
    assert!(!super::today_at(now - DAY, now));
    assert!(!super::today_at(now - DAY * 3, now));
    assert!(super::today_at(now + secs(600), now));
  }
}

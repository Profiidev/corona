use std::time::{Duration, SystemTime};

use corona_notifications::{Notification, NotificationsExt};
use gpui_kit::{
  AnyElement, Context, Div, IntoElement, ParentElement, Render, Styled, Subscription, Task, Window,
  assets::IconName,
  base::Disableable,
  component::{
    ActiveTheme, Sizable, Theme,
    button::{Button, ButtonVariant, ButtonVariants},
    scroll::ScrollableElement,
  },
  div,
  prelude::FluentBuilder,
};

use crate::control_center::{ControlCenterPanel, variants::ControlCenterType};

mod notification;

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

    let ticker = cx.spawn(async move |this, cx| {
      loop {
        cx.background_executor().timer(TICK).await;
        if this.update(cx, |_, cx| cx.notify()).is_err() {
          break;
        }
      }
    });

    Self {
      filter: Filter::All,
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
        .tooltip("Do not disturb")
        .cursor_pointer()
        .when(dnd, |b| b.primary())
        .on_click(move |_, _, cx| {
          cx.notifications().clone().set_do_not_disturb(!dnd, cx);
        })
        .into_any_element(),
      Button::new("notifications-clear")
        .icon(IconName::Trash)
        .tooltip("Clear all")
        .cursor_pointer()
        .with_variant(ButtonVariant::Danger)
        .disabled(empty)
        .on_click(|_, _, cx| cx.notifications().clone().clear_all(cx))
        .into_any_element(),
    ]
  }
}

fn card(theme: &Theme) -> Div {
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
}

fn today(notification: &Notification) -> bool {
  notification
    .time
    .elapsed()
    .is_ok_and(|elapsed| elapsed < DAY)
    || notification.time > SystemTime::now()
}

impl NotificationsPanel {
  fn filters(&self, total: usize, cx: &Context<'_, Self>) -> impl IntoElement {
    let filters = [
      (Filter::All, format!("All ({total})")),
      (Filter::Today, "Today".into()),
      (Filter::Earlier, "Earlier".into()),
    ];
    div()
      .flex()
      .gap_1()
      .children(filters.into_iter().map(|(filter, label)| {
        Button::new(label.clone())
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
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let notifications = cx.notifications();
    let all = notifications.list(cx);
    let shown: Vec<&Notification> = all
      .iter()
      .filter(|n| match self.filter {
        Filter::All => true,
        Filter::Today => today(n),
        Filter::Earlier => !today(n),
      })
      .collect();
    let muted = |text: &'static str| {
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
        d.child(card(theme).child(muted(
          "Another notification daemon is running, corona takes over when it exits",
        )))
      })
      .when_else(
        shown.is_empty(),
        |d| {
          d.child(
            card(theme).child(
              div()
                .flex()
                .justify_center()
                .p_2()
                .child(muted("No notifications")),
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
              .children(shown.iter().map(|n| self.notification(theme, n))),
          )
        },
      )
  }
}

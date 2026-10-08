use std::time::Duration;

use corona_components::components::card::CardExt;
use corona_notifications::{Notification, NotificationsExt, Urgency};
use gpui_kit::{
  App, Div, InteractiveElement, IntoElement, ParentElement, Stateful, StatefulInteractiveElement,
  Styled,
  assets::IconName,
  base::StyledExt,
  component::ActiveTheme,
  component::{Icon, Sizable, button::Button},
  div,
  prelude::FluentBuilder,
};

use crate::control_center::notifications::NotificationsPanel;
use rust_i18n::t;

fn icon(notification: &Notification) -> IconName {
  const ICONS: [(&[&str], IconName); 4] = [
    (&["shot", "screen", "camera"], IconName::Camera),
    (&["nix", "package", "update", "install"], IconName::Package),
    (
      &["element", "discord", "telegram", "signal", "chat", "mail"],
      IconName::MessageSquare,
    ),
    (&["battery", "power", "system"], IconName::BatteryFull),
  ];
  let name = format!(
    "{} {}",
    notification.app_name,
    notification.desktop_entry.as_deref().unwrap_or_default()
  )
  .to_lowercase();
  ICONS
    .iter()
    .find(|(words, _)| words.iter().any(|word| name.contains(word)))
    .map_or(IconName::Bell, |(_, icon)| *icon)
}

fn ago(elapsed: Duration) -> String {
  match elapsed.as_secs() {
    0..60 => t!("app.notifications.ago.now").into(),
    s @ 60..3600 => t!("app.notifications.ago.minutes", count = s / 60).into(),
    s @ 3600..86400 => t!("app.notifications.ago.hours", count = s / 3600).into(),
    s => t!("app.notifications.ago.days", count = s / 86400).into(),
  }
}

impl NotificationsPanel {
  pub fn notification(cx: &App, notification: &Notification) -> impl IntoElement {
    let id = notification.id;
    let clickable = notification.actions.iter().any(|a| a.key == "default");
    Self::notification_card(cx, notification).when(clickable, |d| {
      d.cursor_pointer().on_click(move |_, _, cx| {
        cx.notifications().clone().invoke_action(id, "default", cx);
      })
    })
  }

  pub fn notification_card(cx: &App, notification: &Notification) -> Stateful<Div> {
    let theme = cx.theme();
    let id = notification.id;
    let elapsed = notification.time.elapsed().unwrap_or_default();

    div()
      .id(("notification", id as usize))
      .flex()
      .w_full()
      .gap_2()
      .p_2()
      .items_start()
      .card(cx)
      .border_color(if notification.urgency == Urgency::Critical {
        theme.colors.danger
      } else {
        theme.border
      })
      .child(
        div()
          .flex_none()
          .p_2()
          .rounded_xl()
          .bg(theme.colors.background)
          .child(Icon::new(icon(notification)).small()),
      )
      .child(
        div()
          .flex()
          .flex_col()
          .flex_1()
          .min_w_0()
          .gap_0p5()
          .child(
            div()
              .flex()
              .gap_2()
              .items_center()
              .child(
                div()
                  .text_xs()
                  .font_bold()
                  .truncate()
                  .child(notification.app_name.clone()),
              )
              .child(
                div()
                  .text_xs()
                  .text_color(theme.colors.muted_foreground)
                  .child(ago(elapsed)),
              ),
          )
          .child(
            div()
              .text_sm()
              .font_bold()
              .child(notification.summary.clone()),
          )
          .when(!notification.body.is_empty(), |d| {
            d.child(
              div()
                .text_xs()
                .text_color(theme.colors.muted_foreground)
                .child(notification.body.clone()),
            )
          })
          .when(
            notification.actions.iter().any(|a| a.key != "default"),
            |d| {
              d.child(
                div().flex().gap_1().pt_1().children(
                  notification
                    .actions
                    .iter()
                    .filter(|a| a.key != "default")
                    .map(|action| {
                      let key = action.key.clone();
                      Button::new(format!("notification-{id}-{key}"))
                        .label(action.label.clone())
                        .small()
                        .cursor_pointer()
                        .on_click(move |_, _, cx| {
                          cx.stop_propagation();
                          cx.notifications().clone().invoke_action(id, &key, cx);
                        })
                    }),
                ),
              )
            },
          ),
      )
      .child(
        Button::new(format!("notification-{id}-dismiss"))
          .icon(IconName::X)
          .small()
          .tooltip(t!("app.notifications.dismiss"))
          .cursor_pointer()
          .on_click(move |_, _, cx| {
            cx.stop_propagation();
            cx.notifications().clone().dismiss(id, cx);
          }),
      )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn ago() {
    assert_eq!(super::ago(Duration::from_secs(5)), "now");
    assert_eq!(super::ago(Duration::from_secs(4 * 60)), "4m");
    assert_eq!(super::ago(Duration::from_secs(90 * 60)), "1h");
    assert_eq!(super::ago(Duration::from_secs(3 * 86400)), "3d");
  }
}

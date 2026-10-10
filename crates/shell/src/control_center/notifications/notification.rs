use std::{collections::HashMap, time::Duration};

use corona_components::components::{card::CardExt, window_icon::WindowIcon};
use corona_notifications::{Notification, NotificationImage, NotificationsExt, Urgency};
use gpui_kit::{
  AnyElement, App, AppContext, Context, Div, Entity, Focusable, InteractiveElement, IntoElement,
  ParentElement, Stateful, StatefulInteractiveElement, Styled, StyledImage, Subscription, Window,
  assets::IconName,
  base::StyledExt,
  component::ActiveTheme,
  component::{
    Icon, Sizable,
    button::Button,
    input::{Input, InputEvent, InputState},
    text::TextView,
  },
  div, img,
  prelude::FluentBuilder,
  px,
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

const REPLY: &str = "inline-reply";
const IMAGE_SIZE: f32 = 32.;

/// The app's picture or theme icon, else a bell guessed from its name
fn image(notification: &Notification) -> AnyElement {
  let fallback = icon(notification);
  let guessed = move || {
    div()
      .size(px(IMAGE_SIZE))
      .flex()
      .items_center()
      .justify_center()
      .child(Icon::new(fallback).small())
      .into_any_element()
  };
  match &notification.image {
    Some(NotificationImage::Path(path)) => img(path.clone())
      .size(px(IMAGE_SIZE))
      .rounded_lg()
      .with_fallback(guessed)
      .into_any_element(),
    Some(NotificationImage::Name(name)) => WindowIcon::new(
      name.clone(),
      ("notification-icon", notification.id as usize),
    )
    .size(IMAGE_SIZE as u16)
    .into_any_element(),
    None => guessed(),
  }
}

/// The body markup as html: pictures dropped, line breaks kept
fn body_html(body: &str) -> String {
  let mut out = String::with_capacity(body.len());
  let mut rest = body;
  while let Some(start) = rest.find('<') {
    out.push_str(&rest[..start]);
    rest = &rest[start..];
    // html5ever parses `<image>` as `<img>` too
    let tag = ["<img", "<image"].iter().any(|t| {
      rest
        .get(..t.len())
        .is_some_and(|r| r.eq_ignore_ascii_case(t))
    });
    match rest.find('>') {
      Some(end) if tag => rest = &rest[end + 1..],
      _ => {
        out.push('<');
        rest = &rest[1..];
      }
    }
  }
  out.push_str(rest);
  out.replace('\n', "<br>")
}

/// The inline-reply inputs of the shown notifications, by id
#[derive(Default)]
pub struct ReplyInputs(HashMap<u32, (Entity<InputState>, Subscription)>);

impl ReplyInputs {
  /// one input per shown notification with an `inline-reply` action, dropped with it
  pub fn sync<'a, T: 'static>(
    &mut self,
    shown: impl IntoIterator<Item = &'a Notification>,
    window: &mut Window,
    cx: &mut Context<T>,
  ) {
    let mut kept = HashMap::new();
    for n in shown {
      let Some(action) = n.actions.iter().find(|a| a.key == REPLY) else {
        continue;
      };
      let id = n.id;
      let entry = self.0.remove(&id).unwrap_or_else(|| {
        let placeholder = n.reply_placeholder.clone().unwrap_or(action.label.clone());
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let submit = cx.subscribe_in(
          &input,
          window,
          move |_, input, event: &InputEvent, window, cx| {
            let InputEvent::PressEnter { .. } = event else {
              return;
            };
            let text = input.read(cx).value();
            if text.trim().is_empty() {
              return;
            }
            cx.notifications().clone().reply(id, &text, cx);
            // a resident one stays for the next reply
            input.update(cx, |input, cx| input.set_value("", window, cx));
          },
        );
        (input, submit)
      });
      kept.insert(id, entry);
    }
    self.0 = kept;
  }

  pub fn get(&self, id: u32) -> Option<&Entity<InputState>> {
    self.0.get(&id).map(|(input, _)| input)
  }

  /// typing into it, in a window that has the keyboard
  pub fn typing(&self, id: u32, window: &Window, cx: &App) -> bool {
    window.is_window_active()
      && self
        .get(id)
        .is_some_and(|input| input.read(cx).focus_handle(cx).is_focused(window))
  }
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
  pub fn notification(
    cx: &App,
    notification: &Notification,
    reply: Option<&Entity<InputState>>,
  ) -> impl IntoElement {
    let id = notification.id;
    let clickable = notification.actions.iter().any(|a| a.key == "default");
    Self::notification_card(cx, notification, reply).when(clickable, |d| {
      d.cursor_pointer().on_click(move |_, _, cx| {
        cx.notifications().clone().invoke_action(id, "default", cx);
      })
    })
  }

  /// `reply`: the input for an `inline-reply` action, from [`ReplyInputs`]
  pub fn notification_card(
    cx: &App,
    notification: &Notification,
    reply: Option<&Entity<InputState>>,
  ) -> Stateful<Div> {
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
          .rounded_xl()
          .overflow_hidden()
          .bg(theme.colors.background)
          .child(image(notification)),
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
              TextView::html(
                ("notification-body", id as usize),
                body_html(&notification.body),
              )
              .selectable(false)
              .text_xs()
              .text_color(theme.colors.muted_foreground),
            )
          })
          .when_some(reply, |d, input| {
            d.child(
              div()
                .id(("notification-reply", id as usize))
                .pt_1()
                // typing is not a click on the card
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(Input::new(input).small()),
            )
          })
          .when(
            notification
              .actions
              .iter()
              .any(|a| a.key != "default" && a.key != REPLY),
            |d| {
              d.child(
                div().flex().flex_wrap().gap_1().pt_1().children(
                  notification
                    .actions
                    .iter()
                    .filter(|a| a.key != "default" && a.key != REPLY)
                    .map(|action| {
                      let key = action.key.clone();
                      Button::new(format!("notification-{id}-{key}"))
                        .label(action.label.clone())
                        .small()
                        .max_w_full()
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

  #[test]
  fn ago_boundaries() {
    let ago = |s| super::ago(Duration::from_secs(s));
    assert_eq!(ago(59), "now");
    assert_eq!(ago(60), "1m");
    assert_eq!(ago(3599), "59m");
    assert_eq!(ago(3600), "1h");
    assert_eq!(ago(86399), "23h");
    assert_eq!(ago(86400), "1d");
  }

  fn notification(app_name: &str, desktop_entry: Option<&str>) -> Notification {
    Notification {
      id: 1,
      app_name: app_name.into(),
      app_icon: String::new(),
      image: None,
      summary: String::new(),
      body: String::new(),
      actions: vec![],
      urgency: Urgency::Normal,
      desktop_entry: desktop_entry.map(Into::into),
      reply_placeholder: None,
      expire_timeout: -1,
      resident: false,
      time: std::time::SystemTime::UNIX_EPOCH,
      read: false,
    }
  }

  #[test]
  fn icon() {
    let icon = |app, entry| super::icon(&notification(app, entry));
    assert_eq!(icon("Flameshot", None), IconName::Camera);
    assert_eq!(
      icon("Thunderbird", Some("thunderbird-mail")),
      IconName::MessageSquare
    );
    assert_eq!(icon("DISCORD", None), IconName::MessageSquare);
    assert_eq!(icon("upower", Some("Battery")), IconName::BatteryFull);
    assert_eq!(icon("nix", None), IconName::Package);
    assert_eq!(icon("Firefox", None), IconName::Bell);
    assert_eq!(icon("", None), IconName::Bell);
  }

  #[test]
  fn body_html() {
    assert_eq!(
      super::body_html("<b>hi</b> <IMG src=\"/a.png\"/>there\n<a href=\"x\">l</a>"),
      "<b>hi</b> there<br><a href=\"x\">l</a>"
    );
    assert_eq!(super::body_html("a < b <img"), "a < b <img");
    assert_eq!(super::body_html("x<Image src=\"http://t/\">y"), "xy");
  }

  #[test]
  fn icon_first_row_wins() {
    // "screen" (camera) and "update" (package) both match
    assert_eq!(
      super::icon(&notification("screen update", None)),
      IconName::Camera
    );
  }
}

use corona_notifications::NotificationsExt;
use corona_surface::bar::{Button, Widget};
use gpui_kit::{Context, IntoElement, Render, Subscription, Window, assets::IconName};
use uuid::Uuid;

use crate::control_center::{NotificationsPanel, Standalone};

pub struct NotificationsButton {
  _subscription: Subscription,
}

impl Widget for NotificationsButton {
  const NAME: &'static str = "notifications";

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid) -> Self {
    let notifications = cx.notifications().notifications.clone();
    Self {
      _subscription: cx.observe(&notifications, |_, _, cx| cx.notify()),
    }
  }
}

impl Render for NotificationsButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let unread = cx.notifications().has_unread(cx);
    Button::<_, Standalone<NotificationsPanel>>::new(cx, "notifications-button", IconName::Bell)
      .dot(unread)
  }
}

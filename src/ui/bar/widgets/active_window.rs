use std::time::Duration;

use gpui_kit::{
  AppContext, Axis, Context, Empty, Entity, InteractiveElement, IntoElement, ParentElement, Render,
  StatefulInteractiveElement, Styled, Subscription, Window, component::ActiveTheme, div, px,
};
use uuid::Uuid;

use crate::{
  error::ErrorLogExt,
  integration::compositor::{CompositorExt, event::CompositorEvent, types},
  ui::{
    animation::size::SizeAnimation,
    bar::{style::BarStyle, widgets::Widget},
    components::{
      scrolling_text::{ScrollingText, ScrollingTextExt, ScrollingTextState},
      window_icon::WindowIcon,
    },
  },
};

const ICON_SIZE: u16 = 18;
const WIDTH_CHANGE: Duration = Duration::from_millis(400);

pub struct ActiveWindow {
  active: Option<types::Window>,
  scrolling: Entity<ScrollingTextState>,
  size: SizeAnimation,
  #[allow(dead_code)]
  subscription: Subscription,
}

impl Widget for ActiveWindow {
  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid) -> Self {
    let compositor = cx.compositor();
    let active = compositor.active_window().log_err().ok().flatten();

    let emitter = compositor.emitter().clone();
    let subscription = cx.subscribe(&emitter, move |this, _, e, cx| {
      if let CompositorEvent::ActiveWindow(window) = e {
        this.active = window.clone();
        if window.is_none() {
          this.size.reset();
          this.scrolling.reset_hover(cx);
        }
        cx.notify();
      }
    });

    let scrolling = cx.new(|_| ScrollingTextState::default());

    ActiveWindow {
      active,
      size: SizeAnimation::new(WIDTH_CHANGE),
      scrolling,
      subscription,
    }
  }
}

impl Render for ActiveWindow {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let Some(active_window) = self.active.clone() else {
      return Empty.into_any_element();
    };

    let theme = cx.theme();

    div()
      .id("active-window")
      .flex_bar(window, cx)
      .items_center()
      .justify_center()
      .px_2()
      .h(px(24.))
      .min_w(px(36.))
      .rounded_full()
      .bg(theme.tokens.button_hover)
      .on_hover(self.scrolling.on_hover())
      .child(WindowIcon::new(active_window.class).size(ICON_SIZE))
      .child({
        let title = ScrollingText::new(self.scrolling.clone()).content(active_window.title.clone());

        self.size.animate(
          "active-window-title",
          Axis::Horizontal,
          title.width(window),
          cx,
          title,
        )
      })
      .into_any_element()
  }
}

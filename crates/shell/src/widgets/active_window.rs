use std::time::Duration;

use corona_compositor::CompositorExt;
use corona_surface::bar::{BarStyle, Widget};
use gpui_kit::{
  AppContext, Axis, Context, Empty, Entity, InteractiveElement, IntoElement, ParentElement, Render,
  StatefulInteractiveElement, Styled, Subscription, Window, component::ActiveTheme, div, px,
};
use uuid::Uuid;

use corona_components::{
  animation::size::SizeAnimation,
  components::{
    scrolling_text::{ScrollingText, ScrollingTextExt, ScrollingTextState},
    window_icon::WindowIcon,
  },
};

const ICON_SIZE: u16 = 18;
const WIDTH_CHANGE: Duration = Duration::from_millis(400);

pub struct ActiveWindow {
  scrolling: Entity<ScrollingTextState>,
  size: SizeAnimation,
  _subscription: Subscription,
}

impl Widget for ActiveWindow {
  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid) -> Self {
    let active_window = cx.compositor().active_window.clone();
    let subscription = cx.observe(&active_window, |this, e, cx| {
      if e.read(cx).is_none() {
        this.size.reset();
        this.scrolling.reset_hover(cx);
      }

      cx.notify();
    });

    let scrolling = cx.new(|_| ScrollingTextState::default());

    ActiveWindow {
      size: SizeAnimation::new(WIDTH_CHANGE),
      scrolling,
      _subscription: subscription,
    }
  }
}

impl Render for ActiveWindow {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let Some(active_window) = cx.compositor().active_window(cx) else {
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
      .child(
        WindowIcon::new(active_window.class.clone(), active_window.address.clone()).size(ICON_SIZE),
      )
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

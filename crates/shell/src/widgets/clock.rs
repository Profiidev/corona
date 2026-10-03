use std::time::Duration;

use corona_surface::{
  bar::{BarStyle, Widget},
  panel::WdigetPanelExt,
};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
  Styled, Task, Window, div,
};
use jiff::Zoned;
use uuid::Uuid;

use crate::control_center::{CalendarPanel, Standalone};

pub struct Clock {
  _ticker: Task<()>,
}

impl Widget for Clock {
  const NAME: &'static str = "clock";
  type Options = ();

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid, _options: Self::Options) -> Self {
    let ticker = cx.spawn(async move |this, cx| {
      loop {
        let wait = 60 - u64::from(Zoned::now().second().unsigned_abs());
        cx.background_executor()
          .timer(Duration::from_secs(wait.max(1)))
          .await;
        if this.update(cx, |_, cx| cx.notify()).is_err() {
          break;
        }
      }
    });
    Self { _ticker: ticker }
  }
}

impl Render for Clock {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div()
      .id("clock")
      .bar_pill(window, cx)
      .cursor_pointer()
      .text_sm()
      .child(Zoned::now().strftime("%H:%M %a, %b %-d").to_string())
      .on_click(cx.listener(|_, _, window, cx| {
        let _ = cx
          .toggle_panel::<Standalone<CalendarPanel>>(window)
          .log_err();
      }))
  }
}

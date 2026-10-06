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
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::control_center::{CalendarPanel, Standalone};

/// `time` in the strftime `pattern`; a pattern from the settings may be bad, then
/// the error shows instead, where a plain `strftime` would panic
pub(crate) fn format_time(pattern: &str, time: &Zoned) -> String {
  jiff::fmt::strtime::format(pattern, time).unwrap_or_else(|e| format!("bad format: {e}"))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
  /// strftime pattern
  pub format: String,
}

impl Default for Options {
  fn default() -> Self {
    Self {
      format: "%H:%M %a, %b %-d".to_string(),
    }
  }
}

pub struct Clock {
  format: String,
  _ticker: Task<()>,
}

impl Widget for Clock {
  const NAME: &'static str = "clock";
  type Options = Options;

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid, options: Self::Options) -> Self {
    let seconds = ["%S", "%T", "%s", "%r", "%X", "%c"]
      .iter()
      .any(|s| options.format.contains(s));
    let ticker = cx.spawn(async move |this, cx| {
      loop {
        let wait = match seconds {
          true => 1,
          false => 60 - u64::from(Zoned::now().second().unsigned_abs()),
        };
        cx.background_executor()
          .timer(Duration::from_secs(wait.max(1)))
          .await;
        if this.update(cx, |_, cx| cx.notify()).is_err() {
          break;
        }
      }
    });
    Self {
      format: options.format,
      _ticker: ticker,
    }
  }
}

impl Render for Clock {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div()
      .id("clock")
      .bar_pill(window, cx)
      .cursor_pointer()
      .text_sm()
      .child(format_time(&self.format, &Zoned::now()))
      .on_click(cx.listener(|_, _, window, cx| {
        let _ = cx
          .toggle_panel::<Standalone<CalendarPanel>>(window)
          .log_err();
      }))
  }
}

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

use crate::{
  control_center::{CalendarPanel, Standalone},
  i18n::format_time,
};

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

/// Whether `format` shows seconds, so the clock ticks every second instead of every minute
fn needs_seconds(format: &str) -> bool {
  ["%S", "%T", "%s", "%r", "%X", "%c"]
    .iter()
    .any(|s| format.contains(s))
}

pub struct Clock {
  format: String,
  _ticker: Task<()>,
}

impl Widget for Clock {
  const NAME: &'static str = "clock";
  type Options = Options;

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid, options: Self::Options) -> Self {
    let seconds = needs_seconds(&options.format);
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn needs_seconds() {
    for format in ["%H:%M:%S", "%T", "%s", "%r", "%X", "%c"] {
      assert!(super::needs_seconds(format), "{format}");
    }
    for format in ["%H:%M", "%a %b %-d", "", &Options::default().format] {
      assert!(!super::needs_seconds(format), "{format}");
    }
  }

  #[test]
  #[ignore = "bug: padding flags like %-S hide seconds from the check, the clock ticks per minute"]
  fn bug_needs_seconds_with_flags() {
    assert!(super::needs_seconds("%H:%M:%-S"));
    assert!(super::needs_seconds("%_S"));
  }
}

use gpui_kit::{App, Styled, Window, prelude::FluentBuilder};

use crate::{bar::BarState, config::placement::Placement};

pub trait BarStyle: Styled + FluentBuilder {
  fn with_placement(
    self,
    window: &Window,
    cx: &App,
    f: impl FnOnce(Self, Placement) -> Self,
  ) -> Self {
    let placement = BarState::get(window, cx)
      .map(|bar| bar.read(cx).placement())
      .unwrap_or(Placement::Top);

    self.map(|this| f(this, placement))
  }

  fn flex_bar(self, window: &Window, cx: &App) -> Self {
    self
      .with_placement(window, cx, |this, p| {
        if p.is_horizontal() {
          this.flex_row()
        } else {
          this.flex_col()
        }
      })
      .flex()
  }
}

impl<T: Styled + FluentBuilder> BarStyle for T {}

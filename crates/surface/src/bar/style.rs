use corona_config::placement::Placement;
use gpui_kit::{App, Styled, Window, component::ActiveTheme, prelude::FluentBuilder, px};

use crate::bar::BarState;

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

  fn when_horizontal_else(
    self,
    window: &Window,
    cx: &App,
    h: impl FnOnce(Self) -> Self,
    v: impl FnOnce(Self) -> Self,
  ) -> Self {
    self.with_placement(window, cx, |this, p| {
      if p.is_horizontal() { h(this) } else { v(this) }
    })
  }

  fn flex_bar(self, window: &Window, cx: &App) -> Self {
    self
      .when_horizontal_else(window, cx, |this| this.flex_row(), |this| this.flex_col())
      .flex()
  }

  fn bar_pill(self, window: &Window, cx: &App) -> Self {
    self
      .flex_bar(window, cx)
      .items_center()
      .h(px(24.))
      .px_2()
      .rounded_full()
      .bg(cx.theme().tokens.button_hover)
  }
}

impl<T: Styled + FluentBuilder> BarStyle for T {}

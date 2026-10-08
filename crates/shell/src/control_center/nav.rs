use std::rc::Rc;

use corona_components::components::card::CardExt;
use gpui_kit::{
  App, IntoElement, ParentElement, RenderOnce, Styled, Window,
  component::{
    ActiveTheme,
    button::{Button, ButtonVariant, ButtonVariants},
  },
  div,
  prelude::FluentBuilder,
  px,
};

use crate::control_center::variants::ControlCenterType;

#[derive(IntoElement)]
pub struct ControlCenterNav {
  selected: ControlCenterType,
  #[allow(clippy::type_complexity)]
  on_click: Option<Rc<dyn Fn(ControlCenterType, &mut Window, &mut App) + 'static>>,
}

impl ControlCenterNav {
  pub const WIDTH: f32 = 50.0;

  pub fn new(selected: ControlCenterType) -> Self {
    ControlCenterNav {
      selected,
      on_click: None,
    }
  }

  pub fn on_click<F>(mut self, f: F) -> Self
  where
    F: Fn(ControlCenterType, &mut Window, &mut App) + 'static,
  {
    self.on_click = Some(Rc::new(f));
    self
  }
}

impl RenderOnce for ControlCenterNav {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let theme = cx.theme();
    let on_click = self.on_click;

    div()
      .w(px(Self::WIDTH))
      .h_full()
      .p_2()
      .flex()
      .flex_col()
      .gap_1()
      .card(cx)
      .bg(theme.tokens.accent)
      .children(ControlCenterType::iter().map(|v| {
        Button::new(v.as_str())
          .with_variant(if self.selected == v {
            ButtonVariant::Primary
          } else {
            ButtonVariant::Ghost
          })
          .tooltip(v.title())
          .cursor_pointer()
          .icon(v.icon())
          .when_some(on_click.clone(), |button, on_click| {
            button.on_click(move |_, window, cx| on_click(v, window, cx))
          })
      }))
  }
}

#[cfg(test)]
mod tests {
  use std::cell::RefCell;

  use gpui_kit::{
    self as gpui, AppContext as _, Context, Render, TestAppContext, WindowOptions,
    test::TestWindowExt,
  };

  use super::*;

  struct Host(Rc<RefCell<Vec<ControlCenterType>>>);

  impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
      let clicks = self.0.clone();
      ControlCenterNav::new(ControlCenterType::Audio)
        .on_click(move |page, _, _| clicks.borrow_mut().push(page))
    }
  }

  #[gpui::test]
  fn click_selects_page(cx: &mut TestAppContext) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(corona_config::Config::default());
    });
    let clicks = Rc::new(RefCell::new(vec![]));
    let (window, _) = cx
      .update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
          cx.new(|_| Host(clicks.clone()))
        })
      })
      .unwrap();
    cx.update_window(window, |_, window, cx| {
      window.render_frame(cx);
      for page in ControlCenterType::iter() {
        assert!(window.try_find(page.as_str()).is_some(), "{page:?}");
      }
      // the test platform never reports the pointer over a window, so no hitbox is hovered
      // and clicks do not land; the keyboard activates buttons instead
      for target in ["weather", "audio"] {
        for _ in 0..30 {
          if window.find(target).focused() == Some(true) {
            break;
          }
          window.focus_next(cx);
          window.render_frame(cx);
        }
        assert_eq!(window.find(target).focused(), Some(true), "{target}");
        window.press("space", cx);
      }
    })
    .unwrap();
    assert_eq!(
      *clicks.borrow(),
      [ControlCenterType::Weather, ControlCenterType::Audio]
    );
  }
}

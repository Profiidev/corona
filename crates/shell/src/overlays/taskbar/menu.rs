use corona_compositor::{CompositorExt, types};
use gpui_kit::{
  App, Context, FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Pixels,
  Render, Size, StatefulInteractiveElement, Styled, WeakEntity, Window, component::ActiveTheme,
  div,
};
use tracing::error;

use super::{Taskbar, focus_window};
use crate::widgets::popup::{self, ROW, SEPARATOR};
use rust_i18n::t;

pub fn size(windows: &[types::Window]) -> Size<Pixels> {
  popup::size(ROW * (windows.len() + 1) as f32 + SEPARATOR)
}

pub struct TaskbarMenu {
  windows: Vec<types::Window>,
  focus: FocusHandle,
}

impl TaskbarMenu {
  pub fn new(
    taskbar: WeakEntity<Taskbar>,
    windows: Vec<types::Window>,
    cx: &mut Context<Self>,
  ) -> Self {
    cx.on_release(move |_, cx| {
      let _ = taskbar.update(cx, |taskbar, cx| taskbar.menu_closed(cx));
    })
    .detach();
    Self {
      windows,
      focus: cx.focus_handle(),
    }
  }
}

impl Focusable for TaskbarMenu {
  fn focus_handle(&self, _: &App) -> FocusHandle {
    self.focus.clone()
  }
}

impl Render for TaskbarMenu {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let hover = cx.theme().tokens.button_hover;
    let close = if self.windows.len() == 1 {
      t!("app.taskbar.close_window")
    } else {
      t!("app.taskbar.close_windows", count = self.windows.len())
    };
    let addresses = self
      .windows
      .iter()
      .map(|w| w.address.clone())
      .collect::<Vec<_>>();

    popup::frame(self, cx)
      .children(self.windows.iter().enumerate().map(|(i, w)| {
        let address = w.address.clone();
        popup::row(format!("window-{i}"), cx)
          .cursor_pointer()
          .hover(|d| d.bg(hover))
          .child(div().flex_1().min_w_0().truncate().child(w.title.clone()))
          .on_click(move |_, window, cx| {
            focus_window(&address, cx);
            window.remove_window();
          })
      }))
      .child(popup::separator(cx))
      .child(
        popup::row("close", cx)
          .cursor_pointer()
          .hover(|d| d.bg(hover))
          .child(close)
          .on_click(move |_, window, cx| {
            for address in &addresses {
              if let Err(e) = cx.compositor().close_window(address) {
                error!("failed to close window: {e:#}");
              }
            }
            window.remove_window();
          }),
      )
  }
}

use gpui_kit::{
  Context, IntoElement, ParentElement, Render, Styled, Subscription, Window,
  base::FocusableExt,
  component::{
    Sizable,
    button::{Button, ButtonVariants},
  },
  div,
  prelude::FluentBuilder,
  px,
};

use crate::{
  bar::{style::BarStyle, widgets::Widget},
  compositor::{CompositorExt, event::CompositorEvent, types::Workspace},
  error::ErrorLogExt,
};

pub struct Workspaces {
  workspaces: Vec<Workspace>,
  #[allow(dead_code)]
  subscription: Subscription,
}

impl Widget for Workspaces {
  fn init(cx: &mut Context<'_, Self>) -> Self {
    let compositor = cx.compositor();
    let workspaces = compositor.list_workspaces().log_err().unwrap_or_default();
    let emitter = compositor.emitter().clone();

    let subscription = cx.subscribe(&emitter, |this, _, e, cx| {
      if let CompositorEvent::WorkspaceChanged(workspaces) = e {
        this.workspaces = workspaces.clone();
        cx.notify();
      }
    });

    Workspaces {
      workspaces,
      subscription,
    }
  }
}

impl Render for Workspaces {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div()
      .flex_bar(window, cx)
      .children(self.workspaces.iter().map(|ws| {
        Button::new(format!("workspace-{}", ws.id))
          .label(ws.id.to_string())
          .secondary()
          .rounded_full()
          .focus_ring(false)
          .with_size(px(24.))
          .h(px(24.))
          .min_w(px(24.))
          .cursor_pointer()
          .on_click({
            let id = ws.id;
            move |_, window, cx| {
              println!("Switching to workspace {}", id);
            }
          })
      }))
  }
}

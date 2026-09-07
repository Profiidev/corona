use gpui_kit::{
  Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
  Styled, Subscription, Window, component::ActiveTheme, div, px, relative,
};

use crate::{
  bar::{style::BarStyle, widgets::Widget},
  compositor::{CompositorExt, event::CompositorEvent, types::Workspace},
  error::ErrorLogExt,
};

pub struct Workspaces {
  workspaces: Vec<Workspace>,
  active: Option<u32>,
  #[allow(dead_code)]
  subscription: Subscription,
}

impl Widget for Workspaces {
  fn init(cx: &mut Context<'_, Self>) -> Self {
    let compositor = cx.compositor();

    let mut workspaces = compositor.list_workspaces().log_err().unwrap_or_default();
    workspaces.sort_by_key(|w| w.id);

    let active = compositor.active_workspace().log_err().map(|w| w.id).ok();
    let emitter = compositor.emitter().clone();

    let subscription = cx.subscribe(&emitter, |this, _, e, cx| match e {
      CompositorEvent::ActiveWorkspace(workspace) => {
        this.active = Some(workspace.id);
        cx.notify();
      }
      CompositorEvent::Workspace(workspaces) => {
        this.workspaces = workspaces.clone();
        this.workspaces.sort_by_key(|w| w.id);
        cx.notify();
      }
      _ => {}
    });

    Workspaces {
      workspaces,
      subscription,
      active,
    }
  }
}

impl Render for Workspaces {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();

    div()
      .flex_bar(window, cx)
      .gap_1()
      .children(self.workspaces.iter().map(|ws| {
        let border = if self.active == Some(ws.id) {
          theme.tokens.primary
        } else {
          theme.tokens.secondary
        };

        div()
          .relative()
          .child(
            div()
              .id(("workspace", ws.id))
              .flex()
              .items_center()
              .justify_center()
              .h(px(24.))
              .min_w(px(32.))
              .rounded_full()
              .border_2()
              .border_color(border)
              .bg(theme.tokens.button_hover)
              .hover(|this| this.bg(theme.tokens.secondary))
              .text_color(theme.tokens.secondary_foreground)
              .cursor_pointer()
              .on_click({
                let id = ws.id;
                move |_, _window, _cx| {
                  println!("Switching to workspace {}", id);
                }
              }),
          )
          .child(
            div()
              .absolute()
              .top(px(-2.))
              .left(px(-2.))
              .flex()
              .items_center()
              .justify_center()
              .h(px(14.))
              .min_w(px(14.))
              .px_0p5()
              .rounded_full()
              .bg(border)
              .text_size(px(10.))
              .line_height(relative(1.))
              .text_color(theme.tokens.primary_foreground)
              .child(ws.name.clone()),
          )
      }))
  }
}

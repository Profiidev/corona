use std::{collections::HashMap, time::Duration};

use gpui_kit::{
  Context, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
  Styled, Subscription, Window, component::ActiveTheme, div, img, prelude::FluentBuilder, px,
  relative,
};
use uuid::Uuid;

use crate::{
  ui::bar::{BarState, anim::SizeAnimation, style::BarStyle, widgets::Widget},
  compositor::{
    CompositorExt,
    event::CompositorEvent,
    types::{self, Workspace},
  },
  desktop_entry::icon_for_class_or_default,
  error::ErrorLogExt,
};

const ICON_SIZE: u16 = 18;
const ICON_GAP: f32 = 2.;
const WIDTH_CHANGE: Duration = Duration::from_millis(400);

pub struct Workspaces {
  windows: HashMap<u32, Vec<types::Window>>,
  workspaces: Vec<Workspace>,
  active_workspace: Option<u32>,
  active_window: Option<String>,
  /// Per workspace: eases the pill between sizes as windows come and go.
  pill_size: HashMap<u32, SizeAnimation>,
  #[allow(dead_code)]
  subscription: Subscription,
}

impl Widget for Workspaces {
  fn init(cx: &mut Context<'_, Self>, display_id: Uuid) -> Self {
    let compositor = cx.compositor();

    let mut workspaces = compositor.list_workspaces().log_err().unwrap_or_default();
    workspaces.retain(|w| w.display_id() == display_id);
    workspaces.sort_unstable_by_key(|w| w.id);

    let windows = compositor.list_windows().log_err().unwrap_or_default();
    let mut windows_by_workspace: HashMap<u32, Vec<types::Window>> = HashMap::new();
    for window in windows {
      if workspaces.iter().all(|w| w.id != window.workspace) {
        continue;
      }

      windows_by_workspace
        .entry(window.workspace)
        .or_default()
        .push(window);
    }
    for windows in windows_by_workspace.values_mut() {
      windows.sort_unstable_by(|a, b| a.x.cmp(&b.x).then_with(|| a.y.cmp(&b.y)));
    }

    let active_workspace = compositor.active_workspace().log_err().map(|w| w.id).ok();
    let active_window = compositor
      .active_window()
      .log_err()
      .ok()
      .flatten()
      .map(|w| w.address);

    let emitter = compositor.emitter().clone();
    let subscription = cx.subscribe(&emitter, move |this, _, e, cx| match e {
      CompositorEvent::ActiveWorkspace(workspace) => {
        this.active_workspace = Some(workspace.id);
        cx.notify();
      }
      CompositorEvent::Workspace(workspaces) => {
        this.workspaces = workspaces.clone();
        this.workspaces.retain(|w| w.display_id() == display_id);
        this.workspaces.sort_unstable_by_key(|w| w.id);
        cx.notify();
      }
      CompositorEvent::Window(windows) => {
        this.windows.clear();
        for window in windows.clone() {
          if this.workspaces.iter().all(|w| w.id != window.workspace) {
            continue;
          }

          this
            .windows
            .entry(window.workspace)
            .or_default()
            .push(window);
        }
        for windows in this.windows.values_mut() {
          windows.sort_unstable_by(|a, b| a.x.cmp(&b.x).then_with(|| a.y.cmp(&b.y)));
        }
        cx.notify();
      }
      CompositorEvent::ActiveWindow(window) => {
        this.active_window = window.as_ref().map(|w| w.address.clone());
        cx.notify();
      }
      _ => {}
    });

    Workspaces {
      windows: windows_by_workspace,
      workspaces,
      subscription,
      active_workspace,
      active_window,
      pill_size: HashMap::new(),
    }
  }
}

impl Render for Workspaces {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let axis = BarState::bar_axis(window, cx);
    let Self {
      windows,
      workspaces,
      active_workspace,
      active_window,
      pill_size,
      ..
    } = self;

    div()
      .flex_bar(window, cx)
      .gap_1()
      .children(workspaces.iter().map(|ws| {
        let border = if *active_workspace == Some(ws.id) {
          theme.tokens.primary
        } else {
          theme.tokens.secondary
        };

        div()
          .relative()
          .child(
            div()
              .id(("workspace", ws.id))
              .flex_bar(window, cx)
              .gap_0p5()
              .items_center()
              .justify_center()
              .when_horizontal_else(
                window,
                cx,
                |this| this.h(px(24.)).min_w(px(38.)).px_2(),
                |this| this.w(px(24.)).min_h(px(38.)).py_2(),
              )
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
              })
              .child({
                let icons = windows.get(&ws.id).map_or(vec![], |windows| {
                  windows
                    .iter()
                    .map(|w| {
                      let icon = icon_for_class_or_default(&w.class, ICON_SIZE);

                      div()
                        .flex()
                        .items_center()
                        .justify_center()
                        .relative()
                        .h(px(ICON_SIZE as f32))
                        .w(px(ICON_SIZE as f32))
                        .rounded_full()
                        .map(|this| match icon {
                          Some(path) => this.child(img(path).size_full()),
                          None => this
                            .text_size(px(10.))
                            .line_height(relative(1.))
                            .child(w.class.chars().next().unwrap_or('?').to_string()),
                        })
                        .when(active_window.as_ref() == Some(&w.address), |d| {
                          d.child(
                            div()
                              .absolute()
                              .bottom_0()
                              .rounded_full()
                              .bg(theme.tokens.primary)
                              .h(px(6.))
                              .w(px(6.)),
                          )
                        })
                    })
                    .collect::<Vec<_>>()
                });

                let target = match icons.len() as f32 {
                  0. => 0.,
                  count => count * ICON_SIZE as f32 + (count - 1.) * ICON_GAP,
                };

                pill_size
                  .entry(ws.id)
                  .or_insert_with(|| SizeAnimation::new(WIDTH_CHANGE))
                  .animate(
                    "workspace-icons",
                    axis,
                    target,
                    cx,
                    div().flex_none().overflow_hidden().child(
                      div()
                        .flex_bar(window, cx)
                        .flex_none()
                        .gap(px(ICON_GAP))
                        .items_center()
                        .children(icons),
                    ),
                  )
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

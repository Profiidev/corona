use std::{collections::HashMap, time::Duration};

use gpui_kit::{
  Context, Div, InteractiveElement, IntoElement, ParentElement, Render, StatefulInteractiveElement,
  Styled, Subscription, Window,
  component::{ActiveTheme, Theme, ThemeToken},
  div,
  prelude::FluentBuilder,
  px, relative,
};
use uuid::Uuid;

use crate::{
  error::ErrorLogExt,
  integration::compositor::{
    CompositorExt,
    event::CompositorEvent,
    types::{self, Workspace},
  },
  ui::{
    animation::size::SizeAnimation,
    bar::{BarState, style::BarStyle, widgets::Widget},
    components::window_icon::WindowIcon,
  },
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
              .gap(px(ICON_GAP))
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
                let icons = workspace_windows(windows, active_window, ws, theme);

                let target = match icons.len() as f32 {
                  0. => ICON_SIZE as f32,
                  count => count * ICON_SIZE as f32 + (count - 1.) * ICON_GAP,
                };

                pill_size
                  .entry(ws.id)
                  .or_insert_with(|| SizeAnimation::new(WIDTH_CHANGE).start(ICON_SIZE as f32))
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
          .child(workspace_badge(border, theme, ws))
      }))
  }
}

fn workspace_badge(border: ThemeToken, theme: &Theme, ws: &Workspace) -> Div {
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
    .child(ws.name.clone())
}

fn workspace_windows(
  windows: &HashMap<u32, Vec<types::Window>>,
  active_window: &Option<String>,
  ws: &Workspace,
  theme: &Theme,
) -> Vec<WindowIcon> {
  windows.get(&ws.id).map_or(vec![], |windows| {
    windows
      .iter()
      .map(|w| {
        WindowIcon::new(&w.class).size(ICON_SIZE).when(
          active_window.as_ref() == Some(&w.address),
          |d| {
            d.child(
              div()
                .absolute()
                .bottom_0()
                .rounded_full()
                .bg(theme.tokens.primary)
                .h(px(6.))
                .w(px(6.)),
            )
          },
        )
      })
      .collect::<Vec<_>>()
  })
}

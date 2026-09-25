use std::{cell::Cell, collections::HashMap, rc::Rc, time::Duration};

use corona_components::{animation::size::SizeAnimation, components::window_icon::WindowIcon};
use corona_compositor::{CompositorExt, types};
use corona_surface::{
  bar::{BarState, BarStyle, Widget},
  tooltip::TooltipExt,
};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  AnyWindowHandle, Bounds, Context, Div, InteractiveElement, IntoElement, MouseButton,
  ParentElement, Pixels, Render, StatefulInteractiveElement, Styled, Subscription, Task, Window,
  base::ElementExt,
  component::{ActiveTheme, Theme, ThemeToken},
  div,
  prelude::FluentBuilder,
  px, relative,
};
use uuid::Uuid;

use crate::widgets::workspaces::tooltip::WindowTitle;

const ICON_SIZE: u16 = 18;
const ICON_GAP: f32 = 2.;
const WIDTH_CHANGE: Duration = Duration::from_millis(400);
const TOOLTIP_DELAY: Duration = Duration::from_millis(200);

pub struct Workspaces {
  windows: HashMap<String, Vec<types::Window>>,
  workspaces: Vec<types::Workspace>,
  pill_size: HashMap<String, SizeAnimation>,
  icon_bounds: HashMap<String, Rc<Cell<Bounds<Pixels>>>>,
  current_tooltip: Option<(String, String, AnyWindowHandle)>,
  current_hover: Option<Task<()>>,
  _subscriptions: [Subscription; 4],
}

impl Widget for Workspaces {
  fn init(cx: &mut Context<'_, Self>, display_id: Uuid) -> Self {
    let compositor = cx.compositor();

    let windows = compositor.list_windows(cx);
    let mut workspaces = compositor.list_workspaces(cx).to_vec();
    workspaces.retain(|w| w.display_id() == display_id);

    let mut windows_by_workspace: HashMap<String, Vec<types::Window>> = HashMap::new();
    for window in windows {
      if workspaces.iter().all(|w| w.id != window.workspace) {
        continue;
      }

      windows_by_workspace
        .entry(window.workspace.clone())
        .or_default()
        .push(window.clone());
    }

    let workspace = compositor.workspaces.clone();
    let window = compositor.windows.clone();
    let active_workspace = compositor.active_workspace.clone();
    let active_window = compositor.active_window.clone();

    let workspace_subscription = cx.observe(&workspace, move |this, e, cx| {
      let mut workspaces: Vec<types::Workspace> = e.read(cx).to_vec();
      workspaces.retain(|w| w.display_id() == display_id);

      this
        .pill_size
        .retain(|id, _| workspaces.iter().any(|w| &w.id == id));
      this
        .windows
        .retain(|id, _| workspaces.iter().any(|w| &w.id == id));
      this.workspaces = workspaces;

      if let Some(tooltip) = &this.current_tooltip
        && this.workspaces.iter().all(|w| w.id != tooltip.0)
      {
        this.current_tooltip = None;
        this.current_hover = None;
        cx.hide_tooltip::<WindowTitle>();
      }

      cx.notify();
    });

    let window_subscription = cx.observe(&window, |this, e, cx| {
      this.windows.clear();

      for window in e.read(cx) {
        if this.workspaces.iter().all(|w| w.id != window.workspace) {
          continue;
        }

        this
          .windows
          .entry(window.workspace.clone())
          .or_default()
          .push(window.clone());
      }

      if let Some((workspace, address, handle)) = this.current_tooltip.clone() {
        let title = this
          .windows
          .get(&workspace)
          .and_then(|windows| windows.iter().find(|w| w.address == address))
          .map(|w| w.title.clone());
        let bounds = this.icon_bounds.get(&address).map(|b| b.get());

        match (title, bounds) {
          (Some(title), Some(bounds)) => {
            let _ = handle.update(cx, |_, window, cx| {
              cx.show_bar_tooltip(WindowTitle::new(title), bounds, window)
                .log_err()
                .ok();
            });
          }
          _ => {
            this.current_tooltip = None;
            this.current_hover = None;
            cx.hide_tooltip::<WindowTitle>();
          }
        }
      }

      cx.notify();
    });

    Workspaces {
      windows: windows_by_workspace,
      workspaces,
      _subscriptions: [
        workspace_subscription,
        window_subscription,
        cx.observe(&active_workspace, |_, _, cx| cx.notify()),
        cx.observe(&active_window, |_, _, cx| cx.notify()),
      ],
      pill_size: HashMap::new(),
      icon_bounds: HashMap::new(),
      current_tooltip: None,
      current_hover: None,
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
      pill_size,
      icon_bounds,
      ..
    } = self;
    let active_workspace = cx.compositor().active_workspace(cx);
    let active_window = cx.compositor().active_window(cx).map(|w| &w.address);

    div()
      .flex_bar(window, cx)
      .gap_1()
      .children(workspaces.iter().map(|ws| {
        let border = if active_workspace.id == ws.id {
          theme.tokens.primary
        } else {
          theme.tokens.secondary
        };

        div()
          .relative()
          .child(
            div()
              .id(format!("workspace {}", ws.id))
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
              .on_mouse_down(MouseButton::Left, {
                let id = ws.id.clone();
                move |_, _window, cx| {
                  let _ = cx.compositor().focus_workspace(&id).log_err();
                }
              })
              .child({
                let icons = workspace_windows(windows, active_window, icon_bounds, ws, theme, cx);

                let target = match icons.len() as f32 {
                  0. => ICON_SIZE as f32,
                  count => count * ICON_SIZE as f32 + (count - 1.) * ICON_GAP,
                };

                pill_size
                  .entry(ws.id.clone())
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

fn workspace_badge(border: ThemeToken, theme: &Theme, ws: &types::Workspace) -> Div {
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
  windows: &HashMap<String, Vec<types::Window>>,
  active_window: Option<&String>,
  icon_bounds: &mut HashMap<String, Rc<Cell<Bounds<Pixels>>>>,
  ws: &types::Workspace,
  theme: &Theme,
  cx: &Context<'_, Workspaces>,
) -> Vec<WindowIcon> {
  windows.get(&ws.id).map_or(vec![], |windows| {
    windows
      .iter()
      .map(|w| {
        let bounds = icon_bounds.entry(w.address.clone()).or_default().clone();

        WindowIcon::new(&w.class, w.address.clone())
          .size(ICON_SIZE)
          .when(active_window == Some(&w.address), |d| {
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
          .on_prepaint({
            let bounds = bounds.clone();
            move |b, _, _| bounds.set(b)
          })
          .on_hover(cx.listener({
            let title = w.title.clone();
            let id = ws.id.clone();
            let address = w.address.clone();
            move |this, hovered: &bool, window, cx| {
              if !*hovered {
                this.current_tooltip = None;
                this.current_hover = None;
                cx.hide_tooltip::<WindowTitle>();
                return;
              }

              this.current_hover = Some(cx.spawn_in(window, {
                let address = address.clone();
                let bounds = bounds.clone();
                let title = title.clone();
                let id = id.clone();
                async move |e, cx| {
                  cx.background_executor().timer(TOOLTIP_DELAY).await;
                  e.update_in(cx, |this, window, cx| {
                    if cx
                      .show_bar_tooltip(WindowTitle::new(title), bounds.get(), window)
                      .log_err()
                      .is_ok()
                    {
                      this.current_tooltip = Some((id, address, window.window_handle()));
                    }
                  })
                  .ok();
                }
              }));
            }
          }))
      })
      .collect::<Vec<_>>()
  })
}

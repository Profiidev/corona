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

use crate::widgets::tooltip::TextTooltip;

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
  _subscriptions: [Subscription; 5],
}

impl Widget for Workspaces {
  const NAME: &'static str = "workspaces";
  type Options = ();

  fn init(cx: &mut Context<'_, Self>, display_id: Uuid, _options: Self::Options) -> Self {
    let compositor = cx.compositor();

    let windows = compositor.list_windows(cx);
    let mut workspaces = compositor.list_workspaces(cx).to_vec();
    workspaces.retain(|w| w.display_id() == display_id);

    let windows_by_workspace = group_windows(windows, &workspaces);

    let workspace = compositor.workspaces.clone();
    let window = compositor.windows.clone();
    let active_workspace = compositor.active_workspace.clone();
    let active_window = compositor.active_window.clone();
    let urgent = compositor.urgent.clone();

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
        cx.hide_tooltip::<TextTooltip>();
      }

      cx.notify();
    });

    let window_subscription = cx.observe(&window, |this, e, cx| {
      let icons = |windows: &HashMap<String, Vec<types::Window>>| {
        let mut icons: Vec<_> = windows
          .iter()
          .flat_map(|(id, ws)| {
            ws.iter()
              .map(move |w| (id.clone(), w.address.clone(), w.class.clone()))
          })
          .collect();
        icons.sort();
        icons
      };
      let before = icons(&this.windows);
      this.windows = group_windows(e.read(cx), &this.workspaces);

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
              cx.show_bar_tooltip(TextTooltip::new(title), bounds, window)
                .log_err()
                .ok();
            });
          }
          _ => {
            this.current_tooltip = None;
            this.current_hover = None;
            cx.hide_tooltip::<TextTooltip>();
          }
        }
      }

      // title and position changes only reach the tooltip
      if icons(&this.windows) != before {
        cx.notify();
      }
    });

    Workspaces {
      windows: windows_by_workspace,
      workspaces,
      _subscriptions: [
        workspace_subscription,
        window_subscription,
        cx.observe(&active_workspace, |_, _, cx| cx.notify()),
        cx.observe(&active_window, |_, _, cx| cx.notify()),
        cx.observe(&urgent, |_, _, cx| cx.notify()),
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

                let target = icons_extent(icons.len());

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
          .child({
            let urgent = windows.get(&ws.id).is_some_and(|windows| {
              windows
                .iter()
                .any(|w| cx.compositor().is_urgent(&w.address, cx))
            });
            workspace_badge(border, urgent, theme, ws)
          })
      }))
  }
}

/// The windows on `workspaces` by workspace id, in their order; others are dropped
fn group_windows(
  windows: &[types::Window],
  workspaces: &[types::Workspace],
) -> HashMap<String, Vec<types::Window>> {
  let mut grouped: HashMap<String, Vec<types::Window>> = HashMap::new();
  for window in windows {
    if workspaces.iter().any(|w| w.id == window.workspace) {
      grouped
        .entry(window.workspace.clone())
        .or_default()
        .push(window.clone());
    }
  }
  grouped
}

/// The icon row's length; an empty workspace keeps one icon's room
fn icons_extent(count: usize) -> f32 {
  match count as f32 {
    0. => ICON_SIZE as f32,
    count => count * ICON_SIZE as f32 + (count - 1.) * ICON_GAP,
  }
}

fn workspace_badge(border: ThemeToken, urgent: bool, theme: &Theme, ws: &types::Workspace) -> Div {
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
    .text_size(px(10.))
    .line_height(relative(1.))
    .text_center()
    .whitespace_nowrap()
    .when_else(
      urgent,
      |d| d.bg(theme.danger).text_color(theme.danger_foreground),
      |d| d.bg(border).text_color(theme.tokens.primary_foreground),
    )
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
            let id = ws.id.clone();
            let address = w.address.clone();
            move |this, hovered: &bool, window, cx| {
              if !*hovered {
                this.current_tooltip = None;
                this.current_hover = None;
                cx.hide_tooltip::<TextTooltip>();
                return;
              }

              this.current_hover = Some(cx.spawn_in(window, {
                let address = address.clone();
                let bounds = bounds.clone();
                let id = id.clone();
                async move |e, cx| {
                  cx.background_executor().timer(TOOLTIP_DELAY).await;
                  e.update_in(cx, |this, window, cx| {
                    let Some(title) = this
                      .windows
                      .get(&id)
                      .and_then(|windows| windows.iter().find(|w| w.address == address))
                      .map(|w| w.title.clone())
                    else {
                      return;
                    };
                    if cx
                      .show_bar_tooltip(TextTooltip::new(title), bounds.get(), window)
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

#[cfg(test)]
mod tests {
  use anyhow::Result;
  use corona_compositor::{Compositor, CompositorImpl};
  use gpui_kit::{self as gpui, AppContext as _, TestAppContext};

  use super::*;

  fn workspace(id: &str, monitor: &str) -> types::Workspace {
    types::Workspace {
      id: id.into(),
      name: id.into(),
      monitor: monitor.into(),
      monitor_id: 0,
    }
  }

  fn window(address: &str, workspace: &str) -> types::Window {
    types::Window {
      address: address.into(),
      monitor: 0,
      workspace: workspace.into(),
      class: "class".into(),
      title: address.into(),
      x: 0,
      y: 0,
      width: 1,
      height: 1,
      floating: false,
      pinned: false,
      fullscreen: false,
      hidden: false,
      focus_history_id: 0,
    }
  }

  fn addresses(grouped: &HashMap<String, Vec<types::Window>>, id: &str) -> Vec<String> {
    grouped
      .get(id)
      .map_or(vec![], |ws| ws.iter().map(|w| w.address.clone()).collect())
  }

  #[test]
  fn group_windows() {
    let workspaces = [workspace("1", "DP-1"), workspace("2", "DP-1")];
    let windows = [
      window("a", "1"),
      window("b", "3"),
      window("c", "2"),
      window("d", "1"),
    ];
    let grouped = super::group_windows(&windows, &workspaces);
    assert_eq!(addresses(&grouped, "1"), ["a", "d"]);
    assert_eq!(addresses(&grouped, "2"), ["c"]);
    // a workspace elsewhere drops its windows
    assert!(!grouped.contains_key("3"));
    assert!(super::group_windows(&windows, &[]).is_empty());
  }

  #[test]
  fn icons_extent() {
    assert_eq!(super::icons_extent(0), super::icons_extent(1));
    assert_eq!(super::icons_extent(1), ICON_SIZE as f32);
    assert_eq!(
      super::icons_extent(3),
      3. * ICON_SIZE as f32 + 2. * ICON_GAP
    );
  }

  struct Fake;

  impl CompositorImpl for Fake {
    fn list_workspaces(&self) -> Result<Vec<types::Workspace>> {
      Ok(vec![
        workspace("1", "DP-1"),
        workspace("2", "DP-1"),
        workspace("9", "HDMI-1"),
      ])
    }
    fn active_workspace(&self) -> Result<types::Workspace> {
      Ok(workspace("1", "DP-1"))
    }
    fn list_monitors(&self) -> Result<Vec<types::Monitor>> {
      Ok(vec![])
    }
    fn active_monitor(&self) -> Result<types::Monitor> {
      Ok(types::Monitor {
        id: 0,
        name: "DP-1".into(),
        width: 1,
        height: 1,
        refresh_rate: 60.,
        x: 0,
        y: 0,
        active_scratchpad: None,
        active_workspace: workspace("1", "DP-1"),
        scale: 1.,
        focused: true,
        disabled: false,
        mirror_of: "none".into(),
      })
    }
    fn list_windows(&self) -> Result<Vec<types::Window>> {
      Ok(vec![window("a", "1"), window("b", "9")])
    }
    fn active_window(&self) -> Result<Option<types::Window>> {
      Ok(None)
    }
    fn focus_workspace(&self, _: &str) -> Result<()> {
      Ok(())
    }
    fn focus_window(&self, _: &str) -> Result<()> {
      Ok(())
    }
    fn close_window(&self, _: &str) -> Result<()> {
      Ok(())
    }
    fn cursor_position(&self) -> Result<(i32, i32)> {
      Ok((0, 0))
    }
    fn keyboard_layout(&self) -> Result<Option<String>> {
      Ok(None)
    }
    fn set_dpms(&self, _: bool) -> Result<()> {
      Ok(())
    }
  }

  fn ids(widget: &Workspaces) -> Vec<&str> {
    widget.workspaces.iter().map(|w| w.id.as_str()).collect()
  }

  #[gpui::test]
  fn follows_the_compositor(cx: &mut TestAppContext) {
    let (windows, workspaces) = cx.update(|cx| {
      let compositor = Compositor::new(cx, std::rc::Rc::new(Fake)).unwrap();
      let entities = (compositor.windows.clone(), compositor.workspaces.clone());
      cx.set_global(compositor);
      entities
    });
    let display = workspace("1", "DP-1").display_id();
    let widget = cx.new(|cx| Workspaces::init(cx, display, ()));
    widget.read_with(cx, |w, _| {
      // only this display's workspaces and their windows
      assert_eq!(ids(w), ["1", "2"]);
      assert_eq!(addresses(&w.windows, "1"), ["a"]);
      assert!(!w.windows.contains_key("9"));
    });

    windows.update(cx, |windows, cx| {
      *windows = vec![window("c", "2"), window("d", "1"), window("e", "9")];
      cx.notify();
    });
    cx.run_until_parked();
    widget.read_with(cx, |w, _| {
      assert_eq!(addresses(&w.windows, "1"), ["d"]);
      assert_eq!(addresses(&w.windows, "2"), ["c"]);
      assert!(!w.windows.contains_key("9"));
    });

    workspaces.update(cx, |workspaces, cx| {
      workspaces.retain(|w| w.id != "1");
      cx.notify();
    });
    cx.run_until_parked();
    widget.read_with(cx, |w, _| {
      assert_eq!(ids(w), ["2"]);
      assert!(!w.windows.contains_key("1"));
      assert_eq!(addresses(&w.windows, "2"), ["c"]);
    });
  }
}

use std::collections::HashMap;

use corona_capture::LiveCapture;
use corona_components::components::window_icon::WindowIcon;
use corona_compositor::{CompositorExt, types};
use corona_config::ConfigProvider;
use gpui_kit::{
  App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement, KeyBinding,
  KeyDownEvent, ModifiersChangedEvent, MouseButton, ObjectFit, ParentElement, Render,
  StatefulInteractiveElement, Styled, StyledImage, Subscription, Window, black,
  component::ActiveTheme, div, img, prelude::FluentBuilder, px,
};
use tracing::error;

use crate::overlays::{
  OverlayState,
  switcher::{
    SwitcherState,
    commands::{Mode, Modifier, Options},
  },
  wallpaper,
};
use rust_i18n::t;

const FPS: u32 = 5;
const LABEL: f32 = 20.;
const TITLE: f32 = 24.;
const GAP: f32 = 16.;
const PADDING: f32 = 16.;
const BORDER: f32 = 2.;
const INSET: f32 = 3.;
const ICON_SIZE: u16 = 48;
const CORNER_ICON_SIZE: u16 = 20;
const CORNER_INSET: f32 = 4.;
const CONTEXT: &str = "Switcher";

gpui_kit::actions!(switcher, [Next, Prev]);

/// Tab steps as actions: Root binds tab to focus navigation, which would run before a
/// key-down listener, and this deeper context outranks it
pub(super) fn bind_keys(cx: &mut App) {
  for held in ["", "super-", "alt-", "ctrl-"] {
    cx.bind_keys([
      KeyBinding::new(&format!("{held}tab"), Next, Some(CONTEXT)),
      KeyBinding::new(&format!("{held}shift-tab"), Prev, Some(CONTEXT)),
    ]);
  }
}

pub(super) fn order(mode: Mode, monitor: Option<&str>, cx: &App) -> Vec<String> {
  let compositor = cx.compositor();
  order_of(
    mode,
    monitor,
    compositor.list_workspaces(cx),
    compositor.list_windows(cx),
  )
}

/// Workspace ids, or visible window addresses by workspace, then left to right, then top
/// to bottom; only on `monitor` when set
fn order_of(
  mode: Mode,
  monitor: Option<&str>,
  workspaces: &[types::Workspace],
  windows: &[types::Window],
) -> Vec<String> {
  let workspaces: Vec<&types::Workspace> = workspaces
    .iter()
    .filter(|ws| monitor.is_none_or(|m| ws.monitor == m))
    .collect();
  if mode == Mode::Workspace {
    return workspaces.iter().map(|ws| ws.id.clone()).collect();
  }
  let mut windows: Vec<(usize, &types::Window)> = windows
    .iter()
    .filter(|w| !w.hidden)
    .filter_map(|w| Some((workspaces.iter().position(|ws| ws.id == w.workspace)?, w)))
    .collect();
  windows.sort_by_key(|(workspace, w)| (*workspace, w.x, w.y));
  windows
    .into_iter()
    .map(|(_, w)| w.address.clone())
    .collect()
}

/// The index after `selected` in `n` items, wrapping both ways; `n` must not be 0
fn stepped(selected: usize, n: usize, reverse: bool) -> usize {
  match reverse {
    false => (selected + 1) % n,
    true => (selected + n - 1) % n,
  }
}

/// Where the selection lands in the pruned, non-empty `order`: on the same id when it is
/// still there, else on the same index clamped to the end
fn reselect(previous: Option<&str>, selected: usize, order: &[String]) -> usize {
  previous
    .and_then(|s| order.iter().position(|id| id == s))
    .unwrap_or(selected)
    .min(order.len() - 1)
}

pub struct Switcher {
  pub focus: FocusHandle,
  mode: Mode,
  modifier: Modifier,
  monitor: Option<String>,
  order: Vec<String>,
  selected: usize,
  previews: HashMap<String, Entity<LiveCapture>>,
  _subscriptions: [Subscription; 2],
}

impl Switcher {
  pub fn new(options: Options, monitor: Option<String>, cx: &mut Context<Self>) -> Self {
    let Options { mode, modifier, .. } = options;
    let order = order(mode, monitor.as_deref(), cx);
    let compositor = cx.compositor();
    let active = match mode {
      Mode::Window => compositor.active_window(cx).map(|w| w.address.clone()),
      Mode::Workspace => Some(compositor.active_workspace(cx).id.clone()),
    };
    let selected = active
      .and_then(|active| order.iter().position(|id| *id == active))
      .unwrap_or_default();
    let previews = self::order(Mode::Window, monitor.as_deref(), cx)
      .into_iter()
      .map(|address| {
        let live = cx.new(|cx| LiveCapture::window(&address, FPS, cx).rounded(cx.theme().radius));
        (address, live)
      })
      .collect();

    let windows = cx.compositor().windows.clone();
    let workspaces = cx.compositor().workspaces.clone();
    Self {
      focus: cx.focus_handle(),
      mode,
      modifier,
      monitor,
      order,
      selected,
      previews,
      _subscriptions: [
        cx.observe(&windows, |this, _, cx| this.prune(cx)),
        cx.observe(&workspaces, |this, _, cx| this.prune(cx)),
      ],
    }
  }

  fn prune(&mut self, cx: &mut Context<Self>) {
    let open = order(self.mode, self.monitor.as_deref(), cx);
    let windows = order(Mode::Window, self.monitor.as_deref(), cx);
    let selected = self.order.get(self.selected).cloned();
    self.order.retain(|id| open.contains(id));
    self.previews.retain(|address, _| windows.contains(address));
    if self.order.is_empty() {
      return SwitcherState::close(None, cx);
    }
    self.selected = reselect(selected.as_deref(), self.selected, &self.order);
    cx.notify();
  }

  pub fn step(&mut self, reverse: bool, cx: &mut Context<Self>) {
    let n = self.order.len();
    if n == 0 {
      return;
    }
    self.selected = stepped(self.selected, n, reverse);
    cx.notify();
  }

  fn commit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(id) = self.order.get(self.selected) {
      let focused = match self.mode {
        Mode::Window => cx.compositor().focus_window(id),
        Mode::Workspace => cx.compositor().focus_workspace(id),
      };
      if let Err(e) = focused {
        error!("failed to switch: {e:#}");
      }
    }
    SwitcherState::close(Some(window), cx);
  }

  fn select(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
    if let Some(index) = self.order.iter().position(|o| o == id) {
      self.selected = index;
      self.commit(window, cx);
    }
  }

  fn is_selected(&self, mode: Mode, id: &str) -> bool {
    self.mode == mode && self.order.get(self.selected).is_some_and(|s| s == id)
  }

  fn on_key(&mut self, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
    match e.keystroke.key.as_str() {
      "enter" => self.commit(window, cx),
      "escape" => SwitcherState::close(Some(window), cx),
      _ => {}
    }
  }

  fn on_modifiers(
    &mut self,
    e: &ModifiersChangedEvent,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    if !self.modifier.held(&e.modifiers) {
      self.commit(window, cx);
    }
  }

  fn workspace(
    &self,
    workspace: &types::Workspace,
    windows: Vec<&types::Window>,
    cx: &mut Context<Self>,
  ) -> impl IntoElement + use<> {
    let card_height = cx.config().window_switcher.card_height;
    let theme = cx.theme();
    let monitor = cx
      .compositor()
      .list_monitors(cx)
      .iter()
      .find(|m| m.name == workspace.monitor);
    let monitor = monitor.map_or((0, 0, 16, 9), |m| {
      let scale = m.scale.max(0.1);
      let size = |v: u32| (v as f32 / scale) as i32;
      (m.x, m.y, size(m.width), size(m.height))
    });
    let screen = (
      monitor.0,
      monitor.1,
      monitor.0 + monitor.2,
      monitor.1 + monitor.3,
    );
    let (left, top, right, bottom) = windows
      .iter()
      .map(|w| (w.x, w.y, w.x + w.width, w.y + w.height))
      .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
      .map(|b| {
        (
          b.0.max(screen.0),
          b.1.max(screen.1),
          b.2.min(screen.2),
          b.3.min(screen.3),
        )
      })
      .filter(|b| b.0 < b.2 && b.1 < b.3)
      .unwrap_or(screen);
    let inset = INSET + BORDER;
    let k = (card_height - inset * 2.) / (bottom - top).max(1) as f32;
    let width = (right - left) as f32 * k + inset * 2.;

    let window_mode = self.mode == Mode::Window;
    let tiles = windows.into_iter().map(|w| {
      let selected = self.is_selected(Mode::Window, &w.address);
      let preview = self.previews.get(&w.address).cloned();
      let address = w.address.clone();
      let (tile_w, tile_h) = (w.width as f32 * k, w.height as f32 * k);
      let short = tile_w.min(tile_h);
      let icon = ((short * 0.5) as u16 / 8 * 8).clamp(8, ICON_SIZE);
      let corner = short >= (CORNER_ICON_SIZE as f32 + CORNER_INSET) * 3.;
      div()
        .id(format!("switcher-window-{}", w.address))
        .absolute()
        .left(px(INSET + (w.x - left) as f32 * k))
        .top(px(INSET + (w.y - top) as f32 * k))
        .w(px(tile_w))
        .h(px(tile_h))
        .rounded(theme.radius)
        .overflow_hidden()
        .bg(theme.tokens.button_hover)
        .when(window_mode, |d| {
          d.cursor_pointer()
            .on_click(cx.listener(move |this, _, window, cx| this.select(&address, window, cx)))
        })
        .child(
          div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .child(WindowIcon::new(&w.class, w.address.clone()).size(icon)),
        )
        .when_some(preview, |d, live| {
          d.child(div().absolute().inset_0().child(live))
        })
        .when(corner, |d| {
          d.child(
            div()
              .absolute()
              .top(px(CORNER_INSET))
              .left(px(CORNER_INSET))
              .p(px(2.))
              .rounded(theme.radius)
              .bg(theme.tokens.background)
              .child(
                WindowIcon::new(&w.class, format!("{}-corner", w.address)).size(CORNER_ICON_SIZE),
              ),
          )
        })
        .when(selected, |d| {
          d.child(
            div()
              .absolute()
              .inset_0()
              .rounded(theme.radius)
              .border(px(BORDER))
              .border_color(theme.colors.primary),
          )
        })
    });

    div()
      .flex()
      .flex_col()
      .gap_1()
      .w(px(width))
      .child(
        div()
          .h(px(LABEL))
          .w_full()
          .truncate()
          .text_sm()
          .text_color(theme.muted_foreground)
          .child(workspace.name.clone()),
      )
      .child(
        div()
          .id(format!("switcher-workspace-{}", workspace.id))
          .relative()
          .w(px(width))
          .h(px(card_height))
          .overflow_hidden()
          .rounded(theme.radius)
          .bg(theme.tokens.button_hover.opacity(0.5))
          .border(px(BORDER))
          .border_color(match self.is_selected(Mode::Workspace, &workspace.id) {
            true => theme.colors.primary,
            false => gpui_kit::transparent_black(),
          })
          .when_some(wallpaper::configured(cx), |d, source| {
            d.child(
              img(source)
                .absolute()
                .inset_0()
                .size_full()
                .rounded(theme.radius)
                .object_fit(ObjectFit::Cover),
            )
          })
          .when(!window_mode, |d| {
            let id = workspace.id.clone();
            d.cursor_pointer()
              .on_click(cx.listener(move |this, _, window, cx| this.select(&id, window, cx)))
          })
          .children(tiles),
      )
  }
}

impl Render for Switcher {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let compositor = cx.compositor();
    let windows = compositor.list_windows(cx);
    let shown = |w: &&types::Window| self.previews.contains_key(&w.address);
    let workspaces: Vec<(types::Workspace, Vec<types::Window>)> = compositor
      .list_workspaces(cx)
      .iter()
      .filter_map(|ws| {
        let mut on: Vec<types::Window> = windows
          .iter()
          .filter(|w| w.workspace == ws.id)
          .filter(shown)
          .cloned()
          .collect();
        on.sort_by_key(|w| w.stacking());
        let listed = match self.mode {
          Mode::Window => !on.is_empty(),
          Mode::Workspace => self.order.contains(&ws.id),
        };
        listed.then(|| (ws.clone(), on))
      })
      .collect();
    let selected = self.order.get(self.selected);
    let title = match self.mode {
      Mode::Window => selected
        .and_then(|a| windows.iter().find(|w| &w.address == a))
        .map(|w| w.title.clone()),
      Mode::Workspace => selected
        .and_then(|id| workspaces.iter().find(|(ws, _)| &ws.id == id))
        .map(|(ws, _)| t!("app.switcher.workspace", name = ws.name).into()),
    }
    .unwrap_or_default();

    let cards: Vec<_> = workspaces
      .iter()
      .map(|(ws, on)| self.workspace(ws, on.iter().collect(), cx))
      .collect();
    let theme = cx.theme();
    let config = cx.config();

    div()
      .key_context(CONTEXT)
      .track_focus(&self.focus)
      .on_action(cx.listener(|this, _: &Next, _, cx| this.step(false, cx)))
      .on_action(cx.listener(|this, _: &Prev, _, cx| this.step(true, cx)))
      .on_key_down(cx.listener(Self::on_key))
      .on_modifiers_changed(cx.listener(Self::on_modifiers))
      .on_mouse_down(
        MouseButton::Left,
        cx.listener(|_, _, window, cx| SwitcherState::close(Some(window), cx)),
      )
      .size_full()
      .flex()
      .items_center()
      .justify_center()
      .bg(black().opacity(config.window_switcher.backdrop_opacity))
      .child(
        div()
          .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
          .max_w(gpui_kit::relative(0.9))
          .flex()
          .flex_col()
          .items_center()
          .gap(px(GAP))
          .p(px(PADDING))
          .bg(theme.tokens.background)
          .rounded(theme.radius * 2)
          .border(px(BORDER))
          .border_color(
            config
              .theme
              .popup_border_color(theme.tokens.button_hover.color),
          )
          .text_color(theme.foreground)
          .child(
            div()
              .flex()
              .flex_wrap()
              .justify_center()
              .gap(px(GAP))
              .children(cards),
          )
          .child(
            div().w_full().h(px(TITLE)).relative().child(
              div()
                .absolute()
                .inset_0()
                .truncate()
                .text_center()
                .child(title),
            ),
          ),
      )
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::{
    overlays::taskbar::tests::window,
    test_support::{FakeCompositor, setup, workspace},
  };
  use gpui_kit::{
    self as gpui, AnyWindowHandle, TestAppContext, WindowHandle, WindowOptions, base::Root,
    test::TestWindowExt,
  };
  use std::rc::Rc;

  fn at(address: &str, workspace: &str, x: i32, y: i32) -> types::Window {
    types::Window {
      workspace: workspace.into(),
      x,
      y,
      ..window(address, "app", 100, 100)
    }
  }

  fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
  }

  #[test]
  fn workspace_order_filters_by_monitor() {
    let workspaces = [
      workspace("1", "DP-1"),
      workspace("2", "HDMI-A-1"),
      workspace("3", "DP-1"),
    ];
    assert_eq!(
      order_of(Mode::Workspace, None, &workspaces, &[]),
      ["1", "2", "3"]
    );
    assert_eq!(
      order_of(Mode::Workspace, Some("DP-1"), &workspaces, &[]),
      ["1", "3"]
    );
    assert!(order_of(Mode::Workspace, Some("eDP-1"), &workspaces, &[]).is_empty());
  }

  #[test]
  fn window_order_by_workspace_then_position() {
    let workspaces = [workspace("2", "DP-1"), workspace("1", "DP-1")];
    let windows = [
      at("a", "1", 0, 0),
      at("b", "2", 500, 0),
      at("c", "2", 0, 500),
      at("d", "2", 0, 0),
      at("e", "unknown", 0, 0),
    ];
    // workspaces keep the compositor's order, windows outside them are left out
    assert_eq!(
      order_of(Mode::Window, None, &workspaces, &windows),
      ["d", "c", "b", "a"]
    );
  }

  #[test]
  fn window_order_skips_hidden_and_other_monitors() {
    let workspaces = [workspace("1", "DP-1"), workspace("2", "HDMI-A-1")];
    let mut hidden = at("hidden", "1", 0, 0);
    hidden.hidden = true;
    let windows = [hidden, at("here", "1", 10, 0), at("there", "2", 0, 0)];
    assert_eq!(
      order_of(Mode::Window, Some("DP-1"), &workspaces, &windows),
      ["here"]
    );
    assert_eq!(
      order_of(Mode::Window, None, &workspaces, &windows),
      ["here", "there"]
    );
    assert!(order_of(Mode::Window, None, &[], &windows).is_empty());
  }

  #[test]
  fn step_wraps_both_ways() {
    assert_eq!(stepped(0, 3, false), 1);
    assert_eq!(stepped(2, 3, false), 0);
    assert_eq!(stepped(0, 3, true), 2);
    assert_eq!(stepped(1, 3, true), 0);
    assert_eq!(stepped(0, 1, false), 0);
    assert_eq!(stepped(0, 1, true), 0);
  }

  #[test]
  fn reselect_follows_id_or_clamps() {
    let order = ids(&["a", "c"]);
    // "c" moved from index 2 to 1
    assert_eq!(reselect(Some("c"), 2, &order), 1);
    // "b" is gone, the index stays
    assert_eq!(reselect(Some("b"), 1, &order), 1);
    // "d" at the end is gone, the index is clamped
    assert_eq!(reselect(Some("d"), 3, &order), 1);
    assert_eq!(reselect(None, 5, &order), 1);
    assert_eq!(reselect(None, 0, &order), 0);
  }

  fn open(
    mode: Mode,
    cx: &mut TestAppContext,
  ) -> (Rc<FakeCompositor>, AnyWindowHandle, Entity<Switcher>) {
    // live previews fail fast without a compositor
    unsafe { std::env::set_var("WAYLAND_DISPLAY", "/nonexistent/corona-test") };
    let fake = setup(
      FakeCompositor {
        workspaces: vec![workspace("1", "DP-1"), workspace("2", "DP-1")],
        windows: vec![at("a", "1", 0, 0), at("b", "2", 0, 0), at("c", "1", 500, 0)],
        ..Default::default()
      },
      cx,
    );
    let options = Options {
      mode,
      ..Default::default()
    };
    let (handle, view) = cx.update(|cx| {
      bind_keys(cx);
      let mut view = None;
      let handle: WindowHandle<Root> = cx
        .open_window(WindowOptions::default(), |window, cx| {
          let switcher = cx.new(|cx| Switcher::new(options, None, cx));
          window.focus(&switcher.read(cx).focus.clone(), cx);
          view = Some(switcher.clone());
          cx.new(|cx| Root::new(switcher, window, cx))
        })
        .unwrap();
      let view = view.unwrap();
      cx.set_global(SwitcherState {
        overlays: vec![handle.into()],
        view: view.downgrade(),
      });
      (handle.into(), view)
    });
    draw(handle, cx);
    (fake, handle, view)
  }

  fn draw(handle: AnyWindowHandle, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
      .unwrap();
  }

  fn press(handle: AnyWindowHandle, key: &str, cx: &mut TestAppContext) {
    cx.update_window(handle, |_, window, cx| window.press(key, cx))
      .unwrap();
    cx.run_until_parked();
  }

  fn selected(view: &Entity<Switcher>, cx: &mut TestAppContext) -> String {
    view.read_with(cx, |s, _| s.order[s.selected].clone())
  }

  fn is_open(cx: &mut TestAppContext) -> bool {
    cx.update(|cx| cx.has_global::<SwitcherState>())
  }

  fn step(view: &Entity<Switcher>, reverse: bool, cx: &mut TestAppContext) {
    view.update(cx, |v, cx| v.step(reverse, cx));
  }

  #[gpui::test]
  fn step_and_enter_focuses(cx: &mut TestAppContext) {
    let (fake, handle, view) = open(Mode::Window, cx);
    // starts on the active window
    assert_eq!(selected(&view, cx), "a");
    step(&view, false, cx);
    assert_eq!(selected(&view, cx), "c");
    step(&view, true, cx);
    step(&view, true, cx);
    assert_eq!(selected(&view, cx), "b");
    draw(handle, cx);
    press(handle, "enter", cx);
    assert_eq!(*fake.calls.borrow(), ["window b"]);
    assert!(!is_open(cx));
  }

  #[gpui::test]
  fn tab_steps_the_switcher(cx: &mut TestAppContext) {
    let (_, handle, view) = open(Mode::Window, cx);
    press(handle, "tab", cx);
    assert_eq!(selected(&view, cx), "c");
  }

  #[gpui::test]
  fn escape_closes_without_focusing(cx: &mut TestAppContext) {
    let (fake, handle, _) = open(Mode::Window, cx);
    press(handle, "x", cx);
    assert!(is_open(cx));
    press(handle, "escape", cx);
    assert!(fake.calls.borrow().is_empty());
    assert!(!is_open(cx));
  }

  #[gpui::test]
  fn select_focuses_workspace(cx: &mut TestAppContext) {
    let (fake, handle, view) = open(Mode::Workspace, cx);
    assert_eq!(selected(&view, cx), "1");
    draw(handle, cx);
    cx.update_window(handle, |_, window, cx| {
      view.update(cx, |v, cx| v.select("2", window, cx))
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(*fake.calls.borrow(), ["workspace 2"]);
    assert!(!is_open(cx));
  }

  #[gpui::test]
  fn closed_windows_are_pruned(cx: &mut TestAppContext) {
    let (_, handle, view) = open(Mode::Window, cx);
    step(&view, false, cx);
    assert_eq!(selected(&view, cx), "c");
    let windows = cx.update(|cx| cx.compositor().windows.clone());
    windows.update(cx, |w, cx| {
      w.retain(|w| w.address != "a");
      cx.notify();
    });
    cx.run_until_parked();
    // the selection stays on c
    assert_eq!(view.read_with(cx, |s, _| s.order.clone()), ["c", "b"]);
    assert_eq!(selected(&view, cx), "c");
    draw(handle, cx);

    windows.update(cx, |w, cx| {
      w.clear();
      cx.notify();
    });
    cx.run_until_parked();
    assert!(!is_open(cx));
  }
}

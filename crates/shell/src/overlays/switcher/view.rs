use std::collections::HashMap;

use corona_capture::LiveCapture;
use corona_components::components::window_icon::WindowIcon;
use corona_compositor::{CompositorExt, types};
use gpui_kit::{
  App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement, KeyDownEvent,
  ModifiersChangedEvent, MouseButton, ObjectFit, ParentElement, Render, StatefulInteractiveElement,
  Styled, StyledImage, Subscription, Window, black, component::ActiveTheme, div, img,
  prelude::FluentBuilder, px,
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

const FPS: u32 = 5;
const CARD_HEIGHT: f32 = 180.;
const LABEL: f32 = 20.;
const TITLE: f32 = 24.;
const GAP: f32 = 16.;
const PADDING: f32 = 16.;
const BORDER: f32 = 2.;
const INSET: f32 = 3.;
const ICON_SIZE: u16 = 48;
const CORNER_ICON_SIZE: u16 = 20;
const CORNER_INSET: f32 = 4.;

pub(super) fn order(mode: Mode, monitor: Option<&str>, cx: &App) -> Vec<String> {
  let compositor = cx.compositor();
  let workspaces: Vec<&types::Workspace> = compositor
    .list_workspaces(cx)
    .iter()
    .filter(|ws| monitor.is_none_or(|m| ws.monitor == m))
    .collect();
  if mode == Mode::Workspace {
    return workspaces.iter().map(|ws| ws.id.clone()).collect();
  }
  let mut windows: Vec<(usize, &types::Window)> = compositor
    .list_windows(cx)
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
    self.selected = selected
      .and_then(|s| self.order.iter().position(|id| *id == s))
      .unwrap_or(self.selected)
      .min(self.order.len() - 1);
    cx.notify();
  }

  pub fn step(&mut self, reverse: bool, cx: &mut Context<Self>) {
    let n = self.order.len();
    if n == 0 {
      return;
    }
    self.selected = match reverse {
      false => (self.selected + 1) % n,
      true => (self.selected + n - 1) % n,
    };
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
      "tab" => self.step(e.keystroke.modifiers.shift, cx),
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
    let k = (CARD_HEIGHT - inset * 2.) / (bottom - top).max(1) as f32;
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
          .h(px(CARD_HEIGHT))
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
        .map(|(ws, _)| format!("Workspace {}", ws.name)),
    }
    .unwrap_or_default();

    let cards: Vec<_> = workspaces
      .iter()
      .map(|(ws, on)| self.workspace(ws, on.iter().collect(), cx))
      .collect();
    let theme = cx.theme();

    div()
      .track_focus(&self.focus)
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
      .bg(black().opacity(0.4))
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
          .border_color(theme.tokens.button_hover)
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

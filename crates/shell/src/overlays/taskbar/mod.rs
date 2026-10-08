mod menu;
mod preview;

use std::{cell::Cell, collections::HashMap, rc::Rc, time::Duration};

use anyhow::Result;
use corona_components::{
  animation::{animation_duration, smooth_retarget::SmoothRetarget},
  components::window_icon::WindowIcon,
};
use corona_compositor::{CompositorExt, types};
use corona_config::{APP_NAME, ConfigProvider, observe_section, placement::Placement};
use corona_surface::{
  input_region::InputRegion,
  panel::{Align, PanelStyle, panel_shape},
  per_display::PerDisplay,
  popup::popup_options,
};
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, Context, DisplayId, Entity, Global, InteractiveElement,
  IntoElement, MouseButton, ParentElement, Pixels, Render, Size, StatefulInteractiveElement,
  Styled, Subscription, Task, Window, WindowBackgroundAppearance, WindowBounds, WindowDecorations,
  WindowKind, WindowOptions,
  base::{ElementExt, Root},
  component::ActiveTheme,
  div,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point,
  prelude::FluentBuilder,
  px,
};
use tracing::error;

use crate::{
  overlays::taskbar::{menu::TaskbarMenu, preview::Preview},
  widgets::popup,
};

const NAMESPACE: &str = "corona_taskbar";
const PADDING: f32 = 10.;
const GAP: f32 = 10.;
const TRIGGER: f32 = 2.;
const OPEN_SPEED: Duration = Duration::from_millis(250);
const CLOSE_DELAY: Duration = Duration::from_millis(200);
const PREVIEW_DELAY: Duration = Duration::from_millis(200);
const PREVIEW_GAP: f32 = 8.;
const PREVIEW_HIDE_DELAY: Duration = Duration::from_millis(150);

struct Taskbars {
  displays: Option<Entity<PerDisplay>>,
}

impl Global for Taskbars {}

pub fn init(cx: &mut App) {
  cx.set_global(Taskbars { displays: None });
  open(cx);
  // the window's height follows the icon size, so open them again
  observe_section(
    cx,
    |c| &c.taskbar,
    |_, cx| {
      if let Some(displays) = cx.global_mut::<Taskbars>().displays.take() {
        PerDisplay::close(displays, cx);
      }
      open(cx);
    },
  );
}

fn open(cx: &mut App) {
  if !cx.config().taskbar.enabled {
    return;
  }
  let taskbars = PerDisplay::new(cx, |cx, display| {
    create_taskbar(cx, display)
      .inspect_err(|e| error!("failed to create taskbar: {e:#}"))
      .into_iter()
      .collect()
  });
  cx.global_mut::<Taskbars>().displays = Some(taskbars);
}

fn height(icon_size: f32) -> f32 {
  PADDING + icon_size + PADDING
}

fn width(apps: usize, icon_size: f32) -> f32 {
  let n = apps as f32;
  PADDING * 2. + n * icon_size + (n - 1.).max(0.) * GAP
}

fn create_taskbar(cx: &mut App, display: DisplayId) -> Result<AnyWindowHandle> {
  let handle = cx.open_window(
    WindowOptions {
      kind: WindowKind::LayerShell(LayerShellOptions {
        anchor: Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
        exclusive_zone: Some(px(-1.)),
        exclusive_edge: None,
        margin: None,
        layer: Layer::Top,
        namespace: NAMESPACE.to_string(),
        keyboard_interactivity: KeyboardInteractivity::None,
      }),
      window_background: WindowBackgroundAppearance::Transparent,
      window_decorations: Some(WindowDecorations::Client),
      inactive_frame_interval: None,
      app_id: Some(APP_NAME.to_string()),
      display_id: Some(display),
      titlebar: None,
      window_bounds: Some(WindowBounds::Windowed(Bounds {
        origin: point(px(0.), px(0.)),
        size: Size::new(px(0.), px(height(cx.config().taskbar.icon_size))),
      })),
      ..Default::default()
    },
    |window, cx| {
      let view = cx.new(Taskbar::new);
      cx.new(|cx| Root::new(view, window, cx).bg(gpui_kit::transparent_black()))
    },
  )?;

  Ok(handle.into())
}

struct Application {
  class: String,
  windows: Vec<types::Window>,
}

pub struct Taskbar {
  hovered: bool,
  closing: Option<Task<()>>,
  input_region: InputRegion,
  anim: SmoothRetarget,
  apps: Vec<Application>,
  icon_bounds: HashMap<String, Rc<Cell<Bounds<Pixels>>>>,
  hovered_icon: Option<String>,
  preview: Option<(String, AnyWindowHandle, Entity<Preview>)>,
  preview_hovered: bool,
  pending: Option<Task<()>>,
  menu_open: bool,
  /// from the settings when it opened; it opens again when they change
  icon_size: f32,
  _subscription: Subscription,
}

impl Taskbar {
  fn height(&self) -> f32 {
    height(self.icon_size)
  }

  pub fn new(cx: &mut Context<Self>) -> Self {
    let windows = cx.compositor().windows.clone();
    let subscription = cx.observe(&windows, |this, windows, cx| {
      this.apps = group(windows.read(cx));
      cx.notify();
    });

    Taskbar {
      hovered: false,
      closing: None,
      input_region: InputRegion::default(),
      anim: SmoothRetarget::new(0.),
      apps: group(windows.read(cx)),
      icon_bounds: HashMap::new(),
      hovered_icon: None,
      preview: None,
      preview_hovered: false,
      pending: None,
      menu_open: false,
      icon_size: cx.config().taskbar.icon_size,
      _subscription: subscription,
    }
  }

  fn width(&self) -> f32 {
    width(self.apps.len(), self.icon_size)
  }

  fn set_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
    if hovered {
      self.closing = None;
      self.hovered = true;
      cx.notify();
      return;
    }
    self.closing = Some(cx.spawn(async move |this, cx| {
      cx.background_executor().timer(CLOSE_DELAY).await;
      let _ = this.update(cx, |this, cx| {
        this.hovered = false;
        cx.notify();
      });
    }));
  }

  fn open(&self) -> bool {
    self.hovered || self.preview.is_some() || self.menu_open
  }

  fn windows(&self, class: &str) -> Vec<types::Window> {
    self
      .apps
      .iter()
      .find(|a| a.class == class)
      .map(|a| a.windows.clone())
      .unwrap_or_default()
  }

  fn anchor(&self, class: &str, window: &Window) -> Bounds<Pixels> {
    let icon = self
      .icon_bounds
      .get(class)
      .map(|b| b.get())
      .unwrap_or_default();
    Bounds::new(
      point(
        icon.origin.x,
        window.viewport_size().height - px(self.height()),
      ),
      Size::new(icon.size.width, px(self.height())),
    )
  }

  fn icon_hovered(
    &mut self,
    class: &str,
    hovered: bool,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    if !hovered {
      if self.hovered_icon.as_deref() == Some(class) {
        self.hovered_icon = None;
      }
      return self.schedule_hide_preview(cx);
    }

    self.hovered_icon = Some(class.to_string());
    if !cx.config().taskbar.previews {
      return;
    }
    if self.menu_open || self.preview.as_ref().is_some_and(|(c, _, _)| c == class) {
      self.pending = None;
      return;
    }

    let delay = if self.preview.is_some() {
      Duration::ZERO
    } else {
      PREVIEW_DELAY
    };
    let class = class.to_string();
    self.pending = Some(cx.spawn_in(window, async move |this, cx| {
      cx.background_executor().timer(delay).await;
      let _ = this.update_in(cx, |this, window, cx| this.show_preview(class, window, cx));
    }));
  }

  fn show_preview(&mut self, class: String, window: &mut Window, cx: &mut Context<Self>) {
    let windows = self.windows(&class);
    let center = self
      .icon_bounds
      .get(&class)
      .map_or(px(0.), |b| b.get().center().x)
      .as_f32();

    if let Some((shown, _, view)) = &mut self.preview {
      *shown = class;
      view.update(cx, |view, cx| view.show(windows, center, cx));
      return cx.notify();
    }

    let viewport = window.viewport_size();
    let options = popup_options(
      window.window_handle(),
      Bounds::new(
        point(px(0.), px(0.)),
        Size::new(viewport.width, px(self.height())),
      ),
      Placement::Bottom,
      Size::new(viewport.width, px(preview::POPUP_HEIGHT)),
      px(PREVIEW_GAP),
      false,
    );
    let taskbar = cx.weak_entity();
    let mut view = None;
    let opened = cx.open_window(options, |window, cx| {
      let preview = cx.new(|cx| Preview::new(taskbar, windows, center, cx));
      view = Some(preview.clone());
      cx.new(|cx| Root::new(preview, window, cx).bg(gpui_kit::transparent_black()))
    });
    match (opened, view) {
      (Ok(handle), Some(view)) => self.preview = Some((class, handle.into(), view)),
      (Err(e), _) => error!("failed to show taskbar preview: {e:#}"),
      _ => {}
    }
    cx.notify();
  }

  fn preview_hovered(&mut self, hovered: bool, cx: &mut Context<Self>) {
    self.preview_hovered = hovered;
    if hovered {
      self.pending = None;
    } else {
      self.schedule_hide_preview(cx);
    }
  }

  fn schedule_hide_preview(&mut self, cx: &mut Context<Self>) {
    self.pending = Some(cx.spawn(async move |this, cx| {
      cx.background_executor().timer(PREVIEW_HIDE_DELAY).await;
      let _ = this.update(cx, |this, cx| {
        if !this.preview_hovered && this.hovered_icon.is_none() {
          this.hide_preview(cx);
        }
      });
    }));
  }

  fn hide_preview(&mut self, cx: &mut Context<Self>) {
    if let Some((_, handle, _)) = self.preview.take() {
      cx.spawn(async move |_, cx| {
        let _ = handle.update(cx, |_, window, _| window.remove_window());
      })
      .detach();
    }
    self.preview_hovered = false;
    self.pending = None;
    cx.notify();
  }

  fn open_menu(&mut self, class: &str, window: &mut Window, cx: &mut Context<Self>) {
    self.hide_preview(cx);
    self.menu_open = true;
    let windows = self.windows(class);
    let taskbar = cx.weak_entity();
    popup::open_at(
      self.anchor(class, window),
      menu::size(&windows),
      Placement::Bottom,
      window,
      cx,
      move |_, cx| TaskbarMenu::new(taskbar, windows, cx),
    );
  }

  fn menu_closed(&mut self, cx: &mut Context<Self>) {
    self.menu_open = false;
    cx.notify();
  }
}

fn group(windows: &[types::Window]) -> Vec<Application> {
  let mut apps: Vec<Application> = Vec::new();
  for window in windows {
    match apps.iter_mut().find(|a| a.class == window.class) {
      Some(app) => app.windows.push(window.clone()),
      None => apps.push(Application {
        class: window.class.clone(),
        windows: vec![window.clone()],
      }),
    }
  }
  apps.sort_by(|a, b| a.class.cmp(&b.class));
  apps
}

fn focus_window(address: &str, cx: &App) {
  if let Err(e) = cx.compositor().focus_window(address) {
    error!("failed to focus window: {e:#}");
  }
}

impl Render for Taskbar {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let bg = theme
      .tokens
      .background
      .opacity(cx.config().taskbar.background_opacity);
    let n = theme.panel_radius();

    let speed = animation_duration(OPEN_SPEED, cx);
    self.anim.retarget(if self.open() { 1. } else { 0. }, speed);
    let (progress, animating) = self.anim.value();
    if animating {
      window.request_animation_frame();
    }

    let viewport = window.viewport_size();
    let width = self.width();
    let shape_width = width + n * 2.;
    let left = (viewport.width.as_f32() - shape_width) / 2.;
    let h = self.height() * progress;

    let depth = if self.open() { self.height() } else { TRIGGER };
    let region = Placement::Bottom.rect(viewport, px(left), px(shape_width), px(depth));
    self.input_region.set(region, window);

    let (icon_size, height) = (self.icon_size, self.height());
    let apps = self.apps.iter().enumerate().map(|(i, app)| {
      let count = app.windows.len();
      let bounds = self
        .icon_bounds
        .entry(app.class.clone())
        .or_default()
        .clone();
      let (hover_class, menu_class) = (app.class.clone(), app.class.clone());
      WindowIcon::new(&app.class, ("taskbar-app", i))
        .size(icon_size as u16)
        .on_prepaint(move |b, _, _| bounds.set(b))
        .on_hover(cx.listener(move |this, hovered, window, cx| {
          this.icon_hovered(&hover_class, *hovered, window, cx)
        }))
        .on_mouse_down(
          MouseButton::Right,
          cx.listener(move |this, _, window, cx| this.open_menu(&menu_class, window, cx)),
        )
        .when(count == 1, |icon| {
          let address = app.windows[0].address.clone();
          icon
            .cursor_pointer()
            .on_click(cx.listener(move |this, _, _, cx| {
              focus_window(&address, cx);
              this.hide_preview(cx);
            }))
        })
        .when(count > 1, |icon| {
          icon.child(
            div()
              .absolute()
              .top(px(-4.))
              .right(px(-4.))
              .min_w(px(16.))
              .h(px(16.))
              .px_1()
              .flex()
              .items_center()
              .justify_center()
              .rounded_full()
              .bg(theme.colors.primary)
              .text_color(theme.colors.primary_foreground)
              .text_xs()
              .child(count.to_string()),
          )
        })
    });

    div()
      .id("taskbar")
      .size_full()
      .relative()
      .on_hover(cx.listener(|this, hovered, _, cx| this.set_hovered(*hovered, cx)))
      .child(
        panel_shape(n, Align::Relative(0.), Placement::Bottom, bg)
          .absolute()
          .bottom_0()
          .left(px(left))
          .w(px(shape_width))
          .h(px(h)),
      )
      .child(
        div()
          .absolute()
          .bottom_0()
          .left(px(left + n))
          .w(px(width))
          .h(px(h))
          .overflow_hidden()
          .child(
            div()
              .h(px(height))
              .flex()
              .items_start()
              .gap(px(GAP))
              .p(px(PADDING))
              .children(apps),
          ),
      )
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use super::*;

  pub fn window(address: &str, class: &str, width: i32, height: i32) -> types::Window {
    types::Window {
      address: address.into(),
      monitor: 0,
      workspace: "1".into(),
      class: class.into(),
      title: address.into(),
      x: 0,
      y: 0,
      width,
      height,
      floating: false,
      pinned: false,
      fullscreen: false,
      hidden: false,
      focus_history_id: 0,
    }
  }

  #[test]
  fn group_by_class_sorted() {
    let windows = [
      window("1", "kitty", 1, 1),
      window("2", "firefox", 1, 1),
      window("3", "kitty", 1, 1),
      window("4", "", 1, 1),
    ];
    let apps = group(&windows);
    let classes: Vec<_> = apps.iter().map(|a| a.class.as_str()).collect();
    assert_eq!(classes, ["", "firefox", "kitty"]);
    // order inside a group is kept
    let kitty: Vec<_> = apps[2].windows.iter().map(|w| w.address.as_str()).collect();
    assert_eq!(kitty, ["1", "3"]);
    assert_eq!(apps.iter().map(|a| a.windows.len()).sum::<usize>(), 4);
  }

  #[test]
  fn group_empty() {
    assert!(group(&[]).is_empty());
  }

  #[test]
  fn sizes() {
    assert_eq!(height(32.), 32. + 2. * PADDING);
    assert_eq!(width(0, 32.), 2. * PADDING);
    assert_eq!(width(1, 32.), 2. * PADDING + 32.);
    assert_eq!(width(3, 32.), 2. * PADDING + 3. * 32. + 2. * GAP);
  }

  #[test]
  fn menu_grows_with_windows() {
    let one = menu::size(&[window("1", "a", 1, 1)]);
    let two = menu::size(&[window("1", "a", 1, 1), window("2", "a", 1, 1)]);
    assert!(two.height > one.height);
    assert_eq!(one.width, two.width);
    assert!(menu::size(&[]).height > px(0.));
  }

  use crate::test_support::{FakeCompositor, setup};
  use gpui_kit::{self as gpui, TestAppContext, test::TestWindowExt};

  fn open_taskbar(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<Taskbar>) {
    unsafe { std::env::set_var("WAYLAND_DISPLAY", "/nonexistent/corona-test") };
    setup(
      FakeCompositor {
        windows: vec![
          window("1", "kitty", 800, 600),
          window("2", "firefox", 800, 600),
          window("3", "kitty", 800, 600),
        ],
        ..Default::default()
      },
      cx,
    );
    let handle = cx.update(|cx| {
      let display = cx.displays()[0].id();
      create_taskbar(cx, display).unwrap()
    });
    let view = cx
      .update_window(handle, |root, _, cx| {
        root
          .downcast::<Root>()
          .unwrap()
          .read(cx)
          .view()
          .clone()
          .downcast::<Taskbar>()
          .unwrap()
      })
      .unwrap();
    cx.update_window(handle, |_, window, cx| window.render_frame(cx))
      .unwrap();
    (handle, view)
  }

  fn hover(
    handle: AnyWindowHandle,
    view: &Entity<Taskbar>,
    class: &str,
    hovered: bool,
    cx: &mut TestAppContext,
  ) {
    cx.update_window(handle, |_, window, cx| {
      view.update(cx, |t, cx| t.icon_hovered(class, hovered, window, cx))
    })
    .unwrap();
  }

  fn preview(view: &Entity<Taskbar>, cx: &mut TestAppContext) -> Option<String> {
    view.read_with(cx, |t, _| t.preview.as_ref().map(|(c, _, _)| c.clone()))
  }

  #[gpui::test]
  fn groups_follow_the_compositor(cx: &mut TestAppContext) {
    let (_, view) = open_taskbar(cx);
    let classes = |cx: &mut TestAppContext| {
      view.read_with(cx, |t, _| {
        t.apps.iter().map(|a| a.class.clone()).collect::<Vec<_>>()
      })
    };
    assert_eq!(classes(cx), ["firefox", "kitty"]);
    assert_eq!(view.read_with(cx, |t, _| t.windows("kitty").len()), 2);
    assert!(view.read_with(cx, |t, _| t.windows("nope").is_empty()));

    let windows = cx.update(|cx| cx.compositor().windows.clone());
    windows.update(cx, |w, cx| {
      w.retain(|w| w.class != "firefox");
      cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(classes(cx), ["kitty"]);
  }

  #[gpui::test]
  fn hover_shows_then_hides_preview(cx: &mut TestAppContext) {
    let (handle, view) = open_taskbar(cx);
    hover(handle, &view, "kitty", true, cx);
    // only after a delay
    assert_eq!(preview(&view, cx), None);
    cx.executor().advance_clock(PREVIEW_DELAY);
    cx.run_until_parked();
    assert_eq!(preview(&view, cx).as_deref(), Some("kitty"));
    assert!(view.read_with(cx, |t, _| t.open()));

    // moving to another icon swaps the shown app
    hover(handle, &view, "kitty", false, cx);
    hover(handle, &view, "firefox", true, cx);
    cx.run_until_parked();
    assert_eq!(preview(&view, cx).as_deref(), Some("firefox"));

    // the preview being hovered keeps it
    hover(handle, &view, "firefox", false, cx);
    view.update(cx, |t, cx| t.preview_hovered(true, cx));
    cx.executor().advance_clock(PREVIEW_HIDE_DELAY * 2);
    cx.run_until_parked();
    assert!(preview(&view, cx).is_some());

    view.update(cx, |t, cx| t.preview_hovered(false, cx));
    cx.executor().advance_clock(PREVIEW_HIDE_DELAY * 2);
    cx.run_until_parked();
    assert_eq!(preview(&view, cx), None);
  }

  #[gpui::test]
  fn no_preview_when_turned_off(cx: &mut TestAppContext) {
    let (handle, view) = open_taskbar(cx);
    cx.update(|cx| {
      let mut config = cx.config().clone();
      config.taskbar.previews = false;
      cx.set_global(config);
    });
    hover(handle, &view, "kitty", true, cx);
    cx.executor().advance_clock(PREVIEW_DELAY * 2);
    cx.run_until_parked();
    assert_eq!(preview(&view, cx), None);
  }
}

mod menu;
mod preview;

use std::{cell::Cell, collections::HashMap, rc::Rc, time::Duration};

use anyhow::Result;
use corona_components::{
  animation::{animation_duration, smooth_retarget::SmoothRetarget},
  components::window_icon::WindowIcon,
};
use corona_compositor::{CompositorExt, types};
use corona_config::{APP_NAME, placement::Placement};
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
const ICON_SIZE: f32 = 36.;
const PADDING: f32 = 10.;
const GAP: f32 = 10.;
const HEIGHT: f32 = PADDING + ICON_SIZE + PADDING;
const TRIGGER: f32 = 2.;
const OPEN_SPEED: Duration = Duration::from_millis(250);
const CLOSE_DELAY: Duration = Duration::from_millis(200);
const PREVIEW_DELAY: Duration = Duration::from_millis(200);
const PREVIEW_GAP: f32 = 8.;
const PREVIEW_HIDE_DELAY: Duration = Duration::from_millis(150);

struct Taskbars {
  _displays: Entity<PerDisplay>,
}

impl Global for Taskbars {}

pub fn init(cx: &mut App) {
  let taskbars = PerDisplay::new(cx, |cx, display| {
    create_taskbar(cx, display)
      .inspect_err(|e| error!("failed to create taskbar: {e:#}"))
      .into_iter()
      .collect()
  });
  cx.set_global(Taskbars {
    _displays: taskbars,
  });
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
        size: Size::new(px(0.), px(HEIGHT)),
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
  _subscription: Subscription,
}

impl Taskbar {
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
      _subscription: subscription,
    }
  }

  fn width(&self) -> f32 {
    let n = self.apps.len() as f32;
    PADDING * 2. + n * ICON_SIZE + (n - 1.).max(0.) * GAP
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
      point(icon.origin.x, window.viewport_size().height - px(HEIGHT)),
      Size::new(icon.size.width, px(HEIGHT)),
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
      Bounds::new(point(px(0.), px(0.)), Size::new(viewport.width, px(HEIGHT))),
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
    let bg = theme.tokens.background;
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
    let h = HEIGHT * progress;

    let depth = if self.open() { HEIGHT } else { TRIGGER };
    let region = Placement::Bottom.rect(viewport, px(left), px(shape_width), px(depth));
    self.input_region.set(region, window);

    let apps = self.apps.iter().enumerate().map(|(i, app)| {
      let count = app.windows.len();
      let bounds = self
        .icon_bounds
        .entry(app.class.clone())
        .or_default()
        .clone();
      let (hover_class, menu_class) = (app.class.clone(), app.class.clone());
      WindowIcon::new(&app.class, ("taskbar-app", i))
        .size(ICON_SIZE as u16)
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
              .h(px(HEIGHT))
              .flex()
              .items_start()
              .gap(px(GAP))
              .p(px(PADDING))
              .children(apps),
          ),
      )
  }
}

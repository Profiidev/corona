//! The corona demo shell: a top bar on every output with Hyprland workspace
//! buttons, and a click-to-toggle animated panel.

use std::time::{Duration, Instant};

use corona::Shell;
use gpui::{
  App, AppContext as _, AsyncApp, Bounds, Context, DisplayId, Entity, IntoElement,
  ParentElement as _, Pixels, Render, Styled as _, Task, Window, WindowBackgroundAppearance,
  WindowBounds, WindowHandle, WindowKind, WindowOptions, div,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point, px, rgb, size, transparent_black,
};
use gpui_component::{
  ActiveTheme as _, Root, Sizable as _,
  button::Button,
  h_flex,
  input::{Input, InputState},
};

const BAR_HEIGHT: f32 = 30.;
const OUTPUT_POLL: Duration = Duration::from_millis(16);
const OUTPUT_WAIT_TICKS: usize = 125; // ~2s
const FALLBACK_BAR_WIDTH: f32 = 1920.;
const PANEL_SIZE: f32 = 200.;
const PANEL_ANIM: Duration = Duration::from_millis(300);

fn main() {
  tracing_subscriber::fmt::init();

  gpui_platform::application().run(|cx: &mut App| {
    gpui_component::init(cx);

    let shell = cx.new(Shell::new);
    let panel = cx.new(|_| PanelState::default());

    cx.spawn(async move |cx| {
      // ponytail: outputs are enumerated once, after they show up. gpui only
      // learns about wl_outputs when its event loop dispatches the registry, so
      // `displays()` is still empty inside `run`, and there is no display-change
      // hook to react to hotplug with either.
      let displays = wait_for_displays(cx).await;

      for (id, width) in displays {
        tracing::debug!("opening bar: display={id:?} width={width:?}");
        let shell = shell.clone();
        let panel = panel.clone();

        cx.open_window(bar_options(id, width), move |window, cx| {
          let bar = cx.new(|cx| Bar::new(shell, panel, id, window, cx));
          cx.new(|cx| {
            Root::new(bar, window, cx)
              // The CSD window border sets a client inset that inflates the layer
              // surface by the shadow size; a bar wants its exact geometry.
              .bordered(false)
              .bg(cx.theme().background)
          })
        })
        .expect("failed to open bar window");
      }
    })
    .detach();
  });
}

/// Poll until the compositor's outputs have been announced. Falls back to the
/// default output so the bar still appears if none are ever reported.
async fn wait_for_displays(cx: &mut AsyncApp) -> Vec<(Option<DisplayId>, Pixels)> {
  for _ in 0..OUTPUT_WAIT_TICKS {
    let displays = cx.update(|cx| cx.displays());
    if !displays.is_empty() {
      return displays
        .iter()
        .map(|d| (Some(d.id()), d.bounds().size.width))
        .collect();
    }
    cx.background_executor().timer(OUTPUT_POLL).await;
  }

  tracing::warn!("no outputs announced, falling back to the default output");
  vec![(None, px(FALLBACK_BAR_WIDTH))]
}

fn bar_options(display: Option<DisplayId>, width: Pixels) -> WindowOptions {
  WindowOptions {
    titlebar: None,
    // The layer-shell idiom is width 0 plus a LEFT|RIGHT anchor, letting the
    // compositor pick the width. gpui-ce sizes the surface's wp_viewport from
    // these bounds before the first configure arrives, and a zero destination
    // is a viewport protocol error, so pass the output's own width instead.
    window_bounds: Some(WindowBounds::Windowed(Bounds {
      origin: point(px(0.), px(0.)),
      size: size(width, px(BAR_HEIGHT)),
    })),
    display_id: display,
    app_id: Some("corona".into()),
    kind: WindowKind::LayerShell(LayerShellOptions {
      namespace: "corona".into(),
      layer: Layer::Top,
      anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
      exclusive_zone: Some(px(BAR_HEIGHT)),
      keyboard_interactivity: KeyboardInteractivity::OnDemand,
      ..Default::default()
    }),
    ..Default::default()
  }
}

fn panel_options(display: Option<DisplayId>) -> WindowOptions {
  WindowOptions {
    titlebar: None,
    window_bounds: Some(WindowBounds::Windowed(Bounds {
      origin: point(px(0.), px(0.)),
      size: size(px(PANEL_SIZE), px(PANEL_SIZE)),
    })),
    display_id: display,
    app_id: Some("corona".into()),
    window_background: WindowBackgroundAppearance::Transparent,
    kind: WindowKind::LayerShell(LayerShellOptions {
      namespace: "corona".into(),
      layer: Layer::Top,
      anchor: Anchor::TOP | Anchor::LEFT,
      keyboard_interactivity: KeyboardInteractivity::OnDemand,
      ..Default::default()
    }),
    ..Default::default()
  }
}

struct Bar {
  shell: Entity<Shell>,
  panel: Entity<PanelState>,
  display: Option<DisplayId>,
  input: Entity<InputState>,
}

impl Bar {
  fn new(
    shell: Entity<Shell>,
    panel: Entity<PanelState>,
    display: Option<DisplayId>,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Self {
    // Redraw whenever the shell state changes.
    cx.observe(&shell, |_, _, cx| cx.notify()).detach();

    Self {
      shell,
      panel,
      display,
      input: cx.new(|cx| InputState::new(window, cx)),
    }
  }
}

impl Render for Bar {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let workspaces = self.shell.read(cx).workspaces.clone();

    h_flex()
      .size_full()
      .justify_between()
      .items_center()
      .child(
        Button::new("toggle-panel")
          .label("Click Me")
          .w(px(100.))
          .on_click(cx.listener(|this, _, _, cx| {
            let display = this.display;
            let panel = this.panel.clone();
            // Deferred so the click doesn't run with `Bar` still borrowed.
            cx.defer(move |cx| PanelState::toggle(panel, display, cx));
          })),
      )
      .child(h_flex().children(workspaces.into_iter().map(|name| {
        Button::new(gpui::SharedString::from(name.clone()))
          .label(name.clone())
          .xsmall()
          .w(px(30.))
          .on_click(move |_, _, cx| {
            let name = name.clone();
            cx.background_spawn(async move {
              if let Err(e) = corona::hypr::dispatch_workspace(&name) {
                tracing::error!("failed to dispatch workspace: {e}");
              }
            })
            .detach();
          })
      })))
      .child(div().w(px(200.)).child(Input::new(&self.input)))
  }
}

/// The panel is a second layer-shell window, shared by every bar. Owning it in
/// one entity replaces the `Rc<RefCell<PanelState>>` the Slint version used.
#[derive(Default)]
struct PanelState {
  open: bool,
  window: Option<WindowHandle<Root>>,
  /// Pending teardown after the close animation. Dropping it cancels the
  /// teardown, which is how reopening mid-close keeps the same surface --
  /// Slint did the same with `close-timer.stop()`.
  close: Option<Task<()>>,
}

impl PanelState {
  /// Takes the entity rather than `&mut self`: `open_window` renders the
  /// panel's first frame inline, and that frame reads `PanelState`, so no
  /// borrow of it may be held across the call.
  fn toggle(state: Entity<Self>, display: Option<DisplayId>, cx: &mut App) {
    let (has_window, open) = {
      let this = state.read(cx);
      (this.window.is_some(), this.open)
    };

    if has_window {
      state.update(cx, |this, cx| {
        this.open = !open;
        this.close = open.then(|| {
          cx.spawn(async move |this, cx| {
            cx.background_executor().timer(PANEL_ANIM).await;
            // Take the handle out first: tearing the window down drops the
            // panel view, which must not happen while `PanelState` is borrowed.
            let window = this.update(cx, |this, _| this.window.take()).ok().flatten();
            if let Some(window) = window {
              let _ = window.update(cx, |_, window, _| window.remove_window());
            }
          })
        });
        cx.notify();
      });

      return;
    }

    // Open before the window exists so the first frame already animates open.
    state.update(cx, |this, cx| {
      this.open = true;
      cx.notify();
    });

    let window = cx.open_window(panel_options(display), {
      let state = state.clone();
      move |window, cx| {
        let panel = cx.new(|cx| Panel::new(state, cx));
        cx.new(|cx| {
          Root::new(panel, window, cx)
            .bordered(false)
            // Root paints the theme background over the whole window, which
            // would fill the collapsed area white. The panel wants only its
            // own body drawn, like the Slint version's `background: transparent`.
            .bg(transparent_black())
        })
      }
    });

    state.update(cx, |this, cx| {
      match window {
        Ok(window) => this.window = Some(window),
        Err(e) => {
          tracing::error!("failed to open panel window: {e:#}");
          this.open = false;
        }
      }
      cx.notify();
    });
  }
}

/// A continuous height animation with Slint's `animate` semantics: retargeting
/// mid-flight resumes from the current value instead of restarting. Upstream
/// gpui has no transition primitive, and `with_animation` restarts whenever its
/// element id changes, so the interpolation is done by hand.
struct Anim {
  from: f32,
  to: f32,
  start: Instant,
}

impl Anim {
  fn new(value: f32) -> Self {
    Self {
      from: value,
      to: value,
      start: Instant::now(),
    }
  }

  /// Current value, and whether the animation is still running.
  fn value(&self) -> (f32, bool) {
    let t = (self.start.elapsed().as_secs_f32() / PANEL_ANIM.as_secs_f32()).min(1.);
    let eased = 1. - (1. - t) * (1. - t); // ease-out-quad, as in panel.slint
    (self.from + (self.to - self.from) * eased, t < 1.)
  }

  fn retarget(&mut self, to: f32) {
    if self.to == to {
      return;
    }
    self.from = self.value().0;
    self.to = to;
    self.start = Instant::now();
  }
}

struct Panel {
  state: Entity<PanelState>,
  height: Anim,
}

impl Panel {
  fn new(state: Entity<PanelState>, cx: &mut Context<Self>) -> Self {
    cx.observe(&state, |_, _, cx| cx.notify()).detach();
    Self {
      state,
      height: Anim::new(0.),
    }
  }
}

impl Render for Panel {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let open = self.state.read(cx).open;

    self.height.retarget(if open { PANEL_SIZE } else { 0. });
    let (h, animating) = self.height.value();
    if animating {
      window.request_animation_frame();
    }

    div().size_full().child(
      div()
        .w_full()
        .h(px(h))
        .overflow_hidden()
        .bg(rgb(0xff0fff))
        .child(
          Button::new("panel-button")
            .label("Click Me")
            .w(px(100.))
            .on_click(|_, _, _| println!("Panel clicked")),
        ),
    )
  }
}

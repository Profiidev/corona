//! Wayland-side shared state: registry/output/seat bookkeeping, one always-on bar surface per
//! output, and on-demand notification/OSD/calendar surfaces. `configure()` is the single
//! dispatch point that routes a compositor configure event to whichever surface it belongs to,
//! via `pending` (first configure) or a same-surface scan across the ready surfaces (resizes).
//! `pointer_frame()` is the equivalent dispatch point for input.

use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use anyhow::{Context, Result};
use calloop::LoopHandle;
use calloop::timer::{TimeoutAction, Timer};
use hyprland::dispatch::{Dispatch as HyprDispatch, DispatchType, WorkspaceIdentifierWithSpecial};
use slint::platform::WindowAdapter as _;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::registry_handlers;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface as _;
use smithay_client_toolkit::shell::wlr_layer::{
  Anchor, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use smithay_client_toolkit::{delegate_dispatch2, delegate_registry};
use wayland_client::backend::ObjectId;
use wayland_client::protocol::{wl_output, wl_pointer, wl_seat, wl_surface};
use wayland_client::{Connection, Proxy as _, QueueHandle};

use crate::bar_ui::BarUi;
use crate::egl::{RenderContextFactory, RenderContextManager};
use crate::events::{ShellEvent, ShellSender, WorkspaceInfo};
use crate::platform::CoronaPlatform;
use crate::surface::{SurfaceSpec, build_window, spawn_layer_surface};
use crate::ui;
use crate::window::CoronaWindow;

const BAR_HEIGHT: u32 = 32;
const NOTIFICATION_WIDTH: u32 = 340;
const NOTIFICATION_HEIGHT: u32 = 84;
const NOTIFICATION_GAP: i32 = 8;
const OSD_WIDTH: u32 = 260;
const OSD_HEIGHT: u32 = 64;
const OSD_TIMEOUT: Duration = Duration::from_millis(1500);
const CALENDAR_WIDTH: u32 = 260;
const CALENDAR_HEIGHT: u32 = 120;

enum PendingKind {
  Bar(ObjectId), // output id
  Notification(u32),
  Osd,
  Calendar,
}

struct BarSurface {
  layer_surface: LayerSurface,
  window: Option<Rc<CoronaWindow>>,
  ui: Option<BarUi>,
}

struct NotificationSurface {
  layer_surface: LayerSurface,
  window: Option<Rc<CoronaWindow>>,
  ui: Option<ui::notification::Notification>,
  app_name: String,
  summary: String,
  body: String,
}

struct OsdSurface {
  layer_surface: LayerSurface,
  window: Option<Rc<CoronaWindow>>,
  ui: Option<ui::osd::Osd>,
  label: String,
  value: f32,
}

struct CalendarSurface {
  layer_surface: LayerSurface,
  window: Option<Rc<CoronaWindow>>,
  ui: Option<ui::calendar::Calendar>,
}

pub struct AppState {
  registry_state: RegistryState,
  output_state: OutputState,
  seat_state: SeatState,
  compositor: CompositorState,
  layer_shell: LayerShell,
  qh: QueueHandle<Self>,
  loop_handle: LoopHandle<'static, Self>,
  render_factory: Rc<RenderContextFactory>,
  platform: Rc<CoronaPlatform>,
  /// Clone of the same channel main.rs feeds Hyprland/D-Bus/hot-reload events through — handed
  /// to UI click callbacks so they can route "close this notification"-style actions back into
  /// `handle_shell_event` instead of needing a `&mut AppState` inside a closure.
  shell_tx: ShellSender,
  pointer: Option<wl_pointer::WlPointer>,

  /// Surfaces that requested a layer surface but haven't seen their first `configure` yet.
  pending: HashMap<ObjectId, PendingKind>,

  /// One bar per output, keyed by the output's object id.
  bars: HashMap<ObjectId, BarSurface>,
  notifications: HashMap<u32, NotificationSurface>,
  osd: Option<OsdSurface>,
  osd_generation: u32,
  calendar: Option<CalendarSurface>,

  // Last known values, re-applied whenever a bar UI gets (re)built — new output, hot-reload.
  last_workspaces: Vec<WorkspaceInfo>,
  last_title: String,

  pub exit: bool,
}

impl AppState {
  pub fn new(
    conn: &Connection,
    qh: &QueueHandle<Self>,
    loop_handle: LoopHandle<'static, Self>,
    shell_tx: ShellSender,
    globals: &wayland_client::globals::GlobalList,
  ) -> Result<Self> {
    let compositor = CompositorState::bind(globals, qh).context("wl_compositor not available")?;
    let layer_shell = LayerShell::bind(globals, qh)
      .context("zwlr_layer_shell_v1 not available (compositor isn't wlr-layer-shell capable)")?;
    let output_state = OutputState::new(globals, qh);
    let seat_state = SeatState::new(globals, qh);

    let render_manager = RenderContextManager::new(&conn.display().id())?;
    let render_factory = RenderContextFactory::new(render_manager);

    let platform = CoronaPlatform::new();
    slint::platform::set_platform(Box::new(PlatformWrapper(Rc::clone(&platform))))
      .map_err(|e| anyhow::anyhow!("failed to set Slint platform: {e:?}"))?;

    let mut state = Self {
      registry_state: RegistryState::new(globals),
      output_state,
      seat_state,
      compositor,
      layer_shell,
      qh: qh.clone(),
      loop_handle,
      render_factory,
      platform,
      shell_tx,
      pointer: None,
      pending: HashMap::new(),
      bars: HashMap::new(),
      notifications: HashMap::new(),
      osd: None,
      osd_generation: 0,
      calendar: None,
      last_workspaces: Vec::new(),
      last_title: String::new(),
      exit: false,
    };

    // Bind every output already known at startup; `new_output` picks up ones that appear later.
    let outputs: Vec<_> = state.output_state.outputs().collect();
    for output in outputs {
      state.spawn_bar(output);
    }

    Ok(state)
  }

  fn spawn_bar(&mut self, output: wl_output::WlOutput) {
    let output_id = output.id();
    if self.bars.contains_key(&output_id) {
      return;
    }
    let layer_surface = spawn_layer_surface(
      &self.compositor,
      &self.layer_shell,
      &self.qh,
      &SurfaceSpec {
        namespace: "corona-bar",
        layer: Layer::Top,
        anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
        width: 0, // full anchored width
        height: BAR_HEIGHT,
        exclusive_zone: BAR_HEIGHT as i32,
        margin: (0, 0, 0, 0),
        output: Some(&output),
      },
    );
    self.pending.insert(
      layer_surface.wl_surface().id(),
      PendingKind::Bar(output_id.clone()),
    );
    self.bars.insert(
      output_id,
      BarSurface {
        layer_surface,
        window: None,
        ui: None,
      },
    );
  }

  pub fn render_if_dirty(&self) -> Result<()> {
    for bar in self.bars.values() {
      if let Some(window) = &bar.window {
        window.render_if_dirty()?;
      }
    }
    for n in self.notifications.values() {
      if let Some(window) = &n.window {
        window.render_if_dirty()?;
      }
    }
    if let Some(window) = self.osd.as_ref().and_then(|o| o.window.as_ref()) {
      window.render_if_dirty()?;
    }
    if let Some(window) = self.calendar.as_ref().and_then(|c| c.window.as_ref()) {
      window.render_if_dirty()?;
    }
    Ok(())
  }

  /// Called once a second from main.rs — the only thing on a timer rather than an event.
  pub fn tick_clock(&self) {
    let now = chrono::Local::now();
    let clock = now.format("%H:%M").to_string();
    for bar in self.bars.values() {
      if let Some(ui) = &bar.ui {
        ui.set_clock(&clock);
      }
      if let Some(window) = &bar.window {
        window.request_redraw();
      }
    }
    if let Some(cal) = &self.calendar {
      if let Some(ui) = &cal.ui {
        ui.set_time(clock.clone().into());
        ui.set_weekday(now.format("%A").to_string().into());
        ui.set_date(now.format("%B %-d").to_string().into());
      }
      if let Some(window) = &cal.window {
        window.request_redraw();
      }
    }
  }

  pub fn handle_shell_event(&mut self, event: ShellEvent) {
    match event {
      ShellEvent::Workspaces(workspaces) => {
        self.last_workspaces = workspaces;
        for bar in self.bars.values() {
          if let Some(ui) = &bar.ui {
            ui.set_workspaces(&self.last_workspaces);
          }
          if let Some(window) = &bar.window {
            window.request_redraw();
          }
        }
      }
      ShellEvent::ActiveWindowTitle(title) => {
        self.last_title = title.unwrap_or_default();
        for bar in self.bars.values() {
          if let Some(ui) = &bar.ui {
            ui.set_active_window_title(&self.last_title);
          }
          if let Some(window) = &bar.window {
            window.request_redraw();
          }
        }
      }
      ShellEvent::Notify {
        id,
        app_name,
        summary,
        body,
        timeout_ms,
      } => {
        self.spawn_notification(id, app_name, summary, body);
        self.schedule_close_notification(id, timeout_ms);
      }
      ShellEvent::CloseNotification(id) => {
        self.notifications.remove(&id);
        self.relayout_notifications();
      }
      ShellEvent::ShowOsd { label, value } => self.show_osd(label, value),
      ShellEvent::ToggleCalendar => self.toggle_calendar(),
      #[cfg(feature = "hot-reload")]
      ShellEvent::UiChanged => self.reload_bar_ui(),
    }
  }

  fn spawn_notification(&mut self, id: u32, app_name: String, summary: String, body: String) {
    // Same id as an existing notification (app replacing its own toast): just update text.
    if let Some(existing) = self.notifications.get_mut(&id) {
      existing.app_name = app_name.clone();
      existing.summary = summary.clone();
      existing.body = body.clone();
      if let Some(ui) = &existing.ui {
        ui.set_app_name(app_name.into());
        ui.set_summary(summary.into());
        ui.set_body(body.into());
      }
      if let Some(window) = &existing.window {
        window.request_redraw();
      }
      return;
    }

    let index = self.notifications.len() as i32;
    let layer_surface = spawn_layer_surface(
      &self.compositor,
      &self.layer_shell,
      &self.qh,
      &SurfaceSpec {
        namespace: "corona-notification",
        layer: Layer::Overlay,
        anchor: Anchor::TOP | Anchor::RIGHT,
        width: NOTIFICATION_WIDTH,
        height: NOTIFICATION_HEIGHT,
        exclusive_zone: -1,
        margin: (
          8 + index * (NOTIFICATION_HEIGHT as i32 + NOTIFICATION_GAP),
          8,
          0,
          0,
        ),
        output: None,
      },
    );
    self.pending.insert(
      layer_surface.wl_surface().id(),
      PendingKind::Notification(id),
    );
    self.notifications.insert(
      id,
      NotificationSurface {
        layer_surface,
        window: None,
        ui: None,
        app_name,
        summary,
        body,
      },
    );
  }

  fn schedule_close_notification(&self, id: u32, timeout_ms: i32) {
    let timer = Timer::from_duration(Duration::from_millis(timeout_ms.max(0) as u64));
    let _ = self
      .loop_handle
      .insert_source(timer, move |_, _, state: &mut AppState| {
        state.notifications.remove(&id);
        state.relayout_notifications();
        TimeoutAction::Drop
      });
  }

  /// ponytail: recomputes every remaining notification's top margin from scratch, in whatever
  /// order the HashMap happens to iterate. Fine for the handful of notifications a desktop
  /// realistically has on screen; swap for an ordered Vec if stacking order ever matters.
  fn relayout_notifications(&self) {
    for (index, n) in self.notifications.values().enumerate() {
      n.layer_surface.set_margin(
        8 + index as i32 * (NOTIFICATION_HEIGHT as i32 + NOTIFICATION_GAP),
        8,
        0,
        0,
      );
      n.layer_surface.wl_surface().commit();
    }
  }

  fn show_osd(&mut self, label: String, value: f32) {
    if let Some(osd) = &mut self.osd {
      osd.label = label.clone();
      osd.value = value;
      if let Some(ui) = &osd.ui {
        ui.set_label(label.into());
        ui.set_value(value);
      }
      if let Some(window) = &osd.window {
        window.request_redraw();
      }
    } else {
      let layer_surface = spawn_layer_surface(
        &self.compositor,
        &self.layer_shell,
        &self.qh,
        &SurfaceSpec {
          namespace: "corona-osd",
          layer: Layer::Overlay,
          anchor: Anchor::empty(), // centered
          width: OSD_WIDTH,
          height: OSD_HEIGHT,
          exclusive_zone: -1,
          margin: (0, 0, 0, 0),
          output: None,
        },
      );
      self
        .pending
        .insert(layer_surface.wl_surface().id(), PendingKind::Osd);
      self.osd = Some(OsdSurface {
        layer_surface,
        window: None,
        ui: None,
        label,
        value,
      });
    }
    self.schedule_hide_osd();
  }

  fn schedule_hide_osd(&mut self) {
    self.osd_generation += 1;
    let generation = self.osd_generation;
    let timer = Timer::from_duration(OSD_TIMEOUT);
    let _ = self
      .loop_handle
      .insert_source(timer, move |_, _, state: &mut AppState| {
        if state.osd_generation == generation {
          state.osd = None;
        }
        TimeoutAction::Drop
      });
  }

  fn toggle_calendar(&mut self) {
    if self.calendar.is_some() {
      self.calendar = None;
      return;
    }
    let layer_surface = spawn_layer_surface(
      &self.compositor,
      &self.layer_shell,
      &self.qh,
      &SurfaceSpec {
        namespace: "corona-calendar",
        layer: Layer::Overlay,
        anchor: Anchor::TOP | Anchor::RIGHT,
        width: CALENDAR_WIDTH,
        height: CALENDAR_HEIGHT,
        exclusive_zone: -1,
        margin: (8 + BAR_HEIGHT as i32, 8, 0, 0),
        output: None,
      },
    );
    self
      .pending
      .insert(layer_surface.wl_surface().id(), PendingKind::Calendar);
    self.calendar = Some(CalendarSurface {
      layer_surface,
      window: None,
      ui: None,
    });
  }

  fn wire_bar_callbacks(&self, ui: &BarUi) {
    ui.on_workspace_clicked(|id| {
      if let Err(e) = HyprDispatch::call(DispatchType::Workspace(
        WorkspaceIdentifierWithSpecial::Id(id),
      )) {
        tracing::warn!("failed to switch to workspace {id}: {e:#}");
      }
    });
  }

  #[cfg(feature = "hot-reload")]
  fn reload_bar_ui(&mut self) {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("ui/bar.slint");
    let compiler = slint_interpreter::Compiler::new();
    let result = spin_on::spin_on(compiler.build_from_path(&path));
    if result.has_errors() {
      for diagnostic in result.diagnostics() {
        tracing::error!("bar.slint: {diagnostic:?}");
      }
      return;
    }
    let Some(definition) = result.component("Bar") else {
      tracing::error!("bar.slint: no exported 'Bar' component found");
      return;
    };

    let bar_windows: Vec<(ObjectId, Rc<CoronaWindow>)> = self
      .bars
      .iter()
      .filter_map(|(id, b)| b.window.clone().map(|w| (id.clone(), w)))
      .collect();

    for (output_id, window) in bar_windows {
      self.platform.add_window(Rc::clone(&window));
      match definition.create() {
        Ok(instance) => {
          let ui = BarUi::Interpreted(instance);
          ui.set_workspaces(&self.last_workspaces);
          ui.set_active_window_title(&self.last_title);
          ui.set_clock(&chrono::Local::now().format("%H:%M").to_string());
          self.wire_bar_callbacks(&ui);
          if let Some(bar) = self.bars.get_mut(&output_id) {
            bar.ui = Some(ui);
          }
          window.request_redraw();
        }
        Err(e) => tracing::error!("failed to instantiate reloaded bar.slint: {e}"),
      }
    }
    tracing::info!("bar.slint hot-reloaded");
  }

  fn finish_setup(&mut self, kind: PendingKind, width: u32, height: u32) {
    let result: Result<()> = (|| {
      match kind {
        PendingKind::Bar(output_id) => {
          if let Some(layer_surface) = self.bars.get(&output_id).map(|b| b.layer_surface.clone()) {
            let window = build_window(
              &self.render_factory,
              &layer_surface,
              &self.platform,
              width,
              height,
            )?;
            let compiled = ui::bar::Bar::new().map_err(|e| anyhow::anyhow!("{e}"))?;
            let ui = BarUi::Compiled(compiled);
            ui.set_workspaces(&self.last_workspaces);
            ui.set_active_window_title(&self.last_title);
            ui.set_clock(&chrono::Local::now().format("%H:%M").to_string());
            self.wire_bar_callbacks(&ui);
            if let Some(bar) = self.bars.get_mut(&output_id) {
              bar.window = Some(window);
              bar.ui = Some(ui);
            }
          }
        }
        PendingKind::Notification(id) => {
          if let Some(n) = self.notifications.get_mut(&id) {
            let window = build_window(
              &self.render_factory,
              &n.layer_surface,
              &self.platform,
              width,
              height,
            )?;
            let notif =
              ui::notification::Notification::new().map_err(|e| anyhow::anyhow!("{e}"))?;
            notif.set_app_name(n.app_name.clone().into());
            notif.set_summary(n.summary.clone().into());
            notif.set_body(n.body.clone().into());
            let tx = self.shell_tx.clone();
            notif.on_dismissed(move || {
              let _ = tx.send(ShellEvent::CloseNotification(id));
            });
            n.window = Some(window);
            n.ui = Some(notif);
          }
        }
        PendingKind::Osd => {
          if let Some(o) = &mut self.osd {
            let window = build_window(
              &self.render_factory,
              &o.layer_surface,
              &self.platform,
              width,
              height,
            )?;
            let osd = ui::osd::Osd::new().map_err(|e| anyhow::anyhow!("{e}"))?;
            osd.set_label(o.label.clone().into());
            osd.set_value(o.value);
            o.window = Some(window);
            o.ui = Some(osd);
          }
        }
        PendingKind::Calendar => {
          if let Some(c) = &mut self.calendar {
            let window = build_window(
              &self.render_factory,
              &c.layer_surface,
              &self.platform,
              width,
              height,
            )?;
            let cal = ui::calendar::Calendar::new().map_err(|e| anyhow::anyhow!("{e}"))?;
            let now = chrono::Local::now();
            cal.set_time(now.format("%H:%M").to_string().into());
            cal.set_weekday(now.format("%A").to_string().into());
            cal.set_date(now.format("%B %-d").to_string().into());
            let tx = self.shell_tx.clone();
            cal.on_closed(move || {
              let _ = tx.send(ShellEvent::ToggleCalendar);
            });
            c.window = Some(window);
            c.ui = Some(cal);
          }
        }
      }
      Ok(())
    })();
    if let Err(e) = result {
      tracing::error!("failed to finish surface setup: {e:#}");
    }
  }

  fn resize_ready_surface(&self, id: &ObjectId, width: u32, height: u32) {
    let size = slint::PhysicalSize::new(width, height);
    if let Some(bar) = self
      .bars
      .values()
      .find(|b| b.layer_surface.wl_surface().id() == *id)
    {
      if let Some(window) = &bar.window {
        window.set_physical_size(size);
      }
      return;
    }
    if let Some(n) = self
      .notifications
      .values()
      .find(|n| n.layer_surface.wl_surface().id() == *id)
    {
      if let Some(window) = &n.window {
        window.set_physical_size(size);
      }
      return;
    }
    if let Some(o) = &self.osd {
      if o.layer_surface.wl_surface().id() == *id {
        if let Some(window) = &o.window {
          window.set_physical_size(size);
        }
        return;
      }
    }
    if let Some(c) = &self.calendar {
      if c.layer_surface.wl_surface().id() == *id {
        if let Some(window) = &c.window {
          window.set_physical_size(size);
        }
      }
    }
  }

  /// Finds whichever surface owns `surface` and returns its `CoronaWindow`, for translating
  /// Wayland pointer events into Slint `WindowEvent`s.
  fn window_for_surface(&self, surface: &wl_surface::WlSurface) -> Option<&Rc<CoronaWindow>> {
    if let Some(bar) = self
      .bars
      .values()
      .find(|b| b.layer_surface.wl_surface() == surface)
    {
      return bar.window.as_ref();
    }
    if let Some(n) = self
      .notifications
      .values()
      .find(|n| n.layer_surface.wl_surface() == surface)
    {
      return n.window.as_ref();
    }
    if let Some(o) = &self.osd {
      if o.layer_surface.wl_surface() == surface {
        return o.window.as_ref();
      }
    }
    if let Some(c) = &self.calendar {
      if c.layer_surface.wl_surface() == surface {
        return c.window.as_ref();
      }
    }
    None
  }
}

impl CompositorHandler for AppState {
  fn scale_factor_changed(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    _: &wl_surface::WlSurface,
    _: i32,
  ) {
  }
  fn transform_changed(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    _: &wl_surface::WlSurface,
    _: wl_output::Transform,
  ) {
  }
  // ponytail: no frame-callback throttling yet: we just render whenever something is dirty on
  // the next event-loop tick. Add wl_surface.frame() damage tracking if idle CPU use matters.
  fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
  fn surface_enter(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    _: &wl_surface::WlSurface,
    _: &wl_output::WlOutput,
  ) {
  }
  fn surface_leave(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    _: &wl_surface::WlSurface,
    _: &wl_output::WlOutput,
  ) {
  }
}

impl OutputHandler for AppState {
  fn output_state(&mut self) -> &mut OutputState {
    &mut self.output_state
  }
  fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
    self.spawn_bar(output);
  }
  fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
  fn output_destroyed(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    output: wl_output::WlOutput,
  ) {
    self.bars.remove(&output.id());
  }
}

impl SeatHandler for AppState {
  fn seat_state(&mut self) -> &mut SeatState {
    &mut self.seat_state
  }
  fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
  fn new_capability(
    &mut self,
    _: &Connection,
    qh: &QueueHandle<Self>,
    seat: wl_seat::WlSeat,
    capability: Capability,
  ) {
    if capability == Capability::Pointer && self.pointer.is_none() {
      match self.seat_state.get_pointer(qh, &seat) {
        Ok(pointer) => self.pointer = Some(pointer),
        Err(e) => tracing::warn!("failed to bind pointer: {e}"),
      }
    }
  }
  fn remove_capability(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    _: wl_seat::WlSeat,
    capability: Capability,
  ) {
    if capability == Capability::Pointer {
      if let Some(pointer) = self.pointer.take() {
        pointer.release();
      }
    }
  }
  fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for AppState {
  fn pointer_frame(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    _: &wl_pointer::WlPointer,
    events: &[PointerEvent],
  ) {
    use slint::LogicalPosition;
    use slint::platform::{PointerEventButton, WindowEvent};

    for event in events {
      let Some(window) = self.window_for_surface(&event.surface) else {
        continue;
      };
      let (x, y) = event.position;
      let position = LogicalPosition::new(x as f32, y as f32);

      let slint_event = match event.kind {
        PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
          Some(WindowEvent::PointerMoved { position })
        }
        PointerEventKind::Leave { .. } => Some(WindowEvent::PointerExited),
        PointerEventKind::Press { button, .. } => Some(WindowEvent::PointerPressed {
          position,
          button: wayland_button(button),
        }),
        PointerEventKind::Release { button, .. } => Some(WindowEvent::PointerReleased {
          position,
          button: wayland_button(button),
        }),
        PointerEventKind::Axis {
          horizontal,
          vertical,
          ..
        } => Some(WindowEvent::PointerScrolled {
          position,
          delta_x: horizontal.absolute as f32,
          delta_y: vertical.absolute as f32,
        }),
      };

      if let Some(slint_event) = slint_event {
        window.window().dispatch_event(slint_event);
      }
    }
    let _ = PointerEventButton::Other; // keep import used if match arms above ever get pruned
  }
}

/// evdev button codes (BTN_LEFT/RIGHT/MIDDLE from linux/input-event-codes.h) — Wayland's
/// `wl_pointer.button` is these raw codes, Slint wants its own enum.
fn wayland_button(button: u32) -> slint::platform::PointerEventButton {
  use slint::platform::PointerEventButton;
  match button {
    0x110 => PointerEventButton::Left,
    0x111 => PointerEventButton::Right,
    0x112 => PointerEventButton::Middle,
    _ => PointerEventButton::Other,
  }
}

impl LayerShellHandler for AppState {
  fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
    let id = layer.wl_surface().id();
    let closed_bar_output = self
      .bars
      .iter()
      .find(|(_, b)| b.layer_surface.wl_surface().id() == id)
      .map(|(output_id, _)| output_id.clone());
    if let Some(output_id) = closed_bar_output {
      self.bars.remove(&output_id);
      if self.bars.is_empty() {
        // No monitors left to put a bar on — nothing sensible to keep running for.
        self.exit = true;
      }
      return;
    }
    self
      .notifications
      .retain(|_, n| n.layer_surface.wl_surface().id() != id);
    if self
      .osd
      .as_ref()
      .is_some_and(|o| o.layer_surface.wl_surface().id() == id)
    {
      self.osd = None;
    }
    if self
      .calendar
      .as_ref()
      .is_some_and(|c| c.layer_surface.wl_surface().id() == id)
    {
      self.calendar = None;
    }
  }

  fn configure(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    layer: &LayerSurface,
    configure: LayerSurfaceConfigure,
    _serial: u32,
  ) {
    let id = layer.wl_surface().id();
    let (width, height) = configure.new_size;
    let width = width.max(1);
    let height = height.max(1);

    if let Some(kind) = self.pending.remove(&id) {
      self.finish_setup(kind, width, height);
    } else {
      self.resize_ready_surface(&id, width, height);
    }
  }
}

/// `slint::platform::set_platform` wants `Platform`, not `Rc<dyn Platform>` — this just forwards
/// (see `CoronaPlatform`'s own doc comment for why the indirection is needed).
struct PlatformWrapper(Rc<CoronaPlatform>);
impl slint::platform::Platform for PlatformWrapper {
  fn create_window_adapter(
    &self,
  ) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
    self.0.create_window_adapter()
  }
}

delegate_registry!(AppState);
delegate_dispatch2!(AppState);

impl ProvidesRegistryState for AppState {
  fn registry(&mut self) -> &mut RegistryState {
    &mut self.registry_state
  }
  registry_handlers![OutputState, SeatState];
}

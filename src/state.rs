use std::rc::Rc;

use smithay_client_toolkit::{
  compositor::CompositorHandler,
  delegate_dispatch2, delegate_registry,
  output::{OutputHandler, OutputState},
  registry::{ProvidesRegistryState, RegistryState},
  registry_handlers,
  seat::{
    Capability, SeatHandler, SeatState,
    keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers},
    pointer::{PointerEvent, PointerHandler},
  },
  shell::wlr_layer::{LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
};
use wayland_client::{
  Connection, QueueHandle,
  protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
};

use crate::{
  adapter::{
    gpu::{GpuContext, GpuError},
    slint::{SlintCustomPlatform, SlintCustomPlatformError},
    wayland::{WaylandAdapter, WaylandAdapterError},
  },
  event::{
    event::ShellEvent,
    event_loop::{EventLoop, EventLoopError},
  },
};

pub struct Corona {
  wayland: WaylandAdapter,
  gpu: Rc<GpuContext>,
  platform: Rc<SlintCustomPlatform>,
  event_loop: Option<EventLoop>,
  exit_requested: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum CoronaError {
  #[error("Wayland adapter error: {0}")]
  WaylandAdapterError(#[from] WaylandAdapterError),
  #[error("GPU context error: {0}")]
  GpuError(#[from] GpuError),
  #[error("Slint platform error: {0}")]
  SlintPlatformError(#[from] SlintCustomPlatformError),
  #[error("Slint platform error: {0}")]
  EventLoopError(#[from] EventLoopError),
  #[error("Event loop already taken")]
  EventLoopTaken,
}

impl Corona {
  pub fn init() -> Result<Self, CoronaError> {
    let mut wayland = WaylandAdapter::init()?;
    let gpu = GpuContext::init(&wayland)?;
    let platform = SlintCustomPlatform::init(gpu.clone())?;
    let event_loop = EventLoop::init(&mut wayland)?;

    Ok(Self {
      wayland,
      gpu,
      platform,
      event_loop: Some(event_loop),
      exit_requested: false,
    })
  }

  pub fn run(&mut self) -> Result<(), CoronaError> {
    let mut event_loop = self.event_loop.take().ok_or(CoronaError::EventLoopTaken)?;

    while !self.exit_requested {
      event_loop.dispatch(self)?;
      slint::platform::update_timers_and_animations();
      self.render_if_dirty()?;
      self.wayland.flush()?;
    }
    Ok(())
  }

  pub fn handle_shell_event(&mut self, event: ShellEvent) {
    // TODO
  }

  pub fn tick_clock(&mut self) {
    // TODO
  }

  fn render_if_dirty(&mut self) -> Result<(), CoronaError> {
    // TODO
    Ok(())
  }
}

delegate_dispatch2!(Corona);
delegate_registry!(Corona);

impl CompositorHandler for Corona {
  fn scale_factor_changed(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _surface: &wl_surface::WlSurface,
    _new_factor: i32,
  ) {
    // TODO
  }

  fn transform_changed(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _surface: &wl_surface::WlSurface,
    _new_transform: wl_output::Transform,
  ) {
    // TODO
  }

  fn frame(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _surface: &wl_surface::WlSurface,
    _time: u32,
  ) {
    // TODO
  }

  fn surface_enter(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _surface: &wl_surface::WlSurface,
    _output: &wl_output::WlOutput,
  ) {
    // TODO
  }

  fn surface_leave(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _surface: &wl_surface::WlSurface,
    _output: &wl_output::WlOutput,
  ) {
    // TODO
  }
}

impl OutputHandler for Corona {
  fn output_state(&mut self) -> &mut OutputState {
    self.wayland.output_state_mut()
  }

  fn new_output(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _output: wl_output::WlOutput,
  ) {
    // TODO
  }

  fn update_output(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _output: wl_output::WlOutput,
  ) {
    // TODO
  }

  fn output_destroyed(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _output: wl_output::WlOutput,
  ) {
    // TODO
  }
}

impl LayerShellHandler for Corona {
  fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {}

  fn configure(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _layer: &LayerSurface,
    _configure: LayerSurfaceConfigure,
    _serial: u32,
  ) {
    // TODO
  }
}

impl SeatHandler for Corona {
  fn seat_state(&mut self) -> &mut SeatState {
    self.wayland.seat_state_mut()
  }

  fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {
    // TODO
  }

  fn new_capability(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _seat: wl_seat::WlSeat,
    _capability: Capability,
  ) {
    // TODO
  }

  fn remove_capability(
    &mut self,
    _conn: &Connection,
    _: &QueueHandle<Self>,
    _: wl_seat::WlSeat,
    _capability: Capability,
  ) {
    // TODO
  }

  fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Corona {
  fn enter(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    _: &wl_keyboard::WlKeyboard,
    _surface: &wl_surface::WlSurface,
    _: u32,
    _: &[u32],
    _keysyms: &[Keysym],
  ) {
    // TODO
  }

  fn leave(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    _: &wl_keyboard::WlKeyboard,
    _surface: &wl_surface::WlSurface,
    _: u32,
  ) {
    // TODO
  }

  fn press_key(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _: &wl_keyboard::WlKeyboard,
    _: u32,
    _event: KeyEvent,
  ) {
    // TODO
  }

  fn repeat_key(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _keyboard: &wl_keyboard::WlKeyboard,
    _serial: u32,
    _event: KeyEvent,
  ) {
    // TODO
  }

  fn release_key(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    _: &wl_keyboard::WlKeyboard,
    _: u32,
    _event: KeyEvent,
  ) {
    // TODO
  }

  fn update_modifiers(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    _: &wl_keyboard::WlKeyboard,
    _serial: u32,
    _modifiers: Modifiers,
    _raw_modifiers: RawModifiers,
    _layout: u32,
  ) {
    // TODO
  }
}

impl PointerHandler for Corona {
  fn pointer_frame(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _pointer: &wl_pointer::WlPointer,
    _events: &[PointerEvent],
  ) {
    // TODO
  }
}

impl ProvidesRegistryState for Corona {
  fn registry(&mut self) -> &mut RegistryState {
    self.wayland.registry_state_mut()
  }
  registry_handlers![OutputState, SeatState];
}

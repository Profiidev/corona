use anyhow::Result;
use smithay_client_toolkit::{
  compositor::CompositorHandler,
  delegate_dispatch2, delegate_registry,
  output::{OutputHandler, OutputState},
  registry::{ProvidesRegistryState, RegistryState},
  registry_handlers,
  seat::{
    Capability, SeatHandler, SeatState,
    keyboard::{KeyEvent, KeyboardHandler, Modifiers, RawModifiers},
    pointer::{PointerEvent, PointerHandler},
  },
  shell::wlr_layer::{LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
};
use wayland_client::{
  Connection, QueueHandle,
  protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
};
use xkbcommon::xkb::Keysym;

use crate::adapter::{slint::SlintCustomPlatform, wayland::WaylandAdapter};

pub struct Corona {
  wayland: WaylandAdapter<Self>,
}

impl Corona {
  pub fn init() -> Result<Self> {
    let wayland = WaylandAdapter::<Self>::init()?;
    let platform = SlintCustomPlatform::init()?;

    Ok(Self { wayland })
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
  }

  fn transform_changed(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _surface: &wl_surface::WlSurface,
    _new_transform: wl_output::Transform,
  ) {
  }

  fn frame(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _surface: &wl_surface::WlSurface,
    _time: u32,
  ) {
  }

  fn surface_enter(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _surface: &wl_surface::WlSurface,
    _output: &wl_output::WlOutput,
  ) {
  }

  fn surface_leave(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _surface: &wl_surface::WlSurface,
    _output: &wl_output::WlOutput,
  ) {
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
  }

  fn update_output(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _output: wl_output::WlOutput,
  ) {
  }

  fn output_destroyed(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _output: wl_output::WlOutput,
  ) {
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
  }
}

impl SeatHandler for Corona {
  fn seat_state(&mut self) -> &mut SeatState {
    self.wayland.seat_state_mut()
  }

  fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

  fn new_capability(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _seat: wl_seat::WlSeat,
    _capability: Capability,
  ) {
  }

  fn remove_capability(
    &mut self,
    _conn: &Connection,
    _: &QueueHandle<Self>,
    _: wl_seat::WlSeat,
    _capability: Capability,
  ) {
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
  }

  fn leave(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    _: &wl_keyboard::WlKeyboard,
    _surface: &wl_surface::WlSurface,
    _: u32,
  ) {
  }

  fn press_key(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _: &wl_keyboard::WlKeyboard,
    _: u32,
    _event: KeyEvent,
  ) {
  }

  fn repeat_key(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _keyboard: &wl_keyboard::WlKeyboard,
    _serial: u32,
    _event: KeyEvent,
  ) {
  }

  fn release_key(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    _: &wl_keyboard::WlKeyboard,
    _: u32,
    _event: KeyEvent,
  ) {
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
  }
}

impl ProvidesRegistryState for Corona {
  fn registry(&mut self) -> &mut RegistryState {
    self.wayland.registry_state_mut()
  }
  registry_handlers![OutputState, SeatState];
}

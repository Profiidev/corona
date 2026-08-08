use smithay_client_toolkit::seat::keyboard::{
  KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers,
};
use wayland_client::{
  Connection, QueueHandle,
  protocol::{wl_keyboard, wl_surface},
};

use crate::Corona;

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

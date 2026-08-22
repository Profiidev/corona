use std::rc::Rc;

use slint::{SharedString, platform::WindowEvent};
use smithay_client_toolkit::{
  delegate_dispatch2, delegate_registry,
  output::OutputState,
  registry::{ProvidesRegistryState, RegistryState},
  registry_handlers,
  seat::{
    Capability, SeatState,
    keyboard::{KeyEvent, repeat::RepeatCallback},
  },
};
use wayland_client::{
  backend::ObjectId,
  protocol::{wl_keyboard::WlKeyboard, wl_pointer::WlPointer, wl_seat::WlSeat},
};

use crate::{
  Corona, adapter::slint::SlintWindow, api::event::ShellEvent, wayland::keyboard::key_text,
};

mod compositor;
mod fractional_scale;
mod keyboard;
mod layer_shell;
mod output;
mod pointer;
mod seat;

pub struct Dispatcher {
  pub corona: Corona,
  pub registry_state: RegistryState,
  pub output_state: OutputState,
  pub seat_state: SeatState,
  pub keyboard: Option<WlKeyboard>,
  pub pointer: Option<WlPointer>,
}

impl Dispatcher {
  fn window_for(&self, id: &ObjectId) -> Option<Rc<SlintWindow>> {
    self.corona.widgets().window(id)
  }

  fn frame_done(&self, surface_id: &ObjectId) {
    self.corona.widgets().frame_done(surface_id);
  }

  fn set_scale(&self, surface_id: &ObjectId, scale: f64) {
    self.corona.widgets().set_scale(surface_id, scale);
  }

  fn keyboard_enter(&self, id: ObjectId) {
    if let Some(window) = self.window_for(&id) {
      window.dispatch(WindowEvent::WindowActiveChanged(true));
      self.corona.widgets().set_focus(Some(id));
    }
  }

  fn keyboard_leave(&self, id: ObjectId) {
    if let Some(window) = self.window_for(&id) {
      window.dispatch(WindowEvent::WindowActiveChanged(false));
    }
    self.corona.widgets().set_focus(None);
  }

  fn dispatch_key(&self, event: impl FnOnce(SharedString) -> WindowEvent, key: KeyEvent) {
    let Some(window) = self
      .corona
      .widgets()
      .focus()
      .and_then(|id| self.window_for(&id))
    else {
      return;
    };
    let Some(text) = key_text(&key) else {
      return;
    };

    window.dispatch(event(text));
  }

  fn repeat_callback() -> RepeatCallback<Self> {
    Box::new(|dispatcher, _keyboard, event| {
      dispatcher.dispatch_key(|text| WindowEvent::KeyPressRepeated { text }, event);
    })
  }

  fn dispatch_shell_event(&self, event: ShellEvent) {
    self.corona.handle_shell_event(event);
  }

  fn set_capability(&mut self, seat: &WlSeat, capability: Capability, available: bool) {
    let queue_handle = self.corona.wayland().queue_handle();

    match (capability, available) {
      (Capability::Keyboard, true) if self.keyboard.is_none() => {
        let loop_handle = self.corona.loop_handle().handle.clone();

        let keyboard = self.seat_state.get_keyboard_with_repeat(
          queue_handle,
          seat,
          None,
          loop_handle,
          Self::repeat_callback(),
        );

        match keyboard {
          Ok(keyboard) => self.keyboard = Some(keyboard),
          Err(e) => tracing::warn!("failed to bind keyboard: {e}"),
        }
      }
      (Capability::Pointer, true) if self.pointer.is_none() => {
        match self.seat_state.get_pointer(queue_handle, seat) {
          Ok(pointer) => self.pointer = Some(pointer),
          Err(e) => tracing::warn!("failed to bind pointer: {e}"),
        }
      }
      (Capability::Keyboard, false) => {
        if let Some(keyboard) = self.keyboard.take() {
          keyboard.release();
        }
      }
      (Capability::Pointer, false) => {
        if let Some(pointer) = self.pointer.take() {
          pointer.release();
        }
      }
      _ => {}
    }
  }
}

delegate_dispatch2!(Dispatcher);
delegate_registry!(Dispatcher);

impl ProvidesRegistryState for Dispatcher {
  fn registry(&mut self) -> &mut RegistryState {
    &mut self.registry_state
  }
  registry_handlers![OutputState, SeatState];
}

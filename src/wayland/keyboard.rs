use slint::{SharedString, platform::WindowEvent};
use smithay_client_toolkit::seat::keyboard::{
  KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers, repeat::RepeatCallback,
};
use wayland_client::{
  Connection, Proxy, QueueHandle,
  protocol::{wl_keyboard, wl_surface},
};

use crate::Corona;

impl KeyboardHandler for Corona {
  fn enter(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    _: &wl_keyboard::WlKeyboard,
    surface: &wl_surface::WlSurface,
    _: u32,
    _: &[u32],
    _keysyms: &[Keysym],
  ) {
    let id = surface.id();
    if let Some(window) = self.widgets.window(&id) {
      window.dispatch(WindowEvent::WindowActiveChanged(true));
      self.widgets.focus = Some(id);
    }
  }

  fn leave(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    _: &wl_keyboard::WlKeyboard,
    surface: &wl_surface::WlSurface,
    _: u32,
  ) {
    if let Some(window) = self.widgets.window(&surface.id()) {
      window.dispatch(WindowEvent::WindowActiveChanged(false));
    }
    self.widgets.focus = None;
  }

  fn press_key(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _: &wl_keyboard::WlKeyboard,
    _: u32,
    event: KeyEvent,
  ) {
    self.dispatch_key(|text| WindowEvent::KeyPressed { text }, event);
  }

  fn repeat_key(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _keyboard: &wl_keyboard::WlKeyboard,
    _serial: u32,
    event: KeyEvent,
  ) {
    self.dispatch_key(|text| WindowEvent::KeyPressRepeated { text }, event);
  }

  fn release_key(
    &mut self,
    _: &Connection,
    _: &QueueHandle<Self>,
    _: &wl_keyboard::WlKeyboard,
    _: u32,
    event: KeyEvent,
  ) {
    self.dispatch_key(|text| WindowEvent::KeyReleased { text }, event);
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
    // TODO ?
  }
}

impl Corona {
  pub(crate) fn repeat_callback() -> RepeatCallback<Self> {
    Box::new(|corona, _keyboard, event| {
      corona.dispatch_key(|text| WindowEvent::KeyPressRepeated { text }, event);
    })
  }

  fn dispatch_key(&self, event: impl FnOnce(SharedString) -> WindowEvent, key: KeyEvent) {
    let Some(window) = self
      .widgets
      .focus
      .as_ref()
      .and_then(|id| self.widgets.window(id))
    else {
      return;
    };
    let Some(text) = key_text(&key) else {
      return;
    };

    window.dispatch(event(text));
  }
}

/// Map a key to the unicode representation Slint expects: a `Key` code for the special keys,
/// the composed text otherwise.
fn key_text(key: &KeyEvent) -> Option<SharedString> {
  use slint::platform::Key;

  let special = match key.keysym {
    Keysym::BackSpace => Key::Backspace,
    Keysym::Tab => Key::Tab,
    Keysym::ISO_Left_Tab => Key::Backtab,
    Keysym::Return | Keysym::KP_Enter => Key::Return,
    Keysym::Escape => Key::Escape,
    Keysym::Delete => Key::Delete,
    Keysym::Insert => Key::Insert,
    Keysym::Home => Key::Home,
    Keysym::End => Key::End,
    Keysym::Page_Up => Key::PageUp,
    Keysym::Page_Down => Key::PageDown,
    Keysym::Menu => Key::Menu,
    Keysym::Up => Key::UpArrow,
    Keysym::Down => Key::DownArrow,
    Keysym::Left => Key::LeftArrow,
    Keysym::Right => Key::RightArrow,
    Keysym::Shift_L => Key::Shift,
    Keysym::Shift_R => Key::ShiftR,
    Keysym::Control_L => Key::Control,
    Keysym::Control_R => Key::ControlR,
    Keysym::Alt_L | Keysym::Alt_R => Key::Alt,
    Keysym::ISO_Level3_Shift | Keysym::Mode_switch => Key::AltGr,
    Keysym::Caps_Lock => Key::CapsLock,
    Keysym::Super_L => Key::Meta,
    Keysym::Super_R => Key::MetaR,
    Keysym::F1 => Key::F1,
    Keysym::F2 => Key::F2,
    Keysym::F3 => Key::F3,
    Keysym::F4 => Key::F4,
    Keysym::F5 => Key::F5,
    Keysym::F6 => Key::F6,
    Keysym::F7 => Key::F7,
    Keysym::F8 => Key::F8,
    Keysym::F9 => Key::F9,
    Keysym::F10 => Key::F10,
    Keysym::F11 => Key::F11,
    Keysym::F12 => Key::F12,
    // Release events carry no utf8, so fall back to the keysym's own character.
    _ => {
      return key
        .utf8
        .as_deref()
        .map(SharedString::from)
        .or_else(|| key.keysym.key_char().map(SharedString::from));
    }
  };

  Some(char::from(special).into())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn event(keysym: Keysym, utf8: Option<&str>) -> KeyEvent {
    KeyEvent {
      time: 0,
      raw_code: 0,
      keysym,
      utf8: utf8.map(String::from),
    }
  }

  #[test]
  fn maps_special_keys_and_text() {
    assert_eq!(
      key_text(&event(Keysym::Escape, None)).as_deref(),
      Some("\u{001b}")
    );
    // Text keys prefer the composed utf8 over the raw keysym.
    assert_eq!(key_text(&event(Keysym::a, Some("A"))).as_deref(), Some("A"));
    // Releases carry no utf8 but must still produce the character.
    assert_eq!(key_text(&event(Keysym::a, None)).as_deref(), Some("a"));
    assert_eq!(key_text(&event(Keysym::VoidSymbol, None)), None);
  }
}

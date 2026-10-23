//! Pastes by pressing Ctrl+V (Ctrl+Shift+V in terminals) on a virtual keyboard

use std::{io::Write, os::fd::AsFd, time::Duration};

use anyhow::{Context, Result};
use rustix::fs::{MemfdFlags, memfd_create};
use wayland_client::{
  Connection, Dispatch, Proxy, QueueHandle,
  globals::{GlobalListContents, registry_queue_init},
  protocol::{wl_keyboard::KeymapFormat, wl_registry, wl_seat::WlSeat},
};
use wayland_protocols_misc::zwp_virtual_keyboard_v1::client::{
  zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
  zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
};

/// Only the keys pasting needs; xkb keycodes are evdev codes plus 8
const KEYMAP: &str = r#"xkb_keymap {
  xkb_keycodes "corona" { minimum = 8; maximum = 255; <LCTL> = 37; <LFSH> = 50; <AB04> = 55; };
  xkb_types "corona" { include "complete" };
  xkb_compatibility "corona" { include "complete" };
  xkb_symbols "corona" {
    key <LCTL> { [ Control_L ] };
    key <LFSH> { [ Shift_L ] };
    key <AB04> { [ v, V ] };
    modifier_map Control { <LCTL> };
    modifier_map Shift { <LFSH> };
  };
};
"#;

const KEY_V: u32 = 47;
const SHIFT: u32 = 1;
const CONTROL: u32 = 4;
const PRESSED: u32 = 1;
const RELEASED: u32 = 0;

/// Window classes that paste with Ctrl+Shift+V
pub const TERMINALS: [&str; 12] = [
  "kitty",
  "foot",
  "footclient",
  "alacritty",
  "org.wezfurlong.wezterm",
  "com.mitchellh.ghostty",
  "org.kde.konsole",
  "org.gnome.terminal",
  "org.gnome.console",
  "xterm",
  "st",
  "tilix",
];

pub fn is_terminal(class: &str) -> bool {
  let class = class.to_lowercase();
  TERMINALS.contains(&class.as_str()) || class.contains("terminal")
}

struct State;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
  fn event(
    _: &mut Self,
    _: &wl_registry::WlRegistry,
    _: wl_registry::Event,
    _: &GlobalListContents,
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
  }
}

impl Dispatch<WlSeat, ()> for State {
  fn event(
    _: &mut Self,
    _: &WlSeat,
    _: <WlSeat as Proxy>::Event,
    _: &(),
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
  }
}

impl Dispatch<ZwpVirtualKeyboardManagerV1, ()> for State {
  fn event(
    _: &mut Self,
    _: &ZwpVirtualKeyboardManagerV1,
    _: <ZwpVirtualKeyboardManagerV1 as Proxy>::Event,
    _: &(),
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
  }
}

impl Dispatch<ZwpVirtualKeyboardV1, ()> for State {
  fn event(
    _: &mut Self,
    _: &ZwpVirtualKeyboardV1,
    _: <ZwpVirtualKeyboardV1 as Proxy>::Event,
    _: &(),
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
  }
}

/// Presses the paste keys in the focused window; blocks for a moment, run it off the UI
pub fn paste(terminal: bool) -> Result<()> {
  let conn = Connection::connect_to_env().context("no Wayland display")?;
  let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
  let qh = queue.handle();
  let seat: WlSeat = globals.bind(&qh, 1..=8, ())?;
  let manager: ZwpVirtualKeyboardManagerV1 = globals
    .bind(&qh, 1..=1, ())
    .context("the compositor has no virtual keyboard protocol")?;
  let keyboard = manager.create_virtual_keyboard(&seat, &qh, ());

  let fd = memfd_create("corona-keymap", MemfdFlags::CLOEXEC)?;
  let mut file = std::fs::File::from(fd);
  file.write_all(KEYMAP.as_bytes())?;
  file.write_all(&[0])?;
  keyboard.keymap(
    KeymapFormat::XkbV1.into(),
    file.as_fd(),
    KEYMAP.len() as u32 + 1,
  );
  queue.roundtrip(&mut State)?;

  let mods = CONTROL | if terminal { SHIFT } else { 0 };
  let time = || {
    std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .map_or(0, |d| d.as_millis() as u32)
  };
  keyboard.modifiers(mods, 0, 0, 0);
  keyboard.key(time(), KEY_V, PRESSED);
  queue.roundtrip(&mut State)?;
  std::thread::sleep(Duration::from_millis(10));
  keyboard.key(time(), KEY_V, RELEASED);
  keyboard.modifiers(0, 0, 0, 0);
  queue.roundtrip(&mut State)?;
  keyboard.destroy();
  queue.roundtrip(&mut State)?;
  Ok(())
}

#[cfg(test)]
mod tests {
  #[test]
  fn terminals() {
    assert!(super::is_terminal("kitty"));
    assert!(super::is_terminal("com.mitchellh.ghostty"));
    assert!(super::is_terminal("my-terminal-app"));
    assert!(!super::is_terminal("firefox"));
  }
}

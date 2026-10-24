//! The virtual keyboard's keymap, and which key types which character on it

use std::collections::HashMap;

use anyhow::{Context, Result};
use bitflags::bitflags;
use xkbcommon::xkb;

bitflags! {
  /// Modifiers held with a key
  #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
  pub struct Mods: u8 {
    const SHIFT = 1;
    const CTRL = 1 << 1;
    const ALT = 1 << 2;
    const SUPER = 1 << 3;
    const ALTGR = 1 << 4;
  }
}

/// xkb names of [`Mods`], in flag order
const MOD_NAMES: [(Mods, &str); 5] = [
  (Mods::SHIFT, xkb::MOD_NAME_SHIFT),
  (Mods::CTRL, xkb::MOD_NAME_CTRL),
  (Mods::ALT, xkb::MOD_NAME_ALT),
  (Mods::SUPER, xkb::MOD_NAME_LOGO),
  (Mods::ALTGR, "Mod5"),
];

/// xkb keycodes are evdev codes plus 8
const EVDEV_OFFSET: u32 = 8;

pub struct Keymap {
  /// The keymap as xkb text, what the compositor is sent
  pub text: String,
  /// Each character the keymap types: (evdev code, modifiers), the simplest first
  chars: HashMap<char, (u32, Mods)>,
  /// The xkb modifier mask of each of [`MOD_NAMES`]
  masks: [u32; 5],
}

impl Keymap {
  /// Compiled from an xkb layout name like `us` or `de`
  pub fn new(layout: &str) -> Result<Self> {
    let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
    let keymap = xkb::Keymap::new_from_names(
      &context,
      "",
      "",
      layout,
      "",
      None,
      xkb::KEYMAP_COMPILE_NO_FLAGS,
    )
    .with_context(|| format!("no xkb layout {layout}"))?;
    let masks = MOD_NAMES.map(|(_, name)| match keymap.mod_get_index(name) {
      xkb::MOD_INVALID => 0,
      index => 1 << index,
    });

    let mut state = xkb::State::new(&keymap);
    let mut chars = HashMap::new();
    let levels = [
      Mods::empty(),
      Mods::SHIFT,
      Mods::ALTGR,
      Mods::SHIFT | Mods::ALTGR,
    ];
    for mods in levels {
      state.update_mask(mask(&masks, mods), 0, 0, 0, 0, 0);
      keymap.key_for_each(|_, code| {
        let typed = char::from_u32(state.key_get_utf32(code)).filter(|c| !c.is_control());
        if let Some(c) = typed
          && code.raw() >= EVDEV_OFFSET
        {
          chars.entry(c).or_insert((code.raw() - EVDEV_OFFSET, mods));
        }
      });
    }
    Ok(Self {
      text: keymap.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1),
      chars,
      masks,
    })
  }

  /// The key and modifiers that type `c`, none when the layout has no key for it
  pub fn key_for(&self, c: char) -> Option<(u32, Mods)> {
    self.chars.get(&c).copied()
  }

  /// The xkb modifier mask the compositor is sent for `mods`
  pub fn mask(&self, mods: Mods) -> u32 {
    mask(&self.masks, mods)
  }
}

fn mask(masks: &[u32; 5], mods: Mods) -> u32 {
  MOD_NAMES
    .iter()
    .zip(masks)
    .filter(|((m, _), _)| mods.contains(*m))
    .map(|(_, mask)| mask)
    .sum()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn finds_the_keys_of_a_layout() {
    let de = Keymap::new("de").unwrap();
    // evdev KEY_SEMICOLON, KEY_Q, KEY_Z
    assert_eq!(de.key_for('ö'), Some((39, Mods::empty())));
    assert_eq!(de.key_for('@'), Some((16, Mods::ALTGR)));
    assert_eq!(de.key_for('Z'), Some((21, Mods::SHIFT)));
    assert_eq!(de.key_for(' '), Some((57, Mods::empty())));
    assert_eq!(de.key_for('¿'), Some((12, Mods::ALTGR | Mods::SHIFT)));

    let us = Keymap::new("us").unwrap();
    assert_eq!(us.key_for('z'), Some((44, Mods::empty())));
    assert_eq!(us.key_for('ö'), None);
    assert_eq!(us.mask(Mods::SHIFT | Mods::CTRL), 1 | 4);
    assert!(us.text.starts_with("xkb_keymap"));
  }
}

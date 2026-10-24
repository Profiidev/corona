//! The key matrices of the handoff: every row is 15 units wide

use crate::predict::Language;

/// Units every row adds up to, so the right edges line up
pub const ROW_UNITS: f32 = 15.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
  Qwerty,
  Qwertz,
}

impl Layout {
  /// The xkb layout the virtual keyboard sends with
  pub fn xkb(self) -> &'static str {
    match self {
      Layout::Qwerty => "us",
      Layout::Qwertz => "de",
    }
  }

  /// From the compositor's layout name, like `German` or `English (US)`
  pub fn from_compositor(name: Option<&str>) -> Self {
    // ponytail: only German maps to QWERTZ, AZERTY and others get QWERTY
    match name {
      Some(n) if n.starts_with("German") => Layout::Qwertz,
      _ => Layout::Qwerty,
    }
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
  Letters,
  Symbols,
}

/// Keys that are no character
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Special {
  Backspace,
  Tab,
  Caps,
  Shift,
  Enter,
  Delete,
  Escape,
  Ctrl,
  Super,
  Alt,
  AltGr,
  Space,
  Left,
  Right,
  Up,
  Down,
  /// F1 to F12
  F(u8),
  /// Switches between letters and symbols
  SwitchLayer,
}

impl Special {
  /// The Linux evdev code it sends, none for the ones handled here
  pub fn evdev(self) -> Option<u32> {
    Some(match self {
      Special::Escape => 1,
      Special::Backspace => 14,
      Special::Tab => 15,
      Special::Enter => 28,
      Special::Space => 57,
      Special::F(n @ 1..=10) => 58 + n as u32,
      Special::F(11) => 87,
      Special::F(12) => 88,
      Special::Up => 103,
      Special::Left => 105,
      Special::Right => 106,
      Special::Down => 108,
      Special::Delete => 111,
      _ => return None,
    })
  }

  /// Held to repeat
  pub fn repeats(self) -> bool {
    matches!(
      self,
      Special::Backspace
        | Special::Delete
        | Special::Space
        | Special::Left
        | Special::Right
        | Special::Up
        | Special::Down
    )
  }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
  Char {
    base: char,
    shifted: char,
    altgr: Option<char>,
  },
  Special(Special),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
  pub kind: Kind,
  pub width: f32,
}

impl Key {
  /// A letter: Caps changes it, and its shifted form is its upper case
  pub fn is_letter(&self) -> bool {
    match self.kind {
      Kind::Char { base, shifted, .. } => {
        base.is_alphabetic() && base.to_uppercase().eq(std::iter::once(shifted))
      }
      Kind::Special(_) => false,
    }
  }
}

/// `"q"` is a letter, `"1!"` a symbol key with its shifted character, `"7/{"`
/// adds the AltGr one
fn chars(spec: &str) -> Key {
  let mut c = spec.chars();
  let base = c.next().expect("a key has a character");
  let shifted = c
    .next()
    .unwrap_or_else(|| base.to_uppercase().next().unwrap_or(base));
  Key {
    kind: Kind::Char {
      base,
      shifted,
      altgr: c.next(),
    },
    width: 1.,
  }
}

fn row(specs: &str) -> Vec<Key> {
  specs.split(' ').map(chars).collect()
}

fn special(special: Special, width: f32) -> Key {
  Key {
    kind: Kind::Special(special),
    width,
  }
}

fn wrap(left: Key, mut keys: Vec<Key>, right: Key) -> Vec<Key> {
  keys.insert(0, left);
  keys.push(right);
  keys
}

/// The rows of a layer, top to bottom
pub fn rows(layout: Layout, layer: Layer) -> Vec<Vec<Key>> {
  use Special::*;
  let alt = match layout {
    Layout::Qwerty => Alt,
    Layout::Qwertz => AltGr,
  };
  let bottom = [
    special(SwitchLayer, 1.),
    special(Ctrl, 1.),
    special(Super, 1.),
    special(Alt, 1.),
    special(Space, 6.),
    special(alt, 1.),
    special(Left, 1.),
    special(Up, 1.),
    special(Down, 1.),
    special(Right, 1.),
  ]
  .to_vec();
  let mut rows = match (layer, layout) {
    (Layer::Symbols, _) => vec![
      wrap(
        special(Escape, 1.),
        (1..=12).map(|n| special(F(n), 1.)).collect(),
        special(Backspace, 2.),
      ),
      wrap(
        special(Tab, 2.),
        row("@ # € % & * - + ( ) /"),
        special(Delete, 2.),
      ),
      [row("! \" ' : ; ? ~ | \\ _ ="), vec![special(Enter, 4.)]].concat(),
      row("[ ] { } < > ^ ` $ £ ¥ § ° ² µ"),
    ],
    (Layer::Letters, Layout::Qwerty) => vec![
      [
        row("`~ 1! 2@ 3# 4$ 5% 6^ 7& 8* 9( 0) -_ =+"),
        vec![special(Backspace, 2.)],
      ]
      .concat(),
      wrap(
        special(Tab, 1.5),
        row("q w e r t y u i o p [{ ]}"),
        Key {
          width: 1.5,
          ..chars("\\|")
        },
      ),
      wrap(
        special(Caps, 1.75),
        row("a s d f g h j k l ;: '\""),
        special(Enter, 2.25),
      ),
      wrap(
        special(Shift, 2.25),
        row("z x c v b n m ,< .> /?"),
        special(Shift, 2.75),
      ),
    ],
    (Layer::Letters, Layout::Qwertz) => vec![
      [
        row("^° 1! 2\"² 3§³ 4$ 5% 6& 7/{ 8([ 9)] 0=} ß?\\ ´`"),
        vec![special(Backspace, 2.)],
      ]
      .concat(),
      wrap(
        special(Tab, 1.5),
        row("qQ@ w eE€ r t z u i o p ü +*~"),
        special(Delete, 1.5),
      ),
      wrap(
        special(Caps, 1.5),
        row("a s d f g h j k l ö ä #'"),
        special(Enter, 1.5),
      ),
      wrap(
        special(Shift, 1.25),
        row("<>| y x c v b n mMµ ,; .: -_"),
        special(Shift, 2.75),
      ),
    ],
  };
  rows.push(bottom);
  rows
}

/// Long-press alternates of a key's base character; they follow the language,
/// not the layout
pub fn alternates(language: Language, base: char) -> &'static str {
  match (language, base) {
    (Language::De, 'a') => "äáàâã",
    (Language::De, 'o') => "öóòôõ",
    (Language::De, 'u') => "üúùû",
    (Language::De, 's') => "ß§",
    (Language::En, 'a') => "áàâäã",
    (Language::En, 'o') => "óòôöõ",
    (Language::En, 'u') => "úùûü",
    (Language::En, 's') => "ß$",
    (_, 'e') => "éèêë€",
    (_, 'i') => "íìîï",
    (_, 'c') => "ç",
    (_, 'n') => "ñ",
    (_, '.') => "…:",
    (_, '-') => "–—",
    _ => "",
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn every_row_is_fifteen_units() {
    for layout in [Layout::Qwerty, Layout::Qwertz] {
      for layer in [Layer::Letters, Layer::Symbols] {
        for (i, row) in rows(layout, layer).iter().enumerate() {
          let units: f32 = row.iter().map(|k| k.width).sum();
          assert_eq!(units, ROW_UNITS, "{layout:?} {layer:?} row {i}");
        }
      }
    }
  }

  #[test]
  fn letters_and_symbol_keys() {
    let rows = rows(Layout::Qwertz, Layer::Letters);
    let q = rows[1][1];
    assert!(q.is_letter());
    assert_eq!(
      q.kind,
      Kind::Char {
        base: 'q',
        shifted: 'Q',
        altgr: Some('@')
      }
    );
    let sz = rows[0][11];
    assert!(!sz.is_letter());
    assert_eq!(
      sz.kind,
      Kind::Char {
        base: 'ß',
        shifted: '?',
        altgr: Some('\\')
      }
    );
    assert!(rows[2][10].is_letter(), "ö");
  }

  #[test]
  fn alternates_follow_the_language() {
    assert!(alternates(Language::De, 'a').starts_with('ä'));
    assert!(alternates(Language::En, 'a').starts_with('á'));
    assert_eq!(alternates(Language::En, 'q'), "");
  }

  #[test]
  fn compositor_names_pick_the_layout() {
    assert_eq!(Layout::from_compositor(Some("German")), Layout::Qwertz);
    assert_eq!(
      Layout::from_compositor(Some("English (US)")),
      Layout::Qwerty
    );
    assert_eq!(Layout::from_compositor(None), Layout::Qwerty);
  }
}

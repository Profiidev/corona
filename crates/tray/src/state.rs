use std::path::PathBuf;

use zbus::zvariant::OwnedObjectPath;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
  Passive,
  #[default]
  Active,
  NeedsAttention,
}

impl From<&str> for Status {
  fn from(value: &str) -> Self {
    match value {
      "Passive" => Self::Passive,
      "NeedsAttention" => Self::NeedsAttention,
      _ => Self::Active,
    }
  }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Category {
  #[default]
  ApplicationStatus,
  Communications,
  SystemServices,
  Hardware,
}

impl From<&str> for Category {
  fn from(value: &str) -> Self {
    match value {
      "Communications" => Self::Communications,
      "SystemServices" => Self::SystemServices,
      "Hardware" => Self::Hardware,
      _ => Self::ApplicationStatus,
    }
  }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TrayItem {
  pub address: String,
  pub id: String,
  pub title: Option<String>,
  pub status: Status,
  pub category: Category,
  pub icon: Option<PathBuf>,
  pub tooltip: Option<String>,
  pub item_is_menu: bool,
  pub can_activate: bool,
  pub menu: Vec<MenuItem>,
  pub(crate) menu_path: Option<OwnedObjectPath>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Toggle {
  #[default]
  None,
  Checkmark(bool),
  Radio(bool),
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuItem {
  pub id: i32,
  pub label: String,
  pub separator: bool,
  pub enabled: bool,
  pub visible: bool,
  pub toggle: Toggle,
  pub icon_name: Option<String>,
  pub children: Vec<MenuItem>,
}

pub(crate) fn strip_mnemonic(label: &str) -> String {
  let mut out = String::with_capacity(label.len());
  let mut chars = label.chars().peekable();
  while let Some(c) = chars.next() {
    if c == '_' {
      if chars.peek() == Some(&'_') {
        chars.next();
        out.push('_');
      }
    } else {
      out.push(c);
    }
  }
  out
}

pub(crate) fn argb_to_rgba(pixels: &mut [u8]) {
  for px in pixels.as_chunks_mut::<4>().0 {
    px.rotate_left(1);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn mnemonic() {
    assert_eq!(strip_mnemonic("_Quit"), "Quit");
    assert_eq!(strip_mnemonic("snake__case"), "snake_case");
    assert_eq!(strip_mnemonic("Open _File__x"), "Open File_x");
  }

  #[test]
  fn pixmap() {
    let mut px = [0xff, 1, 2, 3];
    argb_to_rgba(&mut px);
    assert_eq!(px, [1, 2, 3, 0xff]);
  }

  #[test]
  fn mnemonic_edges() {
    assert_eq!(strip_mnemonic(""), "");
    assert_eq!(strip_mnemonic("a_"), "a");
    assert_eq!(strip_mnemonic("___"), "_");
    assert_eq!(strip_mnemonic("____"), "__");
    assert_eq!(strip_mnemonic("Über_ändern"), "Überändern");
    assert_eq!(strip_mnemonic("no mnemonic"), "no mnemonic");
  }

  #[test]
  fn pixmap_edges() {
    let mut empty: [u8; 0] = [];
    argb_to_rgba(&mut empty);
    // a partial pixel at the end is left alone
    let mut px = [0xff, 1, 2, 3, 9, 8];
    argb_to_rgba(&mut px);
    assert_eq!(px, [1, 2, 3, 0xff, 9, 8]);
  }

  #[test]
  fn statuses_and_categories() {
    assert_eq!(Status::from("Passive"), Status::Passive);
    assert_eq!(Status::from("NeedsAttention"), Status::NeedsAttention);
    assert_eq!(Status::from("Active"), Status::Active);
    assert_eq!(Status::from("passive"), Status::Active);
    assert_eq!(Status::from(""), Status::default());
    assert_eq!(Category::from("Communications"), Category::Communications);
    assert_eq!(Category::from("SystemServices"), Category::SystemServices);
    assert_eq!(Category::from("Hardware"), Category::Hardware);
    assert_eq!(Category::from("Other"), Category::ApplicationStatus);
    assert_eq!(Toggle::default(), Toggle::None);
  }
}

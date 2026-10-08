#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayKind {
  Backlight,
  External,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unavailable {
  DdcutilDisabled,
  DdcutilMissing,
  Detecting,
  Unsupported,
  Failed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Display {
  pub id: String,
  pub output: Option<String>,
  /// The model, `None` when the display does not tell; the shell then names it by kind
  pub name: Option<String>,
  pub kind: DisplayKind,
  pub brightness: u32,
  pub max: u32,
  pub unavailable: Option<Unavailable>,
}

impl Display {
  pub fn percent(&self) -> f32 {
    if self.max == 0 {
      return 0.;
    }
    self.brightness as f32 / self.max as f32 * 100.
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn percent() {
    let display = |brightness, max| Display {
      id: String::new(),
      output: None,
      name: None,
      kind: DisplayKind::Backlight,
      brightness,
      max,
      unavailable: None,
    };
    assert_eq!(display(0, 0).percent(), 0.);
    assert_eq!(display(5, 0).percent(), 0.);
    assert_eq!(display(0, 255).percent(), 0.);
    assert_eq!(display(255, 255).percent(), 100.);
    assert!((display(1, 3).percent() - 100. / 3.).abs() < 1e-4);
    assert_eq!(display(u32::MAX, u32::MAX).percent(), 100.);
  }
}

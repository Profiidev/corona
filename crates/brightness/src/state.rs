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
  pub name: String,
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

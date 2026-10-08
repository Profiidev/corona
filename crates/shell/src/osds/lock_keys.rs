use std::{fs, path::Path, time::Duration};

use clap::ValueEnum;
use gpui_kit::{App, assets::IconName};
use serde::{Deserialize, Serialize};

use crate::osds::view::{ToggleOsd, show};
use rust_i18n::t;

const LEDS: &str = "/sys/class/leds";
const SETTLE: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum LockKey {
  Caps,
  Num,
  Scroll,
}

impl LockKey {
  /// The suffix of its LED, like `input3::capslock`
  fn led(self) -> &'static str {
    match self {
      LockKey::Caps => "::capslock",
      LockKey::Num => "::numlock",
      LockKey::Scroll => "::scrolllock",
    }
  }
}

/// Whether any keyboard's LED for `key` is lit, `None` without such an LED.
/// The compositor sets every keyboard's LEDs from one state.
fn locked(leds: &Path, key: LockKey) -> Option<bool> {
  let mut found = None;
  for entry in fs::read_dir(leds).ok()?.flatten() {
    if !entry.file_name().to_string_lossy().ends_with(key.led()) {
      continue;
    }
    let Ok(brightness) = fs::read_to_string(entry.path().join("brightness")) else {
      continue;
    };
    let lit = brightness.trim().parse::<u32>().is_ok_and(|b| b > 0);
    found = Some(found.unwrap_or(false) || lit);
  }
  found
}

/// Shows whether `key` is on now that it was pressed
pub fn pressed(key: LockKey, cx: &mut App) {
  cx.spawn(async move |cx| {
    cx.background_executor().timer(SETTLE).await;
    let Some(on) = locked(Path::new(LEDS), key) else {
      tracing::debug!("no LED for {key:?}, its state is unknown");
      return;
    };
    cx.update(|cx| {
      let (icon, label) = match key {
        LockKey::Caps => (IconName::CaseUpper, t!("app.osd.caps_lock")),
        LockKey::Num => (IconName::Hash, t!("app.osd.num_lock")),
        LockKey::Scroll => (IconName::ArrowDownToLine, t!("app.osd.scroll_lock")),
      };
      show(|k| k.lock_keys, ToggleOsd::on_off(icon, label, on), cx);
    });
  })
  .detach();
}

#[cfg(test)]
mod tests {
  use std::fs;

  use super::{LockKey, locked};

  #[test]
  fn reads_leds() {
    let dir = std::env::temp_dir().join(format!("corona-leds-{}", std::process::id()));
    let led = |name: &str, brightness: &str| {
      let path = dir.join(name);
      fs::create_dir_all(&path).unwrap();
      fs::write(path.join("brightness"), brightness).unwrap();
    };
    led("input3::capslock", "0\n");
    led("input9::capslock", "1\n");
    led("input3::numlock", "0\n");
    led("phy0-led", "1\n");

    assert_eq!(locked(&dir, LockKey::Caps), Some(true));
    assert_eq!(locked(&dir, LockKey::Num), Some(false));
    assert_eq!(locked(&dir, LockKey::Scroll), None);
    fs::remove_dir_all(&dir).unwrap();
  }
}

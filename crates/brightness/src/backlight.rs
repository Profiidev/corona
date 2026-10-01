//! Laptop panels: read from sysfs, written through logind so no root or udev rule is needed.

use std::{
  fs,
  path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use zbus::Connection;

use crate::state::{Display, DisplayKind};

pub(crate) const BACKLIGHT: &str = "/sys/class/backlight";
pub(crate) const DRM: &str = "/sys/class/drm";

#[zbus::proxy(
  interface = "org.freedesktop.login1.Session",
  default_service = "org.freedesktop.login1",
  default_path = "/org/freedesktop/login1/session/auto"
)]
trait Session {
  fn set_brightness(&self, subsystem: &str, name: &str, brightness: u32) -> zbus::Result<()>;
}

pub(crate) fn list(backlight_dir: &Path, drm_dir: &Path) -> Vec<Display> {
  let mut displays: Vec<Display> = fs::read_dir(backlight_dir)
    .into_iter()
    .flatten()
    .flatten()
    .filter_map(|entry| {
      let name = entry.file_name().into_string().ok()?;
      let read = |file: &str| -> Option<u32> {
        fs::read_to_string(entry.path().join(file))
          .ok()?
          .trim()
          .parse()
          .ok()
      };
      Some(Display {
        id: format!("backlight/{name}"),
        output: output(drm_dir, &name),
        name: "Built-in display".into(),
        kind: DisplayKind::Backlight,
        brightness: read("brightness")?,
        max: read("max_brightness")?,
        unavailable: None,
      })
    })
    .collect();
  displays.sort_by(|a, b| a.id.cmp(&b.id));
  displays
}

fn output(drm_dir: &Path, backlight: &str) -> Option<String> {
  fs::read_dir(drm_dir)
    .ok()?
    .flatten()
    .find(|connector| connector.path().join(backlight).exists())
    .and_then(|connector| {
      let name = connector.file_name().into_string().ok()?;
      Some(name.split_once('-')?.1.to_string())
    })
}

pub(crate) async fn set(conn: &Connection, name: &str, brightness: u32) -> Result<()> {
  let logind = async {
    let session = SessionProxy::new(conn).await?;
    session.set_brightness("backlight", name, brightness).await
  };
  if let Err(error) = logind.await {
    tracing::debug!("logind refused the brightness ({error}), writing sysfs directly");
    let path = PathBuf::from(BACKLIGHT).join(name).join("brightness");
    fs::write(&path, brightness.to_string())
      .with_context(|| format!("writing {}", path.display()))?;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn list() {
    let root = std::env::temp_dir().join(format!("corona-backlight-{}", std::process::id()));
    fs::remove_dir_all(&root).ok();
    let backlight = root.join("backlight/intel_backlight");
    fs::create_dir_all(&backlight).unwrap();
    fs::write(backlight.join("brightness"), "4800\n").unwrap();
    fs::write(backlight.join("max_brightness"), "19200\n").unwrap();
    // the connector lists the backlight it drives
    fs::create_dir_all(root.join("drm/card1-eDP-1/intel_backlight")).unwrap();
    fs::create_dir_all(root.join("drm/card1-DP-1")).unwrap();

    let displays = super::list(&root.join("backlight"), &root.join("drm"));
    assert_eq!(displays.len(), 1);
    assert_eq!(displays[0].id, "backlight/intel_backlight");
    assert_eq!(displays[0].output.as_deref(), Some("eDP-1"));
    assert_eq!(displays[0].percent(), 25.);
    fs::remove_dir_all(root).ok();
  }
}

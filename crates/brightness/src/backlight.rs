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

/// `path` under sysfs; tests point `CORONA_TEST_SYSFS` at a fixture tree instead
pub(crate) fn sys(path: &str) -> PathBuf {
  #[cfg(test)]
  if let Some(root) = std::env::var_os("CORONA_TEST_SYSFS") {
    return Path::new(&root).join(path.trim_start_matches("/sys/"));
  }
  PathBuf::from(path)
}

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
        name: None,
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
    let path = sys(BACKLIGHT).join(name).join("brightness");
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

  #[test]
  fn list_edges() {
    let root = tempfile::tempdir().unwrap();
    let write = |file: &str, value: &str| {
      let path = root.path().join(file);
      fs::create_dir_all(path.parent().unwrap()).unwrap();
      fs::write(path, value).unwrap();
    };
    write("backlight/b/brightness", " 7 \n");
    write("backlight/b/max_brightness", "10");
    write("backlight/a/brightness", "1");
    write("backlight/a/max_brightness", "2");
    // unreadable values: skipped
    write("backlight/bad/brightness", "lots");
    write("backlight/bad/max_brightness", "10");
    write("backlight/half/brightness", "1");
    // a connector without a dash names no output
    fs::create_dir_all(root.path().join("drm/weird/b")).unwrap();
    let displays = super::list(&root.path().join("backlight"), &root.path().join("drm"));
    let summary: Vec<_> = displays
      .iter()
      .map(|d| (d.id.as_str(), d.output.as_deref(), d.brightness, d.max))
      .collect();
    assert_eq!(
      summary,
      [("backlight/a", None, 1, 2), ("backlight/b", None, 7, 10)]
    );
    assert!(super::list(&root.path().join("missing"), &root.path().join("missing")).is_empty());
  }

  #[test]
  fn sys_follows_the_test_root() {
    unsafe { std::env::remove_var("CORONA_TEST_SYSFS") };
    assert_eq!(sys(BACKLIGHT), Path::new(BACKLIGHT));
    unsafe { std::env::set_var("CORONA_TEST_SYSFS", "/fixture") };
    assert_eq!(sys(BACKLIGHT), Path::new("/fixture/class/backlight"));
    assert_eq!(sys(DRM), Path::new("/fixture/class/drm"));
  }
}

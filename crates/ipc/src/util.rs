use std::{
  fs::{self, DirBuilder},
  os::unix::fs::{DirBuilderExt, MetadataExt},
  path::{Path, PathBuf},
};

use anyhow::{Result, ensure};

fn socket_file_name() -> String {
  if let Ok(display) = std::env::var("WAYLAND_DISPLAY") {
    format!("corona-{}.sock", display)
  } else {
    "corona.sock".to_string()
  }
}

pub fn socket_path() -> PathBuf {
  if let Ok(path) = std::env::var("CORONA_SOCKET") {
    return PathBuf::from(path);
  }
  let file_name = socket_file_name();
  if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
    PathBuf::from(runtime_dir).join(file_name)
  } else {
    fallback_dir().join(file_name)
  }
}

/// `/tmp/corona-<uid>`: `/tmp` itself is anyone's to squat a socket in
fn fallback_dir() -> PathBuf {
  PathBuf::from(format!(
    "/tmp/corona-{}",
    rustix::process::getuid().as_raw()
  ))
}

/// Creates the fallback directory of `socket` and refuses one that someone
/// else could have planted or can enter
pub(crate) fn private_dir(socket: &Path) -> Result<()> {
  let dir = fallback_dir();
  if socket.parent() != Some(dir.as_path()) {
    return Ok(());
  }
  if let Err(e) = DirBuilder::new().mode(0o700).create(&dir) {
    ensure!(e.kind() == std::io::ErrorKind::AlreadyExists, e);
  }
  let meta = fs::symlink_metadata(&dir)?;
  ensure!(
    meta.is_dir() && meta.uid() == rustix::process::getuid().as_raw() && meta.mode() & 0o077 == 0,
    "{} is not a private directory of this user",
    dir.display()
  );
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn path_from_env() {
    let cases = [
      (
        Some("wayland-1"),
        Some("/run/user/1"),
        "/run/user/1/corona-wayland-1.sock",
      ),
      (None, Some("/run/user/1"), "/run/user/1/corona.sock"),
      (Some("wayland-1"), None, "corona-wayland-1.sock"),
      (None, None, "corona.sock"),
    ];
    unsafe { std::env::remove_var("CORONA_SOCKET") };
    for (display, runtime, want) in cases {
      unsafe {
        match display {
          Some(d) => std::env::set_var("WAYLAND_DISPLAY", d),
          None => std::env::remove_var("WAYLAND_DISPLAY"),
        }
        match runtime {
          Some(r) => std::env::set_var("XDG_RUNTIME_DIR", r),
          None => std::env::remove_var("XDG_RUNTIME_DIR"),
        }
      }
      let want = match runtime {
        Some(_) => PathBuf::from(want),
        None => fallback_dir().join(want),
      };
      assert_eq!(socket_path(), want);
    }
    unsafe { std::env::set_var("CORONA_SOCKET", "/tmp/dev.sock") };
    assert_eq!(socket_path(), PathBuf::from("/tmp/dev.sock"));
  }

  #[test]
  fn fallback_dir_is_private() {
    let socket = fallback_dir().join("x.sock");
    private_dir(&socket).unwrap();
    let mode = fs::metadata(fallback_dir()).unwrap().mode();
    assert_eq!(mode & 0o777, 0o700);
    // anywhere else is left alone
    assert!(private_dir(Path::new("/nonexistent/x.sock")).is_ok());
  }
}

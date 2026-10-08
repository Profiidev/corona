use std::path::PathBuf;

fn socket_file_name() -> String {
  if let Ok(display) = std::env::var("WAYLAND_DISPLAY") {
    format!("corona-{}.sock", display)
  } else {
    "corona.sock".to_string()
  }
}

pub fn socket_path() -> PathBuf {
  let file_name = socket_file_name();
  if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
    PathBuf::from(runtime_dir).join(file_name)
  } else {
    PathBuf::from("/tmp").join(file_name)
  }
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
      (Some("wayland-1"), None, "/tmp/corona-wayland-1.sock"),
      (None, None, "/tmp/corona.sock"),
    ];
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
      assert_eq!(socket_path(), PathBuf::from(want));
    }
  }
}

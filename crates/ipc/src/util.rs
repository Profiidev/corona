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

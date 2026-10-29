use std::path::PathBuf;

use anyhow::Result;
use gpui_kit::{App, ImageSource};

pub mod icons;
mod theme;
mod toolkits;

pub use icons::Assets;
pub use theme::{
  names as theme_names, preview_theme, restore_theme, set_mode, set_theme, theme_font, toggle_mode,
};

pub fn load(cx: &mut App) -> Result<()> {
  theme::load(cx)
}

/// A picture setting: an http(s) URL, or a path where `~/` is the home directory
pub fn image_source(path: &str) -> ImageSource {
  if path.starts_with("http://") || path.starts_with("https://") {
    return path.into();
  }
  match (path.strip_prefix("~/"), dirs::home_dir()) {
    (Some(rest), Some(home)) => home.join(rest).into(),
    _ => PathBuf::from(path).into(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use gpui_kit::Resource;
  use std::path::Path;

  fn path(source: ImageSource) -> PathBuf {
    match source {
      ImageSource::Resource(Resource::Path(p)) => p.to_path_buf(),
      _ => panic!("not a path"),
    }
  }

  #[test]
  fn urls_pass_through() {
    for url in ["http://x.org/a.png", "https://x.org/a.png"] {
      match image_source(url) {
        ImageSource::Resource(Resource::Uri(u)) => assert_eq!(u.as_ref(), url),
        _ => panic!("{url} is not a uri"),
      }
    }
  }

  #[test]
  fn home_is_expanded() {
    let home = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", home.path()) };
    assert_eq!(
      path(image_source("~/walls/a.png")),
      home.path().join("walls/a.png")
    );
    // only `~/` is expanded
    assert_eq!(path(image_source("~user/a.png")), Path::new("~user/a.png"));
    assert_eq!(path(image_source("~")), Path::new("~"));
  }

  #[test]
  fn plain_paths_stay() {
    assert_eq!(
      path(image_source("/usr/share/a.png")),
      Path::new("/usr/share/a.png")
    );
    assert_eq!(path(image_source("a.png")), Path::new("a.png"));
    // not http(s), so a path
    assert_eq!(
      path(image_source("ftp://x/a.png")),
      Path::new("ftp://x/a.png")
    );
  }
}

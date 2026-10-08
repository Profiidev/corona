use std::path::PathBuf;

use anyhow::Result;
use corona_config::{APP_NAME, ConfigProvider, observe_section};
use corona_surface::per_display::PerDisplay;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, Context, DisplayId, Entity, Global, ImageSource,
  IntoElement, ObjectFit, ParentElement, Render, Size, Styled, StyledImage, Window,
  WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind, WindowOptions,
  base::Root,
  div, img,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};
use tracing::error;

const NAMESPACE: &str = "corona_wallpaper";

struct Wallpapers {
  displays: Option<Entity<PerDisplay>>,
}

impl Global for Wallpapers {}

pub fn init(cx: &mut App) {
  cx.set_global(Wallpapers { displays: None });
  open(cx);
  observe_section(
    cx,
    |c| &c.wallpaper,
    |_, cx| {
      if let Some(displays) = cx.global_mut::<Wallpapers>().displays.take() {
        PerDisplay::close(displays, cx);
      }
      open(cx);
    },
  );
}

fn open(cx: &mut App) {
  let Some(source) = configured(cx) else {
    return;
  };
  let wallpapers = PerDisplay::new(cx, move |cx, display| {
    create_wallpaper(source.clone(), display, cx)
      .inspect_err(|e| error!("failed to create wallpaper: {e:#}"))
      .into_iter()
      .collect()
  });
  cx.global_mut::<Wallpapers>().displays = Some(wallpapers);
}

pub fn configured(cx: &App) -> Option<ImageSource> {
  cx.config().wallpaper.path.as_deref().map(source)
}

pub(crate) fn source(wallpaper: &str) -> ImageSource {
  if wallpaper.starts_with("http://") || wallpaper.starts_with("https://") {
    return wallpaper.into();
  }
  let path = match (wallpaper.strip_prefix("~/"), dirs::home_dir()) {
    (Some(rest), Some(home)) => home.join(rest),
    _ => PathBuf::from(wallpaper),
  };
  path.into()
}

fn create_wallpaper(
  source: ImageSource,
  display: DisplayId,
  cx: &mut App,
) -> Result<AnyWindowHandle> {
  let handle = cx.open_window(
    WindowOptions {
      kind: WindowKind::LayerShell(LayerShellOptions {
        anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT | Anchor::BOTTOM,
        exclusive_zone: Some(px(-1.)),
        exclusive_edge: None,
        margin: None,
        layer: Layer::Background,
        namespace: NAMESPACE.to_string(),
        keyboard_interactivity: KeyboardInteractivity::None,
      }),
      window_background: WindowBackgroundAppearance::Opaque,
      window_decorations: Some(WindowDecorations::Client),
      app_id: Some(APP_NAME.to_string()),
      display_id: Some(display),
      titlebar: None,
      window_bounds: Some(WindowBounds::Windowed(Bounds {
        origin: point(px(0.), px(0.)),
        size: Size::new(px(0.), px(0.)),
      })),
      ..Default::default()
    },
    |window, cx| {
      let view = cx.new(|_| Wallpaper { source });
      cx.new(|cx| Root::new(view, window, cx))
    },
  )?;
  Ok(handle.into())
}

struct Wallpaper {
  source: ImageSource,
}

impl Render for Wallpaper {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    div().size_full().bg(gpui_kit::black()).child(
      img(self.source.clone())
        .size_full()
        .object_fit(ObjectFit::Cover),
    )
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
      match source(url) {
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
      path(source("~/walls/a.png")),
      home.path().join("walls/a.png")
    );
    // only `~/` is expanded
    assert_eq!(path(source("~user/a.png")), Path::new("~user/a.png"));
    assert_eq!(path(source("~")), Path::new("~"));
  }

  #[test]
  fn plain_paths_stay() {
    assert_eq!(
      path(source("/usr/share/a.png")),
      Path::new("/usr/share/a.png")
    );
    assert_eq!(path(source("a.png")), Path::new("a.png"));
    // not http(s), so a path
    assert_eq!(path(source("ftp://x/a.png")), Path::new("ftp://x/a.png"));
  }
}

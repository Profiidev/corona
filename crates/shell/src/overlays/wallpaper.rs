use std::path::PathBuf;

use anyhow::Result;
use corona_config::{APP_NAME, ConfigProvider};
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
  _displays: Entity<PerDisplay>,
}

impl Global for Wallpapers {}

pub fn init(cx: &mut App) {
  let Some(source) = configured(cx) else {
    return;
  };
  let wallpapers = PerDisplay::new(cx, move |cx, display| {
    create_wallpaper(source.clone(), display, cx)
      .inspect_err(|e| error!("failed to create wallpaper: {e:#}"))
      .into_iter()
      .collect()
  });
  cx.set_global(Wallpapers {
    _displays: wallpapers,
  });
}

pub fn configured(cx: &App) -> Option<ImageSource> {
  cx.config().wallpaper.as_deref().map(source)
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

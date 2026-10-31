use anyhow::Result;
use corona_components::assets::image_source;
use corona_config::ConfigProvider;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Context, DisplayId, ImageSource, IntoElement, ObjectFit,
  ParentElement, Render, Size, Styled, StyledImage, Window, WindowBackgroundAppearance,
  WindowOptions,
  base::Root,
  div, img,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  px,
};
use tracing::error;

use crate::{overlay::layer_options, per_display::PerDisplay};

const NAMESPACE: &str = "corona_wallpaper";

/// Shows the configured wallpaper on every display, again when it changes
pub fn init(cx: &mut App) {
  PerDisplay::follow(
    cx,
    |c| c.wallpaper.clone(),
    |cx| {
      let source = configured(cx)?;
      Some(move |cx: &mut App, display| {
        create_wallpaper(source.clone(), display, cx)
          .inspect_err(|e| error!("failed to create wallpaper: {e:#}"))
          .into_iter()
          .collect()
      })
    },
  );
}

pub fn configured(cx: &App) -> Option<ImageSource> {
  cx.config().wallpaper.path.as_deref().map(image_source)
}

fn create_wallpaper(
  source: ImageSource,
  display: DisplayId,
  cx: &mut App,
) -> Result<AnyWindowHandle> {
  let handle = cx.open_window(
    WindowOptions {
      window_background: WindowBackgroundAppearance::Opaque,
      ..layer_options(
        LayerShellOptions {
          anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT | Anchor::BOTTOM,
          exclusive_zone: Some(px(-1.)),
          exclusive_edge: None,
          margin: None,
          layer: Layer::Background,
          namespace: NAMESPACE.to_string(),
          keyboard_interactivity: KeyboardInteractivity::None,
        },
        Some(display),
        Size::default(),
      )
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

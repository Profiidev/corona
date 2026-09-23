use std::{
  collections::HashMap,
  path::{Path, PathBuf},
  sync::{Arc, Mutex, OnceLock},
};

use gpui_kit::{
  AnyElement, App, DevicePixels, Div, ElementId, ImageSource, InteractiveElement, IntoElement,
  ParentElement, RenderImage, RenderOnce, Stateful, StatefulInteractiveElement, StyleRefinement,
  Styled, SvgSize, Window, div, img, prelude::FluentBuilder, px, relative, size,
};

use crate::integration::desktop::entry::icon_for_names_or_default;

const ICON_SIZE: u16 = 256;

/// What one name list resolved to, once anything has asked for it.
enum Icon {
  Pending,
  Ready(Option<PathBuf>),
}

/// Resolving a name nothing answers to costs a full walk of the icon theme
/// (~19ms), and a panel can draw twenty applications at once. Keep all of it
/// off the thread that is drawing them: the first render of a name list starts
/// the walk and has nothing to draw yet, and the render after it lands draws
/// the icon. Misses are kept too, so a list is walked once per size.
fn icon(names: Vec<String>, size: u16, cx: &mut App) -> Option<PathBuf> {
  type Cache = Mutex<HashMap<(Vec<String>, u16), Icon>>;
  static CACHE: OnceLock<Cache> = OnceLock::new();
  let cache = CACHE.get_or_init(Mutex::default);

  let key = (names, size);
  {
    let mut icons = cache.lock().ok()?;
    if let Some(icon) = icons.get(&key) {
      return match icon {
        Icon::Pending => None,
        Icon::Ready(path) => path.clone(),
      };
    }

    icons.insert(key.clone(), Icon::Pending);
  }

  cx.spawn(async move |cx| {
    let (names, size) = key;
    let resolved = names.clone();
    let path = cx
      .background_executor()
      .spawn(async move { icon_for_names_or_default(resolved.iter().map(String::as_str), size) })
      .await;

    if let Ok(mut icons) = CACHE.get_or_init(Mutex::default).lock() {
      icons.insert((names, size), Icon::Ready(path));
    }

    // Nothing observes this cache, so the windows holding a placeholder have to
    // be told to draw again.
    cx.refresh();
  })
  .detach();

  None
}

#[derive(IntoElement)]
pub struct WindowIcon {
  base: Stateful<Div>,
  class: String,
  names: Vec<String>,
  size: u16,
  children: Vec<AnyElement>,
}

impl WindowIcon {
  pub fn new(class: impl Into<String>, address: impl Into<ElementId>) -> Self {
    Self {
      base: div().id(address),
      class: class.into(),
      names: Vec::new(),
      size: ICON_SIZE,
      children: Vec::new(),
    }
  }

  pub fn names(mut self, names: impl IntoIterator<Item = String>) -> Self {
    self.names = names.into_iter().collect();
    self
  }

  pub fn size(mut self, size: u16) -> Self {
    self.size = size;
    self
  }
}

impl ParentElement for WindowIcon {
  fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
    self.children.extend(elements);
  }
}

impl InteractiveElement for WindowIcon {
  fn interactivity(&mut self) -> &mut gpui_kit::Interactivity {
    self.base.interactivity()
  }
}

impl Styled for WindowIcon {
  fn style(&mut self) -> &mut StyleRefinement {
    self.base.style()
  }
}

impl StatefulInteractiveElement for WindowIcon {}

/// `img` rasterizes an SVG at twice its intrinsic size, so an icon drawn at
/// 256px reaches an 18px box as a 512px bitmap the GPU
/// then minifies without mipmaps, which is what makes it look coarse. Rasterize
/// at the size actually drawn instead.
fn rasterize(path: &Path, pixels: i32, cx: &mut App) -> Option<Arc<RenderImage>> {
  type Cache = Mutex<HashMap<(PathBuf, i32), Arc<RenderImage>>>;
  static CACHE: OnceLock<Cache> = OnceLock::new();
  let cache = CACHE.get_or_init(Mutex::default);

  let key = (path.to_path_buf(), pixels);
  if let Some(image) = cache.lock().ok()?.get(&key) {
    return Some(image.clone());
  }

  let renderer = cx.svg_renderer();
  let svg = renderer.parse_svg(&std::fs::read(path).ok()?).ok()?;
  let image = renderer
    .render_parsed(
      &svg,
      SvgSize::Size(size(DevicePixels(pixels), DevicePixels(pixels))),
    )
    .ok()?;

  cache.lock().ok()?.insert(key, image.clone());
  Some(image)
}

impl RenderOnce for WindowIcon {
  fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let names: Vec<String> = self
      .names
      .iter()
      .cloned()
      .chain([self.class.clone()])
      .collect();

    let pixels = (self.size as f32 * window.scale_factor()) as i32;
    let icon = icon(names, self.size, cx).map(|path| {
      match path.extension().is_some_and(|extension| extension == "svg") {
        true => rasterize(&path, pixels, cx)
          .map(ImageSource::Render)
          .unwrap_or_else(|| ImageSource::from(path)),
        false => ImageSource::from(path),
      }
    });

    self
      .base
      .flex()
      .items_center()
      .justify_center()
      .relative()
      .h(px(self.size as f32))
      .w(px(self.size as f32))
      .rounded_full()
      .map(|this| match icon {
        Some(source) => this.child(img(source).size_full()),
        None => this
          .text_size(px(10.))
          .line_height(relative(1.))
          .child(self.class.chars().next().unwrap_or('?').to_string()),
      })
      .children(self.children)
  }
}

use std::rc::Rc;

use anyhow::Result;
use gpui_kit::{App, ClipboardItem, Global, Image, ImageFormat, Task};
use rust_i18n::t;

use crate::{
  command::{Clip, Hud},
  ui::hud,
};

pub type PasteFn = Rc<dyn Fn(Clip, &mut App) -> Task<Result<()>>>;

/// Pastes into the window that had focus before the launcher, set by the clipboard crate
pub struct Paster(pub PasteFn);

impl Global for Paster {}

pub fn set_paster(paste: impl Fn(Clip, &mut App) -> Task<Result<()>> + 'static, cx: &mut App) {
  cx.set_global(Paster(Rc::new(paste)));
}

pub fn copy(clip: &Clip, cx: &mut App) {
  let item = match clip {
    Clip::Text(text) => ClipboardItem::new_string(text.clone()),
    Clip::Image { bytes, mime } => {
      let format = match mime.as_str() {
        "image/jpeg" => ImageFormat::Jpeg,
        "image/gif" => ImageFormat::Gif,
        "image/webp" => ImageFormat::Webp,
        _ => ImageFormat::Png,
      };
      ClipboardItem::new_image(&Image::from_bytes(format, bytes.to_vec()))
    }
  };
  cx.write_to_clipboard(item);
}

/// Copies, then pastes when a paster is set; says which in the HUD
pub fn paste(clip: Clip, cx: &mut App) {
  copy(&clip, cx);
  let Some(paster) = cx.try_global::<Paster>().map(|p| p.0.clone()) else {
    hud::show(Hud::new(t!("app.launcher.hud.copied_only")), cx);
    return;
  };
  let task = paster(clip, cx);
  cx.spawn(async move |cx| {
    let text = match task.await {
      Ok(()) => t!("app.launcher.hud.pasted"),
      Err(e) => {
        tracing::warn!("paste failed, copied only: {e:#}");
        t!("app.launcher.hud.copied_only")
      }
    };
    cx.update(|cx| hud::show(Hud::new(text), cx));
  })
  .detach();
}

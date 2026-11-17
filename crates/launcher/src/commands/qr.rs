//! Any text as a QR code: links, deeplinks like `otpauth://` or `WIFI:`, plain text

use std::{io::Cursor, rc::Rc, sync::Arc};

use anyhow::{Context, Result};
use corona_config::ConfigProvider;
use gpui_kit::{App, Image, ImageFormat, assets::IconName};
use qrcode::{Color, QrCode};
use rust_i18n::t;

use super::lookup::{escape_markdown, input};
use crate::{
  args::ArgSpec,
  command::{Action, Clip, Command, Icon, Meta, Outcome, Toast, ToastKind},
  registry,
  view::{DetailView, View},
};

/// Pixels per module, big enough to scan from a screen or print
const SCALE: u32 = 10;
/// The white border scanners need, in modules
const QUIET: u32 = 4;

/// `text` as a PNG: dark modules on white with a quiet zone
fn qr_png(text: &str) -> Result<Vec<u8>> {
  let code = QrCode::new(text.as_bytes())?;
  let width = code.width() as u32;
  let colors = code.to_colors();
  let side = (width + 2 * QUIET) * SCALE;
  let picture = image::GrayImage::from_fn(side, side, |x, y| {
    let (mx, my) = (x / SCALE, y / SCALE);
    let dark = (QUIET..QUIET + width).contains(&mx)
      && (QUIET..QUIET + width).contains(&my)
      && colors[((my - QUIET) * width + mx - QUIET) as usize] == Color::Dark;
    image::Luma([if dark { 0 } else { 255 }])
  });
  let mut png = Cursor::new(Vec::new());
  picture.write_to(&mut png, image::ImageFormat::Png)?;
  Ok(png.into_inner())
}

fn qr_view(text: String) -> Result<Outcome> {
  let png = Arc::new(qr_png(&text)?);
  let copy_png = png.clone();
  let save_png = png.clone();
  let copy_text = text.clone();
  Ok(Outcome::Push(View::Detail(DetailView {
    title: t!("app.launcher.qr.title").into(),
    image: Some(Arc::new(Image::from_bytes(ImageFormat::Png, png.to_vec()))),
    markdown: escape_markdown(&text).into(),
    metadata: Vec::new(),
    actions: vec![
      Action::sync(
        "copy_image",
        t!("app.launcher.qr.copy_image"),
        move |_, _, _| {
          Ok(Outcome::Copy(Clip::Image {
            bytes: copy_png.clone(),
            mime: "image/png".into(),
          }))
        },
      ),
      Action::sync("save", t!("app.launcher.qr.save"), move |_, _, cx| {
        let dir = cx
          .config()
          .launcher
          .qr
          .resolved_directory()
          .context("no directory to save to")?;
        std::fs::create_dir_all(&dir)?;
        let name = jiff::Zoned::now().strftime("qr-%Y-%m-%d-%H%M%S.png");
        let path = dir.join(name.to_string());
        std::fs::write(&path, save_png.as_slice())?;
        Ok(Outcome::Toast(Toast {
          kind: ToastKind::Success,
          title: t!("app.launcher.qr.saved").into(),
          message: Some(path.display().to_string().into()),
          action: None,
        }))
      }),
      Action::sync(
        "copy_text",
        t!("app.launcher.qr.copy_text"),
        move |_, _, _| Ok(Outcome::Copy(Clip::Text(copy_text.clone()))),
      ),
    ],
  })))
}

fn qr_command() -> Rc<dyn Command> {
  crate::simple(
    "qr",
    |_| Meta {
      title: t!("app.launcher.qr.title").into(),
      icon: Icon::Tile(IconName::QrCode, 200.),
      keywords: vec![
        "qrcode".into(),
        "link".into(),
        "deeplink".into(),
        "share".into(),
      ],
      args: vec![ArgSpec::text(t!("app.launcher.qr.text")).optional()],
      args_row: true,
      ..Default::default()
    },
    |_| {
      vec![Action::sync(
        "qr",
        t!("app.launcher.qr.title"),
        |args, _, cx| {
          let text = input(args, cx).context(t!("app.launcher.qr.nothing").to_string())?;
          qr_view(text)
        },
      )]
    },
  )
}

pub fn init(cx: &mut App) {
  registry::register_all([qr_command()], cx);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn png_is_a_square_with_quiet_zone() {
    let png = qr_png("otpauth://totp/corona?secret=ABC").unwrap();
    let picture = image::load_from_memory(&png).unwrap().to_luma8();
    let width = QrCode::new("otpauth://totp/corona?secret=ABC")
      .unwrap()
      .width() as u32;
    assert_eq!(picture.width(), (width + 2 * QUIET) * SCALE);
    assert_eq!(picture.width(), picture.height());
    assert_eq!(picture.get_pixel(0, 0).0, [255]);
    // the top-left finder pattern starts right after the quiet zone
    assert_eq!(picture.get_pixel(QUIET * SCALE, QUIET * SCALE).0, [0]);
  }

  #[test]
  fn too_long_text_errors() {
    assert!(qr_png(&"x".repeat(5000)).is_err());
  }
}

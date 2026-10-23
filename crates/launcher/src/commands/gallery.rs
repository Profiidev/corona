//! Grids of the installed fonts and of saved screenshots

use std::{path::PathBuf, rc::Rc, sync::Arc, time::SystemTime};

use corona_config::ConfigProvider;
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

use crate::{
  command::{Action, Clip, Icon, Meta, Outcome},
  commands::files::file_actions,
  kit::{ListSource, opener},
  registry, simple,
  view::ListView,
};

fn fonts_view() -> crate::View {
  ListView::new(
    t!("app.launcher.gallery.fonts"),
    t!("app.launcher.gallery.search_fonts"),
    Rc::new(ListSource::new("fonts", |_, cx| {
      let mut names = cx.text_system().all_font_names();
      names.sort();
      names.dedup();
      names
        .into_iter()
        .filter(|n| !n.starts_with('.'))
        .map(|family| {
          let shown: SharedString = family.clone().into();
          simple(
            format!("font:{family}"),
            move |_| Meta {
              title: shown.clone(),
              icon: Icon::Sample("Aa".into(), shown.clone()),
              ..Default::default()
            },
            move |_| {
              let family = family.clone();
              vec![Action::sync(
                "copy",
                t!("app.launcher.gallery.copy_family"),
                move |_, _, _| Ok(Outcome::Copy(Clip::Text(family.clone()))),
              )]
            },
          )
        })
        .collect()
    })),
  )
  .grid(4, 140., true)
  .into_view()
}

/// Saved screenshots, newest first
fn screenshots(cx: &App) -> Vec<PathBuf> {
  let Some(dir) = cx.config().screenshot.resolved_directory() else {
    return Vec::new();
  };
  let Ok(read) = std::fs::read_dir(dir) else {
    return Vec::new();
  };
  let mut shots: Vec<(SystemTime, PathBuf)> = read
    .filter_map(Result::ok)
    .filter(|e| {
      e.path().extension().is_some_and(|x| {
        matches!(
          x.to_string_lossy().to_lowercase().as_str(),
          "png" | "jpg" | "jpeg" | "webp"
        )
      })
    })
    .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
    .collect();
  shots.sort_by_key(|s| std::cmp::Reverse(s.0));
  shots.into_iter().map(|(_, p)| p).collect()
}

fn image_clip(path: &PathBuf) -> anyhow::Result<Clip> {
  let mime = match path
    .extension()
    .map(|e| e.to_string_lossy().to_lowercase())
    .as_deref()
  {
    Some("jpg" | "jpeg") => "image/jpeg",
    Some("webp") => "image/webp",
    _ => "image/png",
  };
  Ok(Clip::Image {
    bytes: Arc::new(std::fs::read(path)?),
    mime: mime.into(),
  })
}

fn screenshots_view() -> crate::View {
  ListView::new(
    t!("app.launcher.gallery.screenshots"),
    t!("app.launcher.gallery.search_screenshots"),
    Rc::new(ListSource::new("screenshots", |_, cx| {
      screenshots(cx)
        .into_iter()
        .map(|path| {
          let name: SharedString = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
            .into();
          let shown = path.clone();
          simple(
            format!("screenshot:{}", path.display()),
            move |_| Meta {
              title: name.clone(),
              icon: Icon::Image(shown.as_path().into()),
              ..Default::default()
            },
            move |_| {
              let copy = path.clone();
              let mut actions = vec![Action::sync(
                "copy",
                t!("app.launcher.gallery.copy_image"),
                move |_, _, _| Ok(Outcome::Copy(image_clip(&copy)?)),
              )];
              actions.extend(file_actions(&path));
              actions
            },
          )
        })
        .collect()
    })),
  )
  .grid(4, 140., true)
  .empty(t!("app.launcher.gallery.no_screenshots"))
  .into_view()
}

pub fn init(cx: &mut App) {
  registry::register_all(
    [
      opener(
        "fonts",
        t!("app.launcher.gallery.fonts"),
        Icon::Tile(IconName::Type, 280.),
        &["font", "typeface"],
        |_| fonts_view(),
      ),
      opener(
        "screenshots",
        t!("app.launcher.gallery.screenshots"),
        Icon::Tile(IconName::Images, 190.),
        &["screenshot", "images", "captures"],
        |_| screenshots_view(),
      ),
      simple(
        "screenshots:paste_last",
        |_| Meta {
          title: t!("app.launcher.gallery.paste_last").into(),
          icon: Icon::Tile(IconName::ImageUp, 190.),
          keywords: vec!["screenshot".into()],
          ..Default::default()
        },
        |_| {
          vec![Action::sync(
            "paste",
            t!("app.launcher.gallery.paste_last"),
            |_, _, cx| {
              let last = screenshots(cx)
                .into_iter()
                .next()
                .ok_or_else(|| anyhow::anyhow!("{}", t!("app.launcher.gallery.no_screenshots")))?;
              Ok(Outcome::Paste(image_clip(&last)?))
            },
          )]
        },
      ),
    ],
    cx,
  );
}

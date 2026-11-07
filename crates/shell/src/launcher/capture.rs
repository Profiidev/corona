//! Screenshot, color picker and the saved screenshots as launcher commands

use std::{path::PathBuf, rc::Rc, sync::Arc, time::SystemTime};

use corona_config::ConfigProvider;
use corona_ipc::IpcCommand;
use corona_launcher::{
  Action, Clip, Command, Icon, ListView, Meta, Outcome,
  commands::files::file_actions,
  kit::{ListSource, opener},
  registry, simple,
};
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

use corona_capture::{
  commands::{ColorPicker, Screenshot},
  ui::screenshot::mode::Mode,
};

/// Screenshot, color picker and the saved screenshots
pub(crate) fn register(cx: &mut App) {
  let commands: Vec<Rc<dyn Command>> = vec![
    simple(
      "screenshot",
      |_| Meta {
        title: t!("app.launcher.tools.screenshot").into(),
        icon: Icon::Tile(IconName::Camera, 190.),
        keywords: vec!["capture".into(), "screen".into()],
        ..Default::default()
      },
      |_| {
        [
          (
            "selection",
            t!("app.launcher.tools.screenshot"),
            Mode::Selection,
          ),
          (
            "monitor",
            t!("app.launcher.tools.screenshot_monitor"),
            Mode::Monitor,
          ),
          (
            "window",
            t!("app.launcher.tools.screenshot_window"),
            Mode::Window,
          ),
        ]
        .into_iter()
        .map(|(id, title, mode)| {
          Action::sync(id, title, move |_, _, cx| {
            Screenshot::handle(mode, cx)?;
            Ok(Outcome::close())
          })
        })
        .collect()
      },
    ),
    simple(
      "colorpicker",
      |_| Meta {
        title: t!("app.launcher.tools.color_picker").into(),
        icon: Icon::Tile(IconName::Pipette, 320.),
        keywords: vec!["color".into(), "colour".into(), "eyedropper".into()],
        ..Default::default()
      },
      |_| {
        vec![Action::sync(
          "pick",
          t!("app.launcher.tools.color_picker"),
          |_, _, cx| {
            ColorPicker::handle((), cx)?;
            Ok(Outcome::close())
          },
        )]
      },
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
  ];
  registry::register_all(commands, cx);
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

fn screenshots_view() -> corona_launcher::View {
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

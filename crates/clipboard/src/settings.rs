//! The launcher page's quicklinks and snippets groups and the clipboard history's
//! items in its sources group

use corona_components::settings_fields::{number, switch};
use corona_config::launcher::{Quicklink, Snippet};
use corona_launcher::settings::{LauncherSettings, records_item};
use corona_settings::{SettingsExt, SettingsPage, item};
use gpui_kit::{App, component::setting::SettingItem};
use rust_i18n::t;

pub(crate) fn register(cx: &mut App) {
  cx.settings_mut()
    .group(LauncherSettings::ID, "quicklinks", 0, || {
      t!("app.settings.launcher.groups.quicklinks").into()
    })
    .group(LauncherSettings::ID, "snippets", 10, || {
      t!("app.settings.launcher.groups.snippets").into()
    })
    .group_items(LauncherSettings::ID, "quicklinks", 0, quicklinks)
    .group_items(LauncherSettings::ID, "snippets", 0, snippets)
    .group_items(LauncherSettings::ID, "sources", 0, sources);
}

fn some(value: &str) -> Option<String> {
  (!value.trim().is_empty()).then(|| value.to_string())
}

fn quicklinks(_: &App) -> Vec<SettingItem> {
  vec![records_item(
    "quicklinks",
    t!("app.settings.launcher.quicklinks.title"),
    t!("app.settings.launcher.quicklinks.description"),
    &[
      t!("app.settings.launcher.fields.name"),
      t!("app.settings.launcher.fields.url"),
      t!("app.settings.launcher.fields.icon"),
    ],
    |c| {
      c.launcher
        .quicklinks
        .iter()
        .map(|q| {
          vec![
            q.name.clone(),
            q.url.clone(),
            q.icon.clone().unwrap_or_default(),
          ]
        })
        .collect()
    },
    |c, rows| {
      c.launcher.quicklinks = rows
        .into_iter()
        .map(|r| Quicklink {
          name: r[0].clone(),
          url: r[1].clone(),
          icon: some(&r[2]),
        })
        .collect()
    },
  )]
}

fn snippets(_: &App) -> Vec<SettingItem> {
  vec![records_item(
    "snippets",
    t!("app.settings.launcher.snippets.title"),
    t!("app.settings.launcher.snippets.description"),
    &[
      t!("app.settings.launcher.fields.name"),
      t!("app.settings.launcher.fields.keyword"),
      t!("app.settings.launcher.fields.text"),
    ],
    |c| {
      c.launcher
        .snippets
        .iter()
        .map(|s| {
          vec![
            s.name.clone(),
            s.keyword.clone().unwrap_or_default(),
            s.text.clone(),
          ]
        })
        .collect()
    },
    |c, rows| {
      c.launcher.snippets = rows
        .into_iter()
        .map(|r| Snippet {
          name: r[0].clone(),
          keyword: some(&r[1]),
          text: r[2].clone(),
        })
        .collect()
    },
  )]
}

fn sources(_: &App) -> Vec<SettingItem> {
  vec![
    item(
      t!("app.settings.launcher.clipboard.title"),
      t!("app.settings.launcher.clipboard.description"),
      switch(
        |c| c.launcher.clipboard.enabled,
        |c, v| c.launcher.clipboard.enabled = v,
      ),
    ),
    item(
      t!("app.settings.launcher.clipboard_images.title"),
      t!("app.settings.launcher.clipboard_images.description"),
      switch(
        |c| c.launcher.clipboard.images,
        |c, v| c.launcher.clipboard.images = v,
      ),
    ),
    item(
      t!("app.settings.launcher.clipboard_image_size.title"),
      t!("app.settings.launcher.clipboard_image_size.description"),
      number(
        "launcher.clipboard.image_max_mb",
        (1., 100., 1.),
        |c| c.launcher.clipboard.image_max_mb.into(),
        |c, v| c.launcher.clipboard.image_max_mb = v as f32,
      ),
    ),
    item(
      t!("app.settings.launcher.clipboard_retention.title"),
      t!("app.settings.launcher.clipboard_retention.description"),
      number(
        "launcher.clipboard.image_retention_days",
        (1., 365., 1.),
        |c| c.launcher.clipboard.image_retention_days as f64,
        |c, v| c.launcher.clipboard.image_retention_days = v as u32,
      ),
    ),
  ]
}

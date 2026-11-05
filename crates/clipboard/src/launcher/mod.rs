//! Clipboard history: newest first, preview pane, type filter, pins

pub mod snippets;

use std::{cell::Cell, rc::Rc, sync::Arc};

use crate::{ClipboardExt, Entry, Kind};
use corona_components::time::ago_short;
use gpui_kit::{App, AppContext, SharedString, Task, assets::IconName};
use rust_i18n::t;

use corona_launcher::{
  command::{
    Accessory, Action, Clip, Command, Confirm, Detail, DetailBody, Hud, Icon, Meta, Outcome, Tone,
  },
  kit::{ListSource, watch},
  registry, simple,
  view::{Filter, ListView},
};

const LIMIT: usize = 300;

/// What pasting or copying `entry` puts on the clipboard
pub fn clip(entry: Entry, cx: &App) -> Task<anyhow::Result<Clip>> {
  match entry.path {
    // the file can be big, read it off the UI thread
    Some(path) => cx.background_spawn(async move {
      Ok(Clip::Image {
        bytes: Arc::new(std::fs::read(path)?),
        mime: entry.mime,
      })
    }),
    None => Task::ready(Ok(Clip::Text(entry.text.unwrap_or_default()))),
  }
}

/// The whole entry `id` as a clip, an error when the row is gone
fn load(id: i64, cx: &App) -> Task<anyhow::Result<Clip>> {
  match cx.clipboard_history().and_then(|h| h.get(id)) {
    Some(full) => clip(full, cx),
    None => Task::ready(Err(anyhow::anyhow!("the clipboard entry is gone"))),
  }
}

/// The first line, cut, or the size of an image
pub fn title(entry: &Entry) -> SharedString {
  match &entry.text {
    Some(text) => text
      .lines()
      .find(|l| !l.trim().is_empty())
      .unwrap_or_default()
      .chars()
      .take(120)
      .collect::<String>()
      .into(),
    None => t!(
      "app.launcher.clipboard.image_title",
      size = entry.size / 1024
    )
    .into(),
  }
}

pub fn kind_label(kind: Kind) -> SharedString {
  match kind {
    Kind::Text => t!("app.launcher.clipboard.text"),
    Kind::Image => t!("app.launcher.clipboard.image"),
    Kind::Link => t!("app.launcher.clipboard.link"),
    Kind::Color => t!("app.launcher.clipboard.color"),
  }
  .into()
}

fn ago(millis: u64) -> String {
  let elapsed = corona_launcher::state::now().saturating_sub(millis / 1000);
  ago_short(std::time::Duration::from_secs(elapsed))
}

/// A colour typed as `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa` as a swatch
pub fn swatch(text: &str) -> Option<gpui_kit::Hsla> {
  let hex = text.trim().strip_prefix('#')?;
  if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
    return None;
  }
  let long: String = match hex.len() {
    3 | 4 => hex.chars().flat_map(|c| [c, c]).collect(),
    _ => hex.to_string(),
  };
  let value = u32::from_str_radix(&long, 16).ok()?;
  match long.len() {
    6 => Some(gpui_kit::rgb(value).into()),
    8 => Some(gpui_kit::rgba(value).into()),
    _ => None,
  }
}

struct EntryRow(Entry);

impl Command for EntryRow {
  fn id(&self) -> SharedString {
    format!("clip:{}", self.0.id).into()
  }

  fn meta(&self, _: &App) -> Meta {
    let e = &self.0;
    let title = title(e);
    let icon = match (e.kind, &e.path, e.text.as_deref().and_then(swatch)) {
      (_, Some(path), _) => Icon::Image(crate::store::display_path(path).into()),
      (Kind::Color, _, Some(color)) => Icon::Swatch(color),
      (Kind::Link, ..) => Icon::Named(IconName::Link),
      _ => Icon::Named(IconName::Type),
    };
    let mut accessories = Vec::new();
    if e.pinned {
      accessories.push(Accessory::Icon(IconName::Pin));
    }
    accessories.push(Accessory::Text(ago(e.used).into()));
    Meta {
      title,
      icon,
      accessories,
      ..Default::default()
    }
  }

  fn detail(&self, cx: &App) -> Option<Detail> {
    let full = cx.clipboard_history()?.get(self.0.id)?;
    let body = match (&full.path, &full.text) {
      (Some(path), _) => DetailBody::Image(path.as_path().into()),
      (None, Some(text)) => DetailBody::Code(text.clone().into()),
      (None, None) => DetailBody::None,
    };
    Some(Detail {
      body,
      metadata: vec![
        (
          t!("app.launcher.clipboard.type").into(),
          kind_label(full.kind),
        ),
        (
          t!("app.launcher.clipboard.size").into(),
          format!("{} B", full.size).into(),
        ),
        (
          t!("app.launcher.clipboard.copied").into(),
          ago(full.created).into(),
        ),
      ],
      tags: full
        .pinned
        .then(|| {
          (
            SharedString::from(t!("app.launcher.clipboard.pinned").to_string()),
            Tone::Ok,
          )
        })
        .into_iter()
        .collect(),
    })
  }

  fn actions(&self, _: &App) -> Vec<Action> {
    let (id, pinned) = (self.0.id, self.0.pinned);
    vec![
      Action::new(
        "paste",
        t!("app.launcher.clipboard.paste"),
        move |_, _, cx| {
          let clip = load(id, cx);
          cx.spawn(async move |_| clip.await.map(Outcome::Paste))
        },
      ),
      Action::new(
        "copy",
        t!("app.launcher.clipboard.copy"),
        move |_, _, cx| {
          let clip = load(id, cx);
          cx.spawn(async move |_| clip.await.map(Outcome::Copy))
        },
      )
      .shortcut("ctrl-enter"),
      Action::sync(
        "pin",
        match pinned {
          true => t!("app.launcher.clipboard.unpin"),
          false => t!("app.launcher.clipboard.pin"),
        },
        move |_, _, cx| {
          if let Some(history) = cx.clipboard_history().cloned() {
            history.set_pinned(id, !pinned, cx)?;
          }
          Ok(Outcome::KeepOpen)
        },
      )
      .icon(IconName::Pin)
      .shortcut("ctrl-shift-p")
      .keep_open(),
      Action::sync(
        "delete",
        t!("app.launcher.clipboard.delete"),
        move |_, _, cx| {
          if let Some(history) = cx.clipboard_history().cloned() {
            history.delete(id, cx)?;
          }
          Ok(Outcome::KeepOpen)
        },
      )
      .icon(IconName::Trash)
      .destructive()
      .shortcut("ctrl-x")
      .keep_open(),
    ]
  }
}

fn history_view() -> corona_launcher::View {
  let selected = Rc::new(Cell::new(0));
  let filter = Filter {
    label: t!("app.launcher.clipboard.type").into(),
    options: vec![
      ("all".into(), t!("app.launcher.clipboard.all").into()),
      ("text".into(), kind_label(Kind::Text)),
      ("image".into(), kind_label(Kind::Image)),
      ("link".into(), kind_label(Kind::Link)),
      ("color".into(), kind_label(Kind::Color)),
    ],
    selected: selected.clone(),
  };
  let source = ListSource::new("clipboard", move |query, cx| {
    let Some(history) = cx.clipboard_history() else {
      return Vec::new();
    };
    let kind = match selected.get() {
      1 => Some(Kind::Text),
      2 => Some(Kind::Image),
      3 => Some(Kind::Link),
      4 => Some(Kind::Color),
      _ => None,
    };
    history
      .list(&query.raw, kind, LIMIT)
      .into_iter()
      .map(|e| Rc::new(EntryRow(e)) as Rc<dyn Command>)
      .collect()
  })
  .filters()
  .observe(|notify, cx| match cx.clipboard_history() {
    Some(history) => {
      let revision = history.revision.clone();
      vec![watch(&revision, &notify, cx)]
    }
    None => Vec::new(),
  });
  ListView::new(
    t!("app.launcher.clipboard.history"),
    t!("app.launcher.clipboard.search"),
    Rc::new(source),
  )
  .preview()
  .filter(filter)
  .empty(t!("app.launcher.clipboard.empty"))
  .into_view()
}

pub(crate) fn register(cx: &mut App) {
  snippets::init(cx);
  registry::register_all(
    [
      corona_launcher::kit::opener(
        "clipboard:history",
        t!("app.launcher.clipboard.history"),
        Icon::Tile(IconName::ClipboardList, 45.),
        &["clipboard", "paste", "copied"],
        |_| history_view(),
      ),
      simple(
        "clipboard:clear",
        |_| Meta {
          title: t!("app.launcher.clipboard.clear").into(),
          icon: Icon::Tile(IconName::ClipboardX, 45.),
          keywords: vec!["clipboard".into()],
          ..Default::default()
        },
        |_| {
          vec![
            Action::sync("clear", t!("app.launcher.clipboard.clear"), |_, _, cx| {
              let history = cx
                .clipboard_history()
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("{}", t!("app.launcher.clipboard.off")))?;
              let n = history.clear(cx)?;
              Ok(Outcome::Close(Some(Hud::new(t!(
                "app.launcher.clipboard.cleared",
                count = n
              )))))
            })
            .destructive()
            .confirm(Confirm {
              title: t!("app.launcher.clipboard.clear").into(),
              message: t!("app.launcher.clipboard.clear_message").into(),
              button: t!("app.launcher.clipboard.clear_button").into(),
              skippable: true,
            }),
          ]
        },
      ),
    ],
    cx,
  );
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn swatch_forms() {
    for ok in ["#abc", "#abcd", "#aabbcc", "#aabbccdd"] {
      assert!(swatch(ok).is_some(), "{ok}");
    }
    for bad in ["#abcde", "#", "#zzz"] {
      assert!(swatch(bad).is_none(), "{bad}");
    }
  }
}

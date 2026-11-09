//! What a plugin writes for the launcher: rows, pages and replies. The same types type
//! `corona/launcher` for the plugin and read what it passes in.

use std::collections::HashMap;

use corona_js::Handler;
use serde::Deserialize;
use specta::Type;

/// What a handler returns: one key, or nothing to close the launcher.
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Reply {
  /// Shows a page made with `list`, `form` or `detail`.
  Push(PageRef),
  /// Goes back to the previous page.
  Pop(bool),
  /// Stays open and refreshes.
  KeepOpen(bool),
  /// Stays open with a toast.
  Toast(ToastReply),
  /// Copies.
  Copy(Clip),
  /// Copies, closes, refocuses the previous window and pastes.
  Paste(Clip),
  /// Closes and shows a pill.
  Hud(HudReply),
}

/// A page, as `list`, `form` and `detail` return it.
#[derive(Clone, Copy, Debug, Deserialize, Type)]
pub struct PageRef {
  pub id: u32,
}

#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ToastReply {
  pub title: String,
  #[specta(optional)]
  pub message: Option<String>,
  /// `success` when unset.
  #[specta(optional)]
  pub kind: Option<ToastKind>,
}

#[derive(Clone, Copy, Debug, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum ToastKind {
  Success,
  Failure,
  Progress,
}

#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HudReply {
  pub text: String,
  /// A bundled icon's name.
  #[specta(optional)]
  pub icon: Option<String>,
}

/// What to copy or paste: text, or an image.
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(untagged)]
pub enum Clip {
  Text {
    text: String,
  },
  Image {
    /// Base64.
    image: String,
    /// `image/png` when unset.
    #[specta(optional)]
    mime: Option<String>,
  },
}

/// A typed argument of a command, in the order of its `args`.
// only types what `onRun` handlers get, which `Port` passes as JSON
#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(untagged)]
pub enum Arg {
  Text(String),
  Number(f64),
}

/// An icon.
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Icon {
  /// A bundled icon's kebab-case name, like `folder`.
  Name(String),
  /// Desktop entry or window class names.
  App(Vec<String>),
  /// A file in the plugin's directory.
  Image(String),
  /// An emoji or a symbol.
  Glyph(String),
  /// `#rrggbb`.
  Swatch(String),
}

#[derive(Clone, Copy, Debug, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum Tone {
  Ok,
  Off,
  Warn,
  Err,
}

/// Small text, a tag or a bar at the end of a row.
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum Accessory {
  Tag {
    text: String,
    #[specta(optional)]
    tone: Option<Tone>,
  },
  Text(String),
  /// A bundled icon's name.
  Icon(String),
  /// From 0 to 1.
  Progress(f32),
}

/// What the preview pane shows of a row: `text`, `code` or `image`, and key-value rows.
#[derive(Clone, Debug, Default, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
  #[specta(optional)]
  pub text: Option<String>,
  /// Monospace.
  #[specta(optional)]
  pub code: Option<String>,
  /// A file in the plugin's directory.
  #[specta(optional)]
  pub image: Option<String>,
  #[serde(default)]
  #[specta(optional)]
  pub metadata: Vec<(String, String)>,
  #[serde(default)]
  #[specta(optional)]
  pub tags: Vec<(String, Option<Tone>)>,
}

/// Asked before a destructive action runs.
#[derive(Clone, Debug, Default, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Confirm {
  pub title: String,
  pub message: String,
  pub button: String,
  /// Offers "Don't ask again".
  #[serde(default)]
  #[specta(optional)]
  pub skippable: bool,
}

/// Something to do with a row or a page. The first action of a row runs on Enter, the rest
/// are in the action panel.
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Action {
  pub title: String,
  pub run: Handler<(), Reply>,
  /// A bundled icon's name.
  #[specta(optional)]
  pub icon: Option<String>,
  /// Like `ctrl-enter`.
  #[specta(optional)]
  pub shortcut: Option<String>,
  #[serde(default)]
  #[specta(optional)]
  pub destructive: bool,
  /// Stays open after running.
  #[serde(default)]
  #[specta(optional)]
  pub keep_open: bool,
  #[specta(optional)]
  pub confirm: Option<Confirm>,
}

/// One row of a list.
#[derive(Clone, Debug, Default, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Row {
  /// Stable across updates: it keys the ranking of rows the user picks often.
  pub id: String,
  pub title: String,
  #[specta(optional)]
  pub subtitle: Option<String>,
  /// Found by these after the title, not shown.
  #[serde(default)]
  #[specta(optional)]
  pub keywords: Vec<String>,
  #[specta(optional)]
  pub icon: Option<Icon>,
  #[serde(default)]
  #[specta(optional)]
  pub accessories: Vec<Accessory>,
  /// The preview pane, shown for lists with `preview: true`.
  #[specta(optional)]
  pub detail: Option<Preview>,
  pub actions: Vec<Action>,
}

impl Row {
  /// The handlers the row registered
  pub fn handlers(&self) -> impl Iterator<Item = u32> {
    self.actions.iter().map(|a| a.run.id)
  }
}

#[derive(Clone, Debug, Deserialize, Type)]
pub struct Choice {
  pub id: String,
  pub title: String,
}

/// Cells instead of rows.
#[derive(Clone, Debug, Deserialize, Type)]
pub struct Grid {
  pub columns: u32,
  /// A cell's edge in pixels.
  pub cell: f32,
  /// Shows the titles under the cells.
  pub titles: bool,
}

/// A dropdown in the search bar, like "Sort: Name".
#[derive(Clone, Debug, Deserialize, Type)]
pub struct Filter {
  pub label: String,
  pub options: Vec<Choice>,
}

/// A page of rows, searched by the search bar.
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ListSpec {
  /// The breadcrumb in the search bar.
  pub title: String,
  #[specta(optional)]
  pub placeholder: Option<String>,
  #[serde(default)]
  #[specta(optional)]
  pub rows: Vec<Row>,
  #[specta(optional)]
  pub grid: Option<Grid>,
  /// Has a preview pane, toggled with Ctrl+P.
  #[serde(default)]
  #[specta(optional)]
  pub preview: bool,
  #[specta(optional)]
  pub filter: Option<Filter>,
  /// Shown when no rows match.
  #[specta(optional)]
  pub empty: Option<String>,
  /// The query the page opens with.
  #[specta(optional)]
  pub query: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
  Text,
  Password,
  Textarea,
  Dropdown,
}

#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Field {
  /// The key of its value in `onSubmit`.
  pub id: String,
  pub label: String,
  /// `text` when unset.
  #[specta(optional)]
  pub kind: Option<FieldKind>,
  #[specta(optional)]
  pub placeholder: Option<String>,
  #[specta(optional)]
  pub value: Option<String>,
  /// Cannot be submitted empty.
  #[serde(default)]
  #[specta(optional)]
  pub required: bool,
  /// The values of a `dropdown`.
  #[serde(default)]
  #[specta(optional)]
  pub options: Vec<Choice>,
}

/// A page with fields to fill in.
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FormSpec {
  pub title: String,
  pub fields: Vec<Field>,
  /// `Submit` when unset.
  #[specta(optional)]
  pub submit_label: Option<String>,
  /// Shown above the fields.
  #[specta(optional)]
  pub message: Option<String>,
  /// Gets the values of the fields by id.
  pub on_submit: Handler<HashMap<String, String>, Reply>,
  /// Esc.
  #[specta(optional)]
  pub on_cancel: Option<Handler<(), Reply>>,
}

/// A page of text with key-value rows and actions.
#[derive(Clone, Debug, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct DetailSpec {
  pub title: String,
  pub markdown: String,
  /// The sidebar.
  #[serde(default)]
  #[specta(optional)]
  pub metadata: Vec<(String, String)>,
  #[serde(default)]
  #[specta(optional)]
  pub actions: Vec<Action>,
}

/// A page the launcher shows
#[derive(Clone, Debug)]
pub enum PageSpec {
  List(ListSpec),
  Form(FormSpec),
  Detail(DetailSpec),
}

impl PageSpec {
  /// The handlers of the page without its rows
  pub fn handlers(&self) -> Vec<u32> {
    match self {
      PageSpec::List(_) => Vec::new(),
      PageSpec::Form(f) => [Some(f.on_submit.id), f.on_cancel.map(|h| h.id)]
        .into_iter()
        .flatten()
        .collect(),
      PageSpec::Detail(d) => d.actions.iter().map(|a| a.run.id).collect(),
    }
  }

  /// The rows of a list
  pub fn rows(&self) -> &[Row] {
    match self {
      PageSpec::List(l) => &l.rows,
      _ => &[],
    }
  }
}

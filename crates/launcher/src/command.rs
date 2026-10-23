use std::{rc::Rc, sync::Arc};

use anyhow::Result;
use corona_config::ConfigProvider;
use gpui_kit::{App, Hsla, SharedString, Subscription, Task, Window, assets::IconName};

use crate::{
  args::{ArgSpec, Args},
  matching::{Match, Query, default_match},
  view::View,
};

/// One launcher entry: an app, a toggle, a sink, a calculator result. Plugins implement it too.
pub trait Command: 'static {
  /// Stable across updates: keys frecency, selection, aliases and favorites
  fn id(&self) -> SharedString;

  fn meta(&self, cx: &App) -> Meta;

  /// Whether `query` finds this command and how well. The default ranks the title,
  /// the alias from the config and the keywords, and splits inline arguments.
  fn matches(&self, query: &Query, cx: &App) -> Option<Match> {
    let alias = cx
      .config()
      .launcher
      .command(&self.id())
      .and_then(|c| c.alias.clone());
    default_match(&self.meta(cx), alias.as_slice(), query)
  }

  /// The first one is the primary action, the rest go to the action panel
  fn actions(&self, cx: &App) -> Vec<Action>;

  /// The row title while inline arguments are typed, like "Set volume to 40%"
  fn describe(&self, _args: &Args, _cx: &App) -> Option<SharedString> {
    None
  }

  /// What Tab puts in the search field, like a directory's path while browsing
  fn complete(&self, _cx: &App) -> Option<String> {
    None
  }

  /// Runs when the selection lands on this command, for live previews
  fn on_select(&self, _cx: &mut App) {}

  /// What the preview pane shows
  fn detail(&self, _cx: &App) -> Option<Detail> {
    None
  }

  /// A prefix that scopes the search to this command's results, like `g ` or `>`
  fn scope(&self, _cx: &App) -> Option<Scope> {
    None
  }
}

/// Where results go and how they are matched
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SourceKind {
  /// Ranked into the root list
  #[default]
  Root,
  /// Reads the whole query and is pinned above everything, like the calculator
  Evaluator,
  /// Offered only when nothing else matches
  Fallback,
}

/// A changing set of commands: apps, sinks, Wi-Fi networks, windows, list view rows
pub trait CommandSource: 'static {
  fn id(&self) -> SharedString;

  fn kind(&self) -> SourceKind {
    SourceKind::Root
  }

  /// Commands for `query`; sources that filter on their own return them ranked and
  /// set [`CommandSource::filters`]
  fn commands(&self, query: &Query, cx: &App) -> Vec<Rc<dyn Command>>;

  /// The commands are already filtered and ordered for the query
  fn filters(&self) -> bool {
    false
  }

  /// Calls `notify` when [`CommandSource::commands`] would return something else
  fn observe(&self, _notify: Rc<dyn Fn(&mut App)>, _cx: &mut App) -> Vec<Subscription> {
    Vec::new()
  }

  /// Shows the loading bar
  fn loading(&self, _cx: &App) -> bool {
    false
  }

  /// Run when a view on this source opens and closes, to start and stop scans
  fn open(&self, _cx: &mut App) {}
  fn close(&self, _cx: &mut App) {}
}

#[derive(Clone, Default)]
pub struct Meta {
  pub title: SharedString,
  pub subtitle: Option<SharedString>,
  pub icon: Icon,
  /// Matched after the title, not shown
  pub keywords: Vec<SharedString>,
  /// Header the root list groups it under
  pub section: Section,
  pub accessories: Vec<Accessory>,
  /// Inline arguments, at most three show as fields in the arguments row
  pub args: Vec<ArgSpec>,
  /// Shows its arguments as fields after selection instead of parsing them inline
  pub args_row: bool,
  /// Drawn as the calculator tile instead of a row
  pub evaluation: Option<Evaluation>,
}

/// Input, result and a note line, like an exchange rate's age
#[derive(Clone, Default)]
pub struct Evaluation {
  pub input: SharedString,
  pub output: SharedString,
  pub note: Option<SharedString>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Section {
  Favorites,
  Recent,
  Suggestions,
  Applications,
  #[default]
  Commands,
  Results,
  Fallback,
}

#[derive(Clone, Default)]
pub enum Icon {
  #[default]
  None,
  Named(IconName),
  /// Gradient tile behind a named icon, the hue picks the colour
  Tile(IconName, f32),
  /// Desktop entry or window class names, resolved to an app icon
  App(Vec<SharedString>),
  /// Emoji or a symbol
  Glyph(SharedString),
  /// Sample text in a font family
  Sample(SharedString, SharedString),
  Swatch(Hsla),
  Image(Arc<std::path::Path>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
  Ok,
  Off,
  Warn,
  Err,
}

#[derive(Clone)]
pub enum Accessory {
  Tag(SharedString, Tone),
  Text(SharedString),
  Icon(IconName),
  /// 0..=1, drawn as a small bar
  Progress(f32),
}

pub type Run = Rc<dyn Fn(&Args, &mut Window, &mut App) -> Task<Result<Outcome>>>;

#[derive(Clone)]
pub struct Action {
  pub id: SharedString,
  pub title: SharedString,
  pub icon: Option<IconName>,
  /// Like `ctrl-enter`, shown as key caps and bound while the action panel is closed
  pub shortcut: Option<SharedString>,
  pub destructive: bool,
  pub confirm: Option<Confirm>,
  /// Stay open after running
  pub keep_open: bool,
  pub run: Run,
}

impl Action {
  pub fn new(
    id: impl Into<SharedString>,
    title: impl Into<SharedString>,
    run: impl Fn(&Args, &mut Window, &mut App) -> Task<Result<Outcome>> + 'static,
  ) -> Self {
    Self {
      id: id.into(),
      title: title.into(),
      icon: None,
      shortcut: None,
      destructive: false,
      confirm: None,
      keep_open: false,
      run: Rc::new(run),
    }
  }

  /// An action that finishes right away
  pub fn sync(
    id: impl Into<SharedString>,
    title: impl Into<SharedString>,
    run: impl Fn(&Args, &mut Window, &mut App) -> Result<Outcome> + 'static,
  ) -> Self {
    Self::new(id, title, move |args, window, cx| {
      Task::ready(run(args, window, cx))
    })
  }

  /// Waits for a future that needs no window
  pub fn spawn<F>(
    id: impl Into<SharedString>,
    title: impl Into<SharedString>,
    run: impl Fn(&Args, &mut App) -> F + 'static,
  ) -> Self
  where
    F: Future<Output = Result<Outcome>> + 'static,
  {
    Self::new(id, title, move |args, _, cx| {
      let future = run(args, cx);
      cx.spawn(async move |_| future.await)
    })
  }

  pub fn icon(mut self, icon: IconName) -> Self {
    self.icon = Some(icon);
    self
  }

  pub fn shortcut(mut self, keys: impl Into<SharedString>) -> Self {
    self.shortcut = Some(keys.into());
    self
  }

  pub fn destructive(mut self) -> Self {
    self.destructive = true;
    self
  }

  pub fn confirm(mut self, confirm: Confirm) -> Self {
    self.confirm = Some(confirm);
    self
  }

  pub fn keep_open(mut self) -> Self {
    self.keep_open = true;
    self
  }
}

/// Asked before a destructive action runs
#[derive(Clone)]
pub struct Confirm {
  pub title: SharedString,
  pub message: SharedString,
  pub button: SharedString,
  /// Offers "Don't ask again", remembered per action id
  pub skippable: bool,
}

/// What the launcher does after an action
pub enum Outcome {
  /// Close, then show the HUD pill when there is one
  Close(Option<Hud>),
  KeepOpen,
  Push(View),
  /// Back to the previous view
  Pop,
  /// Close, refocus the previous window and paste
  Paste(Clip),
  Copy(Clip),
  /// Stay open with a toast
  Toast(Toast),
}

impl Outcome {
  pub fn close() -> Self {
    Outcome::Close(None)
  }

  pub fn hud(text: impl Into<SharedString>) -> Self {
    Outcome::Close(Some(Hud::new(text)))
  }
}

#[derive(Clone, Debug)]
pub enum Clip {
  Text(String),
  Image { bytes: Arc<Vec<u8>>, mime: String },
}

#[derive(Clone)]
pub struct Hud {
  pub icon: Option<IconName>,
  pub text: SharedString,
  pub swatch: Option<Hsla>,
}

impl Hud {
  pub fn new(text: impl Into<SharedString>) -> Self {
    Self {
      icon: None,
      text: text.into(),
      swatch: None,
    }
  }

  pub fn icon(mut self, icon: IconName) -> Self {
    self.icon = Some(icon);
    self
  }
}

#[derive(Clone)]
pub struct Toast {
  pub kind: ToastKind,
  pub title: SharedString,
  pub message: Option<SharedString>,
  /// Undo, Retry or Cancel
  pub action: Option<Action>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
  Success,
  Failure,
  Progress,
}

#[derive(Clone)]
pub struct Detail {
  pub body: DetailBody,
  /// Key-value rows under the body
  pub metadata: Vec<(SharedString, SharedString)>,
  pub tags: Vec<(SharedString, Tone)>,
}

#[derive(Clone)]
pub enum DetailBody {
  None,
  Text(SharedString),
  Markdown(SharedString),
  Image(Arc<std::path::Path>),
  /// Monospace, like a clipboard entry of code
  Code(SharedString),
}

/// A prefix scope: typing `prefix ` turns into a chip and searches only `source`
#[derive(Clone)]
pub struct Scope {
  pub prefix: SharedString,
  pub label: SharedString,
  pub placeholder: SharedString,
  pub source: Rc<dyn CommandSource>,
}

type MetaFn = Box<dyn Fn(&App) -> Meta>;
type ActionsFn = Box<dyn Fn(&App) -> Vec<Action>>;

/// A command built from closures, for the many fixed entries
pub struct SimpleCommand {
  pub id: SharedString,
  pub meta: MetaFn,
  pub actions: ActionsFn,
}

/// A [`SimpleCommand`] ready to register
pub fn simple(
  id: impl Into<SharedString>,
  meta: impl Fn(&App) -> Meta + 'static,
  actions: impl Fn(&App) -> Vec<Action> + 'static,
) -> Rc<dyn Command> {
  Rc::new(SimpleCommand {
    id: id.into(),
    meta: Box::new(meta),
    actions: Box::new(actions),
  })
}

impl Command for SimpleCommand {
  fn id(&self) -> SharedString {
    self.id.clone()
  }

  fn meta(&self, cx: &App) -> Meta {
    (self.meta)(cx)
  }

  fn actions(&self, cx: &App) -> Vec<Action> {
    (self.actions)(cx)
  }
}

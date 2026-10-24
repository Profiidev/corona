use std::{cell::RefCell, cmp::Ordering, rc::Rc};

use corona_config::ConfigProvider;
use gpui_kit::{App, Global, SharedString, Subscription};

use crate::{
  command::{Command, CommandSource, Meta, Scope, Section, SourceKind},
  matching::{Match, Query, Tier},
  state::{self, Frecency},
};

/// Every command and source the launcher searches
pub struct Registry {
  statics: Rc<Statics>,
  sources: Vec<Rc<dyn CommandSource>>,
  frecency: Frecency,
}

impl Global for Registry {}

pub(crate) fn init(cx: &mut App) {
  let statics = Rc::new(Statics::default());
  cx.set_global(Registry {
    sources: vec![statics.clone()],
    statics,
    frecency: state::load("frecency"),
  });
}

/// Adds a fixed command to the root search
pub fn register(command: Rc<dyn Command>, cx: &mut App) {
  cx.global::<Registry>().statics.0.borrow_mut().push(command);
}

pub fn register_all(commands: impl IntoIterator<Item = Rc<dyn Command>>, cx: &mut App) {
  for command in commands {
    register(command, cx);
  }
}

/// Adds a changing set of commands, an evaluator or a fallback
pub fn register_source(source: Rc<dyn CommandSource>, cx: &mut App) {
  cx.global_mut::<Registry>().sources.push(source);
}

/// Drops the sources with this id, for plugins that unload
pub fn unregister_source(id: &str, cx: &mut App) {
  cx.global_mut::<Registry>().sources.retain(|s| s.id() != id);
}

impl Registry {
  pub fn sources(&self) -> &[Rc<dyn CommandSource>] {
    &self.sources
  }

  /// The command with this id, from any root source
  pub fn find(&self, id: &str, cx: &App) -> Option<Rc<dyn Command>> {
    let query = Query::default();
    self
      .sources
      .iter()
      .filter(|s| s.kind() == SourceKind::Root)
      .flat_map(|s| s.commands(&query, cx))
      .find(|c| c.id() == id)
  }

  /// Every root command, for the settings page and prefixes
  pub fn all(&self, cx: &App) -> Vec<Rc<dyn Command>> {
    let query = Query::default();
    self
      .sources
      .iter()
      .filter(|s| s.kind() == SourceKind::Root)
      .flat_map(|s| s.commands(&query, cx))
      .collect()
  }

  pub fn scopes(&self, cx: &App) -> Vec<Scope> {
    self.all(cx).iter().filter_map(|c| c.scope(cx)).collect()
  }

  pub fn frecency(&self, id: &str) -> f64 {
    self.frecency.score(id, state::now())
  }

  pub fn observe(&self, notify: Rc<dyn Fn(&mut App)>, cx: &mut App) -> Vec<Subscription> {
    self
      .sources
      .clone()
      .iter()
      .flat_map(|s| s.observe(notify.clone(), cx))
      .collect()
  }

  pub fn loading(&self, cx: &App) -> bool {
    self.sources.iter().any(|s| s.loading(cx))
  }
}

/// Counts a run for ranking
pub fn record_use(id: &str, cx: &mut App) {
  let registry = cx.global_mut::<Registry>();
  registry.frecency.record(id, state::now());
  if let Err(e) = state::save("frecency", &registry.frecency) {
    tracing::warn!("failed to save launcher frecency: {e:#}");
  }
}

#[derive(Default)]
struct Statics(RefCell<Vec<Rc<dyn Command>>>);

impl CommandSource for Statics {
  fn id(&self) -> SharedString {
    "static".into()
  }

  fn commands(&self, _: &Query, _: &App) -> Vec<Rc<dyn Command>> {
    self.0.borrow().clone()
  }
}

/// A result with how it matched
#[derive(Clone)]
pub struct Row {
  pub command: Rc<dyn Command>,
  pub meta: Meta,
  pub found: Match,
  pub frecency: f64,
}

impl Row {
  pub fn id(&self) -> SharedString {
    self.command.id()
  }
}

/// What the root list shows for a query
#[derive(Default, Clone)]
pub struct Results {
  /// Pinned above everything, like the calculator
  pub evaluated: Vec<Row>,
  pub sections: Vec<(Section, Vec<Row>)>,
}

impl Results {
  pub fn rows(&self) -> impl Iterator<Item = &Row> {
    self
      .evaluated
      .iter()
      .chain(self.sections.iter().flat_map(|(_, rows)| rows))
  }

  pub fn len(&self) -> usize {
    self.rows().count()
  }

  pub fn is_empty(&self) -> bool {
    self.len() == 0
  }
}

/// Best first: tier, then use, then match score, then title
pub fn compare(a: &Row, b: &Row) -> Ordering {
  a.found
    .tier
    .cmp(&b.found.tier)
    .then(b.frecency.total_cmp(&a.frecency))
    .then(b.found.score.cmp(&a.found.score))
    .then_with(|| a.meta.title.cmp(&b.meta.title))
}

fn row(command: Rc<dyn Command>, found: Match, registry: &Registry, cx: &App) -> Row {
  Row {
    meta: command.meta(cx),
    frecency: registry.frecency(&command.id()),
    command,
    found,
  }
}

/// The rows of `commands` that `query` finds, best first
pub fn rank_commands(
  commands: Vec<Rc<dyn Command>>,
  query: &Query,
  filtered: bool,
  cx: &App,
) -> Vec<Row> {
  let registry = cx.global::<Registry>();
  if filtered {
    return commands
      .into_iter()
      .map(|c| row(c, Match::new(Tier::Exact), registry, cx))
      .collect();
  }
  let mut rows: Vec<Row> = commands
    .into_iter()
    .filter_map(|c| {
      let found = c.matches(query, cx)?;
      Some(row(c, found, registry, cx))
    })
    .collect();
  if !query.is_empty() {
    rows.sort_by(compare);
  }
  rows
}

/// The root list: evaluators, then sections; fallbacks only when nothing else matched
pub fn rank(query: &Query, cx: &App) -> Results {
  let registry = cx.global::<Registry>();
  let config = &cx.config().launcher;
  let enabled = |id: &str, root: bool| {
    config
      .command(id)
      .is_none_or(|c| c.enabled && !(root && c.hidden))
  };

  let mut results = Results::default();
  let mut rows = Vec::new();
  let mut fallbacks = Vec::new();
  for source in &registry.sources {
    let commands = source.commands(query, cx);
    match source.kind() {
      SourceKind::Evaluator if !query.is_empty() => results.evaluated.extend(
        commands
          .into_iter()
          .map(|c| row(c, Match::new(Tier::Exact), registry, cx)),
      ),
      SourceKind::Fallback if !query.is_empty() => fallbacks.extend(
        commands
          .into_iter()
          .map(|c| row(c, Match::new(Tier::Fuzzy), registry, cx)),
      ),
      SourceKind::Root => rows.extend(
        rank_commands(commands, query, source.filters(), cx)
          .into_iter()
          .filter(|r| enabled(&r.id(), true)),
      ),
      _ => {}
    }
  }

  if query.is_empty() {
    results.sections = empty_sections(rows, cx);
    return results;
  }

  rows.sort_by(compare);
  results.sections = group(rows, config.section_limit.max(1));
  if results.sections.is_empty() && results.evaluated.is_empty() {
    let order = |r: &Row| {
      config
        .fallbacks
        .iter()
        .position(|f| r.id().ends_with(f.as_str()))
        .unwrap_or(usize::MAX)
    };
    fallbacks.retain(|r| order(r) != usize::MAX);
    fallbacks.sort_by_key(order);
    if !fallbacks.is_empty() {
      results.sections.push((Section::Fallback, fallbacks));
    }
  }
  results
}

/// Sorted rows by section, capped at `limit` each; sections in the order of their best row
fn group(rows: Vec<Row>, limit: usize) -> Vec<(Section, Vec<Row>)> {
  let mut sections: Vec<(Section, Vec<Row>)> = Vec::new();
  for r in rows {
    match sections.iter_mut().find(|(s, _)| *s == r.meta.section) {
      Some((_, rows)) if rows.len() < limit => rows.push(r),
      Some(_) => {}
      None => sections.push((r.meta.section, vec![r])),
    }
  }
  sections
}

const RECENT: usize = 6;
const SUGGESTIONS: usize = 6;

/// Running timers, favorites, recently used, suggestions, then every app
fn empty_sections(rows: Vec<Row>, cx: &App) -> Vec<(Section, Vec<Row>)> {
  let config = &cx.config().launcher;
  let favorite = |id: &str| config.command(id).is_some_and(|c| c.favorite);
  let (running, rows): (Vec<Row>, Vec<Row>) = rows
    .into_iter()
    .partition(|r| r.meta.section == Section::Running);
  let (favorites, rest): (Vec<Row>, Vec<Row>) = rows.into_iter().partition(|r| favorite(&r.id()));
  let mut recent: Vec<Row> = rest.iter().filter(|r| r.frecency > 0.).cloned().collect();
  recent.sort_by(|a, b| b.frecency.total_cmp(&a.frecency));
  recent.truncate(RECENT);
  let shown: Vec<SharedString> = recent.iter().map(Row::id).collect();
  let (apps, rest): (Vec<Row>, Vec<Row>) = rest
    .into_iter()
    .partition(|r| r.meta.section == Section::Applications);
  let suggestions: Vec<Row> = rest
    .into_iter()
    .filter(|r| r.frecency == 0. && r.meta.section == Section::Commands)
    .take(SUGGESTIONS)
    .collect();
  // every app, A to Z, but those already shown as recent
  let mut apps: Vec<Row> = apps
    .into_iter()
    .filter(|r| !shown.contains(&r.id()))
    .collect();
  apps.sort_by_key(|r| r.meta.title.to_lowercase());
  [
    (Section::Running, running),
    (Section::Favorites, favorites),
    (Section::Recent, recent),
    (Section::Suggestions, suggestions),
    (Section::Applications, apps),
  ]
  .into_iter()
  .filter(|(_, rows)| !rows.is_empty())
  .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::command::{Action, simple};
  use gpui_kit::{self as gpui, TestAppContext};

  fn command(id: &'static str, title: &'static str, section: Section) -> Rc<dyn Command> {
    simple(
      id,
      move |_| Meta {
        title: title.into(),
        section,
        ..Default::default()
      },
      |_| Vec::<Action>::new(),
    )
  }

  fn setup(cx: &mut TestAppContext) {
    cx.update(|cx| {
      cx.set_global(corona_config::Config::default());
      cx.set_global(Registry {
        statics: Rc::new(Statics::default()),
        sources: Vec::new(),
        frecency: Frecency::default(),
      });
      let statics = cx.global::<Registry>().statics.clone();
      cx.global_mut::<Registry>().sources.push(statics);
      register_all(
        [
          command("app:firefox", "Firefox", Section::Applications),
          command("app:files", "Files", Section::Applications),
          command("session:lock", "Lock", Section::Commands),
          command("fire", "Fire alarm", Section::Commands),
        ],
        cx,
      );
    });
  }

  fn titles(results: &Results) -> Vec<String> {
    results.rows().map(|r| r.meta.title.to_string()).collect()
  }

  #[gpui::test]
  fn ranks_by_tier_then_groups(cx: &mut TestAppContext) {
    setup(cx);
    let results = cx.update(|cx| rank(&Query::new("fi"), cx));
    // both apps are prefixes, Files is shorter; the command section follows
    assert_eq!(titles(&results), ["Files", "Firefox", "Fire alarm"]);
    assert_eq!(results.sections[0].0, Section::Applications);
  }

  #[gpui::test]
  fn frecency_lifts_within_a_tier(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
      cx.global_mut::<Registry>()
        .frecency
        .record("app:firefox", state::now())
    });
    let results = cx.update(|cx| rank(&Query::new("fi"), cx));
    assert_eq!(titles(&results)[0], "Firefox");
  }

  #[gpui::test]
  fn hidden_and_disabled(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
      let mut config = cx.config().clone();
      config.launcher.commands.insert(
        "app:files".into(),
        corona_config::launcher::CommandConfig {
          hidden: true,
          ..Default::default()
        },
      );
      cx.set_global(config);
    });
    let results = cx.update(|cx| rank(&Query::new("files"), cx));
    assert!(results.is_empty());
  }

  #[gpui::test]
  fn empty_query_sections(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| register(command("app:zed", "Zed", Section::Applications), cx));
    cx.update(|cx| {
      let mut config = cx.config().clone();
      config.launcher.commands.insert(
        "app:files".into(),
        corona_config::launcher::CommandConfig {
          favorite: true,
          ..Default::default()
        },
      );
      cx.set_global(config);
      cx.global_mut::<Registry>()
        .frecency
        .record("app:firefox", state::now());
    });
    let results = cx.update(|cx| rank(&Query::default(), cx));
    let sections: Vec<Section> = results.sections.iter().map(|(s, _)| *s).collect();
    assert_eq!(
      sections,
      [
        Section::Favorites,
        Section::Recent,
        Section::Suggestions,
        Section::Applications
      ]
    );
  }
}

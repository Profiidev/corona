use std::{cell::RefCell, collections::HashSet, rc::Rc};

use corona_config::{ConfigProvider, launcher::LauncherConfig};
use gpui_kit::{App, Global, SharedString};

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
  /// The prefixes, for the config they were collected under; dropped when commands change
  scopes: RefCell<Option<(LauncherConfig, Vec<Scope>)>>,
}

impl Global for Registry {}

pub(crate) fn init(cx: &mut App) {
  let statics = Rc::new(Statics::default());
  cx.set_global(Registry {
    sources: vec![statics.clone()],
    statics,
    frecency: state::load("frecency"),
    scopes: RefCell::default(),
  });
}

/// Adds a fixed command to the root search
pub fn register(command: Rc<dyn Command>, cx: &mut App) {
  let registry = cx.global::<Registry>();
  registry.statics.0.borrow_mut().push(command);
  registry.scopes.take();
}

pub fn register_all(commands: impl IntoIterator<Item = Rc<dyn Command>>, cx: &mut App) {
  for command in commands {
    register(command, cx);
  }
}

/// Adds a changing set of commands, an evaluator or a fallback
pub fn register_source(source: Rc<dyn CommandSource>, cx: &mut App) {
  let registry = cx.global_mut::<Registry>();
  registry.sources.push(source);
  registry.scopes.take();
}

/// Drops the cached prefixes, for commands that came or went
pub fn invalidate_scopes(cx: &App) {
  cx.global::<Registry>().scopes.take();
}

impl Registry {
  pub fn sources(&self) -> &[Rc<dyn CommandSource>] {
    &self.sources
  }

  /// The command with this id, from any root source
  pub fn find(&self, id: &str, cx: &App) -> Option<Rc<dyn Command>> {
    // some commands, like desktop actions and setting fields, only show for a query
    [Query::default(), Query::new(id)].iter().find_map(|query| {
      self
        .sources
        .iter()
        .filter(|s| s.kind() == SourceKind::Root)
        .flat_map(|s| s.commands(query, cx))
        .find(|c| c.id() == id)
    })
  }

  /// Every configurable root command, for the settings page and prefixes
  ///
  /// Skips the transient running timers.
  pub fn all(&self, cx: &App) -> Vec<Rc<dyn Command>> {
    let query = Query::default();
    self
      .sources
      .iter()
      .filter(|s| s.kind() == SourceKind::Root)
      .flat_map(|s| s.commands(&query, cx))
      .filter(|c| c.meta(cx).section != Section::Running)
      .collect()
  }

  pub fn scopes(&self, cx: &App) -> Vec<Scope> {
    let launcher = &cx.config().launcher;
    if let Some((config, scopes)) = &*self.scopes.borrow()
      && config == launcher
    {
      return scopes.clone();
    }
    let scopes: Vec<Scope> = self
      .all(cx)
      .iter()
      .filter(|c| launcher.command(&c.id()).is_none_or(|c| c.enabled))
      .filter_map(|c| c.scope(cx))
      .collect();
    *self.scopes.borrow_mut() = Some((launcher.clone(), scopes.clone()));
    scopes
  }

  pub fn frecency(&self, id: &str) -> f64 {
    self.frecency.score(id, state::now())
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
  /// Whether the settings page lists the command, so common actions apply
  pub managed: bool,
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

/// The weight of a tier. Alias and exact matches are far enough apart from the rest that
/// use never lifts anything over them; the others are [`TIER_STEP`] apart.
fn tier_weight(tier: Tier) -> f64 {
  match tier {
    Tier::Alias => 1000.,
    Tier::Exact => 800.,
    Tier::Prefix => 500.,
    Tier::WordPrefix => 400.,
    Tier::Substring => 300.,
    Tier::Keyword => 200.,
    Tier::Fuzzy => 100.,
  }
}

/// The gap between the tiers below exact
const TIER_STEP: f64 = 100.;
/// The most use is worth: more than a step, less than two, so a command used much more than
/// the others passes the one tier above it and no further
const USE_CAP: f64 = 1.5 * TIER_STEP;
/// The decayed uses that are worth the whole cap, unless a row has more: a command used once
/// does not pass a tier on its own
const USE_FULL: f64 = 10.;
/// The most the match score within a tier is worth, below one use
const SCORE_CAP: f64 = 10.;

/// How high a row ranks: the tier it matched in, how much it is used next to the most used
/// row (`most`, decayed uses) and how well it matched within its tier. Use is relative, so a
/// command used more than the one a tier above it passes it, however much both are used.
pub fn rank_score(row: &Row, most: f64) -> f64 {
  let uses = USE_CAP * (row.frecency / most.max(USE_FULL)).min(1.);
  let within = (f64::from(row.found.score) * 0.1).min(SCORE_CAP);
  tier_weight(row.found.tier) + uses + within
}

/// Sorts best first: by [`rank_score`], then title
pub fn sort(rows: &mut [Row]) {
  let most = rows.iter().map(|r| r.frecency).fold(0., f64::max);
  rows.sort_by(|a, b| {
    rank_score(b, most)
      .total_cmp(&rank_score(a, most))
      .then_with(|| a.meta.title.cmp(&b.meta.title))
  });
}

fn row(command: Rc<dyn Command>, found: Match, registry: &Registry, cx: &App) -> Row {
  let meta = command.meta(cx);
  Row {
    managed: meta.section != Section::Running,
    meta,
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
    sort(&mut rows);
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

  // Query-gated commands are absent from the settings page, so favorite, alias, hide and
  // disable on them could never be undone
  let listed = (!query.is_empty()).then(|| {
    registry
      .all(cx)
      .iter()
      .map(|c| c.id())
      .collect::<HashSet<SharedString>>()
  });
  let mut results = Results::default();
  let mut rows = Vec::new();
  let mut fallbacks = Vec::new();
  for source in &registry.sources {
    let commands = source.commands(query, cx);
    match source.kind() {
      SourceKind::Evaluator if !query.is_empty() => results.evaluated.extend(
        commands
          .into_iter()
          .map(|c| Row {
            managed: false,
            ..row(c, Match::new(Tier::Exact), registry, cx)
          })
          .filter(|r| enabled(&r.id(), false)),
      ),
      SourceKind::Fallback if !query.is_empty() => fallbacks.extend(
        commands
          .into_iter()
          .map(|c| Row {
            managed: false,
            ..row(c, Match::new(Tier::Fuzzy), registry, cx)
          })
          .filter(|r| enabled(&r.id(), false)),
      ),
      SourceKind::Root => rows.extend(
        rank_commands(commands, query, source.filters(), cx)
          .into_iter()
          .map(|r| Row {
            managed: r.managed && listed.as_ref().is_none_or(|l| l.contains(&r.id())),
            ..r
          })
          .filter(|r| enabled(&r.id(), true)),
      ),
      _ => {}
    }
  }

  if query.is_empty() {
    results.sections = empty_sections(rows, cx);
    return results;
  }

  sort(&mut rows);
  results.sections = group(rows, config.section_limit.max(1));
  if results.sections.is_empty() && results.evaluated.is_empty() {
    let order = |r: &Row| {
      config
        .fallbacks
        .iter()
        .position(|f| !f.trim().is_empty() && r.id().ends_with(f.as_str()))
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
        scopes: RefCell::default(),
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

  /// Records `times` runs of command `id`
  fn use_it(cx: &mut TestAppContext, id: &str, times: usize) {
    cx.update(|cx| {
      for _ in 0..times {
        cx.global_mut::<Registry>()
          .frecency
          .record(id, state::now());
      }
    });
  }

  /// The `of` titles that `query` finds, in the order it ranks them
  fn order(cx: &mut TestAppContext, query: &str, of: &[&str]) -> Vec<String> {
    let results = cx.update(|cx| rank(&Query::new(query), cx));
    let mut titles = titles(&results);
    titles.retain(|t| of.contains(&t.as_str()));
    titles
  }

  #[gpui::test]
  fn use_lifts_a_command_over_the_next_tier_only(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
      register_all(
        [
          command("files", "Recent files", Section::Commands),
          command("zed", "Zed recent projects", Section::Commands),
          command("share", "Share", Section::Commands),
        ],
        cx,
      );
    });
    let of = ["Recent files", "Zed recent projects", "Share"];
    // prefix, word prefix, substring
    assert_eq!(order(cx, "re", &of), of);
    // a word prefix that is used a lot passes the prefix above it
    use_it(cx, "zed", 8);
    assert_eq!(
      order(cx, "re", &of),
      ["Zed recent projects", "Recent files", "Share"]
    );
    // a substring match, two tiers below the prefix, stays below it however much it is used
    use_it(cx, "share", 1000);
    assert_eq!(
      order(cx, "re", &of),
      ["Recent files", "Share", "Zed recent projects"]
    );
  }

  #[gpui::test]
  fn the_more_used_of_two_adjacent_tiers_wins(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| {
      register_all(
        [
          command("files", "Recent files", Section::Commands),
          command("zed", "Zed recent projects", Section::Commands),
        ],
        cx,
      );
    });
    let of = ["Recent files", "Zed recent projects"];
    // both used, the word prefix much more: it wins, which a capped bonus could not do
    use_it(cx, "files", 5);
    use_it(cx, "zed", 33);
    assert_eq!(
      order(cx, "re", &of),
      ["Zed recent projects", "Recent files"]
    );
    // used about the same, the better match wins
    use_it(cx, "files", 25);
    assert_eq!(
      order(cx, "re", &of),
      ["Recent files", "Zed recent projects"]
    );
  }

  #[gpui::test]
  fn use_never_passes_an_exact_match(cx: &mut TestAppContext) {
    setup(cx);
    cx.update(|cx| register(command("lock-screen", "Lock screen", Section::Commands), cx));
    use_it(cx, "lock-screen", 1000);
    assert_eq!(
      order(cx, "lock", &["Lock", "Lock screen"]),
      ["Lock", "Lock screen"]
    );
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

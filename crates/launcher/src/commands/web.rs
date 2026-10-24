//! Web search with engine prefixes (`g rust`), the web fallback and typed URLs

use std::rc::Rc;

use corona_config::{ConfigProvider, launcher::SearchEngine};
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

use crate::{
  args::{ArgSpec, Args},
  command::{Action, Command, CommandSource, Icon, Meta, Outcome, Scope, SourceKind, simple},
  matching::Query,
  registry,
};

pub fn init(cx: &mut App) {
  registry::register_source(Rc::new(Engines), cx);
  registry::register_source(Rc::new(Url), cx);
  registry::register_source(Rc::new(WebFallback), cx);
}

/// Percent-encodes everything outside the unreserved set
pub fn encode(text: &str) -> String {
  let mut out = String::with_capacity(text.len());
  for b in text.bytes() {
    match b {
      b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
      b => out.push_str(&format!("%{b:02X}")),
    }
  }
  out
}

/// Percent-decoding; bad escapes stay as they are
pub fn decode(text: &str) -> String {
  let bytes = text.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    let hex = (bytes[i] == b'%')
      .then(|| std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok())
      .flatten()
      .and_then(|h| u8::from_str_radix(h, 16).ok());
    match hex {
      Some(b) => {
        out.push(b);
        i += 3;
      }
      None => {
        out.push(bytes[i]);
        i += 1;
      }
    }
  }
  String::from_utf8_lossy(&out).into_owned()
}

pub fn search_url(engine: &SearchEngine, query: &str) -> String {
  engine.url.replace("{query}", &encode(query))
}

fn open(engine: &SearchEngine, query: &str, cx: &mut App) -> anyhow::Result<Outcome> {
  cx.open_url(&search_url(engine, query));
  Ok(Outcome::close())
}

fn default_engine(cx: &App) -> Option<SearchEngine> {
  let launcher = &cx.config().launcher;
  launcher
    .engine(&launcher.search_engine)
    .or(launcher.search_engines.first())
    .cloned()
}

/// "Search Google for …" for one query
fn search(engine: SearchEngine, query: String, id: &str) -> Rc<dyn Command> {
  let title = t!(
    "app.launcher.web.search_for",
    engine = engine.name.clone(),
    query = query.clone()
  );
  simple(
    id.to_string(),
    move |_| Meta {
      title: title.clone().into(),
      icon: Icon::Tile(IconName::Globe, 200.),
      ..Default::default()
    },
    move |_| {
      let (engine, query) = (engine.clone(), query.clone());
      vec![Action::sync(
        "search",
        t!("app.launcher.web.open"),
        move |_, _, cx| open(&engine, &query, cx),
      )]
    },
  )
}

/// One engine as a command; its keyword scopes the search
struct EngineCommand(SearchEngine);

impl Command for EngineCommand {
  fn id(&self) -> SharedString {
    format!("web:{}", self.0.keyword).into()
  }

  fn meta(&self, _: &App) -> Meta {
    Meta {
      title: t!("app.launcher.web.search", engine = self.0.name.clone()).into(),
      icon: Icon::Tile(IconName::Globe, 200.),
      keywords: vec![self.0.keyword.clone().into(), "web".into()],
      args: vec![ArgSpec::text(t!("app.launcher.web.query"))],
      ..Default::default()
    }
  }

  fn describe(&self, args: &Args, _: &App) -> Option<SharedString> {
    let query = args.text(0)?;
    Some(
      t!(
        "app.launcher.web.search_for",
        engine = self.0.name.clone(),
        query = query
      )
      .into(),
    )
  }

  fn actions(&self, _: &App) -> Vec<Action> {
    let engine = self.0.clone();
    vec![Action::sync(
      "search",
      t!("app.launcher.web.open"),
      move |args, _, cx| open(&engine, args.text(0).unwrap_or_default(), cx),
    )]
  }

  fn scope(&self, _: &App) -> Option<Scope> {
    Some(Scope {
      prefix: self.0.keyword.clone().into(),
      label: self.0.name.clone().into(),
      placeholder: t!("app.launcher.web.placeholder", engine = self.0.name.clone()).into(),
      source: Rc::new(EngineScope(self.0.clone())),
    })
  }
}

struct EngineScope(SearchEngine);

impl CommandSource for EngineScope {
  fn id(&self) -> SharedString {
    format!("web:{}:scope", self.0.keyword).into()
  }

  fn filters(&self) -> bool {
    true
  }

  fn commands(&self, query: &Query, _: &App) -> Vec<Rc<dyn Command>> {
    let text = query.raw.trim();
    if text.is_empty() {
      return Vec::new();
    }
    vec![search(self.0.clone(), text.to_string(), "web:scoped")]
  }
}

/// The configured engines
struct Engines;

impl CommandSource for Engines {
  fn id(&self) -> SharedString {
    "web".into()
  }

  fn commands(&self, _: &Query, cx: &App) -> Vec<Rc<dyn Command>> {
    cx.config()
      .launcher
      .search_engines
      .iter()
      .map(|e| Rc::new(EngineCommand(e.clone())) as Rc<dyn Command>)
      .collect()
  }
}

struct WebFallback;

impl CommandSource for WebFallback {
  fn id(&self) -> SharedString {
    "fallback:web".into()
  }

  fn kind(&self) -> SourceKind {
    SourceKind::Fallback
  }

  fn commands(&self, query: &Query, cx: &App) -> Vec<Rc<dyn Command>> {
    let Some(engine) = default_engine(cx) else {
      return Vec::new();
    };
    vec![search(engine, query.raw.trim().to_string(), "fallback:web")]
  }
}

/// A URL as typed, `https://` added when it has none
pub fn as_url(text: &str) -> Option<String> {
  let text = text.trim();
  if text.contains(char::is_whitespace) || text.len() < 4 {
    return None;
  }
  if let Some((scheme, rest)) = text.split_once("://") {
    let known = matches!(scheme, "http" | "https" | "ftp" | "file");
    return (known && !rest.is_empty()).then(|| text.to_string());
  }
  let host = text.split(['/', '?', '#']).next()?;
  let host = host.split(':').next()?;
  let localhost = host == "localhost";
  let labels: Vec<&str> = host.split('.').collect();
  let tld = labels.last()?;
  let domain = labels.len() >= 2
    && labels.iter().all(|l| !l.is_empty())
    && tld.len() >= 2
    && tld.chars().all(|c| c.is_ascii_alphabetic());
  (domain || localhost).then(|| format!("https://{text}"))
}

/// Opens a typed URL, pinned above the rest
struct Url;

impl CommandSource for Url {
  fn id(&self) -> SharedString {
    "url".into()
  }

  fn kind(&self) -> SourceKind {
    SourceKind::Evaluator
  }

  fn commands(&self, query: &Query, _: &App) -> Vec<Rc<dyn Command>> {
    let Some(url) = as_url(&query.raw) else {
      return Vec::new();
    };
    let title = t!("app.launcher.web.open_url", url = url.clone());
    vec![simple(
      "url:open",
      move |_| Meta {
        title: title.clone().into(),
        icon: Icon::Tile(IconName::Link, 210.),
        ..Default::default()
      },
      move |_| {
        let url = url.clone();
        vec![Action::sync(
          "open",
          t!("app.launcher.web.open"),
          move |_, _, cx| {
            cx.open_url(&url);
            Ok(Outcome::close())
          },
        )]
      },
    )]
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn decodes() {
    assert_eq!(decode("a%20b%C3%A4"), "a bä");
    assert_eq!(decode("100%"), "100%");
    assert_eq!(decode("%zz"), "%zz");
  }

  #[test]
  fn encodes() {
    assert_eq!(encode("rust gpui & co"), "rust%20gpui%20%26%20co");
    assert_eq!(encode("ä"), "%C3%A4");
  }

  #[test]
  fn urls() {
    assert_eq!(as_url("github.com").as_deref(), Some("https://github.com"));
    assert_eq!(
      as_url("https://x.org/a?b").as_deref(),
      Some("https://x.org/a?b")
    );
    assert_eq!(
      as_url("localhost:8080").as_deref(),
      Some("https://localhost:8080")
    );
    assert_eq!(
      as_url("example.com/path").as_deref(),
      Some("https://example.com/path")
    );
    for no in [
      "firefox",
      "1.5",
      "2^10 / 3",
      "a.b c",
      "file.",
      "v1.2",
      "javascript:alert",
    ] {
      assert_eq!(as_url(no), None, "{no}");
    }
  }
}

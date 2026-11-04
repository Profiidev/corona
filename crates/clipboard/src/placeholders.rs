//! Raycast's placeholders in snippets and quicklinks:
//! `{clipboard offset=1 | trim | uppercase}`, `{date offset="+2d" format="yyyy-MM-dd"}`,
//! `{argument name="term" default="nixpkgs"}`, `{snippet name="sig"}` and friends

use std::collections::HashMap;

use jiff::{Span, Zoned};

#[derive(Clone, Debug, PartialEq)]
pub struct Placeholder {
  pub name: String,
  pub attrs: HashMap<String, String>,
  pub modifiers: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
enum Piece {
  Text(String),
  Placeholder(Placeholder),
}

const NAMES: [&str; 11] = [
  "clipboard",
  "selection",
  "date",
  "time",
  "datetime",
  "day",
  "uuid",
  "cursor",
  "argument",
  "snippet",
  "calculator",
];

/// `name key="value" key=value | mod | mod`
fn parse_placeholder(inner: &str) -> Option<Placeholder> {
  let mut parts = split_unquoted(inner, '|').into_iter();
  let head = parts.next()?.trim().to_string();
  let modifiers = parts.map(|m| m.trim().to_lowercase()).collect();
  let mut words = split_unquoted(&head, ' ')
    .into_iter()
    .filter(|w| !w.is_empty());
  let name = words.next()?.to_lowercase();
  if !NAMES.contains(&name.as_str()) {
    return None;
  }
  let mut attrs = HashMap::new();
  for word in words {
    let (key, value) = word.split_once('=')?;
    attrs.insert(
      key.trim().to_lowercase(),
      value.trim().trim_matches('"').to_string(),
    );
  }
  Some(Placeholder {
    name,
    attrs,
    modifiers,
  })
}

/// Splits on `sep` outside double quotes
fn split_unquoted(text: &str, sep: char) -> Vec<String> {
  let mut out = vec![String::new()];
  let mut quoted = false;
  for c in text.chars() {
    match c {
      '"' => {
        quoted = !quoted;
        out.last_mut().expect("never empty").push(c);
      }
      c if c == sep && !quoted => out.push(String::new()),
      c => out.last_mut().expect("never empty").push(c),
    }
  }
  out
}

fn parse(template: &str) -> Vec<Piece> {
  let mut pieces = Vec::new();
  let mut text = String::new();
  let mut rest = template;
  while let Some(start) = rest.find('{') {
    text.push_str(&rest[..start]);
    let after = &rest[start + 1..];
    // the closing brace outside quotes
    let mut quoted = false;
    let end = after.char_indices().find_map(|(i, c)| match c {
      '"' => {
        quoted = !quoted;
        None
      }
      '}' if !quoted => Some(i),
      _ => None,
    });
    match end.and_then(|end| Some((end, parse_placeholder(&after[..end])?))) {
      Some((end, placeholder)) => {
        if !text.is_empty() {
          pieces.push(Piece::Text(std::mem::take(&mut text)));
        }
        pieces.push(Piece::Placeholder(placeholder));
        rest = &after[end + 1..];
      }
      None => {
        text.push('{');
        rest = after;
      }
    }
  }
  text.push_str(rest);
  if !text.is_empty() {
    pieces.push(Piece::Text(text));
  }
  pieces
}

/// The `{argument}` fields a template asks for, in order, including those of the
/// snippets it includes; unnamed ones are numbered
pub fn arguments(template: &str, snippet: &dyn Fn(&str) -> Option<String>) -> Vec<Placeholder> {
  let mut out = Vec::new();
  collect(template, snippet, 0, &mut 0, &mut out);
  out
}

fn collect(
  template: &str,
  snippet: &dyn Fn(&str) -> Option<String>,
  depth: usize,
  unnamed: &mut usize,
  out: &mut Vec<Placeholder>,
) {
  for piece in parse(template) {
    let Piece::Placeholder(mut p) = piece else {
      continue;
    };
    match p.name.as_str() {
      "argument" => {
        let name = match p.attrs.get("name") {
          Some(name) => name.clone(),
          None => {
            *unnamed += 1;
            format!("argument {unnamed}")
          }
        };
        if out.iter().any(|o| o.attrs["name"] == name) {
          continue;
        }
        p.attrs.insert("name".into(), name);
        out.push(p);
      }
      "snippet" if depth < 5 => {
        if let Some(text) = p.attrs.get("name").and_then(|n| snippet(n)) {
          collect(&text, snippet, depth + 1, unnamed, out);
        }
      }
      _ => {}
    }
  }
}

/// What placeholders read from
pub struct Env<'a> {
  pub now: Zoned,
  pub clipboard: &'a dyn Fn(usize) -> Option<String>,
  pub selection: Option<String>,
  /// Argument values by name, unnamed ones as `argument 1`, `argument 2`
  pub arguments: HashMap<String, String>,
  pub snippet: &'a dyn Fn(&str) -> Option<String>,
  pub calculate: &'a dyn Fn(&str) -> Option<String>,
  /// Values put into URLs are percent-encoded unless `raw`
  pub url: bool,
}

/// Unicode date patterns like `yyyy-MM-dd HH:mm` as strftime, a subset: `'text'` is
/// literal, `''` a quote
fn strftime(pattern: &str) -> String {
  const TOKENS: [(&str, &str); 17] = [
    ("yyyy", "%Y"),
    ("yy", "%y"),
    ("MMMM", "%B"),
    ("MMM", "%b"),
    ("MM", "%m"),
    ("M", "%-m"),
    ("dd", "%d"),
    ("d", "%-d"),
    ("EEEE", "%A"),
    ("EEE", "%a"),
    ("HH", "%H"),
    ("H", "%-H"),
    ("hh", "%I"),
    ("h", "%-I"),
    ("mm", "%M"),
    ("ss", "%S"),
    ("a", "%p"),
  ];
  let mut out = String::new();
  let mut rest = pattern;
  'outer: while !rest.is_empty() {
    if let Some(after) = rest.strip_prefix('\'') {
      // `''` is one quote, otherwise literal up to the closing quote
      let (literal, after) = match after.strip_prefix('\'') {
        Some(after) => ("'", after),
        None => after.split_once('\'').unwrap_or((after, "")),
      };
      out.push_str(&literal.replace('%', "%%"));
      rest = after;
      continue;
    }
    for (token, spec) in TOKENS {
      if let Some(after) = rest.strip_prefix(token) {
        out.push_str(spec);
        rest = after;
        continue 'outer;
      }
    }
    let c = rest.chars().next().expect("not empty");
    if c == '%' {
      out.push_str("%%");
    } else {
      out.push(c);
    }
    rest = &rest[c.len_utf8()..];
  }
  out
}

fn shifted(now: &Zoned, offset: Option<&String>) -> Option<Zoned> {
  let Some(offset) = offset else {
    return Some(now.clone());
  };
  let negative = offset.trim().starts_with('-');
  let span: Span = offset.trim().trim_start_matches(['+', '-']).parse().ok()?;
  let span = if negative { span.negate() } else { span };
  now.checked_add(span).ok()
}

fn modify(value: String, modifiers: &[String], url: bool) -> String {
  let mut value = value;
  let mut raw = false;
  for m in modifiers {
    value = match m.as_str() {
      "uppercase" => value.to_uppercase(),
      "lowercase" => value.to_lowercase(),
      "trim" => value.trim().to_string(),
      "percent-encode" => corona_launcher::commands::web::encode(&value),
      "json-stringify" => serde_json::to_string(&value).unwrap_or(value),
      "raw" => {
        raw = true;
        value
      }
      _ => value,
    };
  }
  match url && !raw && !modifiers.iter().any(|m| m == "percent-encode") {
    true => corona_launcher::commands::web::encode(&value),
    false => value,
  }
}

/// Expansion stops past this many bytes of output
const MAX_OUTPUT: usize = 1024 * 1024;
/// Expansion stops past this many placeholders
const MAX_PLACEHOLDERS: usize = 1000;

fn value(
  p: &Placeholder,
  env: &Env,
  depth: usize,
  budget: &mut usize,
  unnamed: &mut usize,
) -> Option<String> {
  let attr = |k: &str| p.attrs.get(k);
  let value = match p.name.as_str() {
    "clipboard" => {
      let offset = attr("offset").and_then(|o| o.parse().ok()).unwrap_or(0);
      (env.clipboard)(offset).unwrap_or_default()
    }
    "selection" => env.selection.clone().unwrap_or_default(),
    "date" | "time" | "datetime" | "day" => {
      let at = shifted(&env.now, attr("offset"))?;
      let format = match (attr("format"), p.name.as_str()) {
        (Some(f), _) => strftime(f),
        (None, "date") => "%-d %b %Y".into(),
        (None, "time") => "%H:%M".into(),
        (None, "datetime") => "%-d %b %Y %H:%M".into(),
        _ => "%A".into(),
      };
      at.strftime(&format).to_string()
    }
    "uuid" => uuid::Uuid::new_v4().to_string(),
    "cursor" => String::new(),
    "argument" => {
      let name = attr("name")?;
      env
        .arguments
        .get(name)
        .filter(|v| !v.is_empty())
        .or(attr("default"))
        .cloned()
        .unwrap_or_default()
    }
    "snippet" if depth < 5 => {
      let text = (env.snippet)(attr("name")?)?;
      expand_at(&text, env, depth + 1, budget, unnamed)
    }
    "calculator" => (env.calculate)(attr("expression")?).unwrap_or_default(),
    _ => return None,
  };
  Some(modify(value, &p.modifiers, env.url && p.name != "snippet"))
}

/// Snippets can reference each other, so the placeholders expanded and the text produced
/// are capped for the whole expansion, and `unnamed` arguments are numbered across them
fn expand_at(
  template: &str,
  env: &Env,
  depth: usize,
  budget: &mut usize,
  unnamed: &mut usize,
) -> String {
  let mut out = String::new();
  for piece in parse(template) {
    if out.len() >= MAX_OUTPUT {
      break;
    }
    match piece {
      Piece::Text(text) => out.push_str(&text),
      Piece::Placeholder(mut p) => {
        if *budget == 0 {
          break;
        }
        *budget -= 1;
        if p.name == "argument" && !p.attrs.contains_key("name") {
          *unnamed += 1;
          p.attrs.insert("name".into(), format!("argument {unnamed}"));
        }
        out.push_str(&value(&p, env, depth, budget, unnamed).unwrap_or_default());
      }
    }
  }
  out
}

/// The template with every placeholder replaced
pub fn expand(template: &str, env: &Env) -> String {
  let mut budget = MAX_PLACEHOLDERS;
  expand_at(template, env, 0, &mut budget, &mut 0)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn env<'a>(
    arguments: &[(&str, &str)],
    clipboard: &'a dyn Fn(usize) -> Option<String>,
    snippet: &'a dyn Fn(&str) -> Option<String>,
    url: bool,
  ) -> Env<'a> {
    Env {
      now: "2026-10-23T14:05[Europe/Berlin]".parse().unwrap(),
      clipboard,
      selection: Some("picked".into()),
      arguments: arguments
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect(),
      snippet,
      calculate: &|e| (e == "2+2").then(|| "4".to_string()),
      url,
    }
  }

  #[test]
  fn expands() {
    let clip = |i: usize| ["  newest ", "older"].get(i).map(|s| s.to_string());
    let snippet = |n: &str| (n == "sig").then(|| "-- {selection}".to_string());
    let e = env(&[("argument 1", "x")], &clip, &snippet, false);
    let x = |t| expand(t, &e);
    assert_eq!(x("{clipboard | trim | uppercase}!"), "NEWEST!");
    assert_eq!(x("{clipboard offset=1}"), "older");
    assert_eq!(x("{date}"), "23 Oct 2026");
    assert_eq!(
      x("{date offset=\"+2d\" format=\"yyyy-MM-dd\"}"),
      "2026-10-25"
    );
    assert_eq!(x("{time}"), "14:05");
    assert_eq!(x("{day offset=\"-1d\"}"), "Thursday");
    assert_eq!(x("a {argument} b"), "a x b");
    assert_eq!(x("{snippet name=\"sig\"}"), "-- picked");
    assert_eq!(x("{calculator expression=\"2+2\"}"), "4");
    assert_eq!(x("{cursor}end"), "end");
    assert_eq!(x("{clipboard | json-stringify}"), "\"  newest \"");
    // not placeholders stay as they are
    assert_eq!(x("{unknown} {"), "{unknown} {");
    assert_eq!(x("{uuid}").len(), 36);
  }

  #[test]
  fn urls_encode_unless_raw() {
    let clip = |_: usize| None;
    let none = |_: &str| None;
    let e = env(&[("term", "rust gpui")], &clip, &none, true);
    let template = "https://x.org/?q={argument name=\"term\"}";
    assert_eq!(expand(template, &e), "https://x.org/?q=rust%20gpui");
    assert_eq!(expand("{argument name=\"term\" | raw}", &e), "rust gpui");
    let defaulted = env(&[], &clip, &none, true);
    assert_eq!(
      expand("{argument name=\"t\" default=\"nix\"}", &defaulted),
      "nix"
    );
  }

  #[test]
  fn finds_arguments() {
    let args = arguments(
      "{argument name=\"a\"} {argument} {argument name=\"a\"} {argument default=\"d\"}",
      &|_| None,
    );
    let names: Vec<&str> = args.iter().map(|a| a.attrs["name"].as_str()).collect();
    assert_eq!(names, ["a", "argument 1", "argument 2"]);
    assert_eq!(args[2].attrs.get("default").map(String::as_str), Some("d"));
  }

  #[test]
  fn nested_arguments() {
    let snippet = |n: &str| (n == "s").then(|| "{argument} {argument name=\"b\"}".to_string());
    let args = arguments("{argument} {snippet name=\"s\"}", &snippet);
    let names: Vec<&str> = args.iter().map(|a| a.attrs["name"].as_str()).collect();
    assert_eq!(names, ["argument 1", "argument 2", "b"]);
    let clip = |_: usize| None;
    let e = env(
      &[("argument 1", "x"), ("argument 2", "y"), ("b", "z")],
      &clip,
      &snippet,
      false,
    );
    assert_eq!(expand("{argument} {snippet name=\"s\"}", &e), "x y z");
  }

  #[test]
  fn date_patterns() {
    assert_eq!(strftime("yyyy-MM-dd HH:mm"), "%Y-%m-%d %H:%M");
    assert_eq!(strftime("EEEE, d MMMM"), "%A, %-d %B");
    assert_eq!(strftime("d 'at' HH''mm '%'"), "%-d at %H'%M %%");
  }
}

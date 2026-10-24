use std::cell::RefCell;

use nucleo_matcher::{
  Config, Matcher, Utf32Str,
  pattern::{CaseMatching, Normalization, Pattern},
};

use crate::command::Meta;

/// How a command matched, best first; ranking never lets a lower tier outrank a higher one
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
  Alias,
  Exact,
  Prefix,
  WordPrefix,
  Substring,
  Keyword,
  Fuzzy,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Match {
  pub tier: Tier,
  /// Within a tier, higher is better
  pub score: u32,
  /// Char indices into the title to highlight
  pub indices: Vec<u32>,
  /// Text after the command name when the query carries inline arguments (`vol 40`)
  pub args: Option<String>,
}

impl Match {
  pub fn new(tier: Tier) -> Self {
    Self {
      tier,
      score: 0,
      indices: Vec::new(),
      args: None,
    }
  }
}

/// The query as typed plus its normalized form
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Query {
  pub raw: String,
  pub text: String,
}

impl Query {
  pub fn new(raw: impl Into<String>) -> Self {
    let raw = raw.into();
    let text = raw.trim().to_lowercase();
    Self { raw, text }
  }

  pub fn is_empty(&self) -> bool {
    self.text.is_empty()
  }

  /// The first word and the trimmed rest, for `name args` queries
  pub fn split_first(&self) -> Option<(&str, &str)> {
    let raw = self.raw.trim_start();
    let (head, rest) = raw.split_once(char::is_whitespace)?;
    Some((head, rest.trim_start()))
  }
}

thread_local! {
  static MATCHER: RefCell<Matcher> = RefCell::new(Matcher::new(Config::DEFAULT));
}

/// Fuzzy score and title char indices of `needle` in `haystack`
pub fn fuzzy(needle: &str, haystack: &str) -> Option<(u32, Vec<u32>)> {
  let pattern = Pattern::parse(needle, CaseMatching::Ignore, Normalization::Smart);
  MATCHER.with_borrow_mut(|matcher| {
    let mut buf = Vec::new();
    let mut indices = Vec::new();
    let score = pattern.indices(Utf32Str::new(haystack, &mut buf), matcher, &mut indices)?;
    indices.sort_unstable();
    indices.dedup();
    Some((score, indices))
  })
}

/// Char range of `needle` (already lowercase) in `haystack`, case-insensitive
fn find(haystack: &str, needle: &str) -> Option<(usize, Vec<u32>)> {
  let lower = haystack.to_lowercase();
  // lowercasing can change byte lengths, so map through chars
  if lower.chars().count() != haystack.chars().count() {
    return None;
  }
  let byte = lower.find(needle)?;
  let start = lower[..byte].chars().count();
  let len = needle.chars().count();
  Some((start, (start..start + len).map(|i| i as u32).collect()))
}

fn word_start(haystack: &str, char_index: usize) -> bool {
  char_index == 0
    || haystack
      .chars()
      .nth(char_index - 1)
      .is_some_and(|c| !c.is_alphanumeric())
}

/// Tier of `query` (lowercase, non-empty) against one title, without keywords and aliases
pub fn match_text(title: &str, query: &str) -> Option<Match> {
  let lower = title.to_lowercase();
  let all = |n: usize| (0..n as u32).collect::<Vec<_>>();
  if lower == query {
    return Some(Match {
      indices: all(title.chars().count()),
      ..Match::new(Tier::Exact)
    });
  }
  if lower.starts_with(query) {
    return Some(Match {
      // shorter titles fit the query better
      score: u32::MAX - title.len() as u32,
      indices: all(query.chars().count()),
      ..Match::new(Tier::Prefix)
    });
  }
  if let Some((start, indices)) = find(title, query) {
    let tier = match word_start(title, start) {
      true => Tier::WordPrefix,
      false => Tier::Substring,
    };
    return Some(Match {
      score: u32::MAX - start as u32,
      indices,
      ..Match::new(tier)
    });
  }
  // every word of the query starts a word of the title, in order: `fi pr` for "Firefox Private"
  let words: Vec<&str> = query.split_whitespace().collect();
  if words.len() > 1 {
    let title_words: Vec<String> = lower
      .split(|c: char| !c.is_alphanumeric())
      .filter(|w| !w.is_empty())
      .map(str::to_string)
      .collect();
    let mut at = 0;
    let all_found =
      words.iter().all(
        |w| match title_words[at..].iter().position(|t| t.starts_with(w)) {
          Some(i) => {
            at += i + 1;
            true
          }
          None => false,
        },
      );
    if all_found {
      return Some(Match::new(Tier::WordPrefix));
    }
  }
  None
}

/// The default ranking: alias > exact > prefix > word-prefix > substring > keyword > fuzzy.
/// Commands with arguments also match `name args` (`vol 40`, `theme nord`).
pub fn default_match(meta: &Meta, aliases: &[String], query: &Query) -> Option<Match> {
  if query.is_empty() {
    return Some(Match::new(Tier::Fuzzy));
  }
  if let Some(m) = match_whole(meta, aliases, &query.text) {
    return Some(m);
  }
  if meta.args.is_empty() {
    return None;
  }
  let (head, rest) = query.split_first()?;
  let head = head.to_lowercase();
  // the head must name the command closely, fuzzy heads would swallow normal searches
  let mut m = match_whole(meta, aliases, &head).filter(|m| m.tier <= Tier::Keyword)?;
  m.args = Some(rest.to_string());
  Some(m)
}

fn match_whole(meta: &Meta, aliases: &[String], query: &str) -> Option<Match> {
  if aliases.iter().any(|a| a.eq_ignore_ascii_case(query)) {
    return Some(Match::new(Tier::Alias));
  }
  let title = match_text(&meta.title, query);
  if title.as_ref().is_some_and(|m| m.tier < Tier::Keyword) {
    return title;
  }
  let keyword = meta
    .keywords
    .iter()
    .filter_map(|k| match_text(k, query))
    .filter(|m| m.tier <= Tier::WordPrefix)
    .min_by_key(|m| m.tier);
  if keyword.is_some() {
    return Some(Match {
      score: 0,
      indices: Vec::new(),
      ..Match::new(Tier::Keyword)
    });
  }
  fuzzy(query, &meta.title).map(|(score, indices)| Match {
    score,
    indices,
    ..Match::new(Tier::Fuzzy)
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  fn meta(title: &str, keywords: &[&str]) -> Meta {
    Meta {
      title: title.to_string().into(),
      keywords: keywords.iter().map(|k| k.to_string().into()).collect(),
      ..Default::default()
    }
  }

  fn tier(title: &str, query: &str) -> Option<Tier> {
    default_match(
      &meta(title, &["web browser"]),
      &["ff".into()],
      &Query::new(query),
    )
    .map(|m| m.tier)
  }

  #[test]
  fn tiers() {
    assert_eq!(tier("Firefox", "ff"), Some(Tier::Alias));
    assert_eq!(tier("Firefox", "firefox"), Some(Tier::Exact));
    assert_eq!(tier("Firefox", "Fire"), Some(Tier::Prefix));
    assert_eq!(tier("Mozilla Firefox", "fire"), Some(Tier::WordPrefix));
    assert_eq!(tier("Firefox", "fox"), Some(Tier::Substring));
    assert_eq!(tier("Firefox", "browser"), Some(Tier::Keyword));
    assert_eq!(tier("Firefox", "frfx"), Some(Tier::Fuzzy));
    assert_eq!(tier("Firefox", "zzz"), None);
    assert_eq!(
      tier("Firefox Private Window", "fi pr"),
      Some(Tier::WordPrefix)
    );
  }

  #[test]
  fn tiers_are_ordered() {
    let order = [
      Tier::Alias,
      Tier::Exact,
      Tier::Prefix,
      Tier::WordPrefix,
      Tier::Substring,
      Tier::Keyword,
      Tier::Fuzzy,
    ];
    assert!(order.windows(2).all(|w| w[0] < w[1]));
  }

  #[test]
  fn prefix_indices_and_shorter_wins() {
    let a = match_text("Volume", "vol").unwrap();
    let b = match_text("Volume Presets", "vol").unwrap();
    assert_eq!(a.indices, vec![0, 1, 2]);
    assert!(a.score > b.score);
  }

  #[test]
  fn inline_args() {
    let mut m = meta("Volume", &[]);
    m.args = vec![crate::args::ArgSpec::level("level")];
    let found = default_match(&m, &["vol".into()], &Query::new("vol 40")).unwrap();
    assert_eq!(found.tier, Tier::Alias);
    assert_eq!(found.args.as_deref(), Some("40"));
    let found = default_match(&m, &[], &Query::new("volume  +5")).unwrap();
    assert_eq!(found.args.as_deref(), Some("+5"));
    // no args spec, no split
    assert!(default_match(&meta("Volume", &[]), &[], &Query::new("volume 40")).is_none());
  }

  #[test]
  fn empty_query_matches_everything() {
    assert!(default_match(&meta("X", &[]), &[], &Query::new("  ")).is_some());
  }

  #[test]
  fn fuzzy_indices() {
    let (_, indices) = fuzzy("frfx", "Firefox").unwrap();
    assert_eq!(indices.len(), 4);
  }
}

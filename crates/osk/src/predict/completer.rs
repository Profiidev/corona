//! Prefix completion over a ranked wordlist

use std::collections::HashMap;

pub struct Completer {
  /// (lowercase word, score) sorted by word, for prefix range lookups
  words: Vec<(String, u32)>,
  /// How often the user typed each word, lowercase
  pub(super) learned: HashMap<String, u32>,
}

impl Completer {
  /// `list` has one word per line, most frequent first
  pub fn from_ranked_list(list: &str) -> Self {
    let n = list.lines().count() as u32;
    let mut words: Vec<(String, u32)> = list
      .lines()
      .enumerate()
      .filter(|(_, w)| !w.trim().is_empty())
      .map(|(i, w)| (w.trim().to_lowercase(), n - i as u32))
      .collect();
    // stable, so the first (most frequent) of duplicates is kept
    words.sort_by(|a, b| a.0.cmp(&b.0));
    words.dedup_by(|a, b| a.0 == b.0);
    Self {
      words,
      learned: HashMap::new(),
    }
  }

  pub fn contains(&self, word: &str) -> bool {
    self
      .words
      .binary_search_by(|(w, _)| w.as_str().cmp(word))
      .is_ok()
  }

  /// Up to `limit` completions of `prefix`, best first: words the user typed
  /// by how often, then the list by frequency. Their case follows the prefix
  pub fn complete(&self, prefix: &str, limit: usize) -> Vec<String> {
    if prefix.is_empty() {
      return Vec::new();
    }
    let p = prefix.to_lowercase();
    let start = self.words.partition_point(|(w, _)| w.as_str() < p.as_str());
    let learned = |w: &str| self.learned.get(w).copied().unwrap_or(0);
    // (word, times typed, frequency)
    let mut hits: Vec<(&str, u32, u32)> = self.words[start..]
      .iter()
      .take_while(|(w, _)| w.starts_with(&p))
      // what is typed already is no suggestion
      .filter(|(w, _)| *w != p)
      .map(|(w, s)| (w.as_str(), learned(w), *s))
      .collect();
    for (w, count) in &self.learned {
      if w.starts_with(&p) && *w != p && !self.contains(w) {
        hits.push((w.as_str(), *count, 0));
      }
    }
    hits.sort_by(|a, b| (b.1, b.2).cmp(&(a.1, a.2)).then(a.0.cmp(b.0)));
    hits
      .into_iter()
      .take(limit)
      .map(|(w, ..)| apply_case(prefix, w))
      .collect()
  }

  /// Counts a word the user typed; only words of letters, two or longer
  pub fn learn(&mut self, word: &str) {
    let w = word.to_lowercase();
    if w.chars().count() >= 2 && w.chars().all(|c| c.is_alphabetic() || c == '\'') {
      *self.learned.entry(w).or_insert(0) += 1;
    }
  }
}

/// `word` in the case of what was typed: `He` + `hello` is `Hello`, `HE` is `HELLO`
pub fn apply_case(typed: &str, word: &str) -> String {
  let mut chars = typed.chars();
  let first_up = chars.next().is_some_and(char::is_uppercase);
  let rest_up = typed.chars().count() > 1 && chars.all(char::is_uppercase);
  if first_up && rest_up {
    word.to_uppercase()
  } else if first_up {
    let mut c = word.chars();
    c.next()
      .map(|f| f.to_uppercase().chain(c).collect())
      .unwrap_or_default()
  } else {
    word.to_string()
  }
}

/// The word being typed, the trailing run of letters and inner `'` before the
/// cursor, with the byte it starts at
pub fn current_word(before_cursor: &str) -> (usize, &str) {
  let start = before_cursor
    .char_indices()
    .rev()
    .take_while(|(_, c)| c.is_alphabetic() || *c == '\'')
    .last()
    .map_or(before_cursor.len(), |(i, _)| i);
  let word = before_cursor[start..].trim_start_matches('\'');
  (before_cursor.len() - word.len(), word)
}

#[cfg(test)]
mod tests {
  use super::*;

  const LIST: &str = "the\nto\nand\nthat\nthis\nthere\nthen\nhello\nhelp\nkeyboard\nkey\n";

  #[test]
  fn completes_in_rank_order_and_case() {
    let c = Completer::from_ranked_list(LIST);
    assert_eq!(c.complete("Th", 3), ["The", "That", "This"]);
    assert_eq!(c.complete("HE", 3), ["HELLO", "HELP"]);
    assert_eq!(c.complete("key", 3), ["keyboard"]);
    assert!(c.complete("", 3).is_empty());
  }

  #[test]
  fn learned_words_rise_and_need_not_be_listed() {
    let mut c = Completer::from_ranked_list(LIST);
    c.learn("thesis");
    c.learn("thesis");
    assert_eq!(c.complete("the", 3), ["thesis", "there", "then"]);
    c.learn("x");
    c.learn("a1");
    assert_eq!(c.learned.len(), 1);
  }

  #[test]
  fn german_works_unchanged() {
    let c = Completer::from_ranked_list("der\ndie\ndas\nschön\nschon\nschnell\nüber\n");
    assert_eq!(c.complete("sch", 3), ["schön", "schon", "schnell"]);
    assert_eq!(c.complete("Ü", 1), ["Über"]);
    assert_eq!(apply_case("STR", "straße"), "STRASSE");
  }

  #[test]
  fn current_word_takes_the_trailing_letters() {
    assert_eq!(current_word("say Th"), (4, "Th"));
    assert_eq!(current_word("I don't"), (2, "don't"));
    assert_eq!(current_word("'quote"), (1, "quote"));
    assert_eq!(current_word("end. "), (5, ""));
    assert_eq!(current_word("grüße"), (0, "grüße"));
  }
}

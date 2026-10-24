//! Spell correction: the dictionary words one or two edits away from the
//! typed one, ranked by frequency. Slow at two edits, so only on word ends.

use std::collections::HashMap;

use super::completer::apply_case;

/// Longer words skip distance two, its edits grow too many
const MAX_DISTANCE_TWO: usize = 12;

pub struct Speller {
  /// lowercase word -> score, higher is more frequent
  dict: HashMap<String, u32>,
  /// Every letter in the list, so umlauts and ß come with a German one
  alphabet: Vec<char>,
}

impl Speller {
  /// `list` has one word per line, most frequent first
  pub fn from_ranked_list(list: &str) -> Self {
    let words: Vec<&str> = list
      .lines()
      .map(str::trim)
      .filter(|w| !w.is_empty())
      .collect();
    let n = words.len() as u32;
    let mut dict = HashMap::with_capacity(words.len());
    for (i, w) in words.iter().enumerate() {
      dict.entry(w.to_lowercase()).or_insert(n - i as u32);
    }
    let mut alphabet: Vec<char> = dict
      .keys()
      .flat_map(|w| w.chars())
      .filter(|c| c.is_alphabetic())
      .collect();
    alphabet.sort_unstable();
    alphabet.dedup();
    Self { dict, alphabet }
  }

  pub fn is_known(&self, word: &str) -> bool {
    self.dict.contains_key(&word.to_lowercase())
  }

  /// Corrections of a word, none when it is known or shorter than three letters.
  /// One edit away comes before two, then the more frequent.
  pub fn suggest(&self, word: &str, limit: usize) -> Vec<String> {
    let w = word.to_lowercase();
    if w.chars().count() < 3 || self.dict.contains_key(&w) {
      return Vec::new();
    }
    let e1 = self.edits1(&w);
    let mut out = self.known_sorted(&e1);
    if out.len() < limit && w.chars().count() <= MAX_DISTANCE_TWO {
      let e2: Vec<String> = e1.iter().flat_map(|e| self.edits1(e)).collect();
      for c in self.known_sorted(&e2) {
        if !out.contains(&c) {
          out.push(c);
        }
      }
    }
    out.truncate(limit);
    out.into_iter().map(|c| apply_case(word, &c)).collect()
  }

  fn known_sorted(&self, candidates: &[String]) -> Vec<String> {
    let mut known: Vec<(&String, u32)> = candidates
      .iter()
      .filter_map(|c| self.dict.get(c).map(|s| (c, *s)))
      .collect();
    known.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    known.dedup_by(|a, b| a.0 == b.0);
    known.into_iter().map(|(c, _)| c.clone()).collect()
  }

  /// Every string one delete, swap, replace or insert away
  fn edits1(&self, w: &str) -> Vec<String> {
    let ch: Vec<char> = w.chars().collect();
    let n = ch.len();
    let mut out = Vec::with_capacity((2 * self.alphabet.len() + 2) * (n + 1));
    for i in 0..=n {
      if i < n {
        let mut d = ch.clone();
        d.remove(i);
        out.push(d.into_iter().collect());
      }
      if i + 1 < n {
        let mut t = ch.clone();
        t.swap(i, i + 1);
        out.push(t.into_iter().collect());
      }
      for &a in &self.alphabet {
        if i < n {
          let mut r = ch.clone();
          r[i] = a;
          out.push(r.into_iter().collect());
        }
        let mut ins = ch.clone();
        ins.insert(i, a);
        out.push(ins.into_iter().collect());
      }
    }
    out
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn corrects_one_and_two_edits() {
    let s = Speller::from_ranked_list(include_str!("../../../../assets/wordlists/wordlist-en.txt"));
    assert_eq!(s.suggest("teh", 1), ["the"]);
    assert_eq!(s.suggest("keybord", 1), ["keyboard"]);
    assert_eq!(s.suggest("recieve", 1), ["receive"]);
    assert_eq!(s.suggest("Teh", 1), ["The"]);
    assert!(s.suggest("hello", 3).is_empty());
    assert!(s.suggest("xy", 3).is_empty());
  }

  #[test]
  fn german_umlauts_are_in_the_alphabet() {
    let s = Speller::from_ranked_list(include_str!("../../../../assets/wordlists/wordlist-de.txt"));
    assert_eq!(s.suggest("shön", 1), ["schön"]);
    assert_eq!(s.suggest("nciht", 1), ["nicht"]);
  }
}

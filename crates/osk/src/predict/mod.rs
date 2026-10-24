//! Word completion and spell check for the suggestion strip

mod completer;
mod spell;

use std::{fs, path::PathBuf};

use anyhow::Result;

use completer::Completer;
pub use completer::current_word;
use spell::Speller;

/// Learned words kept, the least typed go first
const LEARNED_CAP: usize = 5000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
  En,
  De,
}

impl Language {
  pub const ALL: [Language; 2] = [Language::En, Language::De];

  pub fn code(self) -> &'static str {
    match self {
      Language::En => "en",
      Language::De => "de",
    }
  }

  /// Its own name, on the space bar
  pub fn name(self) -> &'static str {
    match self {
      Language::En => "English",
      Language::De => "Deutsch",
    }
  }

  /// One word per line, most frequent first; hermitdave/FrequencyWords, CC-BY-SA 4.0
  fn wordlist(self) -> &'static str {
    match self {
      Language::En => include_str!("../../../../assets/wordlists/wordlist-en.txt"),
      Language::De => include_str!("../../../../assets/wordlists/wordlist-de.txt"),
    }
  }
}

/// A suggestion: tapping it replaces `delete` before the cursor with `insert`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chip {
  pub label: String,
  /// The word it stands for, learned when tapped
  pub word: String,
  pub delete: String,
  pub insert: String,
}

/// The strip's three slots; the best suggestion is in the middle
pub type Chips = [Option<Chip>; 3];

pub struct Predictor {
  pub language: Language,
  completer: Completer,
  speller: Speller,
}

impl Predictor {
  /// The language's wordlist with the words learned before; a few ms, not on the UI
  pub fn new(language: Language) -> Self {
    let list = language.wordlist();
    let mut predictor = Self {
      language,
      completer: Completer::from_ranked_list(list),
      speller: Speller::from_ranked_list(list),
    };
    if let Some(text) = learned_path(language).and_then(|p| fs::read_to_string(p).ok()) {
      predictor.completer.learned = parse_learned(&text);
    }
    predictor
  }

  /// What to offer for the text before the cursor: completions of the word
  /// being typed, or corrections of the word just ended
  pub fn predict(&self, before_cursor: &str, complete: bool, spell: bool) -> Chips {
    let (_, word) = current_word(before_cursor);
    if !word.is_empty() {
      if !complete {
        return Chips::default();
      }
      let mut ranked = self.completer.complete(word, 3).into_iter().map(|c| Chip {
        label: c.clone(),
        insert: format!("{c} "),
        word: c,
        delete: word.to_string(),
      });
      let (first, second, third) = (ranked.next(), ranked.next(), ranked.next());
      return [second, first, third];
    }

    let ended = before_cursor.trim_end_matches(|c: char| !c.is_alphabetic() && c != '\'');
    let boundary = &before_cursor[ended.len()..];
    let (_, word) = current_word(ended);
    if !spell || boundary.chars().count() > 2 || !self.misspelled(word) {
      return Chips::default();
    }
    let mut fixes = self.speller.suggest(word, 2).into_iter().map(|c| Chip {
      label: c.clone(),
      insert: format!("{c}{boundary}"),
      word: c,
      delete: format!("{word}{boundary}"),
    });
    let Some(best) = fixes.next() else {
      return Chips::default();
    };
    let keep = Chip {
      label: format!("\"{word}\""),
      word: word.to_string(),
      delete: String::new(),
      insert: String::new(),
    };
    [Some(keep), Some(best), fixes.next()]
  }

  /// A word worth learning when typed: no misspelling
  pub fn knows(&self, word: &str) -> bool {
    !word.is_empty() && !self.misspelled(word)
  }

  fn misspelled(&self, word: &str) -> bool {
    let lower = word.to_lowercase();
    let chars = word.chars().count();
    let short_caps = chars <= 4 && word.chars().all(char::is_uppercase);
    if chars < 3 || short_caps || self.known(&lower) {
      return false;
    }
    // German compounds: two known words make a known one
    !lower.char_indices().skip(3).any(|(i, _)| {
      let (left, right) = lower.split_at(i);
      right.chars().count() >= 3 && self.known(left) && self.known(right)
    })
  }

  fn known(&self, lower: &str) -> bool {
    self.speller.is_known(lower) || self.completer.learned.contains_key(lower)
  }

  pub fn learn(&mut self, word: &str) {
    self.completer.learn(word);
  }

  /// Writes the learned words, the most typed `LEARNED_CAP` of them
  pub fn save(&self) -> Result<()> {
    let Some(path) = learned_path(self.language) else {
      return Ok(());
    };
    let mut learned: Vec<_> = self.completer.learned.iter().collect();
    learned.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    learned.truncate(LEARNED_CAP);
    let text: String = learned.iter().map(|(w, c)| format!("{w}\t{c}\n")).collect();
    if let Some(dir) = path.parent() {
      fs::create_dir_all(dir)?;
    }
    fs::write(path, text)?;
    Ok(())
  }

  /// Forgets the learned words of every language, here and on disk
  pub fn clear_learned(&mut self) -> Result<()> {
    self.completer.learned.clear();
    for language in Language::ALL {
      if let Some(path) = learned_path(language)
        && path.exists()
      {
        fs::remove_file(path)?;
      }
    }
    Ok(())
  }
}

fn learned_path(language: Language) -> Option<PathBuf> {
  let file = format!("learned-{}.txt", language.code());
  Some(dirs::data_dir()?.join("corona").join("osk").join(file))
}

/// `word<TAB>count` lines; broken ones are skipped
fn parse_learned(text: &str) -> std::collections::HashMap<String, u32> {
  text
    .lines()
    .filter_map(|line| {
      let (word, count) = line.split_once('\t')?;
      Some((word.to_string(), count.parse().ok()?))
    })
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn labels(chips: &Chips) -> Vec<Option<&str>> {
    chips
      .iter()
      .map(|c| c.as_ref().map(|c| c.label.as_str()))
      .collect()
  }

  #[test]
  fn completes_with_the_best_in_the_middle() {
    let p = Predictor::new(Language::En);
    let chips = p.predict("say Th", true, true);
    assert_eq!(labels(&chips), [Some("That"), Some("The"), Some("This")]);
    let best = chips[1].as_ref().unwrap();
    assert_eq!((best.delete.as_str(), best.insert.as_str()), ("Th", "The "));
    assert_eq!(p.predict("say Th", false, true), Chips::default());
  }

  #[test]
  fn corrects_the_word_just_ended() {
    let p = Predictor::new(Language::En);
    let chips = p.predict("say teh ", true, true);
    assert_eq!(labels(&chips)[..2], [Some("\"teh\""), Some("the")]);
    let fix = chips[1].as_ref().unwrap();
    assert_eq!((fix.delete.as_str(), fix.insert.as_str()), ("teh ", "the "));
    assert_eq!(p.predict("say teh ", true, false), Chips::default());
    assert_eq!(p.predict("the ", true, true), Chips::default());
    assert_eq!(p.predict("teh   ", true, true), Chips::default());
  }

  #[test]
  fn compounds_and_learned_words_count_as_known() {
    let mut p = Predictor::new(Language::De);
    assert!(!p.misspelled("Haustür"));
    assert!(p.misspelled("nciht"));
    p.learn("nciht");
    assert!(!p.misspelled("nciht"));
    assert!(!p.misspelled("NCHT"));
  }

  #[test]
  fn learned_words_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    // nextest runs each test in its own process, so setting the environment is safe
    unsafe { std::env::set_var("XDG_DATA_HOME", dir.path()) };
    let mut p = Predictor::new(Language::En);
    p.learn("corona");
    p.learn("corona");
    p.save().unwrap();
    let file = dir.path().join("corona/osk/learned-en.txt");
    assert_eq!(fs::read_to_string(&file).unwrap(), "corona\t2\n");
    let mut again = Predictor::new(Language::En);
    assert_eq!(
      labels(&again.predict("coro", true, true))[1],
      Some("corona")
    );
    again.clear_learned().unwrap();
    assert!(!file.exists());
    assert_eq!(parse_learned("a\t1\nbroken\nb\tx\n").len(), 1);
  }
}

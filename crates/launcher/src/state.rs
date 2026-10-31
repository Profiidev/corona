use std::{
  collections::HashMap,
  path::PathBuf,
  time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use corona_utils::persist::{load_json, save_json};
use serde::{Serialize, de::DeserializeOwned};

/// `~/.local/state/corona/launcher`, where runtime data lives
pub fn dir() -> Option<PathBuf> {
  // tests never touch the user's state
  if cfg!(test) {
    return Some(std::env::temp_dir().join(format!("corona-launcher-test-{}", std::process::id())));
  }
  Some(dirs::state_dir()?.join("corona").join("launcher"))
}

/// `name.json` from the state dir, the default when missing or broken
pub fn load<T: DeserializeOwned + Default>(name: &str) -> T {
  dir()
    .and_then(|d| load_json(&d.join(format!("{name}.json"))))
    .unwrap_or_default()
}

/// Writes `name.json` through a temp file, so a crash keeps the old one
pub fn save<T: Serialize>(name: &str, value: &T) -> Result<()> {
  save_json(
    &dir()
      .context("no state directory")?
      .join(format!("{name}.json")),
    value,
  )
}

pub fn now() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |d| d.as_secs())
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct Use {
  pub count: u32,
  /// Unix seconds
  pub last: u64,
}

const HALF_LIFE_DAYS: f64 = 14.;
const KEEP: usize = 500;

/// How often and how recently each command ran
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct Frecency(pub HashMap<String, Use>);

impl Frecency {
  pub fn record(&mut self, id: &str, now: u64) {
    let entry = self.0.entry(id.to_string()).or_default();
    entry.count += 1;
    entry.last = now;
    if self.0.len() > KEEP {
      self.prune(now);
    }
  }

  /// Uses that halve in weight every two weeks
  pub fn score(&self, id: &str, now: u64) -> f64 {
    self.0.get(id).map_or(0., |u| {
      let days = now.saturating_sub(u.last) as f64 / 86_400.;
      u.count as f64 * 0.5f64.powf(days / HALF_LIFE_DAYS)
    })
  }

  /// Most used first
  pub fn recent(&self, now: u64) -> Vec<(&str, f64)> {
    let mut all: Vec<_> = self
      .0
      .keys()
      .map(|id| (id.as_str(), self.score(id, now)))
      .collect();
    all.sort_by(|a, b| b.1.total_cmp(&a.1));
    all
  }

  fn prune(&mut self, now: u64) {
    let keep: Vec<String> = self
      .recent(now)
      .into_iter()
      .take(KEEP)
      .map(|(id, _)| id.to_string())
      .collect();
    self.0.retain(|id, _| keep.contains(id));
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn decays() {
    let mut f = Frecency::default();
    let day = 86_400;
    f.record("a", 0);
    f.record("a", 0);
    f.record("b", 28 * day);
    // two uses four weeks ago weigh half a use now
    assert!((f.score("a", 28 * day) - 0.5).abs() < 1e-9);
    assert_eq!(f.score("b", 28 * day), 1.);
    assert_eq!(f.score("c", 0), 0.);
    assert_eq!(f.recent(28 * day)[0].0, "b");
  }

  #[test]
  fn prunes_to_the_best() {
    let mut f = Frecency::default();
    for i in 0..=KEEP {
      f.record(&i.to_string(), i as u64);
    }
    assert_eq!(f.0.len(), KEEP);
    assert!(!f.0.contains_key("0"));
  }
}

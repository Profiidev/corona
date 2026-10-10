use std::{
  collections::{BTreeMap, HashSet},
  path::PathBuf,
};

use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::APP_NAME;

/// The source plugins come from when none is configured
pub const OFFICIAL_SOURCE: &str = "official";
pub const OFFICIAL_LOCATION: &str = "https://github.com/ProfiiDev/corona-plugins";
/// The implicit source of the plugins in [`PluginDirs::local`]
pub const LOCAL_SOURCE: &str = "local";

/// Folder name of plugins below the data and state directories
const PLUGINS_DIR: &str = "plugins";

/// Where plugins live on disk
#[derive(Debug, Clone, PartialEq)]
pub struct PluginDirs {
  /// `~/.local/share/corona/plugins`, the user's own plugins, the `local` source
  pub local: PathBuf,
  /// `~/.local/state/corona/plugins`, what the shell fetched and plugin data
  pub state: PathBuf,
}

pub fn plugin_dirs() -> PluginDirs {
  let base = |dir: Option<PathBuf>| {
    dir
      .unwrap_or_else(|| PathBuf::from("."))
      .join(APP_NAME)
      .join(PLUGINS_DIR)
  };
  PluginDirs {
    local: base(dirs::data_dir()),
    state: base(dirs::state_dir()),
  }
}

/// Which plugins run and where they come from
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct PluginsConfig {
  /// Ids of the plugins that run; installed is not enabled
  pub enabled: Vec<String>,
  pub auto_update: AutoUpdate,
  /// Later sources win over earlier ones when both offer a plugin
  #[serde(deserialize_with = "unique_sources")]
  pub source: Vec<SourceConfig>,
  /// What the user agreed each plugin may do, by id. A plugin whose source
  /// or capabilities differ from this does not run until approved again.
  pub approved: BTreeMap<String, Approval>,
}

/// A plugin's grants as the user saw and approved them
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Approval {
  /// The source it ran from
  pub source: String,
  /// Its `[capabilities]` table as TOML text
  pub capabilities: String,
}

impl Default for PluginsConfig {
  fn default() -> Self {
    Self {
      enabled: Vec::new(),
      auto_update: AutoUpdate::default(),
      source: vec![SourceConfig::official()],
      approved: BTreeMap::new(),
    }
  }
}

impl PluginsConfig {
  pub fn is_enabled(&self, id: &str) -> bool {
    self.enabled.iter().any(|enabled| enabled == id)
  }
}

/// Which git sources update on their own, every few hours
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutoUpdate {
  All,
  /// Only the built-in official source
  #[default]
  Official,
  None,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SourceConfig {
  /// Also its directory name, so letters, digits, `.`, `_` and `-` only
  #[serde(deserialize_with = "flat_name")]
  pub name: String,
  pub kind: SourceKind,
  /// A git URL, or a directory (may start with `~/`)
  pub location: String,
  #[serde(default = "enabled")]
  pub enabled: bool,
}

impl SourceConfig {
  pub fn official() -> Self {
    Self {
      name: OFFICIAL_SOURCE.to_string(),
      kind: SourceKind::Git,
      location: OFFICIAL_LOCATION.to_string(),
      enabled: true,
    }
  }

  /// The built-in official source: name and location both match, so a source
  /// that only borrows the name is not trusted like it
  pub fn is_official(&self) -> bool {
    self.kind == SourceKind::Git
      && self.name == OFFICIAL_SOURCE
      && self.location == OFFICIAL_LOCATION
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
  /// A git repository, cloned and exported per plugin
  Git,
  /// A directory of plugins, used in place
  Path,
  /// A directory of plugins in development: used in place and wins over every
  /// other source
  Dev,
}

fn enabled() -> bool {
  true
}

/// A name that is also a single directory name: `[A-Za-z0-9][A-Za-z0-9._-]*`
pub fn is_flat_name(name: &str) -> bool {
  let mut chars = name.chars();
  chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
    && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn flat_name<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
  let name = String::deserialize(deserializer)?;
  if !is_flat_name(&name) {
    return Err(D::Error::custom(format!(
      "invalid name `{name}`: letters, digits, `.`, `_` and `-` only"
    )));
  }
  Ok(name)
}

fn unique_sources<'de, D: Deserializer<'de>>(
  deserializer: D,
) -> Result<Vec<SourceConfig>, D::Error> {
  let sources = Vec::<SourceConfig>::deserialize(deserializer)?;
  if sources.iter().any(|s| s.name == LOCAL_SOURCE) {
    return Err(D::Error::custom(format!(
      "plugin source name `{LOCAL_SOURCE}` is reserved"
    )));
  }
  let mut seen = HashSet::new();
  if let Some(source) = sources.iter().find(|s| !seen.insert(&s.name)) {
    return Err(D::Error::custom(format!(
      "plugin source `{}` is declared twice",
      source.name
    )));
  }
  Ok(sources)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn flat_names() {
    for name in ["a", "official", "my-dev", "com.example.x", "A_1"] {
      assert!(is_flat_name(name), "{name}");
    }
    for name in ["", ".", "..", ".hidden", "-x", "a/b", "a\\b", "a b", "a:b"] {
      assert!(!is_flat_name(name), "{name}");
    }
  }

  #[test]
  fn defaults_to_the_official_source() {
    let config: PluginsConfig = toml::from_str("").unwrap();
    assert_eq!(config.source, [SourceConfig::official()]);
    assert!(config.source[0].is_official());
    assert_eq!(config.auto_update, AutoUpdate::Official);
    assert!(config.enabled.is_empty());
  }

  #[test]
  fn sources_parse() {
    let config: PluginsConfig = toml::from_str(
      r#"
enabled = ["a"]
auto_update = "none"
[[source]]
name = "dev"
kind = "dev"
location = "~/dev"
[[source]]
name = "official"
kind = "git"
location = "https://example.com/fork"
enabled = false
"#,
    )
    .unwrap();
    assert!(config.is_enabled("a"));
    assert!(!config.is_enabled("b"));
    assert_eq!(config.auto_update, AutoUpdate::None);
    assert_eq!(config.source[0].kind, SourceKind::Dev);
    assert!(config.source[0].enabled);
    assert!(!config.source[1].enabled);
    // the name alone is not the official source
    assert!(!config.source[1].is_official());
  }

  #[test]
  fn bad_sources_are_rejected() {
    let duplicate = r#"
[[source]]
name = "a"
kind = "git"
location = "x"
[[source]]
name = "a"
kind = "path"
location = "y"
"#;
    let e = toml::from_str::<PluginsConfig>(duplicate).unwrap_err();
    assert!(e.to_string().contains("declared twice"), "{e}");

    let bad_name = "[[source]]\nname = \"../x\"\nkind = \"git\"\nlocation = \"x\"\n";
    assert!(toml::from_str::<PluginsConfig>(bad_name).is_err());
    let reserved = "[[source]]\nname = \"local\"\nkind = \"path\"\nlocation = \"x\"\n";
    assert!(toml::from_str::<PluginsConfig>(reserved).is_err());
    let bad_kind = "[[source]]\nname = \"x\"\nkind = \"tar\"\nlocation = \"x\"\n";
    assert!(toml::from_str::<PluginsConfig>(bad_kind).is_err());
  }

  #[test]
  fn dirs_follow_xdg() {
    let tmp = tempfile::tempdir().unwrap();
    unsafe {
      std::env::set_var("XDG_DATA_HOME", tmp.path().join("data"));
      std::env::set_var("XDG_STATE_HOME", tmp.path().join("state"));
    }
    let dirs = plugin_dirs();
    assert_eq!(dirs.local, tmp.path().join("data/corona/plugins"));
    assert_eq!(dirs.state, tmp.path().join("state/corona/plugins"));
  }
}

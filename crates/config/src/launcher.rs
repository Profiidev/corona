use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

/// The app launcher and command palette
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct LauncherConfig {
  /// Close when another window takes the keyboard
  pub close_on_unfocus: bool,
  /// Command that runs a program in a terminal, like `kitty -e`. Empty: `xdg-terminal-exec`
  /// when installed, else `$TERMINAL -e`
  pub terminal: String,
  /// Show the preview pane where a view has one
  pub preview: bool,
  /// Results per section on the root list while typing
  pub section_limit: usize,
  /// Keyword of the engine the web search fallback uses
  pub search_engine: String,
  /// Engines by keyword: `g rust` searches with the engine `g`
  pub search_engines: Vec<SearchEngine>,
  /// Shown in this order when nothing matches: `web`, `files`, `terminal`
  pub fallbacks: Vec<String>,
  /// Per command id: alias, favorite, hidden, disabled
  pub commands: BTreeMap<String, CommandConfig>,
  /// URL or path templates with `{argument}` placeholders
  pub quicklinks: Vec<Quicklink>,
  /// Text pasted with placeholders expanded
  pub snippets: Vec<Snippet>,
  pub files: FilesConfig,
  pub clipboard: ClipboardConfig,
  pub translate: TranslateConfig,
  /// Window classes "Close all windows" leaves open
  pub close_all_exclude: Vec<String>,
  /// How old exchange rates may get before they are fetched again
  pub currency_refresh_hours: u32,
}

impl Default for LauncherConfig {
  fn default() -> Self {
    Self {
      close_on_unfocus: true,
      terminal: String::new(),
      preview: true,
      section_limit: 5,
      search_engine: "g".to_string(),
      search_engines: SearchEngine::builtin(),
      fallbacks: ["web", "files", "terminal"].map(String::from).to_vec(),
      commands: BTreeMap::new(),
      quicklinks: Vec::new(),
      snippets: Vec::new(),
      files: FilesConfig::default(),
      clipboard: ClipboardConfig::default(),
      translate: TranslateConfig::default(),
      close_all_exclude: Vec::new(),
      currency_refresh_hours: 24,
    }
  }
}

impl LauncherConfig {
  pub fn command(&self, id: &str) -> Option<&CommandConfig> {
    self.commands.get(id)
  }

  pub fn engine(&self, keyword: &str) -> Option<&SearchEngine> {
    self.search_engines.iter().find(|e| e.keyword == keyword)
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SearchEngine {
  pub keyword: String,
  pub name: String,
  /// `{query}` is replaced with the percent-encoded search
  pub url: String,
}

impl SearchEngine {
  fn new(keyword: &str, name: &str, url: &str) -> Self {
    Self {
      keyword: keyword.to_string(),
      name: name.to_string(),
      url: url.to_string(),
    }
  }

  pub fn builtin() -> Vec<Self> {
    vec![
      Self::new("g", "Google", "https://www.google.com/search?q={query}"),
      Self::new("ddg", "DuckDuckGo", "https://duckduckgo.com/?q={query}"),
      Self::new(
        "yt",
        "YouTube",
        "https://www.youtube.com/results?search_query={query}",
      ),
      Self::new("gh", "GitHub", "https://github.com/search?q={query}"),
      Self::new(
        "wiki",
        "Wikipedia",
        "https://en.wikipedia.org/wiki/Special:Search?search={query}",
      ),
    ]
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct CommandConfig {
  /// Typed instead of the name, matched before anything else
  pub alias: Option<String>,
  /// Shown first on an empty query
  pub favorite: bool,
  /// Left out of the root search, still in "Browse apps"
  pub hidden: bool,
  pub enabled: bool,
}

impl Default for CommandConfig {
  fn default() -> Self {
    Self {
      alias: None,
      favorite: false,
      hidden: false,
      enabled: true,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Quicklink {
  pub name: String,
  /// URL or path; `{argument}` placeholders prompt in the arguments row
  pub url: String,
  /// Lucide icon name
  pub icon: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(default)]
pub struct Snippet {
  pub name: String,
  /// Typing it in the launcher offers the snippet first
  pub keyword: Option<String>,
  pub text: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct FilesConfig {
  /// Searched directories; may start with `~/`
  pub roots: Vec<String>,
  /// File or directory names skipped anywhere below the roots
  pub excludes: Vec<String>,
  pub hidden: bool,
  /// `.gitignore` and `.ignore` files hide what they list
  pub respect_ignore: bool,
}

impl Default for FilesConfig {
  fn default() -> Self {
    Self {
      roots: vec!["~".to_string()],
      excludes: ["node_modules", "target", ".git", ".cache"]
        .map(String::from)
        .to_vec(),
      hidden: false,
      respect_ignore: true,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(default)]
pub struct ClipboardConfig {
  /// Keep a history of what is copied
  pub enabled: bool,
  pub images: bool,
  /// Larger images are not kept
  pub image_max_mb: f32,
  /// Images older than this are dropped, text is kept until deleted
  pub image_retention_days: u32,
}

impl Default for ClipboardConfig {
  fn default() -> Self {
    Self {
      enabled: true,
      images: true,
      image_max_mb: 10.,
      image_retention_days: 30,
    }
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(default)]
pub struct TranslateConfig {
  /// DeepL API key
  pub deepl_key: Option<String>,
  /// File holding the DeepL API key, for secret managers; wins over `deepl_key`
  pub deepl_key_file: Option<PathBuf>,
  /// Language code translations go to
  pub target: String,
}

impl Default for TranslateConfig {
  fn default() -> Self {
    Self {
      deepl_key: None,
      deepl_key_file: None,
      target: "EN".to_string(),
    }
  }
}

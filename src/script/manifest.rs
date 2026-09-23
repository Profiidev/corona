use std::{
  collections::HashMap,
  path::{Path, PathBuf},
};

use gpui_shell::{Capabilities, ExecuteGrant, HttpRequestGrant};
use schemars::JsonSchema;
use serde::Deserialize;

pub struct PluginManifest {
  pub id: String,
  #[allow(dead_code)]
  pub name: String,
  #[allow(dead_code)]
  pub version: Option<String>,
  pub views: HashMap<String, String>,
  pub capabilities: Capabilities,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ManifestFile {
  /// Reverse-DNS identity, e.g. `com.example.inbox`. Also the namespace for
  /// panels, storage and capability records.
  pub id: String,
  /// Human-readable name, shown in menus and in the permission prompt.
  pub name: String,
  /// Optional plugin semantic version, e.g. `1.2.0`.
  #[serde(default)]
  pub version: Option<String>,
  #[serde(default)]
  pub views: HashMap<String, String>,
  #[serde(default)]
  pub capabilities: CapabilitiesFile,
}

const PLUGIN_DIR_PLACEHOLDER: &str = "${pluginDir}";
const DATA_DIR_PLACEHOLDER: &str = "${dataDir}";

#[derive(Clone, Debug, PartialEq, Deserialize, Default, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CapabilitiesFile {
  /// Filesystem and subprocess access.
  #[serde(default)]
  fs: Option<FsGrantFile>,
  /// Outbound network access, by host.
  #[serde(default)]
  network: Option<NetworkGrantFile>,
  /// Whether `localStorage` is available.
  ///
  /// The one grant that defaults to *given*, and it follows the web: a
  /// browser hands every origin a `localStorage` unconditionally, because
  /// what it reaches is that origin's own data and nothing else. The same
  /// holds here — storage is keyed by bundle id, cannot name its own file,
  /// and is bounded — so there is nothing for an author to ask permission
  /// for. Defaulting it to `false` also had a trap: an application that
  /// added a manifest to declare a network host silently lost its settings.
  ///
  /// A host that runs code it does not trust can still say `false`, which is
  /// why this stays a capability rather than becoming ambient like
  /// `sessionStorage`.
  #[serde(default = "granted")]
  storage: bool,
  /// Clipboard access.
  #[serde(default)]
  clipboard: Option<ClipboardGrantFile>,
  /// Process-level host requests, separate from filesystem execution.
  #[serde(default)]
  process: Option<ProcessGrantFile>,
}

impl CapabilitiesFile {
  pub fn grant(&self, plugin_dir: &Path, data_dir: &Path) -> Capabilities {
    let fs = self.fs.clone().unwrap_or_default();
    let clipboard = self.clipboard.clone().unwrap_or_default();
    let process = self.process.clone().unwrap_or_default();
    let execute = match fs.execute.clone() {
      None => ExecuteGrant::Denied,
      Some(ExecuteFile::Unrestricted(_)) => ExecuteGrant::Unrestricted,
      Some(ExecuteFile::Allowed(commands)) => ExecuteGrant::Allowed(commands),
    };

    let network = self.network.clone().unwrap_or_default();
    Capabilities::new()
      .read_roots(expand_all(&fs.read, plugin_dir, data_dir))
      .write_roots(expand_all(&fs.write, plugin_dir, data_dir))
      .execute(execute)
      .network_hosts(network.hosts.into_iter().map(|host| host.to_lowercase()))
      .http_requests(network.http.into_iter().map(|request| {
        let mut grant = HttpRequestGrant::new(
          request.host,
          request.methods,
          request.paths,
          request.path_prefixes,
        )
        .scheme(request.scheme);
        if let Some(port) = request.port {
          grant = grant.port(port);
        }
        grant
      }))
      .storage(self.storage)
      .clipboard_read(clipboard.read)
      .clipboard_write(clipboard.write)
      .exit(process.exit)
  }
}

fn expand_all(paths: &[String], plugin_dir: &Path, data_dir: &Path) -> Vec<PathBuf> {
  paths
    .iter()
    .map(|path| expand(path, plugin_dir, data_dir))
    .collect()
}

fn expand(raw: &str, plugin_dir: &Path, data_dir: &Path) -> PathBuf {
  let expanded = raw
    .replace(
      PLUGIN_DIR_PLACEHOLDER,
      plugin_dir.to_string_lossy().as_ref(),
    )
    .replace(DATA_DIR_PLACEHOLDER, data_dir.to_string_lossy().as_ref());

  let path = PathBuf::from(expanded);
  if path.is_absolute() {
    path
  } else {
    plugin_dir.join(path)
  }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct FsGrantFile {
  /// Directories that may be read. `${pluginDir}` and `${dataDir}` expand to
  /// the plugin's own directory and its storage directory.
  #[serde(default)]
  read: Vec<String>,
  /// Directories that may be written.
  #[serde(default)]
  write: Vec<String>,
  /// Commands `process.run` may start.
  #[serde(default)]
  execute: Option<ExecuteFile>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema)]
#[serde(untagged)]
enum ExecuteFile {
  Allowed(Vec<String>),
  Unrestricted(String),
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NetworkGrantFile {
  /// Hosts that may be reached, e.g. `api.example.com`.
  #[serde(default)]
  hosts: Vec<String>,
  /// HTTP requests constrained by host, method and URL path.
  #[serde(default)]
  http: Vec<HttpRequestGrantFile>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct HttpRequestGrantFile {
  #[serde(default = "default_https_scheme")]
  scheme: String,
  host: String,
  #[serde(default)]
  port: Option<u16>,
  methods: Vec<String>,
  #[serde(default)]
  paths: Vec<String>,
  #[serde(default)]
  path_prefixes: Vec<String>,
}

fn default_https_scheme() -> String {
  "https".to_owned()
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ClipboardGrantFile {
  #[serde(default)]
  read: bool,
  #[serde(default)]
  write: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ProcessGrantFile {
  #[serde(default)]
  exit: bool,
}

fn granted() -> bool {
  true
}

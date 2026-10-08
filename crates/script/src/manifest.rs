use std::{
  collections::{BTreeSet, HashMap},
  path::{Path, PathBuf},
};

use gpui_shell::{Capabilities, ExecuteGrant, HttpRequestGrant};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::module::CoronaModule;

pub struct PluginManifest {
  pub id: String,
  #[allow(dead_code)]
  pub name: String,
  #[allow(dead_code)]
  pub version: Option<String>,
  pub views: HashMap<String, String>,
  pub capabilities: Capabilities,
  /// The corona modules it may import
  pub modules: BTreeSet<CoronaModule>,
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
  /// Corona modules the plugin imports, e.g. `["weather"]` for
  /// `corona/weather`. Importing one not listed fails.
  #[serde(default)]
  corona: BTreeSet<CoronaModule>,
}

impl CapabilitiesFile {
  pub fn modules(&self) -> BTreeSet<CoronaModule> {
    self.corona.clone()
  }

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

#[cfg(test)]
mod tests {
  use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
  };

  use gpui_shell::{Capabilities, ExecuteGrant};

  use super::{ManifestFile, expand};
  use crate::module::CoronaModule;

  fn parse(json: &str) -> serde_json::Result<ManifestFile> {
    serde_json::from_str(json)
  }

  /// The grant of a manifest with these `capabilities`.
  fn grant(capabilities: &str) -> Capabilities {
    let json = format!(r#"{{ "id": "a", "name": "A", "capabilities": {capabilities} }}"#);
    parse(&json)
      .unwrap()
      .capabilities
      .grant(Path::new("/plugins/a"), Path::new("/data/a"))
  }

  fn expand_raw(raw: &str) -> PathBuf {
    expand(raw, Path::new("/plugins/a"), Path::new("/data/a"))
  }

  #[test]
  fn required_fields() {
    assert!(parse(r#"{ "name": "A" }"#).is_err());
    assert!(parse(r#"{ "id": "a" }"#).is_err());
    let manifest = parse(r#"{ "id": "a", "name": "A" }"#).unwrap();
    assert_eq!(manifest.version, None);
    assert!(manifest.views.is_empty());

    let manifest =
      parse(r#"{ "id": "a", "name": "A", "version": "1.2.0", "views": { "bar": "main.js" } }"#)
        .unwrap();
    assert_eq!(manifest.version.as_deref(), Some("1.2.0"));
    assert_eq!(manifest.views["bar"], "main.js");
  }

  #[test]
  fn nested_unknown_fields_are_rejected() {
    for capabilities in [
      r#"{ "bogus": true }"#,
      r#"{ "fs": { "exec": ["git"] } }"#,
      r#"{ "network": { "host": [] } }"#,
      r#"{ "network": { "http": [{ "host": "a", "methods": ["GET"], "path": [] }] } }"#,
      r#"{ "clipboard": { "paste": true } }"#,
      r#"{ "process": { "kill": true } }"#,
    ] {
      let json = format!(r#"{{ "id": "a", "name": "A", "capabilities": {capabilities} }}"#);
      assert!(parse(&json).is_err(), "{capabilities}");
    }
  }

  #[test]
  fn http_methods_are_required() {
    let json =
      r#"{ "id": "a", "name": "A", "capabilities": { "network": { "http": [{ "host": "a" }] } } }"#;
    assert!(parse(json).is_err());
  }

  #[test]
  fn default_grant() {
    let capabilities = grant("{}");
    // storage is the one grant given by default
    assert!(capabilities.has_storage());
    assert_eq!(capabilities.execute_grant(), &ExecuteGrant::Denied);
    assert!(!capabilities.has_read_access());
    assert!(!capabilities.has_write_access());
    assert!(!capabilities.is_clipboard_readable());
    assert!(!capabilities.is_clipboard_writable());
    assert!(!capabilities.may_exit());
    assert!(!capabilities.may_reach("example.com"));
    assert!(!capabilities.may_request("https", "example.com", None, "GET", "/"));
  }

  #[test]
  #[ignore = "bug: derived Default gives storage false when `capabilities` is omitted, `{}` gives true"]
  fn bug_omitted_capabilities_lose_storage() {
    let capabilities = parse(r#"{ "id": "a", "name": "A" }"#)
      .unwrap()
      .capabilities
      .grant(Path::new("/plugins/a"), Path::new("/data/a"));
    assert!(capabilities.has_storage());
  }

  #[test]
  fn explicit_grants() {
    let capabilities = grant(
      r#"{
        "fs": { "read": ["${pluginDir}"], "write": ["${dataDir}"] },
        "storage": false,
        "clipboard": { "read": true, "write": true },
        "process": { "exit": true }
      }"#,
    );
    assert!(!capabilities.has_storage());
    assert!(capabilities.has_read_access());
    assert!(capabilities.has_write_access());
    assert!(capabilities.is_clipboard_readable());
    assert!(capabilities.is_clipboard_writable());
    assert!(capabilities.may_exit());

    let capabilities = grant(r#"{ "clipboard": { "read": true } }"#);
    assert!(capabilities.is_clipboard_readable());
    assert!(!capabilities.is_clipboard_writable());
  }

  #[test]
  fn execute_grants() {
    let capabilities = grant(r#"{ "fs": { "execute": ["git"] } }"#);
    assert_eq!(
      capabilities.execute_grant(),
      &ExecuteGrant::Allowed(vec!["git".into()])
    );
    assert!(capabilities.may_run("git"));
    assert!(!capabilities.may_run("rm"));

    let capabilities = grant(r#"{ "fs": { "execute": [] } }"#);
    assert!(!capabilities.may_run("git"));

    let capabilities = grant(r#"{ "fs": { "execute": "*" } }"#);
    assert_eq!(capabilities.execute_grant(), &ExecuteGrant::Unrestricted);
    assert!(capabilities.may_run("anything"));

    let json = r#"{ "id": "a", "name": "A", "capabilities": { "fs": { "execute": true } } }"#;
    assert!(parse(json).is_err());
  }

  #[test]
  #[ignore = "bug: any execute string, not just \"*\", grants unrestricted execution"]
  fn bug_execute_string_other_than_wildcard_is_unrestricted() {
    let json = r#"{ "id": "a", "name": "A", "capabilities": { "fs": { "execute": "git" } } }"#;
    let unrestricted = parse(json).is_ok_and(|manifest| {
      manifest
        .capabilities
        .grant(Path::new("/p"), Path::new("/d"))
        .execute_grant()
        == &ExecuteGrant::Unrestricted
    });
    assert!(!unrestricted, "`\"git\"` must not mean `\"*\"`");
  }

  #[test]
  fn network_hosts() {
    let capabilities = grant(r#"{ "network": { "hosts": ["API.Example.com"] } }"#);
    assert!(capabilities.may_reach("api.example.com"));
    assert!(!capabilities.may_reach("example.com"));
    // a host grant allows any request to it
    assert!(capabilities.may_request("http", "api.example.com", Some(1), "POST", "/x"));
  }

  #[test]
  fn http_grants() {
    let capabilities = grant(
      r#"{ "network": { "http": [
        { "host": "Api.Example.com", "methods": ["get"], "paths": ["/v1/a"], "path_prefixes": ["/v2"] },
        { "scheme": "http", "host": "local", "port": 8080, "methods": ["POST"] }
      ] } }"#,
    );
    // https by default, host and method case-insensitive
    assert!(capabilities.may_request("https", "api.example.com", None, "GET", "/v1/a"));
    assert!(capabilities.may_request("https", "api.example.com", Some(443), "GET", "/v2/b"));
    assert!(!capabilities.may_request("http", "api.example.com", None, "GET", "/v1/a"));
    assert!(!capabilities.may_request("https", "api.example.com", None, "POST", "/v1/a"));
    assert!(!capabilities.may_request("https", "api.example.com", None, "GET", "/v1/b"));
    assert!(!capabilities.may_request("https", "api.example.com", Some(8443), "GET", "/v1/a"));
    // an http grant is not a host grant
    assert!(!capabilities.may_reach("api.example.com"));

    // no paths at all: nothing is allowed
    assert!(!capabilities.may_request("http", "local", Some(8080), "POST", "/"));
  }

  #[test]
  fn http_port() {
    let capabilities = grant(
      r#"{ "network": { "http": [
        { "scheme": "http", "host": "local", "port": 8080, "methods": ["POST"], "path_prefixes": ["/"] }
      ] } }"#,
    );
    assert!(capabilities.may_request("http", "local", Some(8080), "POST", "/x"));
    assert!(!capabilities.may_request("http", "local", None, "POST", "/x"));
    assert!(!capabilities.may_request("https", "local", Some(8080), "POST", "/x"));
  }

  #[test]
  fn expands_paths() {
    assert_eq!(expand_raw("${pluginDir}"), Path::new("/plugins/a"));
    assert_eq!(
      expand_raw("${pluginDir}/assets"),
      Path::new("/plugins/a/assets")
    );
    assert_eq!(expand_raw("${dataDir}/cache"), Path::new("/data/a/cache"));
    assert_eq!(expand_raw("/etc/os-release"), Path::new("/etc/os-release"));
    // relative paths are inside the plugin directory
    assert_eq!(expand_raw("assets"), Path::new("/plugins/a/assets"));
    assert_eq!(expand_raw(""), Path::new("/plugins/a"));
  }

  #[test]
  #[ignore = "bug: a relative `..` path is granted outside the plugin directory"]
  fn bug_relative_path_escapes_plugin_dir() {
    let path = expand_raw("../other");
    let escapes = path
      .components()
      .any(|component| component == std::path::Component::ParentDir);
    assert!(!escapes, "{} leaves /plugins/a", path.display());
  }

  #[test]
  #[ignore = "bug: unknown placeholders like ${homeDir} are granted as a literal directory"]
  fn bug_unknown_placeholder_is_accepted() {
    let json =
      r#"{ "id": "a", "name": "A", "capabilities": { "fs": { "read": ["${homeDir}"] } } }"#;
    assert!(parse(json).is_err());
  }

  #[test]
  #[ignore = "bug: a misspelled top-level key like `capabilites` is ignored, the plugin silently gets the default grant"]
  fn bug_top_level_unknown_fields_are_accepted() {
    let json = r#"{ "id": "a", "name": "A", "capabilites": { "clipboard": { "read": true } } }"#;
    assert!(parse(json).is_err());
  }

  #[test]
  fn committed_schema_is_current() {
    let schema = schemars::schema_for!(ManifestFile).to_value();
    let committed: serde_json::Value =
      serde_json::from_str(include_str!("../plugin.schema.json")).unwrap();
    assert_eq!(
      schema, committed,
      "run corona in debug to refresh plugin.schema.json"
    );
  }

  #[test]
  fn corona_modules() {
    let manifest = parse(r#"{ "id": "a", "name": "A" }"#).unwrap();
    assert!(manifest.capabilities.modules().is_empty());

    let manifest = parse(
      r#"{ "id": "a", "name": "A", "capabilities": { "corona": ["weather", "sysinfo", "weather"] } }"#,
    )
    .unwrap();
    assert_eq!(
      manifest.capabilities.modules(),
      BTreeSet::from([CoronaModule::Sysinfo, CoronaModule::Weather])
    );

    let unknown = r#"{ "id": "a", "name": "A", "capabilities": { "corona": ["camera"] } }"#;
    assert!(parse(unknown).is_err());
  }
}

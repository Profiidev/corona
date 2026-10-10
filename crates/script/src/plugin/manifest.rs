use std::{
  collections::{BTreeMap, BTreeSet},
  path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, ensure};
use corona_config::plugins::is_flat_name;
use gpui_shell::{Capabilities, ExecuteGrant, HttpRequestGrant};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, de::Error as _};
use serde_json::{Map, Value};

use crate::{
  PLUGIN_MANIFEST_FILENAME,
  module::{CoronaModule, dbus::DbusGrant},
  plugin::settings::{self, Setting, SettingKind},
};

/// A plugin as it runs: its manifest, where it was found and what it may do
#[derive(Clone, Debug)]
pub struct PluginManifest {
  pub id: String,
  /// The directory it was found in, not necessarily named after the id
  pub dir: PathBuf,
  pub name: String,
  pub version: Option<String>,
  pub description: Option<String>,
  pub widgets: BTreeMap<String, WidgetFile>,
  pub panels: BTreeMap<String, PanelFile>,
  /// The service's script, relative to the plugin directory
  pub service: Option<String>,
  pub settings: Vec<Setting>,
  /// Settings a `${setting:<key>}` host grant reads: the user's to change,
  /// never the plugin's
  pub host_settings: BTreeSet<String>,
  pub capabilities: Capabilities,
  /// The corona modules it may import
  pub modules: BTreeSet<CoronaModule>,
  /// The bus names `corona/dbus` may reach, which it needs to be importable
  pub dbus: Option<DbusGrant>,
}

impl PluginManifest {
  /// `configured` is `[plugin_settings."<id>"]`, which `${setting:<key>}`
  /// hosts expand from
  pub fn new(
    file: ManifestFile,
    dir: PathBuf,
    data_dir: &Path,
    configured: Option<&Map<String, Value>>,
  ) -> Self {
    let values = settings::resolve(&file.id, &file.settings, configured);
    Self {
      capabilities: file.capabilities.grant(&dir, data_dir, &values),
      host_settings: file
        .capabilities
        .hosts()
        .filter_map(setting_key)
        .map(Into::into)
        .collect(),
      modules: file.capabilities.modules(),
      dbus: file.capabilities.dbus.clone(),
      id: file.id,
      dir,
      name: file.name,
      version: file.version,
      description: file.description,
      widgets: file.widgets,
      panels: file.panels,
      service: file.service,
      settings: file.settings,
    }
  }
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ManifestFile {
  /// The JSON schema an editor validates this file against.
  #[serde(rename = "$schema", default)]
  #[allow(dead_code)]
  pub schema: Option<String>,
  /// Reverse-DNS identity, e.g. `com.example.inbox`: letters, digits, `.`,
  /// `_` and `-`. Also the namespace for widgets, panels, storage and
  /// capability records.
  #[serde(deserialize_with = "flat_name")]
  pub id: String,
  /// Human-readable name, shown in menus and in the permission prompt.
  pub name: String,
  /// Optional plugin semantic version, e.g. `1.2.0`.
  #[serde(default)]
  pub version: Option<String>,
  /// One line about what it does, shown in the settings app.
  #[serde(default)]
  pub description: Option<String>,
  /// Bar widgets by name; a bar lists one as `<id>:<name>`.
  #[serde(default, deserialize_with = "flat_keys")]
  pub widgets: BTreeMap<String, WidgetFile>,
  /// Panels by name, opened as `<id>:<name>`.
  #[serde(default, deserialize_with = "flat_keys")]
  pub panels: BTreeMap<String, PanelFile>,
  /// A script that runs in the background while the plugin is enabled,
  /// relative to the plugin directory, e.g. `service.js`. It default-exports
  /// `async function main(cx)` and answers `call` from `corona/plugin` and
  /// from `corona ipc plugin`.
  #[serde(default)]
  pub service: Option<String>,
  /// Settings the user can change in the settings app, in this order.
  #[serde(default, deserialize_with = "valid_settings")]
  pub settings: Vec<Setting>,
  #[serde(default)]
  pub capabilities: CapabilitiesFile,
}

impl ManifestFile {
  /// The `plugin.toml` in `dir`
  pub fn read(dir: &Path) -> Result<Self> {
    let path = dir.join(PLUGIN_MANIFEST_FILENAME);
    let text = std::fs::read_to_string(&path)?;
    let file: Self = toml::from_str(&text).with_context(|| path.display().to_string())?;
    file.check().with_context(|| path.display().to_string())?;
    Ok(file)
  }

  /// What one table cannot tell alone: a secret setting is read through
  /// `corona/secrets`, so the plugin must be granted it, and a
  /// `${setting:<key>}` host needs a text setting `key`
  fn check(&self) -> Result<()> {
    for host in self.capabilities.hosts() {
      let Some(key) = setting_key(host) else {
        ensure!(
          !host.contains("${"),
          "unknown placeholder in host `{host}`, only ${{setting:<key>}} exists"
        );
        continue;
      };
      ensure!(
        self
          .settings
          .iter()
          .any(|s| s.key == key && matches!(s.kind, SettingKind::Text { .. })),
        "host `{host}` needs a text setting `{key}`"
      );
    }
    if let Some(secret) = self.settings.iter().find(|s| s.is_secret()) {
      ensure!(
        self.capabilities.corona.contains(&CoronaModule::Secrets),
        "setting `{}` is a secret, which needs `corona = [\"secrets\"]`",
        secret.key
      );
    }
    Ok(())
  }
}

#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WidgetFile {
  /// The script of the view, relative to the plugin directory.
  pub view: String,
  /// Shown in the bar editor; the widget's key when unset.
  #[serde(default)]
  pub name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelFile {
  /// The script of the view, relative to the plugin directory.
  pub view: String,
  #[serde(default)]
  pub name: Option<String>,
  #[serde(default = "panel_width")]
  pub width: f32,
  #[serde(default = "panel_height")]
  pub height: f32,
}

fn panel_width() -> f32 {
  400.
}

fn panel_height() -> f32 {
  500.
}

/// Ids and entry names are directory names and the parts of `<id>:<entry>`
fn flat_name<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
  let id = String::deserialize(deserializer)?;
  if !is_flat_name(&id) {
    return Err(D::Error::custom(format!(
      "invalid plugin id `{id}`: letters, digits, `.`, `_` and `-` only"
    )));
  }
  Ok(id)
}

fn flat_keys<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
  deserializer: D,
) -> Result<BTreeMap<String, T>, D::Error> {
  let map = BTreeMap::<String, T>::deserialize(deserializer)?;
  if let Some(key) = map.keys().find(|key| !is_flat_name(key)) {
    return Err(D::Error::custom(format!(
      "invalid name `{key}`: letters, digits, `.`, `_` and `-` only"
    )));
  }
  Ok(map)
}

fn valid_settings<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Setting>, D::Error> {
  let list = Vec::<Setting>::deserialize(deserializer)?;
  settings::validate(&list).map_err(|e| D::Error::custom(e.to_string()))?;
  Ok(list)
}

const PLUGIN_DIR_PLACEHOLDER: &str = "${pluginDir}";
const DATA_DIR_PLACEHOLDER: &str = "${dataDir}";

/// Rejects placeholders other than `${pluginDir}` and `${dataDir}`, which would
/// otherwise be granted as a literal directory.
fn grant_paths<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
  let paths = Vec::<String>::deserialize(deserializer)?;
  for path in &paths {
    let rest = path
      .replace(PLUGIN_DIR_PLACEHOLDER, "")
      .replace(DATA_DIR_PLACEHOLDER, "");
    if rest.contains("${") {
      return Err(D::Error::custom(format!(
        "unknown placeholder in `{path}`, only {PLUGIN_DIR_PLACEHOLDER} and {DATA_DIR_PLACEHOLDER} exist"
      )));
    }
  }
  Ok(paths)
}

/// As [`grant_paths`], but never the plugin directory: a plugin that rewrites
/// its own `plugin.toml` would grant itself anything on the next rescan.
fn write_paths<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
  let paths = grant_paths(deserializer)?;
  for path in &paths {
    // `..` could climb from `${dataDir}` into the plugin directory
    let climbs = path.split('/').any(|part| part == "..");
    if path.contains(PLUGIN_DIR_PLACEHOLDER) || !path.starts_with(['/', '$']) || climbs {
      return Err(D::Error::custom(format!(
        "`{path}` cannot be written, the plugin directory is read only; write to {DATA_DIR_PLACEHOLDER}"
      )));
    }
  }
  Ok(paths)
}

#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema)]
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
  /// Bus names `corona/dbus` may call, read and receive signals from. The
  /// module is only there with this table.
  #[serde(default)]
  dbus: Option<DbusGrant>,
}

// Same as `{}`: an omitted `capabilities` still grants storage.
impl Default for CapabilitiesFile {
  fn default() -> Self {
    Self {
      fs: None,
      network: None,
      storage: granted(),
      clipboard: None,
      process: None,
      corona: BTreeSet::new(),
      dbus: None,
    }
  }
}

impl CapabilitiesFile {
  pub fn modules(&self) -> BTreeSet<CoronaModule> {
    self.corona.clone()
  }

  /// Every host granted, by `hosts` and by `http`, as written
  fn hosts(&self) -> impl Iterator<Item = &str> {
    let network = self.network.iter();
    let hosts = network
      .clone()
      .flat_map(|n| n.hosts.iter().map(String::as_str));
    hosts.chain(network.flat_map(|n| n.http.iter().map(|r| r.host.as_str())))
  }

  /// `settings` are the resolved values `${setting:<key>}` hosts expand from
  pub fn grant(
    &self,
    plugin_dir: &Path,
    data_dir: &Path,
    settings: &Map<String, Value>,
  ) -> Capabilities {
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
      // ponytail: lexical, so a root above the plugin directory still covers
      // it; such a root already reaches the config, deny-paths if that matters
      .write_roots(
        expand_all(&fs.write, plugin_dir, data_dir)
          .into_iter()
          .filter(|root| !root.starts_with(plugin_dir)),
      )
      .execute(execute)
      .network_hosts(
        network
          .hosts
          .iter()
          .filter_map(|host| expand_host(host, settings)),
      )
      .network_unix(expand_all(&network.unix, plugin_dir, data_dir))
      .http_requests(network.http.into_iter().filter_map(|request| {
        let mut grant = HttpRequestGrant::new(
          expand_host(&request.host, settings)?,
          request.methods,
          request.paths,
          request.path_prefixes,
        )
        .scheme(request.scheme);
        if let Some(port) = request.port {
          grant = grant.port(port);
        }
        Some(grant)
      }))
      .storage(self.storage)
      .clipboard_read(clipboard.read)
      .clipboard_write(clipboard.write)
      .exit(process.exit)
      .process_env(process.env)
      .asset_root(plugin_dir.to_path_buf())
  }
}

const SETTING_PLACEHOLDER: &str = "${setting:";

/// The key of a host that is a whole `${setting:<key>}`
fn setting_key(host: &str) -> Option<&str> {
  host.strip_prefix(SETTING_PLACEHOLDER)?.strip_suffix('}')
}

/// A granted host, lowercase. A `${setting:<key>}` is the host of the value,
/// a URL or a bare host with an optional port; none when that is empty or
/// invalid, which drops the grant.
fn expand_host(raw: &str, settings: &Map<String, Value>) -> Option<String> {
  let Some(key) = setting_key(raw) else {
    return Some(raw.to_lowercase());
  };
  let value = settings.get(key)?.as_str()?.trim();
  let url = match value.contains("://") {
    true => url::Url::parse(value),
    false => url::Url::parse(&format!("http://{value}")),
  };
  let host = url.ok()?.host_str()?.to_lowercase();
  (!host.is_empty()).then_some(host)
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
  #[serde(default, deserialize_with = "grant_paths")]
  read: Vec<String>,
  /// Directories that may be written, absolute or `${dataDir}`; never the
  /// plugin's own directory.
  #[serde(default, deserialize_with = "write_paths")]
  write: Vec<String>,
  /// Commands `process.run` may start.
  #[serde(default)]
  execute: Option<ExecuteFile>,
}

/// Either an allowlist of command names, or the string `"*"`.
#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema)]
#[serde(untagged)]
enum ExecuteFile {
  Allowed(Vec<String>),
  Unrestricted(
    #[serde(deserialize_with = "wildcard")]
    #[schemars(extend("const" = "*"))]
    String,
  ),
}

/// Only `"*"` grants every command, so a forgotten `[]` around `"git"` is an
/// error instead of unrestricted execution.
fn wildcard<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
  let value = String::deserialize(deserializer)?;
  if value != "*" {
    return Err(D::Error::custom(format!(
      "execute must be a list of commands or \"*\", not `{value}`"
    )));
  }
  Ok(value)
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct NetworkGrantFile {
  /// Hosts that may be reached, e.g. `api.example.com`. `${setting:<key>}`
  /// grants the host of text setting `key`, a URL or a host, and nothing
  /// while it is empty; the plugin cannot change that setting itself.
  #[serde(default)]
  hosts: Vec<String>,
  /// HTTP requests constrained by host, method and URL path.
  #[serde(default)]
  http: Vec<HttpRequestGrantFile>,
  /// Unix sockets `net.connect({ path })` may reach, by exact path, e.g.
  /// `/run/tailscale/tailscaled.sock`. Placeholders expand as in `fs.read`.
  #[serde(default, deserialize_with = "grant_paths")]
  unix: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct HttpRequestGrantFile {
  #[serde(default = "default_https_scheme")]
  scheme: String,
  /// Expands `${setting:<key>}` as `hosts` do.
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
  /// Host environment variables passed to `run`/`spawn` children, and the
  /// only names their `env` option may set. Children otherwise get an empty
  /// environment.
  #[serde(default, deserialize_with = "env_names")]
  env: Vec<String>,
}

/// Loader variables would run the plugin's own code in any granted command.
fn env_names<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
  let names = Vec::<String>::deserialize(deserializer)?;
  if let Some(name) = names
    .iter()
    .find(|name| name.starts_with("LD_") || *name == "GCONV_PATH" || name.contains('='))
  {
    return Err(D::Error::custom(format!(
      "`{name}` cannot be granted in process.env"
    )));
  }
  Ok(names)
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
  use serde_json::{Map, json};

  use super::{ManifestFile, PluginManifest, expand};
  use crate::module::CoronaModule;

  fn parse(toml: &str) -> Result<ManifestFile, toml::de::Error> {
    toml::from_str(toml)
  }

  /// A manifest with id `a`, name `A` and `rest`.
  fn with(rest: &str) -> Result<ManifestFile, toml::de::Error> {
    parse(&format!("id = \"a\"\nname = \"A\"\n{rest}"))
  }

  /// A manifest with this `[capabilities]` table body.
  fn with_capabilities(body: &str) -> Result<ManifestFile, toml::de::Error> {
    with(&format!("[capabilities]\n{body}"))
  }

  /// The grant of a manifest with this `[capabilities]` table body.
  fn grant(body: &str) -> Capabilities {
    with_capabilities(body).unwrap().capabilities.grant(
      Path::new("/plugins/a"),
      Path::new("/data/a"),
      &Map::new(),
    )
  }

  fn expand_raw(raw: &str) -> PathBuf {
    expand(raw, Path::new("/plugins/a"), Path::new("/data/a"))
  }

  #[test]
  fn required_fields() {
    assert!(parse(r#"name = "A""#).is_err());
    assert!(parse(r#"id = "a""#).is_err());
    let manifest = with("").unwrap();
    assert_eq!(manifest.version, None);

    assert!(manifest.widgets.is_empty() && manifest.panels.is_empty());
    assert!(manifest.settings.is_empty());

    let manifest = with(r#"version = "1.2.0""#).unwrap();
    assert_eq!(manifest.version.as_deref(), Some("1.2.0"));
  }

  #[test]
  fn widgets_panels_and_settings() {
    let manifest = with(
      r#"
description = "Clock"
[widgets.clock]
view = "clock.js"
name = "Clock"
[widgets.small]
view = "small.js"
[panels.main]
view = "panel.js"
width = 300
[[settings]]
key = "seconds"
label = "Seconds"
type = "toggle"
default = false
"#,
    )
    .unwrap();
    assert_eq!(manifest.description.as_deref(), Some("Clock"));
    assert_eq!(manifest.widgets["clock"].view, "clock.js");
    assert_eq!(manifest.widgets["clock"].name.as_deref(), Some("Clock"));
    assert_eq!(manifest.widgets["small"].name, None);
    let panel = &manifest.panels["main"];
    assert_eq!((panel.width, panel.height), (300., 500.));
    assert_eq!(manifest.settings[0].key, "seconds");

    // entry names are part of `<id>:<entry>`
    assert!(with("[widgets.\"a:b\"]\nview = \"x.js\"").is_err());
    assert!(with("[panels.main]\nview = \"x.js\"\nsize = 3").is_err());
    // settings are checked as the manifest loads
    let bad = "[[settings]]\nkey = \"k\"\nlabel = \"K\"\ntype = \"toggle\"\ndefault = \"no\"";
    assert!(with(bad).is_err());
  }

  #[test]
  fn service() {
    assert_eq!(with("").unwrap().service, None);
    let manifest = with("service = \"service.js\"").unwrap();
    assert_eq!(manifest.service.as_deref(), Some("service.js"));
    // the table it was before
    assert!(with("[service]\nview = \"service.js\"").is_err());
  }

  #[test]
  fn ids_are_flat_names() {
    for id in ["com.example.a", "a-b_c", "A1"] {
      assert!(
        parse(&format!("id = \"{id}\"\nname = \"A\"")).is_ok(),
        "{id}"
      );
    }
    for id in ["", ".", "..", "a/b", "a:b", ".a", "a b"] {
      assert!(
        parse(&format!("id = \"{id}\"\nname = \"A\"")).is_err(),
        "{id}"
      );
    }
  }

  #[test]
  fn reads_from_a_directory() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(ManifestFile::read(tmp.path()).is_err());
    std::fs::write(tmp.path().join("plugin.toml"), "id = \"a\"\nname = \"A\"").unwrap();
    assert_eq!(ManifestFile::read(tmp.path()).unwrap().id, "a");
    std::fs::write(tmp.path().join("plugin.toml"), "id = 1").unwrap();
    let e = ManifestFile::read(tmp.path()).unwrap_err();
    assert!(format!("{e:#}").contains("plugin.toml"), "{e:#}");
  }

  #[test]
  fn secret_settings_need_the_secrets_module() {
    let secret = "[[settings]]\nkey = \"token\"\nlabel = \"Token\"\ntype = \"secret\"";
    let error = with(secret).unwrap().check().unwrap_err();
    assert!(error.to_string().contains("token"), "{error}");
    let granted = with(&format!("{secret}\n[capabilities]\ncorona = [\"secrets\"]"));
    assert!(granted.unwrap().check().is_ok());
    // and `read` checks it
    let tmp = tempfile::tempdir().unwrap();
    let toml = format!("id = \"a\"\nname = \"A\"\n{secret}");
    std::fs::write(tmp.path().join("plugin.toml"), toml).unwrap();
    assert!(ManifestFile::read(tmp.path()).is_err());
  }

  #[test]
  fn nested_unknown_fields_are_rejected() {
    for body in [
      r#"bogus = true"#,
      r#"fs = { exec = ["git"] }"#,
      r#"network = { host = [] }"#,
      r#"network = { http = [{ host = "a", methods = ["GET"], path = [] }] }"#,
      r#"clipboard = { paste = true }"#,
      r#"process = { kill = true }"#,
      r#"dbus = { sesion = ["org.kde.*"] }"#,
    ] {
      assert!(with_capabilities(body).is_err(), "{body}");
    }
  }

  #[test]
  fn values_of_the_wrong_type_are_rejected() {
    for body in [
      r#"storage = "yes""#,
      r#"fs = { read = "/etc" }"#,
      r#"fs = { write = ["${home}"] }"#,
      r#"fs = { write = ["${pluginDir}"] }"#,
      r#"fs = { write = ["${pluginDir}/cache"] }"#,
      r#"fs = { write = ["cache"] }"#,
      r#"fs = { write = ["${dataDir}/../../plugins/a"] }"#,
      r#"network = { hosts = "a" }"#,
      r#"network = { unix = "/run/a.sock" }"#,
      r#"network = { http = [{ host = "a", methods = ["GET"], port = 70000 }] }"#,
      r#"clipboard = { read = "yes" }"#,
      r#"process = { exit = 1 }"#,
      r#"process = { env = "HOME" }"#,
      r#"corona = "weather""#,
      r#"dbus = { session = "org.kde.*" }"#,
    ] {
      assert!(with_capabilities(body).is_err(), "{body}");
    }
  }

  #[test]
  fn http_methods_are_required() {
    assert!(with_capabilities(r#"network = { http = [{ host = "a" }] }"#).is_err());
  }

  #[test]
  fn default_grant() {
    let capabilities = grant("");
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
  fn omitted_capabilities_keep_storage() {
    let capabilities = with("").unwrap().capabilities.grant(
      Path::new("/plugins/a"),
      Path::new("/data/a"),
      &Map::new(),
    );
    assert!(capabilities.has_storage());
  }

  #[test]
  fn explicit_grants() {
    let capabilities = grant(
      r#"
fs = { read = ["${pluginDir}"], write = ["${dataDir}"] }
storage = false
clipboard = { read = true, write = true }
process = { exit = true }
"#,
    );
    assert!(!capabilities.has_storage());
    assert!(capabilities.has_read_access());
    assert!(capabilities.has_write_access());
    assert!(capabilities.is_clipboard_readable());
    assert!(capabilities.is_clipboard_writable());
    assert!(capabilities.may_exit());

    // an absolute path into the plugin directory is dropped
    assert!(!grant(r#"fs = { write = ["/plugins/a/x"] }"#).has_write_access());

    let capabilities = grant(r#"clipboard = { read = true }"#);
    assert!(capabilities.is_clipboard_readable());
    assert!(!capabilities.is_clipboard_writable());
  }

  #[test]
  fn execute_grants() {
    let capabilities = grant(r#"fs = { execute = ["git"] }"#);
    assert_eq!(
      capabilities.execute_grant(),
      &ExecuteGrant::Allowed(vec!["git".into()])
    );
    assert!(capabilities.may_run("git"));
    assert!(!capabilities.may_run("rm"));

    let capabilities = grant(r#"fs = { execute = [] }"#);
    assert!(!capabilities.may_run("git"));

    let capabilities = grant(r#"fs = { execute = "*" }"#);
    assert_eq!(capabilities.execute_grant(), &ExecuteGrant::Unrestricted);
    assert!(capabilities.may_run("anything"));

    assert!(with_capabilities(r#"fs = { execute = true }"#).is_err());
  }

  #[test]
  fn execute_string_must_be_wildcard() {
    assert!(with_capabilities(r#"fs = { execute = "git" }"#).is_err());
  }

  #[test]
  fn process_env_and_unix_sockets() {
    let capabilities = grant(
      r#"
process = { env = ["HOME", "XDG_RUNTIME_DIR"] }
network = { unix = ["/run/tailscale/tailscaled.sock", "${dataDir}/adb.sock"] }
"#,
    );
    assert!(capabilities.may_pass_env("HOME"));
    assert!(!capabilities.may_pass_env("LD_PRELOAD"));
    assert!(capabilities.may_connect_unix(Path::new("/run/tailscale/tailscaled.sock")));
    assert!(capabilities.may_connect_unix(Path::new("/data/a/adb.sock")));
    assert!(!capabilities.may_connect_unix(Path::new("/run/other.sock")));

    let none = grant("");
    assert!(!none.may_pass_env("HOME"));
    assert!(!none.may_connect_unix(Path::new("/run/tailscale/tailscaled.sock")));
    assert!(with_capabilities(r#"network = { unix = ["${home}/x.sock"] }"#).is_err());
    for bad in ["LD_PRELOAD", "LD_LIBRARY_PATH", "GCONV_PATH", "A=B"] {
      let body = format!("process = {{ env = [\"{bad}\"] }}");
      assert!(with_capabilities(&body).is_err(), "{bad}");
    }
  }

  #[test]
  fn images_resolve_against_the_plugin_dir() {
    assert!(format!("{:?}", grant("")).contains(r#"asset_root: Some("/plugins/a")"#));
  }

  #[test]
  fn network_hosts() {
    let capabilities = grant(r#"network = { hosts = ["API.Example.com"] }"#);
    assert!(capabilities.may_reach("api.example.com"));
    assert!(!capabilities.may_reach("example.com"));
    // a host grant allows any request to it
    assert!(capabilities.may_request("http", "api.example.com", Some(1), "POST", "/x"));
  }

  #[test]
  fn http_grants() {
    let capabilities = grant(
      r#"
[[capabilities.network.http]]
host = "Api.Example.com"
methods = ["get"]
paths = ["/v1/a"]
path_prefixes = ["/v2"]

[[capabilities.network.http]]
scheme = "http"
host = "local"
port = 8080
methods = ["POST"]
"#,
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
      r#"network = { http = [{ scheme = "http", host = "local", port = 8080, methods = ["POST"], path_prefixes = ["/"] }] }"#,
    );
    assert!(capabilities.may_request("http", "local", Some(8080), "POST", "/x"));
    assert!(!capabilities.may_request("http", "local", None, "POST", "/x"));
    assert!(!capabilities.may_request("https", "local", Some(8080), "POST", "/x"));
  }

  /// A plugin with text setting `url` and this `[capabilities]` body, the
  /// setting configured as `url`.
  fn with_url(body: &str, url: Option<&str>) -> Result<PluginManifest, anyhow::Error> {
    let file = with(&format!(
      "[[settings]]\nkey = \"url\"\nlabel = \"URL\"\ntype = \"text\"\ndefault = \"\"\n[capabilities]\n{body}"
    ))?;
    file.check()?;
    let configured = url.map(|url| Map::from_iter([("url".into(), json!(url))]));
    let manifest = PluginManifest::new(
      file,
      "/plugins/a".into(),
      Path::new("/data/a"),
      configured.as_ref(),
    );
    Ok(manifest)
  }

  #[test]
  fn setting_hosts() {
    let hosts = r#"network = { hosts = ["${setting:url}"] }"#;
    for (url, host) in [
      ("https://HA.local:8123/api", "ha.local"),
      ("ha.local:8123", "ha.local"),
      ("192.168.1.5", "192.168.1.5"),
      ("ha.local/x", "ha.local"),
    ] {
      let manifest = with_url(hosts, Some(url)).unwrap();
      assert!(manifest.capabilities.may_reach(host), "{url}");
      assert_eq!(manifest.host_settings, BTreeSet::from(["url".into()]));
    }
    // empty or invalid: no grant, and the placeholder is never a host
    for url in [None, Some(""), Some("a b"), Some("file:///etc")] {
      let manifest = with_url(hosts, url).unwrap();
      assert!(
        !manifest.capabilities.may_reach("${setting:url}"),
        "{url:?}"
      );
      let debug = format!("{:?}", manifest.capabilities);
      assert!(debug.contains("network_hosts: []"), "{url:?}");
    }

    let http = r#"network = { http = [{ host = "${setting:url}", methods = ["GET"], path_prefixes = ["/"] }] }"#;
    let manifest = with_url(http, Some("http://ha.local")).unwrap();
    assert!(
      manifest
        .capabilities
        .may_request("https", "ha.local", None, "GET", "/api")
    );
    let manifest = with_url(http, Some("")).unwrap();
    assert!(!format!("{:?}", manifest.capabilities).contains("HttpRequestGrant"));

    // only declared text settings, only as a whole host
    for body in [
      r#"network = { hosts = ["${setting:other}"] }"#,
      r#"network = { hosts = ["${setting:url}.example.com"] }"#,
      r#"network = { hosts = ["${pluginDir}"] }"#,
      r#"network = { http = [{ host = "${setting:nope}", methods = ["GET"] }] }"#,
    ] {
      assert!(with_url(body, None).is_err(), "{body}");
    }
    let toggle = "[[settings]]\nkey = \"on\"\nlabel = \"On\"\ntype = \"toggle\"\ndefault = true\n";
    let toggle = with(&format!(
      "{toggle}[capabilities]\nnetwork = {{ hosts = [\"${{setting:on}}\"] }}"
    ));
    assert!(toggle.unwrap().check().is_err());
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
    // relative paths resolve against the plugin directory
    assert_eq!(expand_raw("assets"), Path::new("/plugins/a/assets"));
    assert_eq!(expand_raw(""), Path::new("/plugins/a"));
    // but are not confined to it: `..` reaches outside, like an absolute path
    assert_eq!(expand_raw("../other"), Path::new("/plugins/a/../other"));
  }

  #[test]
  fn unknown_placeholder_is_rejected() {
    assert!(with_capabilities(r#"fs = { read = ["${homeDir}"] }"#).is_err());
  }

  #[test]
  fn top_level_unknown_fields_are_rejected() {
    assert!(with(r#"capabilites = { clipboard = { read = true } }"#).is_err());
    // the editor's schema pointer is the one extra key
    assert!(with(r#""$schema" = "plugin.schema.json""#).is_ok());
  }

  #[test]
  fn corona_modules() {
    assert!(with("").unwrap().capabilities.modules().is_empty());

    let manifest = with_capabilities(r#"corona = ["weather", "sysinfo", "weather"]"#).unwrap();
    assert_eq!(
      manifest.capabilities.modules(),
      BTreeSet::from([CoronaModule::Sysinfo, CoronaModule::Weather])
    );

    assert!(with_capabilities(r#"corona = ["camera"]"#).is_err());
  }

  #[test]
  fn dbus_grants() {
    assert_eq!(with("").unwrap().capabilities.dbus, None);
    let manifest = with_capabilities(r#"dbus = { session = ["org.kde.*"] }"#).unwrap();
    let grant = manifest.capabilities.dbus.unwrap();
    assert_eq!(grant.session, ["org.kde.*"]);
    assert!(grant.system.is_empty());

    for bad in [r#"["*"]"#, r#"["org.freedesktop.*"]"#, r#"[":1.2"]"#] {
      assert!(
        with_capabilities(&format!("dbus = {{ system = {bad} }}")).is_err(),
        "{bad}"
      );
    }
  }
}

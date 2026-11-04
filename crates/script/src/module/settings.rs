use std::collections::BTreeSet;

use anyhow::{Result, anyhow, ensure};
use corona_config::{Config, ConfigProvider};
use corona_js::{Cx, Data, Json, Listener, ModuleBuilder, Throws, Unsubscribe, host_module, track};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{App, AppContext, Context, Entity, EventEmitter, Subscription};
use serde::Serialize;
use serde_json::{Map, Value};
use specta::{Type, specta};

use crate::{
  ScriptManager,
  plugin::settings::{DynamicOptions, SelectOption, Setting, SettingKind, resolve},
};

/// A setting that changed, or a secret that was stored or removed, without its value
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
#[serde(untagged)]
pub enum Change {
  Setting { key: String, value: Value },
  Secret { key: String, secret: bool },
}

impl Change {
  pub fn secret(key: String) -> Self {
    Self::Secret { key, secret: true }
  }
}

/// A plugin's settings as the user set them, its defaults for the rest, and the shell's
/// language its translations follow. Emits each change.
pub struct Settings {
  id: String,
  declared: Vec<Setting>,
  pub values: Map<String, Value>,
  pub language: String,
  _config: Subscription,
}

impl EventEmitter<Change> for Settings {}

impl Settings {
  pub fn new(id: &str, declared: &[Setting], cx: &mut App) -> Entity<Self> {
    cx.new(|cx| Self {
      id: id.into(),
      declared: declared.to_vec(),
      values: resolved(id, declared, cx),
      language: rust_i18n::locale().to_string(),
      _config: cx.observe_global::<Config>(Self::refresh),
    })
  }

  /// Follows the config and the plugin's manifest as it is rescanned
  pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
    let manifest = cx
      .try_global::<ScriptManager>()
      .and_then(|m| m.plugins().get(&self.id));
    if let Some(manifest) = manifest {
      self.declared = manifest.settings.clone();
    }
    let now = resolved(&self.id, &self.declared, cx);
    let mut changed = false;
    for (key, value) in &now {
      if self.values.get(key) != Some(value) {
        changed = true;
        let (key, value) = (key.clone(), value.clone());
        cx.emit(Change::Setting { key, value });
      }
    }
    self.values = now;
    let language = rust_i18n::locale().to_string();
    if language != self.language {
      self.language = language;
      changed = true;
    }
    if changed {
      cx.notify();
    }
  }
}

fn resolved(id: &str, settings: &[Setting], cx: &App) -> Map<String, Value> {
  let config = cx.try_global::<Config>();
  resolve(id, settings, config.and_then(|c| c.plugin_settings.get(id)))
}

/// What a runtime of the plugin knows of its settings: `hosts` are the ones its network
/// grant reads, which it cannot `set`
struct Declared {
  id: String,
  settings: Vec<Setting>,
  hosts: BTreeSet<String>,
  entity: Entity<Settings>,
}

impl Declared {
  /// The declared setting `key`
  fn lookup(&self, key: &str) -> Result<&Setting> {
    let setting = self.settings.iter().find(|s| s.key == key);
    setting.ok_or_else(|| anyhow!("no setting `{key}`"))
  }

  fn values(&self, cx: &App) -> Map<String, Value> {
    track(&self.entity);
    self.entity.read(cx).values.clone()
  }
}

/// The value of setting `key`, its default when unset; null when the manifest does not
/// declare it.
#[specta]
fn get(cx: Cx, declared: Data<Declared>, key: String) -> Json<Value> {
  Json(declared.values(&cx).remove(&key).unwrap_or_default())
}

/// Every declared setting by key.
#[specta]
fn all(cx: Cx, declared: Data<Declared>) -> Json<Map<String, Value>> {
  Json(declared.values(&cx))
}

/// Calls `cb` with each change of a setting, or of a secret without its value.
#[specta]
fn on_change(mut cx: Cx, declared: Data<Declared>, cb: Listener<Json<Change>>) -> Unsubscribe {
  let listener = cb.clone();
  let sub = cx.subscribe(&declared.entity, move |_, change: &Change, cx| {
    listener.call(cx, Json(change.clone()))
  });
  cb.keep(sub)
}

/// Changes setting `key` in the config, as the settings app would.
#[specta]
fn set(mut cx: Cx, declared: Data<Declared>, key: String, value: Json<Value>) -> Throws<()> {
  let set = || -> Result<()> {
    let value = value.0;
    let setting = declared.lookup(&key)?;
    // a plugin setting its own host would grant itself any host
    ensure!(
      !declared.hosts.contains(&key),
      "setting `{key}` is a network grant, only the user changes it"
    );
    ensure!(setting.accepts(&value), "setting `{key}` cannot be {value}");
    let id = declared.id.clone();
    // the config observers render scripts again, so not from inside one
    cx.defer(move |cx| {
      // an unchanged value would rewrite the config for nothing
      if cx
        .config()
        .plugin_settings
        .get(&id)
        .and_then(|m| m.get(&key))
        == Some(&value)
      {
        return;
      }
      corona_config::update(cx, |c| {
        c.plugin_settings.entry(id).or_default().insert(key, value);
      })
      .log_err()
      .ok();
    });
    Ok(())
  };
  set().into()
}

/// The choices the settings app offers for dynamic select `key`.
#[specta]
fn set_options(
  mut cx: Cx,
  declared: Data<Declared>,
  key: String,
  choices: Json<Vec<SelectOption>>,
) -> Throws<()> {
  let set = || -> Result<()> {
    let setting = declared.lookup(&key)?;
    ensure!(
      matches!(setting.kind, SettingKind::Select { dynamic: true, .. }),
      "setting `{key}` is not a dynamic select"
    );
    cx.default_global::<DynamicOptions>()
      .0
      .entry(declared.id.clone())
      .or_default()
      .insert(key, choices.0);
    cx.refresh_windows();
    Ok(())
  };
  set().into()
}

/// `corona/settings`: the plugin's own settings, as the user set them in the settings app
/// or the config. Every plugin has it, for its own settings only. `hosts` are the settings
/// its network grant reads; `entity` is the plugin's [`Settings`].
pub fn module(
  id: &str,
  settings: &[Setting],
  hosts: &BTreeSet<String>,
  entity: Entity<Settings>,
  m: &mut ModuleBuilder,
) {
  m.provide(Declared {
    id: id.into(),
    settings: settings.to_vec(),
    hosts: hosts.clone(),
    entity,
  });
  host_module!(m, { sync: [get, all, on_change, set, set_options] });
  // typed by this plugin's manifest rather than by the Rust signatures
  m.declarations(declarations(settings));
}

/// The TypeScript of `corona/settings` for these settings: every key with its
/// exact type, so `get` checks the key and knows what it returns
fn declarations(settings: &[Setting]) -> String {
  let mut fields = String::new();
  // secrets are read from `corona/secrets`
  for setting in settings.iter().filter(|s| !s.is_secret()) {
    fields.push_str(&doc(&field_doc(setting), "    "));
    fields.push_str(&format!(
      "    {}: {};\n",
      quoted(&setting.key),
      ts_type(&setting.kind)
    ));
  }
  let dynamic = settings
    .iter()
    .filter(|s| matches!(s.kind, SettingKind::Select { dynamic: true, .. }))
    .map(|s| quoted(&s.key))
    .collect::<Vec<_>>();
  let dynamic = match dynamic.is_empty() {
    true => "never".to_string(),
    false => dynamic.join(" | "),
  };
  format!(
    "  /** This plugin's settings by key, as its manifest declares them. */
  export interface Settings {{
{fields}  }}
  /** The value of setting `key`; its default when the user did not set it. */
  export function get<K extends keyof Settings>(key: K): Settings[K];
  /** Every setting by key. */
  export function all(): Settings;
  /** A setting that changed, or a secret that was stored or removed, without its value. */
  export type Change = {{ [K in keyof Settings]: {{ key: K; value: Settings[K] }} }}[keyof Settings] | {{ key: string; secret: true }};
  /** Calls `cb` with each change; returns the fn that stops it. */
  export function onChange(cb: (change: Change) => void): () => void;
  /** Changes setting `key` in the config, as the settings app would. */
  export function set<K extends keyof Settings>(key: K, value: Settings[K]): void;
  /** The choices the settings app offers for dynamic select `key`. */
  export function setOptions(key: {dynamic}, options: {{ value: string; label: string }}[]): void;
"
  )
}

fn ts_type(kind: &SettingKind) -> String {
  match kind {
    SettingKind::Toggle { .. } => "boolean".into(),
    SettingKind::Text { .. } => "string".into(),
    SettingKind::Number { .. } | SettingKind::Slider { .. } => "number".into(),
    SettingKind::Select { dynamic: true, .. } => "string".into(),
    SettingKind::Select { options, .. } => options
      .iter()
      .map(|option| quoted(&option.value))
      .collect::<Vec<_>>()
      .join(" | "),
    SettingKind::List { .. } => "string[]".into(),
    SettingKind::Secret { .. } => "never".into(),
  }
}

/// The label, description, range and default, one line each
fn field_doc(setting: &Setting) -> Vec<String> {
  let mut lines = vec![setting.label.clone()];
  lines.extend(setting.description.clone());
  match &setting.kind {
    SettingKind::Number { min, max, step, .. } => {
      let bounds = [("min", min), ("max", max), ("step", step)]
        .into_iter()
        .filter_map(|(name, value)| value.map(|v| format!("{name} {v}")))
        .collect::<Vec<_>>();
      if !bounds.is_empty() {
        lines.push(bounds.join(", "));
      }
    }
    SettingKind::Slider { min, max, step, .. } => {
      lines.push(format!("min {min}, max {max}, step {step}"));
    }
    SettingKind::Select { options, .. } => lines.extend(
      options
        .iter()
        .map(|option| format!("{}: {}", quoted(&option.value), option.label)),
    ),
    _ => {}
  }
  lines.push(format!("@default {}", setting.default_value()));
  lines
}

/// A JSDoc block; `*/` in the text cannot end it early
fn doc(lines: &[String], indent: &str) -> String {
  let mut out = format!("{indent}/**\n");
  for line in lines.iter().flat_map(|l| l.lines()) {
    out.push_str(&format!("{indent} * {}\n", line.replace("*/", "*\\/")));
  }
  out.push_str(&format!("{indent} */\n"));
  out
}

/// A string literal, valid as a TypeScript key and type
fn quoted(text: &str) -> String {
  serde_json::to_string(text).unwrap_or_default()
}

#[cfg(test)]
mod tests {
  use corona_config::ConfigProvider;
  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;
  use crate::module::harness;

  #[derive(serde::Deserialize)]
  struct File {
    settings: Vec<Setting>,
  }

  fn settings(toml: &str) -> Vec<Setting> {
    toml::from_str::<File>(toml).unwrap().settings
  }

  #[test]
  fn declares_each_setting_with_its_type() {
    let settings = settings(
      r#"
[[settings]]
key = "on"
label = "On"
type = "toggle"
default = true
[[settings]]
key = "label"
label = "Label"
description = "Shown in the */ bar"
type = "text"
default = "x"
[[settings]]
key = "count"
label = "Count"
type = "number"
default = 1
min = 0
[[settings]]
key = "opacity"
label = "Opacity"
type = "slider"
default = 0.5
min = 0
max = 1
step = 0.1
[[settings]]
key = "units"
label = "Units"
type = "select"
default = "metric"
options = [{ value = "metric", label = "Metric" }, { value = "imperial", label = "Imperial" }]
[[settings]]
key = "host-names"
label = "Hosts"
type = "list"
default = ["a"]
"#,
    );
    let ts = declarations(&settings);
    for line in [
      "    \"on\": boolean;",
      "    \"label\": string;",
      "    \"count\": number;",
      "    \"opacity\": number;",
      "    \"units\": \"metric\" | \"imperial\";",
      "    \"host-names\": string[];",
      "     * Shown in the *\\/ bar",
      "     * min 0",
      "     * min 0, max 1, step 0.1",
      "     * \"imperial\": Imperial",
      "     * @default [\"a\"]",
      "  export function get<K extends keyof Settings>(key: K): Settings[K];",
      "  export function all(): Settings;",
      "  export function onChange(cb: (change: Change) => void): () => void;",
      "  export function set<K extends keyof Settings>(key: K, value: Settings[K]): void;",
      "  export function setOptions(key: never, options: { value: string; label: string }[]): void;",
    ] {
      assert!(ts.lines().any(|l| l == line), "missing {line:?} in\n{ts}");
    }
    // a comment cannot be closed from a label or description
    assert_eq!(ts.matches("*/").count(), settings.len() + 7);
  }

  #[test]
  fn secrets_are_left_out_and_dynamic_selects_are_strings() {
    let settings = settings(
      r#"
[[settings]]
key = "token"
label = "Token"
type = "secret"
[[settings]]
key = "device"
label = "Device"
type = "select"
default = ""
dynamic = true
"#,
    );
    let ts = declarations(&settings);
    assert!(!ts.contains("token"), "{ts}");
    assert!(ts.lines().any(|l| l == "    \"device\": string;"), "{ts}");
    assert!(ts.contains("setOptions(key: \"device\","), "{ts}");
  }

  const DECLARED: &str = r#"
[[settings]]
key = "on"
label = "On"
type = "toggle"
default = true
[[settings]]
key = "device"
label = "Device"
type = "select"
default = ""
dynamic = true
"#;

  #[gpui::test]
  fn set_writes_the_config(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    unsafe {
      std::env::set_var("XDG_CONFIG_HOME", tmp.path().join("config"));
      std::env::set_var("XDG_STATE_HOME", tmp.path().join("state"));
    }
    std::fs::create_dir_all(tmp.path().join("config/corona")).unwrap();
    cx.update(|cx| corona_config::load(cx).unwrap());
    let body = r#"if (!globalThis.started) {
      globalThis.started = true;
      report([
        attempt(() => m.set("on", false)),
        attempt(() => m.set("on", 1)),
        attempt(() => m.set("nope", 1)),
        attempt(() => m.set("device", "phone")),
      ]);
    }"#;
    let declared = settings(DECLARED);
    let (view, cx) = harness::view(cx, "corona/settings", body, move |_, m, cx| {
      module(
        "p",
        &declared,
        &BTreeSet::new(),
        Settings::new("p", &declared, cx),
        m,
      )
    });
    cx.run_until_parked();
    let report = view.last(cx);
    assert_eq!(report[0], Value::Null);
    assert_eq!(report[1]["message"], "setting `on` cannot be 1");
    assert_eq!(report[2]["message"], "no setting `nope`");
    cx.update(|_, cx| {
      let values = &cx.config().plugin_settings["p"];
      assert_eq!(values["on"], Value::Bool(false));
      assert_eq!(values["device"], "phone");
    });
    let written = std::fs::read_to_string(tmp.path().join("state/corona/settings.toml")).unwrap();
    assert!(written.contains("device = \"phone\""), "{written}");
  }

  #[gpui::test]
  fn set_options_are_kept_for_the_settings_app(cx: &mut TestAppContext) {
    let body = r#"if (!globalThis.started) {
      globalThis.started = true;
      report([
        attempt(() => m.setOptions("device", [{ value: "phone", label: "Phone" }])),
        attempt(() => m.setOptions("on", [])),
      ]);
    }"#;
    cx.update(|cx| cx.set_global(Config::default()));
    let declared = settings(DECLARED);
    let device = declared[1].clone();
    let (view, cx) = harness::view(cx, "corona/settings", body, move |_, m, cx| {
      module(
        "p",
        &declared,
        &BTreeSet::new(),
        Settings::new("p", &declared, cx),
        m,
      )
    });
    cx.run_until_parked();
    let report = view.last(cx);
    assert_eq!(report[0], Value::Null);
    assert_eq!(report[1]["message"], "setting `on` is not a dynamic select");
    cx.update(|_, cx| {
      let options = DynamicOptions::of("p", &device, cx);
      assert_eq!(options.len(), 1);
      assert_eq!(options[0].value, "phone");
      // another plugin's are not this one's
      assert!(DynamicOptions::of("q", &device, cx).is_empty());
    });
  }

  #[gpui::test]
  fn host_settings_are_the_users(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Config::default()));
    let body = r#"report(attempt(() => m.set("on", false)));"#;
    let declared = settings(DECLARED);
    let hosts = BTreeSet::from(["on".to_string()]);
    let (view, cx) = harness::view(cx, "corona/settings", body, move |_, m, cx| {
      module("p", &declared, &hosts, Settings::new("p", &declared, cx), m)
    });
    cx.run_until_parked();
    let message = view.last(cx)["message"].as_str().unwrap().to_string();
    assert!(message.contains("network grant"), "{message}");
    cx.update(|_, cx| assert!(cx.config().plugin_settings.is_empty()));
  }

  #[test]
  fn no_settings_is_an_empty_interface() {
    let ts = declarations(&[]);
    assert!(ts.contains("export interface Settings {\n  }"), "{ts}");
  }

  #[gpui::test]
  fn the_module_is_typed_by_its_declarations(cx: &mut TestAppContext) {
    cx.update(|cx| {
      let mut m = ModuleBuilder::new("corona/settings", &Default::default());
      module(
        "p",
        &[],
        &BTreeSet::new(),
        Settings::new("p", &[], cx),
        &mut m,
      );
      let ts = corona_js::d_ts([&m]);
      assert!(ts.contains("export function get"), "{ts}");
      // once, from the declarations, not again from the Rust types
      assert_eq!(ts.matches("export type Change").count(), 1, "{ts}");
    });
  }

  #[test]
  fn secrets_change_without_their_value() {
    let change = Change::Secret {
      key: "token".into(),
      secret: true,
    };
    let json = serde_json::to_value(change).unwrap();
    assert_eq!(json, serde_json::json!({ "key": "token", "secret": true }));
  }
}

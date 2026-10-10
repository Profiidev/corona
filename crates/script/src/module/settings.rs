use anyhow::{Result, ensure};
use corona_config::{Config, ConfigProvider};
use corona_macros::named;
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;
use gpui_shell::HostModule;
use serde_json::{Map, Value};

use crate::{
  host_fn::{Cx, Module},
  module::Subscribe,
  plugin::settings::{DynamicOptions, SelectOption, Setting, SettingKind, resolve},
};

/// `corona/settings`: the plugin's own settings, as the user set them in the
/// settings app or the config. Every plugin has it, for its own settings only.
pub fn module(id: &str, settings: &[Setting], subs: &mut Vec<Subscribe>) -> HostModule {
  let current = {
    let (id, settings) = (id.to_string(), settings.to_vec());
    move |cx: &App| resolved(&id, &settings, cx)
  };
  subs.push(watch(current.clone()));

  let get = current.clone();
  let (set, options) = (lookup(id, settings), lookup(id, settings));
  let module: HostModule = Module::new("corona/settings")
    .func(named!(
      "get",
      /// The value of setting `key`, its default when unset; null when the
      /// manifest does not declare it.
      move |cx: Cx, key: String| get(&cx).remove(&key)
    ))
    .func(named!(
      "all",
      /// Every declared setting by key.
      move |cx: Cx| Value::Object(current(&cx))
    ))
    .func(named!(
      "set",
      /// Changes setting `key` in the config, as the settings app would.
      move |cx: &mut App, key: String, value: Value| -> Result<()> {
        let (id, setting) = set(&key)?;
        ensure!(setting.accepts(&value), "setting `{key}` cannot be {value}");
        // the config observers render scripts again, so not from inside one
        cx.defer(move |cx| {
          corona_config::update(cx, |c| {
            c.plugin_settings.entry(id).or_default().insert(key, value);
          })
          .log_err()
          .ok();
        });
        Ok(())
      }
    ))
    .func(named!(
      "setOptions",
      /// The choices the settings app offers for dynamic select `key`.
      move |cx: &mut App, key: String, choices: Vec<SelectOption>| -> Result<()> {
        let (id, setting) = options(&key)?;
        ensure!(
          matches!(setting.kind, SettingKind::Select { dynamic: true, .. }),
          "setting `{key}` is not a dynamic select"
        );
        cx.default_global::<DynamicOptions>()
          .0
          .entry(id)
          .or_default()
          .insert(key, choices);
        cx.refresh_windows();
        Ok(())
      }
    ))
    .into();
  // typed by this plugin's manifest rather than by the Rust signatures
  module.declarations(declarations(settings))
}

/// The plugin id and the declared setting `key`
fn lookup(id: &str, settings: &[Setting]) -> impl Fn(&str) -> Result<(String, Setting)> + 'static {
  let (id, settings) = (id.to_string(), settings.to_vec());
  move |key| {
    let setting = settings.iter().find(|s| s.key == key);
    let setting = setting.ok_or_else(|| anyhow::anyhow!("no setting `{key}`"))?;
    Ok((id.clone(), setting.clone()))
  }
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
  /** Changes setting `key` in the config, as the settings app would. */
  export function set<K extends keyof Settings>(key: K, value: Settings[K]): void | Error;
  /** The choices the settings app offers for dynamic select `key`. */
  export function setOptions(key: {dynamic}, options: {{ value: string; label: string }}[]): void | Error;"
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

fn resolved(id: &str, settings: &[Setting], cx: &App) -> Map<String, Value> {
  resolve(id, settings, cx.config().plugin_settings.get(id))
}

/// Renders the script again when its settings change
fn watch(current: impl Fn(&App) -> Map<String, Value> + 'static) -> Subscribe {
  Subscribe::Refresh(Box::new(move |runtime, root, cx| {
    let (runtime, root) = (runtime.clone(), root.clone());
    let mut last = current(cx);
    cx.observe_global::<Config>(move |cx| {
      let now = current(cx);
      if now != last {
        last = now;
        runtime.refresh(&root, cx).log_err().ok();
      }
    })
  }))
}

#[cfg(test)]
mod tests {
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
      "  export function set<K extends keyof Settings>(key: K, value: Settings[K]): void | Error;",
      "  export function setOptions(key: never, options: { value: string; label: string }[]): void | Error;",
    ] {
      assert!(ts.lines().any(|l| l == line), "missing {line:?} in\n{ts}");
    }
    // a comment cannot be closed from a label or description
    assert_eq!(ts.matches("*/").count(), settings.len() + 5);
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
      report([m.set("on", false), m.set("on", 1), m.set("nope", 1), m.set("device", "phone")]);
    }"#;
    let declared = settings(DECLARED);
    let (view, cx) = harness::view(cx, body, |_, subs, _| module("p", &declared, subs));
    cx.run_until_parked();
    let report = view.last();
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
        m.setOptions("device", [{ value: "phone", label: "Phone" }]),
        m.setOptions("on", []),
      ]);
    }"#;
    cx.update(|cx| cx.set_global(Config::default()));
    let declared = settings(DECLARED);
    let device = declared[1].clone();
    let (view, cx) = harness::view(cx, body, |_, subs, _| module("p", &declared, subs));
    cx.run_until_parked();
    let report = view.last();
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

  #[test]
  fn no_settings_is_an_empty_interface() {
    let ts = declarations(&[]);
    assert!(ts.contains("export interface Settings {\n  }"), "{ts}");
  }

  #[test]
  fn the_module_accepts_its_declarations() {
    // gpui-shell checks the declared functions against the registered ones
    let module = module("p", &[], &mut Vec::new());
    assert!(module.declared().unwrap().contains("export function get"));
  }
}

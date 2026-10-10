use corona_config::{Config, ConfigProvider};
use corona_macros::named;
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;
use gpui_shell::HostModule;
use serde_json::{Map, Value};

use crate::{
  host_fn::{Cx, Module},
  module::Subscribe,
  plugin::settings::{Setting, SettingKind, resolve},
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
    .into();
  // typed by this plugin's manifest rather than by the Rust signatures
  module.declarations(declarations(settings))
}

/// The TypeScript of `corona/settings` for these settings: every key with its
/// exact type, so `get` checks the key and knows what it returns
fn declarations(settings: &[Setting]) -> String {
  let mut fields = String::new();
  for setting in settings {
    fields.push_str(&doc(&field_doc(setting), "    "));
    fields.push_str(&format!(
      "    {}: {};\n",
      quoted(&setting.key),
      ts_type(&setting.kind)
    ));
  }
  format!(
    "  /** This plugin's settings by key, as its manifest declares them. */
  export interface Settings {{
{fields}  }}
  /** The value of setting `key`; its default when the user did not set it. */
  export function get<K extends keyof Settings>(key: K): Settings[K];
  /** Every setting by key. */
  export function all(): Settings;"
  )
}

fn ts_type(kind: &SettingKind) -> String {
  match kind {
    SettingKind::Toggle { .. } => "boolean".into(),
    SettingKind::Text { .. } => "string".into(),
    SettingKind::Number { .. } | SettingKind::Slider { .. } => "number".into(),
    SettingKind::Select { options, .. } => options
      .iter()
      .map(|option| quoted(&option.value))
      .collect::<Vec<_>>()
      .join(" | "),
    SettingKind::List { .. } => "string[]".into(),
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
  use super::*;

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
    ] {
      assert!(ts.lines().any(|l| l == line), "missing {line:?} in\n{ts}");
    }
    // a comment cannot be closed from a label or description
    assert_eq!(ts.matches("*/").count(), settings.len() + 3);
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

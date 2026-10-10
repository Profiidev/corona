//! Settings a plugin declares in its manifest. Each has one fixed type, and each
//! type one control in the settings app; the values live in the shell config
//! under `[plugin_settings."<id>"]`.

use std::collections::{HashMap, HashSet};

use anyhow::{Result, bail};
use gpui_kit::{App, Global};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value};

#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema)]
pub struct Setting {
  /// The key in `[plugin_settings."<id>"]` and for `get` in `corona/settings`.
  pub key: String,
  /// Shown in the settings app.
  pub label: String,
  #[serde(default)]
  pub description: Option<String>,
  #[serde(flatten)]
  pub kind: SettingKind,
}

/// The type of a setting, which picks its control in the settings app.
#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SettingKind {
  /// A switch.
  Toggle { default: bool },
  /// A text input.
  Text {
    default: String,
    #[serde(default)]
    placeholder: Option<String>,
  },
  /// A number input.
  Number {
    default: f64,
    #[serde(default)]
    min: Option<f64>,
    #[serde(default)]
    max: Option<f64>,
    #[serde(default)]
    step: Option<f64>,
  },
  /// A slider.
  Slider {
    default: f64,
    min: f64,
    max: f64,
    step: f64,
  },
  /// A dropdown of choices; the value is the chosen `value`.
  Select {
    default: String,
    #[serde(default)]
    options: Vec<SelectOption>,
    /// The plugin replaces `options` at runtime with `setOptions` from
    /// `corona/settings`, and any string is a value.
    #[serde(default)]
    dynamic: bool,
  },
  /// A list of strings, rows added and removed one by one.
  List { default: Vec<String> },
  /// A masked input that stores into the keyring, never the config. The
  /// plugin reads it with `get` from `corona/secrets` under the same key,
  /// which needs `corona = ["secrets"]`.
  Secret {
    #[serde(default)]
    placeholder: Option<String>,
  },
}

#[derive(Clone, Debug, PartialEq, Deserialize, JsonSchema, ts_rs::TS)]
#[serde(deny_unknown_fields)]
pub struct SelectOption {
  pub value: String,
  pub label: String,
}

impl Setting {
  pub fn default_value(&self) -> Value {
    match &self.kind {
      SettingKind::Toggle { default } => Value::Bool(*default),
      SettingKind::Text { default, .. } => Value::String(default.clone()),
      SettingKind::Number { default, .. } | SettingKind::Slider { default, .. } => number(*default),
      SettingKind::Select { default, .. } => Value::String(default.clone()),
      SettingKind::List { default } => {
        Value::Array(default.iter().cloned().map(Value::String).collect())
      }
      SettingKind::Secret { .. } => Value::Null,
    }
  }

  /// Whether `value` is one this setting can take
  pub fn accepts(&self, value: &Value) -> bool {
    match &self.kind {
      SettingKind::Toggle { .. } => value.is_boolean(),
      SettingKind::Text { .. } => value.is_string(),
      SettingKind::Number { min, max, .. } => {
        value.as_f64().is_some_and(|v| in_range(v, *min, *max))
      }
      SettingKind::Slider { min, max, .. } => value
        .as_f64()
        .is_some_and(|v| in_range(v, Some(*min), Some(*max))),
      SettingKind::Select { dynamic: true, .. } => value.is_string(),
      SettingKind::Select { options, .. } => value
        .as_str()
        .is_some_and(|v| options.iter().any(|o| o.value == v)),
      SettingKind::List { .. } => value
        .as_array()
        .is_some_and(|items| items.iter().all(Value::is_string)),
      // in the keyring, never in the config
      SettingKind::Secret { .. } => false,
    }
  }

  pub fn is_secret(&self) -> bool {
    matches!(self.kind, SettingKind::Secret { .. })
  }

  fn validate(&self) -> Result<()> {
    let key = &self.key;
    if let SettingKind::Slider { min, max, step, .. } = &self.kind
      && !(min.is_finite() && max.is_finite() && min < max && step.is_finite() && *step > 0.)
    {
      bail!("setting `{key}`: a slider needs finite min < max and a positive step");
    }
    if let SettingKind::Number { min, max, step, .. } = &self.kind
      && !([min, max].into_iter().flatten().all(|v| v.is_finite())
        && step.is_none_or(|step| step.is_finite() && step > 0.))
    {
      bail!("setting `{key}`: a number needs finite bounds and a positive step");
    }
    if let SettingKind::Number {
      min: Some(min),
      max: Some(max),
      ..
    } = &self.kind
      && min > max
    {
      bail!("setting `{key}`: min is above max");
    }
    if let SettingKind::Select { options, .. } = &self.kind {
      let mut seen = HashSet::new();
      if let Some(option) = options.iter().find(|o| !seen.insert(&o.value)) {
        bail!("setting `{key}`: option `{}` is listed twice", option.value);
      }
    }
    if !self.is_secret() && !self.accepts(&self.default_value()) {
      bail!("setting `{key}`: the default is not one of its values");
    }
    Ok(())
  }
}

/// Checks the declared settings: unique keys, defaults they accept
pub fn validate(settings: &[Setting]) -> Result<()> {
  let mut keys = HashSet::new();
  for setting in settings {
    if !keys.insert(&setting.key) {
      bail!("setting `{}` is declared twice", setting.key);
    }
    setting.validate()?;
  }
  Ok(())
}

/// The value of every declared setting: the configured one if it fits, else the
/// default. Configured keys the manifest does not declare are dropped, and
/// secrets are not here at all.
pub fn resolve(
  id: &str,
  settings: &[Setting],
  configured: Option<&Map<String, Value>>,
) -> Map<String, Value> {
  settings
    .iter()
    .filter(|setting| !setting.is_secret())
    .map(|setting| {
      let value = match configured.and_then(|c| c.get(&setting.key)) {
        Some(value) if setting.accepts(value) => value.clone(),
        Some(value) => {
          tracing::warn!(
            "plugin `{id}`: setting `{}` cannot be {value}, using its default",
            setting.key
          );
          setting.default_value()
        }
        None => setting.default_value(),
      };
      (setting.key.clone(), value)
    })
    .collect()
}

/// The options plugins set for their dynamic selects, by plugin id and key.
/// Only in memory: a plugin sets them again when it starts.
#[derive(Default)]
pub struct DynamicOptions(pub HashMap<String, HashMap<String, Vec<SelectOption>>>);

impl Global for DynamicOptions {}

impl DynamicOptions {
  /// What the dropdown of `setting` offers: the plugin's options once it set
  /// some, else the manifest's
  pub fn of<'a>(id: &str, setting: &'a Setting, cx: &'a App) -> &'a [SelectOption] {
    let SettingKind::Select { options, .. } = &setting.kind else {
      return &[];
    };
    cx.try_global::<Self>()
      .and_then(|all| all.0.get(id)?.get(&setting.key))
      .unwrap_or(options)
  }
}

fn in_range(value: f64, min: Option<f64>, max: Option<f64>) -> bool {
  value.is_finite() && min.is_none_or(|min| value >= min) && max.is_none_or(|max| value <= max)
}

/// Whole numbers as integers, so JS and the TOML file read `5`, not `5.0`
pub fn number(value: f64) -> Value {
  if value.fract() == 0. && value.abs() < i64::MAX as f64 {
    Value::from(value as i64)
  } else {
    Value::from(value)
  }
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  #[derive(Deserialize)]
  struct File {
    settings: Vec<Setting>,
  }

  fn parse(toml: &str) -> Result<Vec<Setting>> {
    let settings = toml::from_str::<File>(toml)?.settings;
    validate(&settings)?;
    Ok(settings)
  }

  const ALL: &str = r#"
[[settings]]
key = "on"
label = "On"
type = "toggle"
default = true

[[settings]]
key = "name"
label = "Name"
description = "Who"
type = "text"
default = "x"
placeholder = "name"

[[settings]]
key = "interval"
label = "Interval"
type = "number"
default = 5
min = 1

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
key = "hosts"
label = "Hosts"
type = "list"
default = ["a"]
"#;

  #[test]
  fn every_type_parses() {
    let settings = parse(ALL).unwrap();
    let defaults: Vec<_> = settings.iter().map(Setting::default_value).collect();
    assert_eq!(
      defaults,
      [
        json!(true),
        json!("x"),
        json!(5),
        json!(0.5),
        json!("metric"),
        json!(["a"])
      ]
    );
    assert_eq!(settings[1].description.as_deref(), Some("Who"));
    assert!(matches!(
      &settings[1].kind,
      SettingKind::Text { placeholder: Some(p), .. } if p == "name"
    ));
  }

  #[test]
  fn bad_declarations_are_rejected() {
    let setting = |rest: &str| format!("[[settings]]\nkey = \"k\"\nlabel = \"K\"\n{rest}\n");
    for bad in [
      "type = \"color\"\ndefault = \"red\"",
      "type = \"toggle\"",
      "type = \"toggle\"\ndefault = 1",
      "type = \"number\"\ndefault = 0\nmin = 1",
      "type = \"number\"\ndefault = 1\nmin = 2\nmax = 1",
      "type = \"slider\"\ndefault = 2\nmin = 0\nmax = 1\nstep = 0.1",
      "type = \"slider\"\ndefault = 0\nmin = 0\nmax = 1\nstep = 0",
      "type = \"select\"\ndefault = \"c\"\noptions = [{ value = \"a\", label = \"A\" }]",
      "type = \"select\"\ndefault = \"a\"\noptions = [{ value = \"a\", label = \"A\" }, { value = \"a\", label = \"B\" }]",
      "type = \"select\"\ndefault = \"a\"\noptions = [{ value = \"a\", lable = \"A\" }]",
      "type = \"list\"\ndefault = [1]",
    ] {
      assert!(parse(&setting(bad)).is_err(), "{bad}");
    }
    let twice = format!(
      "{}{}",
      setting("type = \"toggle\"\ndefault = true"),
      setting("type = \"toggle\"\ndefault = false")
    );
    assert!(parse(&twice).unwrap_err().to_string().contains("twice"));
  }

  #[test]
  fn numbers_must_be_finite() {
    let setting = |rest: &str| format!("[[settings]]\nkey = \"k\"\nlabel = \"K\"\n{rest}\n");
    for bad in [
      "type = \"number\"\ndefault = inf",
      "type = \"number\"\ndefault = nan",
      "type = \"number\"\ndefault = 1\nmin = nan",
      "type = \"number\"\ndefault = 1\nmax = inf",
      "type = \"number\"\ndefault = 1\nstep = 0",
      "type = \"number\"\ndefault = 1\nstep = -1",
      "type = \"number\"\ndefault = 1\nstep = nan",
      "type = \"slider\"\ndefault = 0\nmin = nan\nmax = 1\nstep = 0.1",
      "type = \"slider\"\ndefault = 0\nmin = 0\nmax = 1\nstep = nan",
      "type = \"slider\"\ndefault = 0\nmin = 0\nmax = inf\nstep = 1",
    ] {
      assert!(parse(&setting(bad)).is_err(), "{bad}");
    }
  }

  #[test]
  fn resolves_configured_over_defaults() {
    let settings = parse(ALL).unwrap();
    let configured = json!({
      "on": false,
      "name": 3,
      "interval": 0,
      "opacity": 0.7,
      "units": "kelvin",
      "hosts": ["b", "c"],
      "unknown": true,
    });
    let resolved = resolve("p", &settings, configured.as_object());
    assert_eq!(
      Value::Object(resolved),
      json!({
        "on": false,
        // wrong type, below min, not an option: defaults
        "name": "x",
        "interval": 5,
        "opacity": 0.7,
        "units": "metric",
        "hosts": ["b", "c"],
      })
    );
    let defaults = resolve("p", &settings, None);
    assert_eq!(defaults["on"], json!(true));
    assert_eq!(defaults.len(), settings.len());
  }

  #[test]
  fn secrets_have_no_value() {
    let settings = parse(
      r#"
[[settings]]
key = "token"
label = "Token"
type = "secret"
placeholder = "paste it"
"#,
    )
    .unwrap();
    assert!(matches!(
      &settings[0].kind,
      SettingKind::Secret { placeholder: Some(p) } if p == "paste it"
    ));
    assert!(!settings[0].accepts(&json!("x")));
    // not resolved, not even from the config
    let configured = json!({ "token": "x" });
    assert!(resolve("p", &settings, configured.as_object()).is_empty());
  }

  #[test]
  fn dynamic_selects_take_any_string() {
    let settings = parse(
      r#"
[[settings]]
key = "device"
label = "Device"
type = "select"
default = ""
dynamic = true
"#,
    )
    .unwrap();
    assert!(settings[0].accepts(&json!("phone")));
    assert!(!settings[0].accepts(&json!(1)));
    let resolved = resolve("p", &settings, json!({ "device": "phone" }).as_object());
    assert_eq!(resolved["device"], json!("phone"));
    // without `dynamic` the default must be an option
    assert!(
      parse("[[settings]]\nkey = \"k\"\nlabel = \"K\"\ntype = \"select\"\ndefault = \"\"").is_err()
    );
  }

  #[test]
  fn numbers() {
    assert_eq!(number(5.), json!(5));
    assert_eq!(number(0.5), json!(0.5));
    assert_eq!(number(-2.), json!(-2));
  }
}

use std::{
  cell::RefCell,
  collections::{BTreeSet, VecDeque},
  rc::Rc,
};

use anyhow::{Result, anyhow, ensure};
use corona_config::{Config, ConfigProvider};
use corona_macros::named;
use corona_utils::error::ErrorLogExt;
use gpui_kit::{App, Subscription};
use gpui_shell::HostModule;
use serde::Serialize;
use serde_json::{Map, Value};
use ts_rs::TS;

use crate::{
  host_fn::{Cx, Module},
  module::{
    Subscribe,
    plugin::{Hub, SecretChanged},
  },
  plugin::settings::{DynamicOptions, SelectOption, Setting, SettingKind, resolve},
};

/// Changes kept for a script that is not waiting for one
const CHANGE_BUFFER: usize = 16;

/// A setting that changed, or a secret that was stored or removed, without
/// its value
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(untagged)]
enum Change {
  Setting { key: String, value: Value },
  Secret { key: String, secret: bool },
}

/// The changes for one script's `nextChange`
#[derive(Default)]
struct Changes {
  kept: VecDeque<Change>,
  waiters: Vec<flume::Sender<Change>>,
}

impl Changes {
  fn next(&mut self) -> flume::Receiver<Change> {
    let (tx, rx) = flume::bounded(1);
    match self.kept.pop_front() {
      Some(change) => drop(tx.send(change)),
      None => self.waiters.push(tx),
    }
    rx
  }

  /// To every call waiting; kept for the next one when none is
  fn push(&mut self, change: Change) {
    let waiters = self.waiters.drain(..);
    let delivered = waiters.filter(|tx| tx.send(change.clone()).is_ok()).count();
    if delivered == 0 {
      self.kept.push_back(change);
      if self.kept.len() > CHANGE_BUFFER {
        self.kept.pop_front();
      }
    }
  }
}

/// `corona/settings`: the plugin's own settings, as the user set them in the
/// settings app or the config. Every plugin has it, for its own settings only.
/// `hosts` are the settings its network grant reads, which it cannot `set`;
/// `hub` tells of its secrets changing.
pub fn module(
  id: &str,
  settings: &[Setting],
  hosts: &BTreeSet<String>,
  hub: &Hub,
  subs: &mut Vec<Subscribe>,
  cx: &mut App,
) -> HostModule {
  let current = {
    let (id, settings) = (id.to_string(), settings.to_vec());
    move |cx: &App| resolved(&id, &settings, cx)
  };
  subs.push(watch(current.clone()));
  let changes = Rc::<RefCell<Changes>>::default();
  subs.push(listen(current.clone(), hub, changes.clone(), cx));

  let get = current.clone();
  let (set, options) = (lookup(id, settings), lookup(id, settings));
  let hosts = hosts.clone();
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
      "nextChange",
      /// The next change of a setting, or of a secret without its value,
      /// once one comes.
      move || {
        let rx = changes.borrow_mut().next();
        async move { rx.recv_async().await.map_err(|_| anyhow!("plugin stopped")) }
      }
    ))
    .func(named!(
      "set",
      /// Changes setting `key` in the config, as the settings app would.
      move |cx: &mut App, key: String, value: Value| -> Result<()> {
        let (id, setting) = set(&key)?;
        // a plugin setting its own host would grant itself any host
        ensure!(
          !hosts.contains(&key),
          "setting `{key}` is a network grant, only the user changes it"
        );
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
  /** A setting that changed, or a secret that was stored or removed, without its value. */
  export type Change = {{ [K in keyof Settings]: {{ key: K; value: Settings[K] }} }}[keyof Settings] | {{ key: string; secret: true }};
  /** The next change, once one comes. Changes while none is awaited are kept, the last 16. */
  export function nextChange(): Promise<Change | Error>;
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

/// Keeps the changes for `nextChange`, in views and services alike: only the
/// settings that changed, and secret keys without their values
fn listen(
  current: impl Fn(&App) -> Map<String, Value> + 'static,
  hub: &Hub,
  changes: Rc<RefCell<Changes>>,
  cx: &mut App,
) -> Subscribe {
  let mut last = current(cx);
  let push = changes.clone();
  let settings = cx.observe_global::<Config>(move |cx| {
    let now = current(cx);
    for (key, value) in &now {
      if last.get(key) != Some(value) {
        let (key, value) = (key.clone(), value.clone());
        push.borrow_mut().push(Change::Setting { key, value });
      }
    }
    last = now;
  });
  let push = changes.clone();
  let secrets = cx.subscribe(&hub.secrets, move |_, SecretChanged(key), _| {
    let key = key.clone();
    push.borrow_mut().push(Change::Secret { key, secret: true });
  });
  Subscribe::Cleanup(Subscription::new(move || {
    drop((settings, secrets));
    // gpui-shell never drops a pending call's future, so wake it with an error
    changes.borrow_mut().waiters.clear();
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
      report([m.set("on", false), m.set("on", 1), m.set("nope", 1), m.set("device", "phone")]);
    }"#;
    let declared = settings(DECLARED);
    let (view, cx) = harness::view(cx, body, |_, subs, cx| {
      module("p", &declared, &BTreeSet::new(), &Hub::new(cx), subs, cx)
    });
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
    let (view, cx) = harness::view(cx, body, |_, subs, cx| {
      module("p", &declared, &BTreeSet::new(), &Hub::new(cx), subs, cx)
    });
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

  #[gpui::test]
  fn host_settings_are_the_users(cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_global(Config::default()));
    let body = r#"report(m.set("on", false));"#;
    let declared = settings(DECLARED);
    let hosts = BTreeSet::from(["on".to_string()]);
    let (view, cx) = harness::view(cx, body, |_, subs, cx| {
      module("p", &declared, &hosts, &Hub::new(cx), subs, cx)
    });
    cx.run_until_parked();
    let message = view.last()["message"].as_str().unwrap().to_string();
    assert!(message.contains("network grant"), "{message}");
    cx.update(|_, cx| assert!(cx.config().plugin_settings.is_empty()));
  }

  #[test]
  fn no_settings_is_an_empty_interface() {
    let ts = declarations(&[]);
    assert!(ts.contains("export interface Settings {\n  }"), "{ts}");
  }

  #[gpui::test]
  fn the_module_accepts_its_declarations(cx: &mut TestAppContext) {
    cx.update(|cx| {
      cx.set_global(Config::default());
      // gpui-shell checks the declared functions against the registered ones
      let module = module(
        "p",
        &[],
        &BTreeSet::new(),
        &Hub::new(cx),
        &mut Vec::new(),
        cx,
      );
      let declared = module.declared().unwrap();
      assert!(declared.contains("export function get"));
      assert!(declared.contains("export function nextChange(): Promise<Change | Error>;"));
    });
  }

  #[test]
  fn changes_go_to_every_waiter_or_are_kept() {
    let mut changes = Changes::default();
    let change = |key: usize| Change::Setting {
      key: key.to_string(),
      value: Value::Null,
    };
    let (a, b) = (changes.next(), changes.next());
    changes.push(change(1));
    assert_eq!(a.try_recv().unwrap(), change(1));
    assert_eq!(b.try_recv().unwrap(), change(1));

    for key in 0..20 {
      changes.push(change(key));
    }
    assert_eq!(changes.kept.len(), CHANGE_BUFFER);
    assert_eq!(changes.next().try_recv().unwrap(), change(4));
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

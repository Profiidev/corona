//! `corona/secrets`: strings in the user's keyring, one namespace per plugin.

use anyhow::{Result, ensure};
use corona_js::{Cx, ModuleBuilder, Plugin, PluginRef, ScriptModule, Throws, host_module};
use specta::specta;

use crate::ScriptManager;
use gpui_kit::App;
use oo7::Keyring;

/// The attributes the secret `key` of plugin `id` is stored under.
fn attributes(id: &str, key: &str) -> Result<[(&'static str, String); 3]> {
  ensure!(!key.is_empty(), "the key is empty");
  Ok([
    ("application", "corona".into()),
    ("plugin", id.into()),
    ("key", key.into()),
  ])
}

// ponytail: a keyring connection per call, keep one around if plugins call this often
async fn keyring() -> Result<Keyring> {
  let keyring = Keyring::new().await?;
  keyring.unlock().await?;
  Ok(keyring)
}

/// Stores `value` as secret `key` of plugin `id`, replacing what was there.
/// The settings app stores secret settings with it.
pub async fn store(id: &str, key: &str, value: String) -> Result<()> {
  let attributes = attributes(id, key)?;
  let label = format!("Corona plugin {id}: {key}");
  keyring()
    .await?
    .create_item(&label, &attributes, value, true)
    .await?;
  Ok(())
}

/// Deletes secret `key` of plugin `id`.
pub async fn remove(id: &str, key: &str) -> Result<()> {
  let attributes = attributes(id, key)?;
  keyring().await?.delete(&attributes).await?;
  Ok(())
}

async fn get_secret(id: &str, key: &str) -> Result<Option<String>> {
  let attributes = attributes(id, key)?;
  let items = keyring().await?.search_items(&attributes).await?;
  let Some(item) = items.first() else {
    return Ok(None);
  };
  let secret = item.secret().await?;
  Ok(Some(String::from_utf8_lossy(&secret).into_owned()))
}

/// The secret stored under `key`, null when there is none.
#[specta]
async fn get(plugin: Plugin, key: String) -> Throws<Option<String>> {
  get_secret(&plugin.id, &key).await.into()
}

/// Stores `value` under `key`, replacing what was there.
#[specta]
async fn set(mut cx: Cx, plugin: Plugin, key: String, value: String) -> Throws<()> {
  let stored = store(&plugin.id, &key, value).await;
  if stored.is_ok() {
    ScriptManager::secret_changed(&plugin.id, &key, &mut cx);
  }
  stored.into()
}

/// Deletes the secret under `key`. Not `delete`, a reserved word in JS.
#[specta(rename = "remove")]
async fn remove_secret(mut cx: Cx, plugin: Plugin, key: String) -> Throws<()> {
  let removed = remove(&plugin.id, &key).await;
  if removed.is_ok() {
    ScriptManager::secret_changed(&plugin.id, &key, &mut cx);
  }
  removed.into()
}

/// `corona/secrets`
pub struct Secrets;

impl ScriptModule for Secrets {
  const NAME: &str = "secrets";
  const DOC: &str = "Strings in the user's keyring, e.g. tokens.";

  fn module(_: &PluginRef, m: &mut ModuleBuilder, _: &mut App) {
    host_module!(m, { async: [get, set, remove_secret] });
  }
}

#[cfg(test)]
mod tests {
  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;
  use crate::module::harness;

  #[test]
  fn keys_are_namespaced_and_not_empty() {
    assert!(attributes("a", "").is_err());
    let attributes = attributes("a", "token").unwrap();
    assert_eq!(attributes[1], ("plugin", "a".into()));
    assert_eq!(attributes[2], ("key", "token".into()));
  }

  #[gpui::test]
  fn declarations(cx: &mut TestAppContext) {
    let m = cx.update(|cx| {
      let mut m = ModuleBuilder::new("corona/secrets", &Default::default());
      Secrets::module(&PluginRef::default(), &mut m, cx);
      m
    });
    let declared = corona_js::d_ts([&m]);
    for line in [
      "  export function get(key: string): Promise<string | null>;",
      "  export function set(key: string, value: string): Promise<void>;",
      "  export function remove(key: string): Promise<void>;",
    ] {
      assert!(declared.lines().any(|l| l == line), "{line} in\n{declared}");
    }
  }

  #[gpui::test]
  fn empty_keys_fail_without_the_keyring(cx: &mut TestAppContext) {
    let body = r#"if (!globalThis.started) {
      globalThis.started = true;
      const failed = (p) => p.then(() => null, (e) => ({ message: e.message }));
      Promise.all([failed(m.get("")), failed(m.set("", "x")), failed(m.remove(""))]).then(report);
    }"#;
    let (view, cx) = harness::module::<Secrets>(cx, body);
    cx.run_until_parked();
    let error = serde_json::json!({ "message": "the key is empty" });
    assert_eq!(view.last(cx), serde_json::json!([error, error, error]));
  }
}

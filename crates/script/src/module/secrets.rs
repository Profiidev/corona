//! `corona/secrets`: strings in the user's keyring, one namespace per plugin.

use std::sync::Arc;

use anyhow::{Result, ensure};
use corona_macros::named;
use futures_lite::{FutureExt as _, future::Boxed};
use gpui_shell::HostModule;
use oo7::Keyring;

use crate::{host_fn::Module, module::PluginRef};

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

pub fn module(plugin: PluginRef) -> HostModule {
  let id: Arc<str> = plugin.id.into();

  let plugin = id.clone();
  let module = Module::new("corona/secrets").func(named!(
    "get",
    /// The secret stored under `key`, null when there is none.
    move |key: String| -> Result<Boxed<Result<Option<String>>>> {
      let attributes = attributes(&plugin, &key)?;
      Ok(
        async move {
          let items = keyring().await?.search_items(&attributes).await?;
          let Some(item) = items.first() else {
            return Ok(None);
          };
          let secret = item.secret().await?;
          Ok(Some(String::from_utf8_lossy(&secret).into_owned()))
        }
        .boxed(),
      )
    }
  ));
  let plugin = id.clone();
  let module = module.func(named!(
    "set",
    /// Stores `value` under `key`, replacing what was there.
    move |key: String, value: String| -> Result<Boxed<Result<()>>> {
      attributes(&plugin, &key)?;
      let plugin = plugin.clone();
      Ok(async move { store(&plugin, &key, value).await }.boxed())
    }
  ));
  let plugin = id;
  module
    .func(named!(
      // not `delete`, a reserved word in JS
      "remove",
      /// Deletes the secret under `key`.
      move |key: String| -> Result<Boxed<Result<()>>> {
        attributes(&plugin, &key)?;
        let plugin = plugin.clone();
        Ok(async move { remove(&plugin, &key).await }.boxed())
      }
    ))
    .into()
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

  #[test]
  fn declarations() {
    let module: HostModule = module(PluginRef {
      id: "a",
      name: "A",
      capabilities: &Default::default(),
    });
    let declared = module.declared().unwrap();
    for line in [
      "export function get(key: string): Promise<string | null | Error>;",
      "export function set(key: string, value: string): Promise<void | Error>;",
      "export function remove(key: string): Promise<void | Error>;",
    ] {
      assert!(declared.lines().any(|l| l == line), "{line} in\n{declared}");
    }
  }

  #[gpui::test]
  fn empty_keys_fail_without_the_keyring(cx: &mut TestAppContext) {
    let body = r#"if (!globalThis.started) {
      globalThis.started = true;
      Promise.all([m.get(""), m.set("", "x"), m.remove("")]).then(report);
    }"#;
    let (view, cx) = harness::view(cx, body, |_, _, _| {
      module(PluginRef {
        id: "a",
        name: "A",
        capabilities: &Default::default(),
      })
    });
    cx.run_until_parked();
    let error = serde_json::json!({ "message": "the key is empty" });
    assert_eq!(view.last(), serde_json::json!([error, error, error]));
  }
}

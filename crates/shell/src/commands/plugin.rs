use anyhow::Result;
use corona_ipc::{IpcCommand, IpcServer, Reply};
use gpui_kit::App;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub fn register_commands(server: &mut IpcServer) {
  server.register::<PluginCall>();
}

/// Calls `method` of plugin `id`'s service, its answer
#[derive(Serialize, Deserialize)]
pub struct PluginCall {
  pub id: String,
  pub method: String,
  pub args: Value,
}

impl IpcCommand for PluginCall {
  const COMMAND: &'static str = "plugin:call";

  type Payload = Self;
  type Response = Value;

  fn handle(_: Self, _: &mut App) -> Result<Value> {
    unreachable!("answered by `reply`")
  }

  fn reply(call: Self, cx: &mut App) -> Reply<Value> {
    match corona_script::call(&call.id, call.method, call.args, cx) {
      Ok(answer) => Box::pin(answer),
      Err(e) => Box::pin(std::future::ready(Err(e))),
    }
  }
}

#[cfg(test)]
mod tests {
  use std::{collections::HashMap, path::Path};

  use corona_script::{
    ScriptManager,
    plugin::{
      manifest::{ManifestFile, PluginManifest},
      paths::Paths,
    },
  };
  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;

  #[gpui::test]
  fn calls_fail_without_a_plugin_or_service(cx: &mut TestAppContext) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = Paths {
      state: tmp.path().join("state"),
      local: tmp.path().join("local"),
    };
    let file: ManifestFile =
      serde_json::from_value(serde_json::json!({ "id": "a", "name": "A" })).unwrap();
    let manifest = PluginManifest::new(file, tmp.path().into(), Path::new("/data"), None);
    cx.update(|cx| {
      let runtime = gpui_shell::ShellRuntime::new_isolated_with_components(
        gpui_component_shell::components().unwrap(),
      )
      .unwrap();
      let mut manager = ScriptManager::new(runtime, paths);
      manager.set_plugins(HashMap::from([("a".into(), manifest)]));
      cx.set_global(manager);
    });
    let error = |id: &str, cx: &mut TestAppContext| {
      let call = PluginCall {
        id: id.into(),
        method: "ping".into(),
        args: Value::Null,
      };
      let reply = cx.update(|cx| PluginCall::reply(call, cx));
      futures::executor::block_on(reply).unwrap_err().to_string()
    };
    assert!(error("b", cx).contains("not found"));
    assert!(error("a", cx).contains("no running service"));
  }
}

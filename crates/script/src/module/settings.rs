use corona_config::{Config, ConfigProvider};
use corona_macros::named;
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;
use gpui_shell::HostModule;
use serde_json::{Map, Value};

use crate::{
  host_fn::{Cx, Module},
  module::Subscribe,
  plugin::settings::{Setting, resolve},
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
  Module::new("corona/settings")
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
    .into()
}

fn resolved(id: &str, settings: &[Setting], cx: &App) -> Map<String, Value> {
  resolve(id, settings, cx.config().plugin_settings.get(id))
}

/// Renders the script again when its settings change
fn watch(current: impl Fn(&App) -> Map<String, Value> + 'static) -> Subscribe {
  Box::new(move |runtime, root, cx| {
    let (runtime, root) = (runtime.clone(), root.clone());
    let mut last = current(cx);
    cx.observe_global::<Config>(move |cx| {
      let now = current(cx);
      if now != last {
        last = now;
        runtime.refresh(&root, cx).log_err().ok();
      }
    })
  })
}

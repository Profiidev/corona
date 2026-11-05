//! The `corona/*` modules integrations offer plugins, by name.

use std::{collections::BTreeMap, path::PathBuf};

use gpui_kit::{App, Global};

use crate::{Capabilities, ModuleBuilder, Watches};

/// The plugin a module is built for, and that a runtime runs
#[derive(Clone, Debug, Default)]
pub struct PluginRef {
  pub id: String,
  pub name: String,
  /// Where its sources are; relative imports stay inside
  pub dir: PathBuf,
  /// Where its `localStorage` file is
  pub data_dir: PathBuf,
  pub capabilities: Capabilities,
}

/// A module plugins import as `corona/<NAME>` once their manifest grants `NAME`.
pub trait ScriptModule: 'static {
  /// Imported as `corona/<NAME>`; also the manifest grant key
  const NAME: &'static str;
  /// Shown in the plugin JSON schema
  const DOC: &'static str;
  /// Reaches the user's credentials, notifications or windows; approving a plugin that
  /// imports it warns
  const PRIVILEGED: bool = false;
  fn module(plugin: &PluginRef, m: &mut ModuleBuilder, cx: &mut App);
}

/// A registered [`ScriptModule`]
#[derive(Clone, Copy)]
pub struct Registered {
  pub name: &'static str,
  pub doc: &'static str,
  pub privileged: bool,
  module: fn(&PluginRef, &mut ModuleBuilder, &mut App),
}

impl Registered {
  /// The module for `plugin`, subscribing through `watches`
  pub fn build(self, plugin: &PluginRef, watches: &Watches, cx: &mut App) -> ModuleBuilder {
    let mut m = ModuleBuilder::new(format!("corona/{}", self.name), watches);
    (self.module)(plugin, &mut m, cx);
    m
  }
}

#[derive(Default)]
pub struct ScriptRegistry {
  modules: BTreeMap<&'static str, Registered>,
}

impl Global for ScriptRegistry {}

impl ScriptRegistry {
  pub fn register<M: ScriptModule>(&mut self) -> &mut Self {
    let module = Registered {
      name: M::NAME,
      doc: M::DOC,
      privileged: M::PRIVILEGED,
      module: M::module,
    };
    self.modules.insert(M::NAME, module);
    self
  }

  pub fn get(&self, name: &str) -> Option<Registered> {
    self.modules.get(name).copied()
  }

  /// Every module by name, e.g. for the grants of the plugin schema
  pub fn all(&self) -> impl Iterator<Item = Registered> + '_ {
    self.modules.values().copied()
  }
}

pub trait ScriptExt {
  fn script_mut(&mut self) -> &mut ScriptRegistry;
}

impl ScriptExt for App {
  fn script_mut(&mut self) -> &mut ScriptRegistry {
    self.default_global::<ScriptRegistry>()
  }
}

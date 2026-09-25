use std::{collections::HashMap, fs, path::PathBuf, rc::Rc};

use anyhow::{Context as _, Result};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{AnyView, App, Entity, Global, Subscription, Window};
use gpui_shell::{
  ShellRoot, ShellRuntime, Watcher,
  policy::{self, Policy},
};

use crate::{
  PLUGIN_MANIFEST_FILENAME, PLUGIN_STORAGE_FILENAME,
  manifest::{ManifestFile, PluginManifest},
  module::ModuleExt,
};

pub struct Script {
  root: Entity<ShellRoot>,
  _watcher: Option<Watcher>,
  _subscriptions: Vec<Subscription>,
}

impl Script {
  pub fn view(&self) -> AnyView {
    self.root.clone().into()
  }
}

pub struct ScriptManager {
  runtime: Rc<ShellRuntime>,
  data_dir: PathBuf,
  plugin_dir: PathBuf,
  plugins: HashMap<String, PluginManifest>,
}

impl Global for ScriptManager {}

impl ScriptManager {
  pub fn new(runtime: Rc<ShellRuntime>, data_dir: PathBuf, plugin_dir: PathBuf) -> Self {
    Self {
      runtime,
      data_dir,
      plugin_dir,
      plugins: HashMap::new(),
    }
  }

  pub fn discover(&mut self) {
    self.plugins = fs::read_dir(&self.plugin_dir)
      .into_iter()
      .flatten()
      .flatten()
      .map(|entry| entry.path())
      .filter(|path| path.join(PLUGIN_MANIFEST_FILENAME).is_file())
      .flat_map(|path| {
        let file = fs::File::open(path.join(PLUGIN_MANIFEST_FILENAME))
          .log_err()
          .ok()?;
        let data = serde_json::from_reader::<fs::File, ManifestFile>(file)
          .log_err()
          .ok()?;

        Some((
          data.id.clone(),
          PluginManifest {
            id: data.id,
            name: data.name,
            version: data.version,
            views: data.views,
            capabilities: data.capabilities.grant(&self.plugin_dir, &self.data_dir),
          },
        ))
      })
      .collect();
  }

  pub fn load(id: &str, view: &str, window: &mut Window, cx: &mut App) -> Result<Script> {
    let manager = cx.global::<ScriptManager>();
    let manifest = manager
      .plugins
      .get(id)
      .with_context(|| format!("Plugin `{id}` not found"))?;
    let id = manifest.id.clone();
    let view = manifest
      .views
      .get(view)
      .with_context(|| format!("script `{id}` has no view `{view}`"))?;

    let data_dir = manager.data_dir.join(&id);
    if let Err(error) = fs::create_dir_all(&data_dir) {
      tracing::warn!("storage unavailable for `{id}`: {error}");
    }

    let runtime = manager.runtime.clone();
    let root = manager.plugin_dir.join(&id).join(view);

    let (policy, subscribes) = Policy::new()
      .with_application(&id)
      .with_capabilities(manifest.capabilities.clone())
      .with_storage_path(data_dir.join(PLUGIN_STORAGE_FILENAME))
      .with_corona_modules(cx)?;

    // The one seam that carries a policy into a view from outside the crate.
    // Reset afterwards so a later load cannot inherit this script's grant.
    policy::set_default(policy);
    let root = runtime.load(root, window, cx);
    policy::set_default(Policy::new());

    let subscriptions = subscribes
      .into_iter()
      .map(|subscription| subscription(&runtime, &root, cx))
      .collect();

    let watcher = match runtime.watch(&root, window, cx) {
      Ok(watcher) => Some(watcher),
      Err(error) => {
        tracing::debug!("`{id}` not watched: {error}");
        None
      }
    };

    Ok(Script {
      root,
      _watcher: watcher,
      _subscriptions: subscriptions,
    })
  }
}

//! The root source of every plugin's `[commands]`

use std::{
  cell::RefCell,
  collections::{BTreeMap, HashMap},
  rc::Rc,
};

use corona_launcher::{
  Command, CommandSource, Icon, Meta, Query, Section, icon_by_name,
  kit::Notify,
  registry::{self, Registry},
};
use gpui_kit::{App, Global, SharedString, Subscription};

use super::{Port, view::PluginCommand};
use crate::plugin::manifest::CommandFile;

#[derive(Default)]
struct Shared {
  commands: RefCell<HashMap<String, Vec<Rc<dyn Command>>>>,
  notifiers: RefCell<Vec<(u32, Notify)>>,
  next: RefCell<u32>,
}

/// The commands of all running plugins; one source in the launcher's registry, as it
/// cannot drop sources
#[derive(Clone, Default)]
pub struct PluginSource(Rc<Shared>);

impl Global for PluginSource {}

impl CommandSource for PluginSource {
  fn id(&self) -> SharedString {
    "plugins".into()
  }

  fn commands(&self, _: &Query, _: &App) -> Vec<Rc<dyn Command>> {
    let mut all: Vec<_> = self
      .0
      .commands
      .borrow()
      .iter()
      .map(|(id, c)| (id.clone(), c.clone()))
      .collect();
    all.sort_by(|a, b| a.0.cmp(&b.0));
    all.into_iter().flat_map(|(_, c)| c).collect()
  }

  fn observe(&self, notify: Notify, _: &mut App) -> Vec<Subscription> {
    let shared = self.0.clone();
    let id = {
      let mut next = shared.next.borrow_mut();
      *next += 1;
      *next
    };
    shared.notifiers.borrow_mut().push((id, notify));
    vec![Subscription::new(move || {
      shared.notifiers.borrow_mut().retain(|(i, _)| *i != id)
    })]
  }
}

/// Registers the source, when there is a launcher
pub fn init(cx: &mut App) {
  if !cx.has_global::<Registry>() {
    return;
  }
  let source = PluginSource::default();
  registry::register_source(Rc::new(source.clone()), cx);
  cx.set_global(source);
}

/// Sets the commands of plugin `plugin` to `commands`
pub fn set_commands(
  plugin: &str,
  commands: &BTreeMap<String, CommandFile>,
  port: &Port,
  cx: &mut App,
) {
  let Some(source) = cx.try_global::<PluginSource>().cloned() else {
    return;
  };
  let built = commands
    .iter()
    .map(|(name, file)| {
      let meta = Meta {
        title: file.title.clone().into(),
        subtitle: file.subtitle.clone().map(Into::into),
        icon: (file.icon.as_deref().and_then(icon_by_name)).map_or(Icon::None, Icon::Named),
        keywords: file.keywords.iter().map(|k| k.clone().into()).collect(),
        section: (file.section.as_deref().and_then(Section::from_name)).unwrap_or_default(),
        args: file.arg_specs(),
        ..Meta::default()
      };
      Rc::new(PluginCommand {
        id: format!("{plugin}:{name}").into(),
        name: name.clone(),
        scope: file.scope.as_ref().map(|s| {
          (
            s.prefix.clone(),
            s.label.clone().unwrap_or_else(|| file.title.clone()),
            s.placeholder.clone().unwrap_or_default(),
          )
        }),
        meta,
        port: port.clone(),
      }) as Rc<dyn Command>
    })
    .collect();
  source.0.commands.borrow_mut().insert(plugin.into(), built);
  changed(&source, cx);
}

/// Drops the commands of plugin `plugin`
pub fn remove(plugin: &str, cx: &mut App) {
  let Some(source) = cx.try_global::<PluginSource>().cloned() else {
    return;
  };
  if source.0.commands.borrow_mut().remove(plugin).is_some() {
    changed(&source, cx);
  }
}

fn changed(source: &PluginSource, cx: &mut App) {
  registry::invalidate_scopes(cx);
  let notifiers: Vec<Notify> = source
    .0
    .notifiers
    .borrow()
    .iter()
    .map(|(_, n)| n.clone())
    .collect();
  for notify in notifiers {
    notify(cx);
  }
}

#[cfg(test)]
mod tests {
  use corona_config::Config;
  use corona_launcher::registry::Registry;
  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;

  fn commands() -> BTreeMap<String, CommandFile> {
    let toml = r#"
      title = "Recent Projects"
      icon = "folder"
      scope = { prefix = "rp" }
    "#;
    [("recent".to_string(), toml::from_str(toml).unwrap())].into()
  }

  fn ids(cx: &App) -> Vec<String> {
    let all = cx.global::<Registry>().all(cx);
    all.iter().map(|c| c.id().to_string()).collect()
  }

  #[gpui::test]
  fn plugin_commands_come_and_go(cx: &mut TestAppContext) {
    cx.update(|cx| {
      gpui_kit::init(cx);
      cx.set_global(Config::default());
      corona_launcher::init(cx);
      init(cx);
      assert!(ids(cx).is_empty());

      set_commands("p", &commands(), &Port::new("p"), cx);
      assert_eq!(ids(cx), ["p:recent"]);
      let command = cx.global::<Registry>().find("p:recent", cx).unwrap();
      assert_eq!(command.meta(cx).title, "Recent Projects");
      let scopes = cx.global::<Registry>().scopes(cx);
      assert_eq!(scopes[0].prefix, "rp");

      // a plugin that changed has its commands replaced
      set_commands("p", &BTreeMap::new(), &Port::new("p"), cx);
      assert!(ids(cx).is_empty() && cx.global::<Registry>().scopes(cx).is_empty());

      set_commands("p", &commands(), &Port::new("p"), cx);
      remove("p", cx);
      assert!(ids(cx).is_empty());
    });
  }
}

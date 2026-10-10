use std::{
  cell::RefCell,
  collections::{BTreeSet, HashSet},
  rc::Rc,
};

use anyhow::Result;
use corona_utils::error::ErrorLogExt;
use gpui_kit::{App, Entity, Subscription};
use gpui_shell::{Capabilities, HostModule, ShellRoot, ShellRuntime, policy::Policy};
use schemars::JsonSchema;
use serde::Deserialize;

use corona_macros::named;

use crate::host_fn::{Cx, HostReturn, Named};

pub mod auth;
pub mod bluetooth;
pub mod brightness;
pub mod compositor;
pub mod dbus;
pub mod desktop;
pub mod i18n;
pub mod mpris;
pub mod network;
pub mod notifications;
pub mod pipewire;
pub mod plugin;
pub mod power;
pub mod secrets;
pub mod settings;
pub mod surface;
pub mod sysinfo;
pub mod tray;
pub mod weather;

/// A corona module a plugin may import, as `corona/<name>`. Plugins list the
/// ones they use under `capabilities.corona`; the others are not there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CoronaModule {
  /// Workspaces, monitors and windows.
  Compositor,
  /// Audio devices, streams and who records.
  Pipewire,
  /// Network interfaces, Wi-Fi and VPNs.
  Network,
  /// Media players.
  Mpris,
  Bluetooth,
  /// Battery, power profiles and the session (suspend, reboot, …).
  Power,
  /// Screen brightness.
  Brightness,
  /// Sending notifications, and acting on the plugin's own.
  Notifications,
  /// Every app's notifications, do not disturb, and acting on them.
  NotificationCenter,
  /// System tray items.
  Tray,
  /// System information and usage.
  Sysinfo,
  Weather,
  Auth,
  /// Strings in the user's keyring, e.g. tokens.
  Secrets,
  /// The file picker and opening files and URIs in the user's apps.
  Desktop,
}

/// The plugin a module is built for.
#[derive(Clone, Copy)]
pub struct PluginRef<'a> {
  pub id: &'a str,
  pub name: &'a str,
  /// The grant its policy holds, shared by clones: `pickFiles` adds to it
  pub capabilities: &'a Capabilities,
}

impl CoronaModule {
  fn module(
    self,
    plugin: PluginRef,
    reads: &Subscriptions,
    subs: &mut Vec<Subscribe>,
    cx: &mut App,
  ) -> HostModule {
    match self {
      Self::Compositor => compositor::module(reads, subs, cx),
      Self::Pipewire => pipewire::module(reads, subs, cx),
      Self::Network => network::module(reads, subs, cx),
      Self::Mpris => mpris::module(reads, subs, cx),
      Self::Bluetooth => bluetooth::module(reads, subs, cx),
      Self::Power => power::module(reads, subs, cx),
      Self::Brightness => brightness::module(reads, subs, cx),
      Self::Notifications => notifications::module(plugin, subs, cx),
      Self::NotificationCenter => notifications::center(reads, subs, cx),
      Self::Tray => tray::module(reads, subs, cx),
      Self::Sysinfo => sysinfo::module(reads, subs, cx),
      Self::Weather => weather::module(reads, subs, cx),
      Self::Auth => auth::module(),
      Self::Secrets => secrets::module(plugin),
      Self::Desktop => desktop::module(plugin.capabilities.clone(), subs),
    }
  }
}

#[derive(Clone, Default)]
pub struct Subscriptions(Rc<RefCell<HashSet<Updates>>>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Bluetooth(bluetooth::Updates),
  Brightness(brightness::Updates),
  Compositor(compositor::Updates),
  Mpris(mpris::Updates),
  Network(network::Updates),
  Notifications(notifications::Updates),
  Pipewire(pipewire::Updates),
  /// `corona/plugin`'s shared state
  Plugin,
  Power(power::Updates),
  Surface(surface::Updates),
  Sysinfo(sysinfo::Updates),
  Tray(tray::Updates),
  Weather(weather::Updates),
}

type Refresh = Box<dyn FnOnce(&Rc<ShellRuntime>, &Entity<ShellRoot>, &mut App) -> Subscription>;

/// What a module keeps while its script runs
pub enum Subscribe {
  /// Renders the view again on a change; a service has nothing to render
  Refresh(Refresh),
  /// Cleans up after the script, view or service
  Cleanup(Subscription),
}

impl Subscribe {
  pub fn subscribe(
    self,
    runtime: &Rc<ShellRuntime>,
    root: &Entity<ShellRoot>,
    cx: &mut App,
  ) -> Subscription {
    match self {
      Self::Refresh(refresh) => refresh(runtime, root, cx),
      Self::Cleanup(cleanup) => cleanup,
    }
  }

  /// What a service keeps
  pub fn cleanup(self) -> Option<Subscription> {
    match self {
      Self::Refresh(_) => None,
      Self::Cleanup(cleanup) => Some(cleanup),
    }
  }
}

fn watch<T: 'static>(reads: &Subscriptions, update: Updates, entity: Entity<T>) -> Subscribe {
  let reads = reads.clone();

  Subscribe::Refresh(Box::new(move |runtime, root, cx| {
    let (runtime, root) = (runtime.clone(), root.clone());
    cx.observe(&entity, move |_, cx| {
      if reads.contains(update) {
        runtime.refresh(&root, cx).log_err().ok();
      }
    })
  }))
}

/// A read that re-renders the script when `entity` changes, if the script called it.
fn read<W: 'static, R: HostReturn<M>, M: 'static>(
  reads: &Subscriptions,
  subs: &mut Vec<Subscribe>,
  name: &'static str,
  update: impl Into<Updates>,
  entity: Entity<W>,
  read: impl Fn(&App) -> R + 'static,
) -> Named<impl Fn(Cx) -> R + 'static> {
  let update = update.into();
  subs.push(watch(reads, update, entity));
  let reads = reads.clone();

  named!(name, move |cx: Cx| {
    reads.record(update);
    read(&cx)
  })
}

impl Subscriptions {
  pub fn record(&self, update: Updates) {
    self.0.borrow_mut().insert(update);
  }

  pub fn contains(&self, update: Updates) -> bool {
    self.0.borrow().contains(&update)
  }
}

pub trait ModuleExt: Sized {
  /// Adds the `granted` corona modules; importing any other fails.
  fn with_corona_modules(
    self,
    plugin: PluginRef,
    granted: &BTreeSet<CoronaModule>,
    cx: &mut App,
  ) -> Result<(Self, Vec<Subscribe>)>;
}

impl ModuleExt for Policy {
  fn with_corona_modules(
    self,
    plugin: PluginRef,
    granted: &BTreeSet<CoronaModule>,
    cx: &mut App,
  ) -> Result<(Self, Vec<Subscribe>)> {
    let reads = Subscriptions::default();
    let mut subs = Vec::new();

    let mut policy = self;
    for module in granted {
      policy = policy.with_host_module(module.module(plugin, &reads, &mut subs, cx))?;
    }
    Ok((policy, subs))
  }
}

/// Runs a module in a real script view, for tests of what plugins see.
#[cfg(test)]
pub(crate) mod harness {
  use std::{cell::RefCell, fs, rc::Rc};

  use gpui_kit::{
    AnyView, App, Context, Entity, IntoElement, ParentElement as _, Render, Subscription,
    TestAppContext, VisualTestContext, Window, div,
  };
  use gpui_shell::{
    HostModule, ShellRoot, ShellRuntime,
    policy::{self, Policy},
  };
  use serde_json::Value;

  use super::{Subscribe, Subscriptions};
  use crate::host_fn::Module;
  use corona_macros::named;

  /// Shows the script, so it renders
  struct Host(Option<AnyView>);

  impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
      div().children(self.0.clone())
    }
  }

  pub(crate) struct View {
    /// What `report(value)` was called with, in order
    pub reports: Rc<RefCell<Vec<Value>>>,
    pub reads: Subscriptions,
    _root: Entity<ShellRoot>,
    _subs: Vec<Subscription>,
    _dir: tempfile::TempDir,
  }

  impl View {
    pub fn last(&self) -> Value {
      let reports = self.reports.borrow();
      reports.last().cloned().expect("nothing reported")
    }
  }

  /// A view running `body` on every render, with the module `module` builds
  /// imported as `m` and `report` from `corona/test`.
  pub(crate) fn view<'a>(
    cx: &'a mut TestAppContext,
    body: &str,
    module: impl FnOnce(&Subscriptions, &mut Vec<Subscribe>, &mut App) -> HostModule,
  ) -> (View, &'a mut VisualTestContext) {
    let reports: Rc<RefCell<Vec<Value>>> = Rc::default();
    let report = reports.clone();
    let test: HostModule = Module::new("corona/test")
      .func(named!("report", move |value: Value| {
        report.borrow_mut().push(value)
      }))
      .into();
    let (reads, mut subs) = (Subscriptions::default(), Vec::new());
    let module = cx.update(|cx| module(&reads, &mut subs, cx));

    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("main.js");
    let source = format!(
      r#"
import {{ View }} from "gpui-kit";
import {{ v_flex }} from "gpui-base";
import {{ report }} from "corona/test";
import * as m from "{}";

export default class Main extends View {{
  render(_cx) {{
    {body}
    return v_flex().child("test");
  }}
}}
"#,
      module.name()
    );
    fs::write(&main, source).unwrap();
    let runtime =
      ShellRuntime::new_isolated_with_components(gpui_component_shell::components().unwrap())
        .unwrap();
    let policy = Policy::new()
      .with_host_module(test)
      .unwrap()
      .with_host_module(module)
      .unwrap();

    let (host, cx) = cx.add_window_view(|_, _| Host(None));
    let (root, subs) = cx.update(|window, cx| {
      policy::set_default(policy);
      let root = runtime.try_load_entry(&main, window, cx);
      policy::set_default(Policy::new());
      let root = root.unwrap();
      let subs = subs
        .into_iter()
        .map(|s| s.subscribe(&runtime, &root, cx))
        .collect();
      host.update(cx, |host, cx| {
        host.0 = Some(root.clone().into());
        cx.notify();
      });
      (root, subs)
    });
    cx.run_until_parked();
    let view = View {
      reports,
      reads,
      _root: root,
      _subs: subs,
      _dir: dir,
    };
    (view, cx)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn module_names() {
    let all = [
      (CoronaModule::Compositor, "compositor"),
      (CoronaModule::Pipewire, "pipewire"),
      (CoronaModule::Network, "network"),
      (CoronaModule::Mpris, "mpris"),
      (CoronaModule::Bluetooth, "bluetooth"),
      (CoronaModule::Power, "power"),
      (CoronaModule::Brightness, "brightness"),
      (CoronaModule::Notifications, "notifications"),
      (CoronaModule::NotificationCenter, "notification_center"),
      (CoronaModule::Tray, "tray"),
      (CoronaModule::Sysinfo, "sysinfo"),
      (CoronaModule::Weather, "weather"),
      (CoronaModule::Auth, "auth"),
      (CoronaModule::Secrets, "secrets"),
      (CoronaModule::Desktop, "desktop"),
    ];
    for (module, name) in all {
      assert_eq!(
        serde_json::from_value::<CoronaModule>(name.into()).unwrap(),
        module
      );
    }
    assert!(serde_json::from_value::<CoronaModule>("Weather".into()).is_err());
  }

  #[test]
  fn subscriptions_are_shared_between_clones() {
    let reads = Subscriptions::default();
    let clone = reads.clone();
    let update = Updates::Tray(tray::Updates::Items);
    assert!(!reads.contains(update));

    clone.record(update);
    clone.record(update);
    assert!(reads.contains(update));
    assert!(!reads.contains(Updates::Weather(weather::Updates::Error)));

    // a fresh set does not see it
    assert!(!Subscriptions::default().contains(update));
  }
}

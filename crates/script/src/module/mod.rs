use std::{
  cell::RefCell,
  collections::{BTreeSet, HashSet},
  rc::Rc,
};

use anyhow::Result;
use corona_utils::error::ErrorLogExt;
use gpui_kit::{App, Entity, Subscription};
use gpui_shell::{HostModule, ShellRoot, ShellRuntime, policy::Policy};
use schemars::JsonSchema;
use serde::Deserialize;

use corona_macros::named;

use crate::host_fn::{Cx, HostReturn, Named};

pub mod bluetooth;
pub mod brightness;
pub mod compositor;
pub mod mpris;
pub mod network;
pub mod notifications;
pub mod pipewire;
pub mod power;
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
  Notifications,
  /// System tray items.
  Tray,
  /// System information and usage.
  Sysinfo,
  Weather,
}

impl CoronaModule {
  fn module(self, reads: &Subscriptions, subs: &mut Vec<Subscribe>, cx: &mut App) -> HostModule {
    match self {
      Self::Compositor => compositor::module(reads, subs, cx),
      Self::Pipewire => pipewire::module(reads, subs, cx),
      Self::Network => network::module(reads, subs, cx),
      Self::Mpris => mpris::module(reads, subs, cx),
      Self::Bluetooth => bluetooth::module(reads, subs, cx),
      Self::Power => power::module(reads, subs, cx),
      Self::Brightness => brightness::module(reads, subs, cx),
      Self::Notifications => notifications::module(reads, subs, cx),
      Self::Tray => tray::module(reads, subs, cx),
      Self::Sysinfo => sysinfo::module(reads, subs, cx),
      Self::Weather => weather::module(reads, subs, cx),
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
  Power(power::Updates),
  Sysinfo(sysinfo::Updates),
  Tray(tray::Updates),
  Weather(weather::Updates),
}

type Subscribe = Box<dyn FnOnce(&Rc<ShellRuntime>, &Entity<ShellRoot>, &mut App) -> Subscription>;

fn watch<T: 'static>(reads: &Subscriptions, update: Updates, entity: Entity<T>) -> Subscribe {
  let reads = reads.clone();

  Box::new(move |runtime, root, cx| {
    let (runtime, root) = (runtime.clone(), root.clone());
    cx.observe(&entity, move |_, cx| {
      if reads.contains(update) {
        runtime.refresh(&root, cx).log_err().ok();
      }
    })
  })
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
    granted: &BTreeSet<CoronaModule>,
    cx: &mut App,
  ) -> Result<(Self, Vec<Subscribe>)>;
}

impl ModuleExt for Policy {
  fn with_corona_modules(
    self,
    granted: &BTreeSet<CoronaModule>,
    cx: &mut App,
  ) -> Result<(Self, Vec<Subscribe>)> {
    let reads = Subscriptions::default();
    let mut subs = Vec::new();

    let mut policy = self;
    for module in granted {
      policy = policy.with_host_module(module.module(&reads, &mut subs, cx))?;
    }
    Ok((policy, subs))
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
      (CoronaModule::Tray, "tray"),
      (CoronaModule::Sysinfo, "sysinfo"),
      (CoronaModule::Weather, "weather"),
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

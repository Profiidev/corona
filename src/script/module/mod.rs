use std::{cell::RefCell, collections::HashSet, rc::Rc};

use anyhow::Result;
use gpui_kit::{App, Entity, Subscription};
use gpui_shell::{ShellRoot, ShellRuntime, policy::Policy};

use corona_macros::named;

use crate::{
  error::ErrorLogExt,
  script::host_fn::{Cx, HostReturn, Named},
};

pub mod compositor;
pub mod pipewire;

#[derive(Clone, Default)]
pub struct Subscriptions(Rc<RefCell<HashSet<Updates>>>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Compositor(compositor::Updates),
  Pipewire(pipewire::Updates),
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
  fn with_corona_modules(self, cx: &mut App) -> Result<(Self, Vec<Subscribe>)>;
}

impl ModuleExt for Policy {
  fn with_corona_modules(self, cx: &mut App) -> Result<(Self, Vec<Subscribe>)> {
    let reads = Subscriptions::default();
    let mut subs = Vec::new();

    let policy = self
      .with_host_module(compositor::module(&reads, &mut subs, cx))?
      .with_host_module(pipewire::module(&reads, &mut subs, cx))?;

    Ok((policy, subs))
  }
}

//! Fixed resource limits of a plugin runtime. A plugin that hits one is recorded in
//! [`take_tripped`], and [`crate::watch`] restarts it.

use std::{
  cell::{Cell, RefCell},
  rc::Rc,
  time::{Duration, Instant},
};

use gpui_kit::{App, Global};
use rquickjs::AsyncRuntime;

use crate::now;

/// QuickJS heap of one plugin. Buffers of the stdlib live outside it.
const MEMORY: usize = 64 << 20;
const STACK: usize = 1 << 20;
/// One call into JS (init, render, event, driving futures) may run this long. Tests run
/// debug builds with heavy probes, see [`set_budget`].
const BUDGET: Duration = Duration::from_secs(if cfg!(test) { 30 } else { 1 });

/// Which limit a plugin hit
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Trip {
  Time,
  Memory,
}

thread_local! {
  static BUDGET_NOW: Cell<Duration> = const { Cell::new(BUDGET) };
  static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
  static TRIPPED: RefCell<Vec<(String, Trip)>> = const { RefCell::new(Vec::new()) };
}

/// Called with the id of a plugin that hit a limit, when [`crate::watch`] restarts it
pub struct TripHook(pub Rc<TripFn>);

pub type TripFn = dyn Fn(&str, Trip, &mut App);

impl Global for TripHook {}

/// Starts the budget of a call into JS; an outer call keeps its own. Returns the deadline
/// to restore.
pub(crate) fn start() -> Option<Instant> {
  DEADLINE.replace(DEADLINE.get().or(Some(Instant::now() + BUDGET_NOW.get())))
}

pub(crate) fn restore(outer: Option<Instant>) {
  DEADLINE.set(outer);
}

pub(crate) fn trip(id: &str, trip: Trip) {
  TRIPPED.with_borrow_mut(|t| {
    if !t.iter().any(|(i, _)| i == id) {
      t.push((id.to_string(), trip));
    }
  });
}

/// The limit plugin `id` hit since the last call
pub fn take_tripped(id: &str) -> Option<Trip> {
  TRIPPED.with_borrow_mut(|t| Some(t.remove(t.iter().position(|(i, _)| i == id)?).1))
}

pub(crate) fn apply(rt: &AsyncRuntime, id: String) {
  now(rt.set_memory_limit(MEMORY));
  now(rt.set_max_stack_size(STACK));
  now(rt.set_interrupt_handler(Some(Box::new(move || {
    let over = DEADLINE.get().is_some_and(|d| Instant::now() > d);
    if over {
      trip(&id, Trip::Time);
    }
    over
  }))));
}

/// Records a memory trip when the heap is near its limit. ponytail: checked after each
/// drive poll only, a view that fills the heap between polls is noticed at the next one
pub(crate) fn check_memory(rt: &AsyncRuntime, id: &str) {
  if now(rt.memory_usage()).malloc_size as usize > MEMORY / 10 * 9 {
    trip(id, Trip::Memory);
  }
}

#[cfg(test)]
pub(crate) fn set_budget(budget: Duration) {
  BUDGET_NOW.set(budget);
}

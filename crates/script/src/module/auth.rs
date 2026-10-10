use std::{
  collections::BTreeMap,
  sync::{Mutex, PoisonError},
  time::{Duration, Instant},
};

use anyhow::{Result, bail, ensure};
use corona_auth::Auth;
use gpui_shell::HostModule;

use crate::host_fn::{Cx, Glob, Module};
use corona_macros::named;

/// Only ever the user running corona, a plugin cannot probe other accounts
fn user() -> String {
  std::env::var("USER").unwrap_or_default()
}

/// Failed attempts a plugin gets before it has to wait
const FREE_FAILURES: u32 = 3;
const FIRST_WAIT: Duration = Duration::from_secs(5);
const MAX_WAIT: Duration = Duration::from_secs(300);

/// A plugin's failed attempts, so it cannot hold the auth lock the lock screen
/// shares by guessing in a loop.
#[derive(Default)]
struct Backoff {
  failures: u32,
  until: Option<Instant>,
  /// One attempt at a time
  pending: bool,
}

// ponytail: in memory by plugin id, kept across reloads but not restarts;
// persist it if plugins can restart corona
static BACKOFF: Mutex<BTreeMap<String, Backoff>> = Mutex::new(BTreeMap::new());

impl Backoff {
  fn start(&mut self, now: Instant) -> Result<()> {
    ensure!(!self.pending, "another attempt is still running");
    if let Some(until) = self.until.filter(|until| now < *until) {
      bail!(
        "too many failed attempts, try again in {}s",
        (until - now).as_secs() + 1
      );
    }
    self.pending = true;
    Ok(())
  }

  fn finish(&mut self, ok: bool, now: Instant) {
    if ok {
      *self = Self::default();
      return;
    }
    self.pending = false;
    self.failures += 1;
    if let Some(over) = self.failures.checked_sub(FREE_FAILURES) {
      let wait = FIRST_WAIT.saturating_mul(1 << over.min(16)).min(MAX_WAIT);
      self.until = Some(now + wait);
    }
  }
}

fn backoff<R>(plugin: &str, f: impl FnOnce(&mut Backoff) -> R) -> R {
  let mut all = BACKOFF.lock().unwrap_or_else(PoisonError::into_inner);
  f(all.entry(plugin.to_owned()).or_default())
}

/// A started attempt; dropping it unfinished, as an unload does, frees the slot.
struct Attempt(String);

impl Attempt {
  /// Fails if the plugin has to wait
  fn start(plugin: &str) -> Result<Self> {
    backoff(plugin, |b| b.start(Instant::now()))?;
    Ok(Self(plugin.to_owned()))
  }
}

impl Drop for Attempt {
  fn drop(&mut self) {
    backoff(&self.0, |b| b.pending = false);
  }
}

/// Runs a started attempt; anything but a match counts as a failure.
async fn limited<F: Future<Output = Result<bool>>>(started: Result<(Attempt, F)>) -> Result<bool> {
  let (attempt, run) = started?;
  let result = run.await;
  let ok = matches!(result, Ok(true));
  backoff(&attempt.0, |b| b.finish(ok, Instant::now()));
  result
}

pub fn module(plugin: &str) -> HostModule {
  let (by, scanned_by) = (plugin.to_owned(), plugin.to_owned());
  Module::new("corona/auth")
    .func(named!(
      "password",
      /// Whether `password` is the user's, checked by PAM. After a few
      /// failures, attempts fail at once for a while.
      move |cx: Cx, auth: Glob<Auth>, password: String| limited(
        Attempt::start(&by).map(|a| (a, auth.password(user(), password, &cx)))
      )
    ))
    .func(named!(
      "fingerprint",
      /// Waits for one scan on the fingerprint reader. `false` when the finger
      /// did not match, call again for the next try. Limited as `password`.
      move |auth: Glob<Auth>| limited(
        Attempt::start(&scanned_by).map(|a| (a, auth.fingerprint(user())))
      )
    ))
    .into()
}

#[cfg(test)]
mod tests {
  use corona_utils::test_bus::{TestBus, wait_until};
  use futures_lite::future::block_on;
  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;
  use crate::module::harness;

  #[test]
  fn the_user_is_not_the_plugins_to_pick() {
    let module: HostModule = module("a");
    let declared = module.declared().unwrap();
    for line in [
      "export function password(password: string): Promise<boolean | Error>;",
      "export function fingerprint(): Promise<boolean | Error>;",
    ] {
      assert!(declared.lines().any(|l| l == line), "{line} in\n{declared}");
    }
  }

  #[test]
  fn failures_back_off() {
    let now = Instant::now();
    let mut b = Backoff::default();
    for _ in 0..FREE_FAILURES {
      b.start(now).unwrap();
      assert!(b.start(now).is_err(), "one at a time");
      b.finish(false, now);
    }
    assert!(b.start(now).is_err());
    let later = now + FIRST_WAIT;
    b.start(later).unwrap();
    b.finish(false, later);
    // doubles, up to the cap
    assert!(b.start(later + FIRST_WAIT).is_err());
    b.start(later + FIRST_WAIT * 2).unwrap();
    b.failures = 40;
    b.finish(false, now);
    assert_eq!(b.until, Some(now + MAX_WAIT));
    b.finish(true, now);
    b.start(now).unwrap();
  }

  #[gpui::test]
  fn a_failed_scan_is_an_error_value(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    // no fprintd on this bus
    let bus = TestBus::new();
    let conn = block_on(bus.conn());
    cx.update(|cx| corona_auth::init(cx, &conn));
    let body =
      "if (!globalThis.started) { globalThis.started = true; m.fingerprint().then(report); }";
    let (view, cx) = harness::view(cx, body, |_, _, _| module("scan"));
    wait_until(cx, |_| !view.reports.borrow().is_empty());
    let message = view.last()["message"].as_str().unwrap().to_string();
    assert!(message.contains("Fprint"), "{message}");
  }
}

use corona_auth::Auth;
use gpui_shell::HostModule;

use crate::host_fn::{Cx, Glob, Module};
use corona_macros::named;

/// Only ever the user running corona, a plugin cannot probe other accounts
fn user() -> String {
  std::env::var("USER").unwrap_or_default()
}

pub fn module() -> HostModule {
  Module::new("corona/auth")
    .func(named!(
      "password",
      /// Whether `password` is the user's, checked by PAM.
      |cx: Cx, auth: Glob<Auth>, password: String| auth.password(user(), password, &cx)
    ))
    .func(named!(
      "fingerprint",
      /// Waits for one scan on the fingerprint reader. `false` when the finger
      /// did not match, call again for the next try.
      |auth: Glob<Auth>| auth.fingerprint(user())
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
    let module: HostModule = module();
    let declared = module.declared().unwrap();
    for line in [
      "export function password(password: string): Promise<boolean | Error>;",
      "export function fingerprint(): Promise<boolean | Error>;",
    ] {
      assert!(declared.lines().any(|l| l == line), "{line} in\n{declared}");
    }
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
    let (view, cx) = harness::view(cx, body, |_, _, _| module());
    wait_until(cx, |_| !view.reports.borrow().is_empty());
    let message = view.last()["message"].as_str().unwrap().to_string();
    assert!(message.contains("Fprint"), "{message}");
  }
}

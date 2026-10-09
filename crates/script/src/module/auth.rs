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

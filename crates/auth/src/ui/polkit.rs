use std::rc::Rc;

use crate::polkit::Polkit;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Context, IntoElement, Render, SharedString, Window,
};
use rust_i18n::t;

use corona_surface::overlay::{
  open_on_focused,
  password::{PasswordCard, PasswordForm, follow_queue},
};

const NAMESPACE: &str = "corona_polkit";

/// Opens a dialog on the focused monitor for the oldest polkit request, a
/// fresh one for each request
pub(crate) fn init(cx: &mut App) {
  let requests = cx.global::<Polkit>().requests.clone();
  follow_queue(requests, "polkit", |r| r.cookie.clone(), open_dialog, cx);
}

fn open_dialog(cx: &mut App) -> anyhow::Result<AnyWindowHandle> {
  open_on_focused(
    NAMESPACE,
    |_| {},
    cx,
    |window, cx| cx.new(|cx| PolkitDialog::new(window, cx)),
  )
}

struct PolkitDialog {
  form: PasswordForm,
  error: Option<SharedString>,
  checking: bool,
}

impl PolkitDialog {
  fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    Self {
      form: PasswordForm::new(window, cx, Self::submit),
      error: None,
      checking: false,
    }
  }

  fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if self.checking {
      return;
    }
    self.checking = true;
    let password = self.form.input.read(cx).value().to_string();
    let check = cx.global::<Polkit>().authenticate(password, cx);
    cx.spawn_in(window, async move |this, cx| {
      let result = check.await;
      // a right password closes the window, so nothing is left to update
      let _ = this.update_in(cx, |this, window, cx| {
        this.checking = false;
        this.error = match result {
          Ok(_) => Some(t!("app.lock.wrong").into()),
          Err(e) => {
            tracing::warn!("polkit: password check failed: {e:#}");
            Some(t!("app.lock.auth_failed").into())
          }
        };
        this.form.input.update(cx, |input, cx| {
          input.set_value("", window, cx);
          input.focus(window, cx);
        });
        cx.notify();
      });
    })
    .detach();
    cx.notify();
  }
}

impl Render for PolkitDialog {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let request = cx.global::<Polkit>().request(cx);
    let message = request.map(|r| r.message.clone()).unwrap_or_default();
    let user = request.map(|r| r.user.clone()).unwrap_or_default();
    PasswordCard {
      id: "polkit",
      title: t!("app.polkit.title").into(),
      message: message.into(),
      detail: Some(t!("app.polkit.as_user", user = user).into()),
      form: &self.form,
      error: self.error.clone(),
      busy: self.checking,
      submit_label: t!("app.polkit.authenticate").into(),
      on_cancel: Rc::new(|_, cx| cx.global::<Polkit>().clone().cancel(cx)),
      on_submit: Box::new(cx.listener(|this, _, window, cx| this.submit(window, cx))),
    }
    .render(cx)
  }
}

#[cfg(test)]
mod tests {
  use std::os::unix::fs::PermissionsExt;

  use gpui_kit::{self as gpui, Entity, TestAppContext, VisualTestContext};
  use tempfile::TempDir;

  use super::*;
  use corona_surface::testing::{FakeCompositor, setup};

  /// A Polkit whose helper takes the password `right`
  fn polkit(cx: &mut TestAppContext) -> (Polkit, TempDir) {
    setup(FakeCompositor::default(), cx);
    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("helper");
    let script = "#!/bin/sh\nread cookie\necho 'PAM_PROMPT_ECHO_OFF Password: '\nread password\n\
                  [ \"$password\" = right ] && echo SUCCESS || echo FAILURE\n";
    std::fs::write(&helper, script).unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let polkit = cx.update(|cx| {
      let polkit = Polkit::fake(helper, cx);
      cx.set_global(polkit.clone());
      polkit
    });
    (polkit, dir)
  }

  #[gpui::test]
  fn a_window_per_request(cx: &mut TestAppContext) {
    let (polkit, _dir) = polkit(cx);
    cx.update(init);
    let mut first = cx.update(|cx| polkit.ask("Do it", "c1", cx));
    cx.run_until_parked();
    let shown = cx.windows();
    assert_eq!(shown.len(), 1);

    // the next one waits for the first
    let mut second = cx.update(|cx| polkit.ask("Do that", "c2", cx));
    cx.run_until_parked();
    assert_eq!(cx.windows(), shown);

    // and gets a fresh window
    cx.update(|cx| polkit.cancel(cx));
    cx.run_until_parked();
    assert!(first.try_recv().is_err());
    let next = cx.windows();
    assert_eq!(next.len(), 1);
    assert_ne!(next, shown);

    cx.update(|cx| polkit.cancel(cx));
    cx.run_until_parked();
    assert!(second.try_recv().is_err());
    assert!(cx.windows().is_empty());
  }

  fn enter(dialog: &Entity<PolkitDialog>, password: &str, cx: &mut VisualTestContext) {
    dialog.update_in(cx, |dialog, window, cx| {
      dialog
        .form
        .input
        .update(cx, |input, cx| input.set_value(password, window, cx));
      dialog.submit(window, cx);
    });
    cx.run_until_parked();
  }

  #[gpui::test]
  fn passwords(cx: &mut TestAppContext) {
    let (polkit, _dir) = polkit(cx);
    let mut asked = cx.update(|cx| polkit.ask("Do it", "c1", cx));
    let (dialog, cx) = cx.add_window_view(PolkitDialog::new);

    enter(&dialog, "wrong", cx);
    dialog.read_with(cx, |dialog, cx| {
      assert_eq!(dialog.error, Some(t!("app.lock.wrong").into()));
      assert!(!dialog.checking);
      // cleared for the next try
      assert_eq!(dialog.form.input.read(cx).value(), "");
    });
    assert!(cx.read(|cx| polkit.request(cx).is_some()));

    enter(&dialog, "right", cx);
    assert_eq!(asked.try_recv(), Ok(Some(())));
    assert!(cx.read(|cx| polkit.request(cx).is_none()));
  }

  #[gpui::test]
  fn broken_helper_is_no_wrong_password(cx: &mut TestAppContext) {
    let (polkit, dir) = polkit(cx);
    std::fs::remove_file(dir.path().join("helper")).unwrap();
    let _asked = cx.update(|cx| polkit.ask("Do it", "c1", cx));
    let (dialog, cx) = cx.add_window_view(PolkitDialog::new);
    enter(&dialog, "right", cx);
    dialog.read_with(cx, |dialog, _| {
      assert_eq!(dialog.error, Some(t!("app.lock.auth_failed").into()));
    });
    assert!(cx.read(|cx| polkit.request(cx).is_some()));
  }

  #[gpui::test]
  fn escape_cancels(cx: &mut TestAppContext) {
    let (polkit, _dir) = polkit(cx);
    let mut asked = cx.update(|cx| polkit.ask("Do it", "c1", cx));
    let (_dialog, cx) = cx.add_window_view(PolkitDialog::new);
    cx.simulate_keystrokes("escape");
    assert!(asked.try_recv().is_err());
    assert!(cx.read(|cx| polkit.request(cx).is_none()));
  }
}

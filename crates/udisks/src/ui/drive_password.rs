//! A passphrase dialog for other programs unlocking drives, udiskie's `password_prompt` say,
//! asked for over IPC

use std::{
  collections::VecDeque,
  rc::Rc,
  sync::atomic::{AtomicU64, Ordering},
};

use futures::channel::oneshot;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Context, Entity, Global, IntoElement, Render, Window,
};
use rust_i18n::t;

use corona_surface::overlay::{
  open_on_focused,
  password::{PasswordCard, PasswordForm, follow_queue},
};

const NAMESPACE: &str = "corona_drive_password";

pub struct DrivePasswordRequest {
  /// tells requests for one device apart
  id: u64,
  pub device: String,
  reply: oneshot::Sender<String>,
}

/// The open requests, the shown one first
#[derive(Clone)]
pub struct DrivePasswords {
  pub requests: Entity<VecDeque<DrivePasswordRequest>>,
}

impl Global for DrivePasswords {}

/// Opens a dialog on the focused monitor for the oldest request, a fresh one for each request
pub fn init(cx: &mut App) {
  let requests = cx.new(|_| VecDeque::new());
  cx.set_global(DrivePasswords {
    requests: requests.clone(),
  });
  follow_queue(requests, "drive passphrase", |r| r.id, open_dialog, cx);
}

fn open_dialog(cx: &mut App) -> anyhow::Result<AnyWindowHandle> {
  open_on_focused(
    NAMESPACE,
    |_| {},
    cx,
    |window, cx| cx.new(|cx| DrivePasswordDialog::new(window, cx)),
  )
}

/// Queues a request for `device`'s passphrase; the receiver fails on cancel
pub fn ask(device: String, cx: &mut App) -> oneshot::Receiver<String> {
  // never reused, a request answered and one asked right after are two windows
  static NEXT: AtomicU64 = AtomicU64::new(0);
  let id = NEXT.fetch_add(1, Ordering::Relaxed);
  let (reply, done) = oneshot::channel();
  let requests = cx.global::<DrivePasswords>().requests.clone();
  requests.update(cx, |requests, cx| {
    requests.retain(|r| !r.reply.is_canceled());
    requests.push_back(DrivePasswordRequest { id, device, reply });
    cx.notify();
  });
  done
}

/// Answers the shown request, None cancels it
fn answer(password: Option<String>, cx: &mut App) {
  let requests = cx.global::<DrivePasswords>().requests.clone();
  requests.update(cx, |requests, cx| {
    if let (Some(request), Some(password)) = (requests.pop_front(), password) {
      let _ = request.reply.send(password);
    }
    // requests whose client is gone are skipped
    requests.retain(|r| !r.reply.is_canceled());
    cx.notify();
  });
}

struct DrivePasswordDialog {
  form: PasswordForm,
}

impl DrivePasswordDialog {
  fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    Self {
      form: PasswordForm::new(window, cx, Self::submit),
    }
  }

  /// an empty passphrase is no answer, cancel is
  fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
    let password = self.form.input.read(cx).value().to_string();
    if !password.is_empty() {
      answer(Some(password), cx);
    }
  }
}

impl Render for DrivePasswordDialog {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let requests = cx.global::<DrivePasswords>().requests.read(cx);
    let device = requests
      .front()
      .map(|r| r.device.clone())
      .unwrap_or_default();
    PasswordCard {
      id: "drive-password",
      title: t!("app.drive_password.title").into(),
      message: t!("app.drive_password.message", device = device).into(),
      detail: None,
      form: &self.form,
      error: None,
      busy: false,
      submit_label: t!("app.drive_password.unlock").into(),
      on_cancel: Rc::new(|_, cx| answer(None, cx)),
      on_submit: Box::new(cx.listener(|this, _, window, cx| this.submit(window, cx))),
    }
    .render(cx)
  }
}

#[cfg(test)]
mod tests {
  use gpui_kit::{self as gpui, TestAppContext, VisualTestContext};

  use super::*;
  use corona_surface::testing::{FakeCompositor, setup};

  fn enter(dialog: &Entity<DrivePasswordDialog>, password: &str, cx: &mut VisualTestContext) {
    dialog.update_in(cx, |dialog, window, cx| {
      dialog
        .form
        .input
        .update(cx, |input, cx| input.set_value(password, window, cx));
      dialog.submit(window, cx);
    });
    cx.run_until_parked();
  }

  fn pending(cx: &mut VisualTestContext) -> usize {
    cx.read(|cx| cx.global::<DrivePasswords>().requests.read(cx).len())
  }

  #[gpui::test]
  fn a_window_per_request(cx: &mut TestAppContext) {
    setup(FakeCompositor::default(), cx);
    cx.update(init);
    let mut first = cx.update(|cx| ask("/dev/sdb1".into(), cx));
    let mut second = cx.update(|cx| ask("/dev/sdb1".into(), cx));
    cx.run_until_parked();
    let shown = cx.windows();
    assert_eq!(shown.len(), 1);

    // the same device again still gets a fresh window
    cx.update(|cx| answer(None, cx));
    cx.run_until_parked();
    assert!(first.try_recv().is_err());
    let next = cx.windows();
    assert_eq!(next.len(), 1);
    assert_ne!(next, shown);

    cx.update(|cx| answer(Some("secret".into()), cx));
    cx.run_until_parked();
    assert_eq!(second.try_recv(), Ok(Some("secret".into())));
    assert!(cx.windows().is_empty());
  }

  #[gpui::test]
  fn passphrases(cx: &mut TestAppContext) {
    setup(FakeCompositor::default(), cx);
    cx.update(|cx| {
      let requests = cx.new(|_| VecDeque::new());
      cx.set_global(DrivePasswords { requests });
    });
    let mut asked = cx.update(|cx| ask("/dev/sdb1".into(), cx));
    let (dialog, cx) = cx.add_window_view(DrivePasswordDialog::new);

    // nothing typed is no answer
    enter(&dialog, "", cx);
    assert_eq!(pending(cx), 1);
    assert_eq!(asked.try_recv(), Ok(None));

    enter(&dialog, "secret", cx);
    assert_eq!(asked.try_recv(), Ok(Some("secret".into())));
    assert_eq!(pending(cx), 0);
  }

  #[gpui::test]
  fn escape_cancels(cx: &mut TestAppContext) {
    setup(FakeCompositor::default(), cx);
    cx.update(|cx| {
      let requests = cx.new(|_| VecDeque::new());
      cx.set_global(DrivePasswords { requests });
    });
    let mut asked = cx.update(|cx| ask("/dev/sdb1".into(), cx));
    let (_dialog, cx) = cx.add_window_view(DrivePasswordDialog::new);
    cx.simulate_keystrokes("escape");
    assert!(asked.try_recv().is_err());
    assert_eq!(pending(cx), 0);
  }
}

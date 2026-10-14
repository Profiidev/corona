use anyhow::Context as _;
use corona_auth::polkit::Polkit;
use corona_components::components::input::on_enter;
use corona_compositor::CompositorExt;
use corona_utils::display::display_id_for;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Context, Entity, FocusHandle, InteractiveElement, IntoElement,
  KeyDownEvent, ParentElement, Render, SharedString, Styled, Subscription, Window,
  WindowBackgroundAppearance,
  assets::IconName,
  base::{Root, input::InputState},
  component::{
    ActiveTheme, Icon, Sizable,
    button::{Button, ButtonVariants},
    input::Input,
  },
  div,
  prelude::FluentBuilder,
  px,
};
use rust_i18n::t;

use crate::overlays::fullscreen_options;

const NAMESPACE: &str = "corona_polkit";
const WIDTH: f32 = 360.;

/// Opens a dialog on the focused monitor for the oldest polkit request, a
/// fresh one for each request
pub fn init(cx: &mut App) {
  let requests = cx.global::<Polkit>().requests.clone();
  let mut open: Option<(String, AnyWindowHandle)> = None;
  cx.observe(&requests, move |requests, cx| {
    let shown = requests.read(cx).front().map(|r| r.cookie.clone());
    if open.as_ref().map(|(key, _)| key) == shown.as_ref() {
      return;
    }
    if let Some((_, handle)) = open.take() {
      let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
    let Some(key) = shown else { return };
    match open_dialog(cx) {
      Ok(handle) => open = Some((key, handle)),
      Err(e) => tracing::error!("Failed to open the polkit dialog: {e:#}"),
    }
  })
  .detach();
}

fn open_dialog(cx: &mut App) -> anyhow::Result<AnyWindowHandle> {
  let monitor = cx.compositor().active_monitor(cx).name.clone();
  let display = display_id_for(&monitor, cx)
    .or_else(|| cx.primary_display().map(|d| d.id()))
    .context("no display")?;
  let mut options = fullscreen_options(NAMESPACE, display);
  options.window_background = WindowBackgroundAppearance::Transparent;
  let handle = cx.open_window(options, |window, cx| {
    let dialog = cx.new(|cx| PolkitDialog::new(window, cx));
    cx.new(|cx| Root::new(dialog, window, cx).bg(gpui_kit::transparent_black()))
  })?;
  Ok(handle.into())
}

struct PolkitDialog {
  focus: FocusHandle,
  input: Entity<InputState>,
  error: Option<SharedString>,
  checking: bool,
  _submit: Subscription,
}

impl PolkitDialog {
  fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
    let input = cx.new(|cx| {
      InputState::new(window, cx)
        .masked(true)
        .placeholder(t!("app.lock.password"))
    });
    input.update(cx, |input, cx| input.focus(window, cx));
    Self {
      focus: cx.focus_handle(),
      _submit: on_enter(&input, window, cx, Self::submit),
      input,
      error: None,
      checking: false,
    }
  }

  fn submit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    if self.checking {
      return;
    }
    self.checking = true;
    let password = self.input.read(cx).value().to_string();
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
        this.input.update(cx, |input, cx| {
          input.set_value("", window, cx);
          input.focus(window, cx);
        });
        cx.notify();
      });
    })
    .detach();
    cx.notify();
  }

  fn on_key(&mut self, e: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
    if e.keystroke.key == "escape" {
      cx.global::<Polkit>().clone().cancel(cx);
    }
  }
}

impl Render for PolkitDialog {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let request = cx.global::<Polkit>().request(cx);
    let message = request.map(|r| r.message.clone()).unwrap_or_default();
    let user = request.map(|r| r.user.clone()).unwrap_or_default();

    div()
      .track_focus(&self.focus)
      .capture_key_down(cx.listener(Self::on_key))
      .size_full()
      .flex()
      .items_center()
      .justify_center()
      .bg(theme.colors.background.opacity(0.6))
      .child(
        div()
          .flex()
          .flex_col()
          .gap_3()
          .p_5()
          .w(px(WIDTH))
          .rounded_xl()
          .border_1()
          .border_color(theme.colors.border)
          .bg(theme.colors.background)
          .child(
            div()
              .flex()
              .items_center()
              .gap_2()
              .font_weight(gpui_kit::FontWeight::BOLD)
              .child(Icon::new(IconName::Lock).small())
              .child(t!("app.polkit.title").to_string()),
          )
          .child(div().text_sm().child(message))
          .child(
            div()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .child(t!("app.polkit.as_user", user = user).to_string()),
          )
          .child(
            Input::new(&self.input)
              .mask_toggle()
              .disabled(self.checking),
          )
          .when_some(self.error.clone(), |d, error| {
            d.child(div().text_xs().text_color(theme.colors.danger).child(error))
          })
          .child(
            div()
              .flex()
              .gap_2()
              .justify_end()
              .child(
                Button::new("polkit-cancel")
                  .label(t!("app.common.cancel"))
                  .on_click(|_, _, cx| cx.global::<Polkit>().clone().cancel(cx)),
              )
              .child(
                Button::new("polkit-authenticate")
                  .primary()
                  .label(t!("app.polkit.authenticate"))
                  .loading(self.checking)
                  .on_click(cx.listener(|this, _, window, cx| this.submit(window, cx))),
              ),
          ),
      )
  }
}

#[cfg(test)]
mod tests {
  use std::os::unix::fs::PermissionsExt;

  use gpui_kit::{self as gpui, TestAppContext, VisualTestContext};
  use tempfile::TempDir;

  use super::*;
  use crate::test_support::{FakeCompositor, setup};

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
      assert_eq!(dialog.input.read(cx).value(), "");
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

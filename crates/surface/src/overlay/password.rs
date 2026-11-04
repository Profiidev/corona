//! What the polkit and drive passphrase dialogs share: a window per queued request and the card
//! asking for a password

use std::{collections::VecDeque, rc::Rc};

use corona_components::components::input::on_enter;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, ClickEvent, Context, Div, Entity, FocusHandle,
  InteractiveElement, KeyDownEvent, ParentElement, SharedString, Styled, Subscription, Window,
  assets::IconName,
  base::input::InputState,
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

const WIDTH: f32 = 360.;

/// Keeps one window open for the front of `requests`, a fresh one for each request, `key` tells
/// them apart
pub fn follow_queue<T: 'static, K: PartialEq + 'static>(
  requests: Entity<VecDeque<T>>,
  what: &'static str,
  key: impl Fn(&T) -> K + 'static,
  open: impl Fn(&mut App) -> anyhow::Result<AnyWindowHandle> + 'static,
  cx: &mut App,
) {
  let mut shown: Option<(K, AnyWindowHandle)> = None;
  cx.observe(&requests, move |requests, cx| {
    let front = requests.read(cx).front().map(&key);
    // A dialog closed by the compositor leaves a dead handle, reopen it.
    let alive = shown
      .as_ref()
      .is_some_and(|(_, handle)| handle.update(cx, |_, _, _| ()).is_ok());
    if alive && shown.as_ref().map(|(key, _)| key) == front.as_ref() {
      return;
    }
    if let Some((_, handle)) = shown.take() {
      let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
    let Some(front) = front else { return };
    match open(cx) {
      Ok(handle) => shown = Some((front, handle)),
      Err(e) => tracing::error!("Failed to open the {what} dialog: {e:#}"),
    }
  })
  .detach();
  // Nothing else notifies when the compositor closes the dialog, so the check above would wait
  // for the next queue change.
  let closed = requests.downgrade();
  cx.on_window_closed(move |cx, _| {
    if let Some(requests) = closed.upgrade() {
      requests.update(cx, |_, cx| cx.notify());
    }
  })
  .detach();
}

/// A masked input focused for typing a password, submitting on enter
pub struct PasswordForm {
  focus: FocusHandle,
  pub input: Entity<InputState>,
  _submit: Subscription,
}

impl PasswordForm {
  pub fn new<T: 'static>(
    window: &mut Window,
    cx: &mut Context<T>,
    submit: fn(&mut T, &mut Window, &mut Context<T>),
  ) -> Self {
    let input = cx.new(|cx| {
      InputState::new(window, cx)
        .masked(true)
        .placeholder(t!("app.lock.password"))
    });
    input.update(cx, |input, cx| input.focus(window, cx));
    Self {
      focus: cx.focus_handle(),
      _submit: on_enter(&input, window, cx, submit),
      input,
    }
  }
}

type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type OnCancel = Rc<dyn Fn(&mut Window, &mut App)>;

/// The dimmed backdrop with a card asking for a password
pub struct PasswordCard<'a> {
  /// prefixes the buttons' ids
  pub id: &'static str,
  pub title: SharedString,
  pub message: SharedString,
  /// muted, under the message
  pub detail: Option<SharedString>,
  pub form: &'a PasswordForm,
  pub error: Option<SharedString>,
  pub busy: bool,
  pub submit_label: SharedString,
  /// on the cancel button and escape
  pub on_cancel: OnCancel,
  pub on_submit: OnClick,
}

impl PasswordCard<'_> {
  pub fn render(self, cx: &App) -> Div {
    let theme = cx.theme();
    let on_cancel = self.on_cancel;
    let on_escape = on_cancel.clone();
    let on_submit = self.on_submit;
    div()
      .track_focus(&self.form.focus)
      .capture_key_down(move |e: &KeyDownEvent, window, cx| {
        if e.keystroke.key == "escape" {
          on_escape(window, cx);
        }
      })
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
              .child(self.title),
          )
          .child(div().text_sm().child(self.message))
          .when_some(self.detail, |d, detail| {
            d.child(
              div()
                .text_xs()
                .text_color(theme.colors.muted_foreground)
                .child(detail),
            )
          })
          .child(
            Input::new(&self.form.input)
              .mask_toggle()
              .disabled(self.busy),
          )
          .when_some(self.error, |d, error| {
            d.child(div().text_xs().text_color(theme.colors.danger).child(error))
          })
          .child(
            div()
              .flex()
              .gap_2()
              .justify_end()
              .child(
                Button::new(SharedString::from(format!("{}-cancel", self.id)))
                  .label(t!("app.common.cancel"))
                  .on_click(move |_, window, cx| on_cancel(window, cx)),
              )
              .child(
                Button::new(SharedString::from(format!("{}-submit", self.id)))
                  .primary()
                  .label(self.submit_label)
                  .loading(self.busy)
                  .on_click(move |e, window, cx| on_submit(e, window, cx)),
              ),
          ),
      )
  }
}

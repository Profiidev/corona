//! What the polkit and drive passphrase dialogs share: a window per queued request and the card
//! asking for a password

use std::collections::VecDeque;

use anyhow::Context as _;
use corona_compositor::CompositorExt;
use corona_utils::display::display_id_for;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, ClickEvent, Context, Div, Entity, ParentElement, Render,
  SharedString, Styled, Window, WindowBackgroundAppearance,
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

const WIDTH: f32 = 360.;

/// Keeps one window open for the front of `requests`, a fresh one for each request, `key` tells
/// them apart
pub(super) fn follow_queue<T: 'static, K: PartialEq + 'static>(
  requests: Entity<VecDeque<T>>,
  what: &'static str,
  key: impl Fn(&T) -> K + 'static,
  open: impl Fn(&mut App) -> anyhow::Result<AnyWindowHandle> + 'static,
  cx: &mut App,
) {
  let mut shown: Option<(K, AnyWindowHandle)> = None;
  cx.observe(&requests, move |requests, cx| {
    let front = requests.read(cx).front().map(&key);
    if shown.as_ref().map(|(key, _)| key) == front.as_ref() {
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
}

/// A transparent overlay window on the focused monitor showing `build`'s view
pub(super) fn open_window<V: Render + 'static>(
  namespace: &str,
  cx: &mut App,
  build: impl FnOnce(&mut Window, &mut Context<V>) -> V + 'static,
) -> anyhow::Result<AnyWindowHandle> {
  let monitor = cx.compositor().active_monitor(cx).name.clone();
  let display = display_id_for(&monitor, cx)
    .or_else(|| cx.primary_display().map(|d| d.id()))
    .context("no display")?;
  let mut options = fullscreen_options(namespace, display);
  options.window_background = WindowBackgroundAppearance::Transparent;
  let handle = cx.open_window(options, |window, cx| {
    let dialog = cx.new(|cx| build(window, cx));
    cx.new(|cx| Root::new(dialog, window, cx).bg(gpui_kit::transparent_black()))
  })?;
  Ok(handle.into())
}

/// A masked input focused for typing a password
pub(super) fn password_input(window: &mut Window, cx: &mut App) -> Entity<InputState> {
  let input = cx.new(|cx| {
    InputState::new(window, cx)
      .masked(true)
      .placeholder(t!("app.lock.password"))
  });
  input.update(cx, |input, cx| input.focus(window, cx));
  input
}

type OnClick = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// The dimmed backdrop with a card asking for a password
pub(super) struct PasswordCard<'a> {
  /// prefixes the buttons' ids
  pub id: &'static str,
  pub title: SharedString,
  pub message: SharedString,
  /// muted, under the message
  pub detail: Option<SharedString>,
  pub input: &'a Entity<InputState>,
  pub error: Option<SharedString>,
  pub busy: bool,
  pub submit_label: SharedString,
  pub on_cancel: OnClick,
  pub on_submit: OnClick,
}

impl PasswordCard<'_> {
  pub fn render(self, cx: &App) -> Div {
    let theme = cx.theme();
    let on_cancel = self.on_cancel;
    let on_submit = self.on_submit;
    div()
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
          .child(Input::new(self.input).mask_toggle().disabled(self.busy))
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
                  .on_click(move |e, window, cx| on_cancel(e, window, cx)),
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

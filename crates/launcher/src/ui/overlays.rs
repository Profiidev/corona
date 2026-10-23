//! The action panel, confirmation dialog and toast drawn over the launcher

use gpui_kit::{
  AnyElement, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
  StatefulInteractiveElement, Styled,
  component::{ActiveTheme, Icon, Sizable, checkbox::Checkbox, input::Input},
  div,
  prelude::FluentBuilder,
  px,
};
use rust_i18n::t;

use super::{launcher::Launcher, style};
use crate::command::ToastKind;

pub(super) fn panel(this: &Launcher, cx: &mut Context<Launcher>) -> Option<AnyElement> {
  let panel = this.panel.as_ref()?;
  let theme = cx.theme();
  let visible = panel.visible(cx);
  let destructive_start = visible.iter().position(|(_, a)| a.destructive);
  let rows = visible.iter().enumerate().map(|(i, (_, action))| {
    let selected = i == panel.selected;
    let color = match action.destructive {
      true => theme.colors.danger,
      false => theme.foreground,
    };
    div()
      .id(("launcher-action", i))
      .h(px(36.))
      .px(px(10.))
      .flex()
      .items_center()
      .gap_2()
      .rounded(px(8.))
      .text_size(px(13.5))
      .text_color(color)
      .when(selected, |el| el.bg(style::selected(cx)))
      .when(!selected, |el| el.hover(|s| s.bg(style::hover(cx))))
      .when(Some(i) == destructive_start && i > 0, |el| {
        el.mt_1().border_t_1().border_color(theme.colors.border)
      })
      .cursor_pointer()
      .on_click(cx.listener(move |this, _, window, cx| this.click_action(i, window, cx)))
      .when_some(action.icon, |el, icon| {
        el.child(Icon::new(icon).with_size(px(14.)))
      })
      .child(div().flex_1().child(action.title.clone()))
      .when_some(action.shortcut.clone(), |el, keys| {
        el.child(style::kbd(&keys, cx))
      })
      .when(i == 0, |el| el.child(style::kbd("enter", cx)))
  });
  Some(
    div()
      .absolute()
      .right(px(10.))
      .bottom(px(style::FOOTER_HEIGHT + 6.))
      .w(px(style::PANEL_WIDTH))
      .max_h(px(320.))
      .flex()
      .flex_col()
      .rounded(px(12.))
      .border_1()
      .border_color(theme.colors.border)
      .bg(style::popover_fill(cx))
      .shadow_lg()
      .overflow_hidden()
      .child(
        div()
          .id("launcher-actions")
          .flex_1()
          .min_h_0()
          .overflow_y_scroll()
          .p(px(6.))
          .children(rows),
      )
      .child(
        div()
          .h(px(40.))
          .px(px(10.))
          .flex()
          .items_center()
          .border_t_1()
          .border_color(theme.colors.border)
          .text_size(px(13.))
          .child(Input::new(&panel.input).appearance(false).small()),
      )
      .into_any_element(),
  )
}

pub(super) fn confirm(this: &Launcher, cx: &mut Context<Launcher>) -> Option<AnyElement> {
  let state = this.confirm.as_ref()?;
  let theme = cx.theme();
  let button = |id: &'static str, label, focused: bool, destructive: bool| {
    let (fill, color) = match destructive {
      true => (theme.colors.danger, theme.colors.danger_foreground),
      false => (style::key_cap(cx), theme.foreground),
    };
    div()
      .id(id)
      .h(px(32.))
      .px(px(14.))
      .flex()
      .items_center()
      .rounded(px(8.))
      .bg(fill)
      .text_color(color)
      .text_size(px(13.5))
      .font_weight(FontWeight::MEDIUM)
      .when(focused, |el| {
        el.border_2().border_color(style::focus_ring(cx))
      })
      .cursor_pointer()
      .child(label)
  };
  Some(
    div()
      .absolute()
      .inset_0()
      .flex()
      .items_center()
      .justify_center()
      .bg(style::scrim())
      .child(
        div()
          .w(px(380.))
          .p(px(20.))
          .flex()
          .flex_col()
          .gap_3()
          .rounded(px(12.))
          .border_1()
          .border_color(theme.colors.border)
          .bg(style::popover_fill(cx))
          .child(
            div()
              .text_size(px(15.))
              .font_weight(FontWeight::SEMIBOLD)
              .child(state.confirm.title.clone()),
          )
          .child(
            div()
              .text_size(px(13.5))
              .text_color(theme.colors.muted_foreground)
              .child(state.confirm.message.clone()),
          )
          .when(state.confirm.skippable, |el| {
            el.child(
              Checkbox::new("launcher-dont-ask")
                .checked(state.dont_ask)
                .label(t!("app.launcher.dont_ask").to_string())
                .small()
                .on_click(cx.listener(|this, _, _, cx| this.toggle_dont_ask(cx))),
            )
          })
          .child(
            div()
              .flex()
              .justify_end()
              .gap_2()
              .child(
                button(
                  "launcher-cancel",
                  t!("app.common.cancel").to_string(),
                  !state.on_confirm,
                  false,
                )
                .on_click(cx.listener(|this, _, window, cx| this.click_confirm(false, window, cx))),
              )
              .child(
                button(
                  "launcher-confirm",
                  state.confirm.button.to_string(),
                  state.on_confirm,
                  true,
                )
                .on_click(cx.listener(|this, _, window, cx| this.click_confirm(true, window, cx))),
              ),
          ),
      )
      .into_any_element(),
  )
}

pub(super) fn toast(this: &Launcher, cx: &mut Context<Launcher>) -> Option<AnyElement> {
  let toast = this.toast.as_ref()?;
  let theme = cx.theme();
  let (icon, color) = match toast.kind {
    ToastKind::Success => (
      gpui_kit::assets::IconName::CircleCheck,
      theme.colors.success,
    ),
    ToastKind::Failure => (gpui_kit::assets::IconName::CircleX, theme.colors.danger),
    ToastKind::Progress => (
      gpui_kit::assets::IconName::LoaderCircle,
      theme.colors.primary,
    ),
  };
  Some(
    div()
      .absolute()
      .bottom(px(style::FOOTER_HEIGHT + 10.))
      .left_0()
      .right_0()
      .flex()
      .justify_center()
      .child(
        div()
          .min_w(px(340.))
          .max_w(px(560.))
          .px(px(14.))
          .py(px(10.))
          .flex()
          .items_center()
          .gap_3()
          .rounded(px(12.))
          .border_1()
          .border_color(theme.colors.border)
          .bg(style::popover_fill(cx))
          .shadow_lg()
          .child(Icon::new(icon).with_size(px(16.)).text_color(color))
          .child(
            div()
              .flex_1()
              .min_w_0()
              .flex()
              .flex_col()
              .child(
                div()
                  .text_size(px(13.5))
                  .font_weight(FontWeight::MEDIUM)
                  .child(toast.title.clone()),
              )
              .when_some(toast.message.clone(), |el, message| {
                el.child(
                  div()
                    .text_size(px(12.5))
                    .text_color(theme.colors.muted_foreground)
                    .child(message),
                )
              }),
          )
          .when_some(toast.action.clone(), |el, action| {
            el.child(
              div()
                .id("launcher-toast-action")
                .px(px(10.))
                .h(px(28.))
                .flex()
                .items_center()
                .rounded(px(7.))
                .bg(style::key_cap(cx))
                .text_size(px(13.))
                .cursor_pointer()
                .on_click(cx.listener(|this, _, window, cx| this.click_toast(window, cx)))
                .child(action.title),
            )
          }),
      )
      .into_any_element(),
  )
}

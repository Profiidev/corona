//! The tray widget's options form

use corona_config::bar::BarConfig;
use corona_settings::{
  SettingsExt, Slot, WidgetOptions,
  bar::{current_options, form, form_input, form_key, options, set_options},
};
use gpui_kit::{
  App, IntoElement, ParentElement, Styled, Window,
  assets::IconName,
  component::{
    Sizable,
    button::{Button, ButtonVariants},
  },
  div,
};
use rust_i18n::t;

use crate::bar::tray::{Options, Tray};

pub(super) fn register(cx: &mut App) {
  cx.settings_mut().widget_options::<Tray>(WidgetOptions {
    form: tray_form,
    label: None,
  });
}

fn tray_form(
  name: &str,
  bar: &BarConfig,
  slot: Slot,
  window: &mut Window,
  cx: &mut App,
) -> gpui_kit::AnyElement {
  let current: Options = options(bar, slot);
  let rows: Vec<_> = current
    .blacklist
    .iter()
    .enumerate()
    .map(|(i, pattern)| {
      let (edit_name, remove_name) = (name.to_string(), name.to_string());
      div()
        .flex()
        .items_center()
        .gap_1()
        .child(form_input(
          form_key(name, slot, &format!("blacklist-{i}")),
          pattern.clone(),
          window,
          cx,
          move |text, cx| {
            let Some(mut options) = current_options::<Options>(cx, &edit_name, slot) else {
              return;
            };
            if let Some(pattern) = options.blacklist.get_mut(i) {
              *pattern = text;
              set_options(cx, &edit_name, slot, options);
            }
          },
        ))
        .child(
          Button::new(form_key(name, slot, &format!("blacklist-remove-{i}")))
            .cursor_pointer()
            .icon(IconName::X)
            .ghost()
            .xsmall()
            .on_click(move |_, _, cx| {
              let Some(mut options) = current_options::<Options>(cx, &remove_name, slot) else {
                return;
              };
              if i < options.blacklist.len() {
                options.blacklist.remove(i);
                set_options(cx, &remove_name, slot, options);
              }
            }),
        )
    })
    .collect();
  let add_name = name.to_string();
  form()
    .child(
      div()
        .text_sm()
        .child(t!("app.settings.bar.form.hidden_items")),
    )
    .children(rows)
    .child(
      div().child(
        Button::new(form_key(name, slot, "blacklist-add"))
          .label(t!("app.settings.bar.form.add_pattern"))
          .cursor_pointer()
          .icon(IconName::Plus)
          .xsmall()
          .outline()
          .on_click(move |_, _, cx| {
            let Some(mut options) = current_options::<Options>(cx, &add_name, slot) else {
              return;
            };
            options.blacklist.push(String::new());
            set_options(cx, &add_name, slot, options);
          }),
      ),
    )
    .into_any_element()
}

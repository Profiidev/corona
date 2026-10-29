//! The option forms of the shell's bar widgets

use corona_config::bar::BarConfig;
use corona_settings::{
  SettingsExt, Slot, WidgetOptions,
  bar::{current_options, form, form_input, form_key, form_row, options, set_options},
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

use crate::widgets::{
  control_center::disks::{self, DisksButton},
  privacy::{self, Privacy},
  tray::{self, Tray},
};

pub(super) fn register(cx: &mut App) {
  let form = |form| WidgetOptions { form, label: None };
  cx.settings_mut()
    .widget_options::<Tray>(form(tray_form))
    .widget_options::<Privacy>(form(privacy_form))
    .widget_options::<DisksButton>(form(disks_form));
}

fn tray_form(
  name: &str,
  bar: &BarConfig,
  slot: Slot,
  window: &mut Window,
  cx: &mut App,
) -> gpui_kit::AnyElement {
  let current: tray::Options = options(bar, slot);
  let rows: Vec<_> = current
    .blacklist
    .iter()
    .enumerate()
    .map(|(i, pattern)| {
      let (edit_name, remove_name) = (name.to_string(), name.to_string());
      let remove_list = current.blacklist.clone();
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
            let Some(mut options) = current_options::<tray::Options>(cx, &edit_name, slot) else {
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
              let mut blacklist = remove_list.clone();
              blacklist.remove(i);
              set_options(cx, &remove_name, slot, tray::Options { blacklist });
            }),
        )
    })
    .collect();
  let add_name = name.to_string();
  let add_list = current.blacklist.clone();
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
            let mut blacklist = add_list.clone();
            blacklist.push(String::new());
            set_options(cx, &add_name, slot, tray::Options { blacklist });
          }),
      ),
    )
    .into_any_element()
}

fn privacy_form(
  name: &str,
  bar: &BarConfig,
  slot: Slot,
  _: &mut Window,
  _: &mut App,
) -> gpui_kit::AnyElement {
  let current: privacy::Options = options(bar, slot);
  let name = name.to_string();
  form()
    .child(form_row(
      t!("app.settings.bar.form.hide_idle"),
      gpui_kit::component::switch::Switch::new(form_key(&name, slot, "idle"))
        .checked(current.hide_when_idle)
        .small()
        .on_click(move |checked, _, cx| {
          set_options(
            cx,
            &name,
            slot,
            privacy::Options {
              hide_when_idle: *checked,
            },
          );
        }),
    ))
    .into_any_element()
}

fn disks_form(
  name: &str,
  bar: &BarConfig,
  slot: Slot,
  _: &mut Window,
  _: &mut App,
) -> gpui_kit::AnyElement {
  let current: disks::Options = options(bar, slot);
  let name = name.to_string();
  form()
    .child(form_row(
      t!("app.settings.bar.form.hide_no_disks"),
      gpui_kit::component::switch::Switch::new(form_key(&name, slot, "idle"))
        .checked(current.hide_when_idle)
        .small()
        .on_click(move |checked, _, cx| {
          set_options(
            cx,
            &name,
            slot,
            disks::Options {
              hide_when_idle: *checked,
            },
          );
        }),
    ))
    .into_any_element()
}

//! The option forms of the shell's bar widgets

use corona_config::bar::BarConfig;
use corona_settings::{
  SettingsExt, Slot, WidgetOptions,
  bar::{form, form_key, form_row, options, set_options},
};
use gpui_kit::{App, IntoElement, ParentElement, Window, component::Sizable};
use rust_i18n::t;

use crate::widgets::{
  control_center::disks::{self, DisksButton},
  privacy::{self, Privacy},
};

pub(super) fn register(cx: &mut App) {
  let form = |form| WidgetOptions { form, label: None };
  cx.settings_mut()
    .widget_options::<Privacy>(form(privacy_form))
    .widget_options::<DisksButton>(form(disks_form));
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

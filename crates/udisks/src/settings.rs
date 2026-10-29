//! The disks button's options form

use corona_config::bar::BarConfig;
use corona_settings::{
  SettingsExt, Slot, WidgetOptions,
  bar::{form, form_key, form_row, options, set_options},
};
use gpui_kit::{App, IntoElement, ParentElement, Window, component::Sizable};
use rust_i18n::t;

use crate::ui::widget::{DisksButton, Options};

pub(crate) fn register(cx: &mut App) {
  cx.settings_mut()
    .widget_options::<DisksButton>(WidgetOptions {
      form: disks_form,
      label: None,
    });
}

fn disks_form(
  name: &str,
  bar: &BarConfig,
  slot: Slot,
  _: &mut Window,
  _: &mut App,
) -> gpui_kit::AnyElement {
  let current: Options = options(bar, slot);
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
            Options {
              hide_when_idle: *checked,
            },
          );
        }),
    ))
    .into_any_element()
}

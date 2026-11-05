//! The disks button's options form

use corona_config::bar::BarConfig;
use corona_settings::{SettingsExt, Slot, WidgetOptions, bar::idle_form};
use gpui_kit::{App, Window};
use rust_i18n::t;

use corona_surface::bar::IdlePageButton;

pub(crate) fn register(cx: &mut App) {
  cx.settings_mut()
    .widget_options::<IdlePageButton<crate::ui::page::DisksPanel>>(WidgetOptions {
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
  idle_form(name, bar, slot, t!("app.settings.bar.form.hide_no_disks"))
}

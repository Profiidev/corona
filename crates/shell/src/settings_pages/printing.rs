//! The printers button's options form

use corona_config::bar::BarConfig;
use corona_settings::{SettingsExt, Slot, WidgetOptions, bar::idle_form};
use gpui_kit::{App, Window};
use rust_i18n::t;

use corona_surface::bar::IdlePageButton;

pub(super) fn register(cx: &mut App) {
  cx.settings_mut()
    .widget_options::<IdlePageButton<crate::panel::printers::PrintersPanel>>(WidgetOptions {
      form: printers_form,
      label: None,
    });
}

fn printers_form(
  name: &str,
  bar: &BarConfig,
  slot: Slot,
  _: &mut Window,
  _: &mut App,
) -> gpui_kit::AnyElement {
  idle_form(name, bar, slot, t!("app.settings.bar.form.hide_no_jobs"))
}

//! The privacy page, apps hidden from the privacy widget and OSD, and the widget's options form

use corona_components::settings_fields::text;
use corona_config::bar::BarConfig;
use corona_settings::{
  SettingsExt, SettingsPage, Slot, WidgetOptions,
  bar::{form, form_key, form_row, options, set_options},
  group, item, page,
};
use gpui_kit::{
  App, Axis, IntoElement, ParentElement, Window,
  assets::IconName,
  component::{Sizable, setting::SettingPage},
};
use rust_i18n::t;

use crate::ui::privacy::{Options, Privacy};

pub struct PrivacySettings;

impl SettingsPage for PrivacySettings {
  const ID: &'static str = "privacy";
  const ORDER: i32 = 140;

  fn page(_: &App) -> SettingPage {
    privacy()
  }
}

pub(crate) fn register(cx: &mut App) {
  cx.settings_mut()
    .register::<PrivacySettings>()
    .widget_options::<Privacy>(WidgetOptions {
      form: privacy_form,
      label: None,
    });
}

fn privacy() -> SettingPage {
  page(
    t!("app.settings.privacy.title"),
    IconName::Shield,
    vec![group(
      t!("app.settings.privacy.groups.hidden_apps"),
      vec![
        item(
          t!("app.settings.privacy.microphone.title"),
          t!("app.settings.privacy.microphone.description"),
          text(
            "shell.privacy.mic",
            |c| c.shell.privacy.mic_filter_regex.clone(),
            |c, v| c.shell.privacy.mic_filter_regex = v,
          ),
        )
        .layout(Axis::Vertical),
        item(
          t!("app.settings.privacy.camera.title"),
          t!("app.settings.privacy.camera.description"),
          text(
            "shell.privacy.cam",
            |c| c.shell.privacy.cam_filter_regex.clone(),
            |c, v| c.shell.privacy.cam_filter_regex = v,
          ),
        )
        .layout(Axis::Vertical),
        item(
          t!("app.settings.privacy.screen.title"),
          t!("app.settings.privacy.screen.description"),
          text(
            "shell.privacy.screen",
            |c| c.shell.privacy.screen_filter_regex.clone(),
            |c, v| c.shell.privacy.screen_filter_regex = v,
          ),
        )
        .layout(Axis::Vertical),
      ],
    )],
  )
}

fn privacy_form(
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
      t!("app.settings.bar.form.hide_idle"),
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

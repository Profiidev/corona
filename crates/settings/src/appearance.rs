//! The appearance page: theme, language, fonts, surfaces and animation

use std::iter;

use corona_components::{
  assets::{set_theme, theme_font, theme_names},
  i18n,
  settings_fields::{Choice, choice, optional_text, searchable, slider, switch},
};
use corona_config::{Config, ConfigProvider, ThemeMode};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  App, Styled, Window,
  assets::IconName,
  component::{
    Sizable,
    setting::{RenderOptions, SettingField, SettingPage},
  },
};
use rust_i18n::t;

use crate::{group, item, page as new_page};

pub(crate) fn page(cx: &App) -> SettingPage {
  let themes: Vec<Choice> = theme_names(cx)
    .into_iter()
    .map(|name| Choice {
      value: name.to_string(),
      label: name.to_string(),
    })
    .collect();
  new_page(
    t!("app.settings.appearance.title"),
    IconName::Palette,
    vec![
      group(
        t!("app.settings.appearance.groups.theme"),
        vec![
          item(
            t!("app.settings.appearance.theme.title"),
            t!("app.settings.appearance.theme.description"),
            SettingField::render(
              move |options: &RenderOptions, window: &mut Window, cx: &mut App| {
                let current = cx.config().theme.name.clone();
                searchable(
                  "theme.name",
                  themes.clone(),
                  Some(current),
                  t!("app.settings.appearance.theme.title"),
                  window,
                  cx,
                  |name, cx| {
                    let _ = set_theme(name.to_string(), cx).log_err();
                  },
                )
                .with_size(options.size())
                .w_64()
              },
            )
            .on_reset(
              |cx: &App| cx.config().theme.name != Config::default().theme.name,
              |_, cx: &mut App| {
                let _ = set_theme(Config::default().theme.name, cx).log_err();
              },
            ),
          ),
          item(
            t!("app.settings.appearance.language.title"),
            t!("app.settings.appearance.language.description"),
            choice(
              // each language under its own name
              &iter::once((None, t!("app.settings.options.language_auto")))
                .chain(i18n::languages().into_iter().map(|language| {
                  let name = t!("app.language_name", locale = &language);
                  (Some(language), name)
                }))
                .collect::<Vec<_>>(),
              |c| c.shell.language.clone(),
              |c, v| c.shell.language = v,
            ),
          ),
          item(
            t!("app.settings.appearance.mode.title"),
            t!("app.settings.appearance.mode.description"),
            choice(
              &[
                (None, t!("app.settings.options.theme_default")),
                (Some(ThemeMode::Dark), t!("app.settings.options.dark")),
                (Some(ThemeMode::Light), t!("app.settings.options.light")),
              ],
              |c| c.theme.mode,
              |c, v| c.theme.mode = v,
            ),
          ),
          item(
            t!("app.settings.appearance.font_family.title"),
            t!("app.settings.appearance.font_family.description"),
            optional_text(
              "theme.font_family",
              |c| c.theme.font_family.clone(),
              |c, v| c.theme.font_family = v,
              |cx| theme_font(cx).to_string(),
            ),
          ),
          item(
            t!("app.settings.appearance.font_scale.title"),
            t!("app.settings.appearance.font_scale.description"),
            slider(
              "theme.font_scale",
              (0.5, 2., 0.05),
              |c| c.theme.font_scale,
              |c, v| c.theme.font_scale = v,
            ),
          ),
          item(
            t!("app.settings.appearance.corner_radius_scale.title"),
            t!("app.settings.appearance.corner_radius_scale.description"),
            slider(
              "theme.corner_radius_scale",
              (0., 3., 0.1),
              |c| c.theme.corner_radius_scale,
              |c, v| c.theme.corner_radius_scale = v,
            ),
          ),
          item(
            t!("app.settings.appearance.sync_toolkits.title"),
            t!("app.settings.appearance.sync_toolkits.description"),
            switch(|c| c.theme.sync_toolkits, |c, v| c.theme.sync_toolkits = v),
          ),
        ],
      ),
      group(
        t!("app.settings.appearance.groups.surfaces"),
        vec![
          item(
            t!("app.settings.appearance.shadows.title"),
            t!("app.settings.appearance.shadows.description"),
            switch(|c| c.theme.shadow, |c, v| c.theme.shadow = v),
          ),
          item(
            t!("app.settings.appearance.popup_borders.title"),
            t!("app.settings.appearance.popup_borders.description"),
            switch(|c| c.theme.popup_borders, |c, v| c.theme.popup_borders = v),
          ),
          item(
            t!("app.settings.appearance.card_borders.title"),
            t!("app.settings.appearance.card_borders.description"),
            switch(|c| c.theme.card_borders, |c, v| c.theme.card_borders = v),
          ),
          item(
            t!("app.settings.appearance.screen_corners.title"),
            t!("app.settings.appearance.screen_corners.description"),
            switch(
              |c| c.theme.screen_corners,
              |c, v| c.theme.screen_corners = v,
            ),
          ),
        ],
      ),
      group(
        t!("app.settings.appearance.groups.animation"),
        vec![
          item(
            t!("app.settings.appearance.animations.title"),
            t!("app.settings.appearance.animations.description"),
            switch(
              |c| c.shell.animation.enabled,
              |c, v| c.shell.animation.enabled = v,
            ),
          ),
          item(
            t!("app.settings.appearance.speed.title"),
            t!("app.settings.appearance.speed.description"),
            slider(
              "shell.animation.speed",
              (0.25, 4., 0.25),
              |c| c.shell.animation.speed,
              |c, v| c.shell.animation.speed = v,
            ),
          ),
        ],
      ),
    ],
  )
}

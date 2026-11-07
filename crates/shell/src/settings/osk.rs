//! The on-screen keyboard's settings page

use corona_components::settings_fields::{choice, switch};
use corona_config::osk::{AutoShow, OskLanguage, OskLayout, TabletSource};
use corona_settings::{SettingsExt, group, item, page, settings_page};
use gpui_kit::{
  App, BorrowAppContext, Styled,
  assets::IconName,
  component::{
    Sizable,
    button::Button,
    setting::{SettingField, SettingItem, SettingPage},
  },
};
use rust_i18n::t;

use corona_osk::Osk;

settings_page!(OskSettings, "osk", 90, |cx| osk());

pub(super) fn register(cx: &mut App) {
  cx.settings_mut().register::<OskSettings>();
}

fn osk() -> SettingPage {
  page(
    t!("app.settings.osk.title"),
    IconName::Keyboard,
    vec![
      group(
        t!("app.settings.osk.groups.keyboard"),
        vec![
          item(
            t!("app.settings.osk.enabled.title"),
            t!("app.settings.osk.enabled.description"),
            switch(|c| c.osk.enabled, |c, v| c.osk.enabled = v),
          ),
          item(
            t!("app.settings.osk.auto_show.title"),
            t!("app.settings.osk.auto_show.description"),
            choice(
              &[
                (AutoShow::Never, t!("app.settings.options.never")),
                (AutoShow::Always, t!("app.settings.options.always")),
                (AutoShow::TabletMode, t!("app.settings.options.tablet_mode")),
              ],
              |c| c.osk.auto_show,
              |c, v| c.osk.auto_show = v,
            ),
          ),
          item(
            t!("app.settings.osk.tablet_source.title"),
            t!("app.settings.osk.tablet_source.description"),
            choice(
              &[
                (TabletSource::Ipc, t!("app.settings.options.ipc")),
                (TabletSource::Hinge, t!("app.settings.options.hinge")),
              ],
              |c| c.osk.tablet_source,
              |c, v| c.osk.tablet_source = v,
            ),
          ),
          item(
            t!("app.settings.osk.layout.title"),
            t!("app.settings.osk.layout.description"),
            choice(
              &[
                (OskLayout::Auto, t!("app.settings.options.language_auto")),
                (OskLayout::Qwerty, "QWERTY".into()),
                (OskLayout::Qwertz, "QWERTZ".into()),
              ],
              |c| c.osk.layout,
              |c, v| c.osk.layout = v,
            ),
          ),
          item(
            t!("app.settings.osk.language.title"),
            t!("app.settings.osk.language.description"),
            // each language under its own name
            choice(
              &[
                (OskLanguage::Auto, t!("app.settings.options.language_auto")),
                (OskLanguage::En, "English".into()),
                (OskLanguage::De, "Deutsch".into()),
              ],
              |c| c.osk.language,
              |c, v| c.osk.language = v,
            ),
          ),
        ],
      ),
      group(
        t!("app.settings.osk.groups.suggestions"),
        vec![
          item(
            t!("app.settings.osk.suggestions.title"),
            t!("app.settings.osk.suggestions.description"),
            switch(|c| c.osk.suggestions, |c, v| c.osk.suggestions = v),
          ),
          item(
            t!("app.settings.osk.spell_check.title"),
            t!("app.settings.osk.spell_check.description"),
            switch(|c| c.osk.spell_check, |c, v| c.osk.spell_check = v),
          ),
          item(
            t!("app.settings.osk.learn_words.title"),
            t!("app.settings.osk.learn_words.description"),
            switch(|c| c.osk.learn_words, |c, v| c.osk.learn_words = v),
          ),
          SettingItem::new(
            t!("app.settings.osk.clear_learned.title"),
            SettingField::render(|_, _, _| {
              Button::new("osk-clear-learned")
                .cursor_pointer()
                .label(t!("app.settings.osk.clear_learned.button"))
                .outline()
                .small()
                .on_click(|_, _, cx| {
                  if cx.has_global::<Osk>() {
                    cx.update_global::<Osk, _>(|osk, cx| osk.clear_learned(cx));
                  }
                })
            }),
          )
          .description(t!("app.settings.osk.clear_learned.description").to_string()),
        ],
      ),
    ],
  )
}

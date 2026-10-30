//! Theme and dark mode

use std::rc::Rc;

use crate::{
  Action, ArgSpec, Command, Hud, Icon, Meta, Outcome,
  kit::{Choices, Group, pick_or_list},
  simple,
};
use corona_components::assets::{preview_theme, restore_theme, set_mode, set_theme, theme_names};
use corona_config::ConfigProvider;
use gpui_kit::{App, assets::IconName, component::ActiveTheme};
use rust_i18n::t;

pub(crate) fn init(cx: &mut App) {
  Group::new("system", vec![dark_mode(), theme()]).register(cx);
}

fn theme() -> Rc<dyn Command> {
  pick_or_list(
    "theme",
    t!("app.launcher.system.theme").into(),
    Icon::Tile(IconName::Palette, 300.),
    &["colors", "appearance"],
    theme_names,
    |name, cx| set_theme(name.to_string(), cx),
    themes,
  )
}

fn dark_mode() -> Rc<dyn Command> {
  simple(
    "theme:mode",
    |cx| {
      let dark = cx.theme().is_dark();
      Meta {
        title: match dark {
          true => t!("app.launcher.system.light_mode"),
          false => t!("app.launcher.system.dark_mode"),
        }
        .into(),
        icon: Icon::Tile(if dark { IconName::Sun } else { IconName::Moon }, 260.),
        keywords: vec!["dark".into(), "light".into(), "theme".into()],
        args: vec![
          ArgSpec::choice(t!("app.launcher.system.mode"), ["dark", "light", "toggle"]).optional(),
        ],
        ..Default::default()
      }
    },
    |_| {
      vec![Action::sync(
        "mode",
        t!("app.launcher.switch"),
        |args, _, cx| {
          let dark = match args.text(0) {
            Some("dark") => true,
            Some("light") => false,
            _ => !cx.theme().is_dark(),
          };
          set_mode(dark, cx)?;
          Ok(Outcome::Close(Some(Hud::new(match dark {
            true => t!("app.launcher.system.dark_mode"),
            false => t!("app.launcher.system.light_mode"),
          }))))
        },
      )]
    },
  )
}

fn themes() -> crate::View {
  Choices {
    id: "theme",
    items: Rc::new(|cx| {
      theme_names(cx)
        .into_iter()
        .map(|n| (n.clone().into(), n.into()))
        .collect()
    }),
    current: Rc::new(|cx| Some(cx.config().theme.name.clone().into())),
    apply: Rc::new(|name, cx| {
      if name == cx.config().theme.name {
        return restore_theme(cx);
      }
      set_theme(name.to_string(), cx)
    }),
    preview: Some(Rc::new(|name, cx| {
      let _ = preview_theme(name, cx);
    })),
    icon: Icon::Named(IconName::Palette),
  }
  .view(
    t!("app.launcher.system.theme"),
    t!("app.launcher.system.search_themes"),
  )
}

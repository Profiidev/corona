//! A grid of the installed fonts

use std::rc::Rc;

use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

use crate::{
  command::{Action, Clip, Icon, Meta, Outcome},
  kit::{ListSource, opener},
  registry, simple,
  view::ListView,
};

fn fonts_view() -> crate::View {
  ListView::new(
    t!("app.launcher.gallery.fonts"),
    t!("app.launcher.gallery.search_fonts"),
    Rc::new(ListSource::new("fonts", |_, cx| {
      let mut names = cx.text_system().all_font_names();
      names.sort();
      names.dedup();
      names
        .into_iter()
        .filter(|n| !n.starts_with('.'))
        .map(|family| {
          let shown: SharedString = family.clone().into();
          simple(
            format!("font:{family}"),
            move |_| Meta {
              title: shown.clone(),
              icon: Icon::Sample("Aa".into(), shown.clone()),
              ..Default::default()
            },
            move |_| {
              let family = family.clone();
              vec![Action::sync(
                "copy",
                t!("app.launcher.gallery.copy_family"),
                move |_, _, _| Ok(Outcome::Copy(Clip::Text(family.clone()))),
              )]
            },
          )
        })
        .collect()
    })),
  )
  .grid(4, 140., true)
  .into_view()
}

pub fn init(cx: &mut App) {
  registry::register_all(
    [opener(
      "fonts",
      t!("app.launcher.gallery.fonts"),
      Icon::Tile(IconName::Type, 280.),
      &["font", "typeface"],
      |_| fonts_view(),
    )],
    cx,
  );
}

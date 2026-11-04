//! Settings pages, panels, control center pages

use std::rc::Rc;

use crate::{
  Action, Command, CommandSource, Icon, ListView, Meta, Outcome, Query, Section, kit::opener,
  registry, simple,
};
use corona_ipc::IpcCommand;
use corona_surface::{commands::OpenPage, control_center::ControlCenterExt, panel::PanelState};
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

use corona_settings::SettingsExt;

fn settings_title(page: &str) -> String {
  let key = format!("app.settings.{page}.title");
  t!(&key).to_string()
}

/// Settings › Page, one per page
struct SettingsPages;

impl CommandSource for SettingsPages {
  fn id(&self) -> SharedString {
    "settings".into()
  }

  fn commands(&self, _: &Query, cx: &App) -> Vec<Rc<dyn Command>> {
    (cx.settings().ids().into_iter())
      .map(|page| {
        let title = format!(
          "{} › {}",
          t!("app.launcher.tools.settings"),
          settings_title(page)
        );
        simple(
          format!("settings:{page}"),
          move |_| Meta {
            title: title.clone().into(),
            icon: Icon::Tile(IconName::Settings, 220.),
            keywords: vec!["settings".into(), "preferences".into(), page.into()],
            section: Section::Commands,
            ..Default::default()
          },
          move |_| {
            vec![Action::sync(
              "open",
              t!("app.launcher.open"),
              move |_, _, cx| {
                corona_settings::open(Some(page), cx)?;
                Ok(Outcome::close())
              },
            )]
          },
        )
      })
      .collect()
  }
}

/// `(page, field)` of every setting with a title in the settings strings, on
/// the `pages` there are
fn setting_fields(pages: &[&'static str]) -> Vec<(&'static str, &'static str)> {
  static FIELDS: std::sync::OnceLock<Vec<(&'static str, &'static str)>> =
    std::sync::OnceLock::new();
  let fields = FIELDS.get_or_init(|| {
    const YAML: &str = include_str!("../../../../assets/locales/settings.yml");
    let lines: Vec<&str> = YAML.lines().collect();
    let indent = |l: &str| l.len() - l.trim_start().len();
    let mut out = Vec::new();
    let mut page: Option<&'static str> = None;
    for (i, line) in lines.iter().enumerate() {
      let key = line.trim().trim_end_matches(':');
      match indent(line) {
        4 => page = Some(key),
        6 if line.trim().ends_with(':') => {
          let titled = lines
            .get(i + 1)
            .is_some_and(|next| indent(next) == 8 && next.trim() == "title:");
          if let (Some(page), true) = (page, titled) {
            out.push((page, key));
          }
        }
        _ => {}
      }
    }
    out
  });
  (fields.iter().copied())
    .filter(|(page, _)| pages.contains(page))
    .collect()
}

/// Individual settings, opening their page
struct SettingFields;

impl CommandSource for SettingFields {
  fn id(&self) -> SharedString {
    "settings:fields".into()
  }

  fn commands(&self, query: &Query, cx: &App) -> Vec<Rc<dyn Command>> {
    if query.is_empty() {
      return Vec::new();
    }
    setting_fields(&cx.settings().ids())
      .into_iter()
      .map(|(page, field)| {
        let key = format!("app.settings.{page}.{field}.title");
        let title = t!(&key).to_string();
        let key = format!("app.settings.{page}.{field}.description");
        let description = t!(&key).to_string();
        let page_title = settings_title(page);
        simple(
          format!("setting:{page}:{field}"),
          move |_| Meta {
            title: title.clone().into(),
            subtitle: Some(format!("{} › {page_title}", t!("app.launcher.tools.settings")).into()),
            icon: Icon::Named(IconName::SlidersHorizontal),
            keywords: vec![description.clone().into(), page_title.clone().into()],
            section: Section::Results,
            ..Default::default()
          },
          move |_| {
            vec![Action::sync(
              "open",
              t!("app.launcher.open"),
              move |_, _, cx| {
                corona_settings::open(Some(page), cx)?;
                Ok(Outcome::close())
              },
            )]
          },
        )
      })
      .collect()
  }
}

/// Every surface panel, plugin ones too
struct Panels;

impl CommandSource for Panels {
  fn id(&self) -> SharedString {
    "panels".into()
  }

  fn commands(&self, _: &Query, cx: &App) -> Vec<Rc<dyn Command>> {
    PanelState::names(cx)
      .into_iter()
      .map(|name| {
        let open = PanelState::is_open(&name, cx);
        let title: SharedString = name.replace(['_', ':'], " ").into();
        simple(
          format!("panel:{name}"),
          move |_| Meta {
            title: title.clone(),
            icon: Icon::Named(IconName::PanelTop),
            accessories: vec![crate::kit::tag(open)],
            ..Default::default()
          },
          move |_| {
            let name = name.clone();
            vec![Action::sync(
              "toggle",
              t!("app.launcher.tools.toggle_panel"),
              move |_, _, cx| {
                PanelState::show(&name, !open, cx)?;
                Ok(Outcome::close())
              },
            )]
          },
        )
      })
      .collect()
  }
}

fn control_center_pages(cx: &App) -> Vec<Rc<dyn Command>> {
  (cx.control_center().pages().iter())
    .map(|page| {
      let title = format!(
        "{} › {}",
        t!("app.launcher.tools.control_center"),
        (page.title)()
      );
      let (id, icon) = (page.id, (page.icon)());
      simple(
        format!("control_center:{id}"),
        move |_| Meta {
          title: title.clone().into(),
          icon: Icon::Tile(icon, 250.),
          keywords: vec!["control center".into(), id.into()],
          ..Default::default()
        },
        move |_| {
          vec![Action::sync(
            "open",
            t!("app.launcher.open"),
            move |_, _, cx| {
              OpenPage::handle(id.to_string(), cx)?;
              Ok(Outcome::close())
            },
          )]
        },
      )
    })
    .collect()
}

pub(crate) fn init(cx: &mut App) {
  let pages = cx.settings().ids();
  let mut commands: Vec<Rc<dyn Command>> = vec![
    simple(
      "settings",
      move |_| Meta {
        title: t!("app.launcher.tools.settings").into(),
        icon: Icon::Tile(IconName::Settings, 220.),
        keywords: vec!["preferences".into(), "config".into()],
        args: vec![crate::ArgSpec::choice(t!("app.launcher.tools.page"), pages.clone()).optional()],
        ..Default::default()
      },
      |_| {
        vec![Action::sync(
          "open",
          t!("app.launcher.open"),
          |args, _, cx| {
            corona_settings::open(args.text(0), cx)?;
            Ok(Outcome::close())
          },
        )]
      },
    ),
    opener(
      "panels",
      t!("app.launcher.tools.panels"),
      Icon::Tile(IconName::PanelTop, 250.),
      &["surface", "widget"],
      |_| {
        ListView::new(
          t!("app.launcher.tools.panels"),
          t!("app.launcher.tools.search_panels"),
          Rc::new(Panels),
        )
        .into_view()
      },
    ),
  ];
  commands.extend(control_center_pages(cx));
  registry::register_all(commands, cx);
  registry::register_source(Rc::new(SettingsPages), cx);
  registry::register_source(Rc::new(SettingFields), cx);
}

#[cfg(test)]
mod tests {
  #[test]
  fn finds_setting_fields() {
    let fields = super::setting_fields(&["window_switcher", "launcher"]);
    assert!(
      fields
        .iter()
        .any(|(p, f)| *p == "window_switcher" && *f == "card_height")
    );
    assert!(
      fields
        .iter()
        .any(|(p, f)| *p == "launcher" && *f == "close_on_unfocus")
    );
    assert!(!fields.iter().any(|(_, f)| *f == "groups" || *f == "title"));
    // only the pages asked for
    assert!(
      fields
        .iter()
        .all(|(p, _)| ["window_switcher", "launcher"].contains(p))
    );
  }
}

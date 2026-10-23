//! Screenshot, color picker, settings pages, panels, control center pages, notifications

use std::rc::Rc;

use corona_ipc::IpcCommand;
use corona_launcher::{
  Action, Command, CommandSource, Icon, ListView, Meta, Outcome, Query, Section, kit::opener,
  registry, simple,
};
use corona_notifications::NotificationsExt;
use corona_surface::panel::PanelState;
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

use crate::{
  commands::{control_center::OpenPage, notification::ClearHistory},
  control_center::variants::ControlCenterType,
  overlays::{
    colorpicker::commands::ColorPicker,
    screenshot::{commands::Screenshot, mode::Mode},
  },
  settings::{self, PAGES},
};

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

  fn commands(&self, _: &Query, _: &App) -> Vec<Rc<dyn Command>> {
    PAGES
      .iter()
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
            keywords: vec!["settings".into(), "preferences".into(), (*page).into()],
            section: Section::Commands,
            ..Default::default()
          },
          move |_| {
            vec![Action::sync(
              "open",
              t!("app.launcher.open"),
              move |_, _, cx| {
                settings::open(Some(page), cx)?;
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
            accessories: vec![corona_launcher::kit::tag(open)],
            ..Default::default()
          },
          move |_| {
            let name = name.clone();
            vec![Action::sync(
              "toggle",
              t!("app.launcher.tools.toggle_panel"),
              move |_, _, cx| {
                PanelState::show(&name, true, cx)?;
                Ok(Outcome::close())
              },
            )]
          },
        )
      })
      .collect()
  }
}

fn control_center_pages() -> Vec<Rc<dyn Command>> {
  ControlCenterType::iter()
    .map(|page| {
      let title = format!(
        "{} › {}",
        t!("app.launcher.tools.control_center"),
        page.title()
      );
      let icon = page.icon();
      simple(
        format!("control_center:{}", page.as_str()),
        move |_| Meta {
          title: title.clone().into(),
          icon: Icon::Tile(icon, 250.),
          keywords: vec!["control center".into(), page.as_str().into()],
          ..Default::default()
        },
        move |_| {
          vec![Action::sync(
            "open",
            t!("app.launcher.open"),
            move |_, _, cx| {
              OpenPage::handle(page.as_str().to_string(), cx)?;
              Ok(Outcome::close())
            },
          )]
        },
      )
    })
    .collect()
}

pub fn init(cx: &mut App) {
  let mut commands: Vec<Rc<dyn Command>> = vec![
    simple(
      "screenshot",
      |_| Meta {
        title: t!("app.launcher.tools.screenshot").into(),
        icon: Icon::Tile(IconName::Camera, 190.),
        keywords: vec!["capture".into(), "screen".into()],
        ..Default::default()
      },
      |_| {
        [
          (
            "selection",
            t!("app.launcher.tools.screenshot"),
            Mode::Selection,
          ),
          (
            "monitor",
            t!("app.launcher.tools.screenshot_monitor"),
            Mode::Monitor,
          ),
          (
            "window",
            t!("app.launcher.tools.screenshot_window"),
            Mode::Window,
          ),
        ]
        .into_iter()
        .map(|(id, title, mode)| {
          Action::sync(id, title, move |_, _, cx| {
            Screenshot::handle(mode, cx)?;
            Ok(Outcome::close())
          })
        })
        .collect()
      },
    ),
    simple(
      "colorpicker",
      |_| Meta {
        title: t!("app.launcher.tools.color_picker").into(),
        icon: Icon::Tile(IconName::Pipette, 320.),
        keywords: vec!["color".into(), "colour".into(), "eyedropper".into()],
        ..Default::default()
      },
      |_| {
        vec![Action::sync(
          "pick",
          t!("app.launcher.tools.color_picker"),
          |_, _, cx| {
            ColorPicker::handle((), cx)?;
            Ok(Outcome::close())
          },
        )]
      },
    ),
    simple(
      "settings",
      |_| Meta {
        title: t!("app.launcher.tools.settings").into(),
        icon: Icon::Tile(IconName::Settings, 220.),
        keywords: vec!["preferences".into(), "config".into()],
        args: vec![
          corona_launcher::ArgSpec::choice(t!("app.launcher.tools.page"), PAGES).optional(),
        ],
        ..Default::default()
      },
      |_| {
        vec![Action::sync(
          "open",
          t!("app.launcher.open"),
          |args, _, cx| {
            settings::open(args.text(0), cx)?;
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
    simple(
      "notification:show",
      |_| Meta {
        title: t!("app.launcher.tools.show_notifications").into(),
        icon: Icon::Tile(IconName::Bell, 280.),
        keywords: vec!["notification center".into()],
        ..Default::default()
      },
      |_| {
        vec![Action::sync("open", t!("app.launcher.open"), |_, _, cx| {
          OpenPage::handle(ControlCenterType::Notifications.as_str().into(), cx)?;
          Ok(Outcome::close())
        })]
      },
    ),
    simple(
      "notification:clear_history",
      |cx| Meta {
        title: t!("app.launcher.tools.clear_notifications").into(),
        subtitle: Some(
          t!(
            "app.launcher.tools.notification_count",
            count = cx.notifications().list(cx).len()
          )
          .into(),
        ),
        icon: Icon::Tile(IconName::BellMinus, 280.),
        keywords: vec!["notifications".into()],
        ..Default::default()
      },
      |_| {
        vec![
          Action::sync(
            "clear",
            t!("app.launcher.tools.clear_notifications"),
            |_, _, cx| {
              ClearHistory::handle((), cx)?;
              Ok(Outcome::hud(t!("app.launcher.tools.cleared")))
            },
          )
          .destructive(),
        ]
      },
    ),
  ];
  commands.extend(control_center_pages());
  registry::register_all(commands, cx);
  registry::register_source(Rc::new(SettingsPages), cx);
}

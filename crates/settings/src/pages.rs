//! The wallpaper, OSD, control center and system pages

use corona_config::OsdPosition;
use corona_surface::{
  control_center::{ControlCenterExt, DASHBOARD},
  dashboard::DashboardExt,
  osd::OsdExt,
};
use gpui_kit::{App, Axis, assets::IconName, component::setting::SettingPage};

use corona_components::settings_fields::{
  Choice, choice, non_empty, number,
  order::{OrderList, order_item},
  slider, switch, text,
};

use crate::{SettingsExt, SettingsPage, group, item, page, shared_groups, shared_items};

use rust_i18n::t;

// the order is the sidebar's; the appearance (0) and bar (10) pages are in lib.rs, the
// integrations' pages in their crates
settings_page!(WallpaperSettings, "wallpaper", 20, |cx| wallpaper());
settings_page!(OsdSettings, "osd", 40, |cx| osd(cx));
settings_page!(ControlCenterSettings, "control_center", 50, |cx| {
  control_center(cx)
});
settings_page!(SystemSettings, "system", 160, |cx| system(cx));

pub(crate) fn register(cx: &mut App) {
  cx.settings_mut()
    .register::<WallpaperSettings>()
    .register::<OsdSettings>()
    .register::<ControlCenterSettings>()
    .register::<SystemSettings>();
}

const OPACITY: (f32, f32, f32) = (0., 1., 0.05);

fn wallpaper() -> SettingPage {
  page(
    t!("app.settings.wallpaper.title"),
    IconName::Image,
    vec![group(
      t!("app.settings.wallpaper.groups.wallpaper"),
      vec![
        item(
          t!("app.settings.wallpaper.picture.title"),
          t!("app.settings.wallpaper.picture.description"),
          text(
            "wallpaper.path",
            |c| c.wallpaper.path.clone().unwrap_or_default(),
            |c, v| c.wallpaper.path = non_empty(v),
          ),
        )
        .layout(Axis::Vertical),
      ],
    )],
  )
}

fn osd(cx: &App) -> SettingPage {
  page(
    t!("app.settings.osd.title"),
    IconName::Gauge,
    vec![
      group(
        t!("app.settings.osd.groups.on_screen_display"),
        vec![
          item(
            t!("app.settings.osd.osd.title"),
            t!("app.settings.osd.osd.description"),
            switch(|c| c.osd.enabled, |c, v| c.osd.enabled = v),
          ),
          item(
            t!("app.settings.osd.position.title"),
            t!("app.settings.osd.position.description"),
            choice(
              &[
                (OsdPosition::TopCenter, t!("app.settings.options.top")),
                (OsdPosition::BottomCenter, t!("app.settings.options.bottom")),
                (OsdPosition::CenterLeft, t!("app.settings.options.left")),
                (OsdPosition::CenterRight, t!("app.settings.options.right")),
              ],
              |c| c.osd.position,
              |c, v| c.osd.position = v,
            ),
          ),
          item(
            t!("app.settings.osd.offset.title"),
            t!("app.settings.osd.offset.description"),
            number(
              "osd.offset",
              (0., 400., 1.),
              |c| c.osd.offset.into(),
              |c, v| c.osd.offset = v as f32,
            ),
          ),
          item(
            t!("app.settings.osd.hide_after.title"),
            t!("app.settings.osd.hide_after.description"),
            number(
              "osd.hide_delay_ms",
              (250., 10000., 250.),
              |c| c.osd.hide_delay_ms as f64,
              |c, v| c.osd.hide_delay_ms = v as u64,
            ),
          ),
          item(
            t!("app.settings.osd.background_opacity.title"),
            t!("app.settings.osd.background_opacity.description"),
            slider(
              "osd.background_opacity",
              OPACITY,
              |c| c.osd.background_opacity,
              |c, v| c.osd.background_opacity = v,
            ),
          ),
        ],
      ),
      group(
        t!("app.settings.osd.groups.shown_for"),
        cx.osds()
          .iter()
          .map(|kind| {
            let id = kind.id;
            item(
              (kind.title)(),
              (kind.description)(),
              switch(
                move |c| c.osd.kinds.get(id).copied().unwrap_or(true),
                move |c, v| {
                  c.osd.kinds.insert(id.to_string(), v);
                },
              ),
            )
          })
          .collect(),
      ),
    ],
  )
}

/// The dashboard's tiles on offer, plugins' too, by id with their titles
fn quick_action_choices(cx: &App) -> Vec<Choice> {
  (cx.dashboard().quick_actions().iter())
    .map(|a| Choice {
      value: a.id.to_string(),
      label: (a.title)(cx).to_string(),
    })
    .collect()
}

fn slider_choices(cx: &App) -> Vec<Choice> {
  (cx.dashboard().sliders().iter())
    .map(|s| Choice {
      value: s.id.to_string(),
      label: (s.title)(cx).to_string(),
    })
    .collect()
}

fn card_choices(cx: &App) -> Vec<Choice> {
  (cx.dashboard().cards().iter())
    .map(|c| Choice {
      value: c.id.to_string(),
      label: (c.title)(cx).to_string(),
    })
    .collect()
}

fn control_center(cx: &App) -> SettingPage {
  // the items other crates add, like the clock's formats, come first
  let mut items = shared_items(ControlCenterSettings::ID, "control_center", cx);
  items.extend([
    item(
      t!("app.settings.control_center.background_opacity.title"),
      t!("app.settings.control_center.background_opacity.description"),
      slider(
        "control_center.background_opacity",
        OPACITY,
        |c| c.control_center.background_opacity,
        |c, v| c.control_center.background_opacity = v,
      ),
    ),
    order_item([
      OrderList {
        key: "control_center.sidebar",
        title: || t!("app.settings.control_center.sidebar.title"),
        description: || t!("app.settings.control_center.sidebar.description"),
        options: |cx| {
          (cx.control_center().pages().iter())
            .filter(|p| p.id != DASHBOARD)
            .map(|p| Choice {
              value: p.id.into(),
              label: (p.title)().into(),
            })
            .collect()
        },
        get: |c| c.control_center.sidebar.clone(),
        set: |c, v| c.control_center.sidebar = v,
      },
      OrderList {
        key: "control_center.quick_actions",
        title: || t!("app.settings.control_center.quick_actions.title"),
        description: || t!("app.settings.control_center.quick_actions.description"),
        options: quick_action_choices,
        get: |c| c.control_center.quick_actions.clone(),
        set: |c, v| c.control_center.quick_actions = v,
      },
    ]),
    order_item([
      OrderList {
        key: "control_center.sliders",
        title: || t!("app.settings.control_center.sliders.title"),
        description: || t!("app.settings.control_center.sliders.description"),
        options: slider_choices,
        get: |c| c.control_center.sliders.clone(),
        set: |c, v| c.control_center.sliders = v,
      },
      OrderList {
        key: "control_center.cards",
        title: || t!("app.settings.control_center.cards.title"),
        description: || t!("app.settings.control_center.cards.description"),
        options: card_choices,
        get: |c| c.control_center.cards.clone(),
        set: |c, v| c.control_center.cards = v,
      },
    ]),
  ]);
  page(
    t!("app.settings.control_center.title"),
    IconName::LayoutDashboard,
    vec![group(
      t!("app.settings.control_center.groups.control_center"),
      items,
    )],
  )
}

fn system(cx: &App) -> SettingPage {
  // only the integrations' groups, like the avatar from auth and night light from
  // brightness
  page(
    t!("app.settings.system.title"),
    IconName::Cpu,
    shared_groups(SystemSettings::ID, cx),
  )
}

#[cfg(test)]
mod tests {
  use std::rc::Rc;

  use corona_surface::dashboard::{CardEntry, QuickActionEntry, SliderEntry, Tile, percent};
  use gpui_kit::{self as gpui, TestAppContext, assets::IconName};

  use super::*;

  #[gpui::test]
  fn the_dashboard_lists_offer_what_is_registered(cx: &mut TestAppContext) {
    cx.update(|cx| {
      assert!(quick_action_choices(cx).is_empty());
      cx.dashboard_mut()
        .quick_action_entry(QuickActionEntry {
          id: "p:lamp".into(),
          title: Rc::new(|_| "Lamp".into()),
          tile: Rc::new(|_| Tile {
            icon: IconName::Sun,
            status: "".into(),
            active: false,
            toggle: None,
          }),
          secondary: None,
          observe: Rc::new(|_, _| vec![]),
        })
        .slider_entry(SliderEntry {
          id: "p:heat".into(),
          title: Rc::new(|_| "Heat".into()),
          throttle: None,
          step: 0.01,
          icon: Rc::new(|_| IconName::Sun),
          value: Rc::new(|_| None),
          label: Rc::new(|v, _| percent(v)),
          set: Rc::new(|_, _| {}),
          muted: Rc::new(|_| None),
          toggle_mute: Rc::new(|_| {}),
          secondary: None,
          observe: Rc::new(|_, _| vec![]),
        })
        .card_entry(CardEntry {
          id: "p:card".into(),
          title: Rc::new(|_| "Card".into()),
          open: Rc::new(|_, _| unreachable!()),
        });
      let pairs = |choices: Vec<Choice>| -> Vec<(String, String)> {
        choices.into_iter().map(|c| (c.value, c.label)).collect()
      };
      let pair = |a: &str, b: &str| vec![(a.to_string(), b.to_string())];
      assert_eq!(pairs(quick_action_choices(cx)), pair("p:lamp", "Lamp"));
      assert_eq!(pairs(slider_choices(cx)), pair("p:heat", "Heat"));
      assert_eq!(pairs(card_choices(cx)), pair("p:card", "Card"));
    });
  }
}

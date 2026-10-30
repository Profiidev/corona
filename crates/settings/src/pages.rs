//! The wallpaper, OSD, control center and system pages

use corona_config::{OsdPosition, Weekday};
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

use crate::{SettingsExt, SettingsPage, group, item, page, shared_group};

use rust_i18n::t;

/// A settings page `$page` builds
macro_rules! settings_page {
  ($name:ident, $id:literal, $order:literal, |$cx:ident| $page:expr) => {
    pub struct $name;

    impl SettingsPage for $name {
      const ID: &'static str = $id;
      const ORDER: i32 = $order;

      #[allow(unused_variables)]
      fn page($cx: &App) -> SettingPage {
        $page
      }
    }
  };
}

// the order is the sidebar's; the appearance (0) and bar (10) pages are in lib.rs, the
// integrations' pages in their crates
settings_page!(WallpaperSettings, "wallpaper", 20, |cx| wallpaper());
settings_page!(OsdSettings, "osd", 40, |cx| osd(cx));
settings_page!(ControlCenterSettings, "control_center", 50, |cx| {
  control_center()
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

fn control_center() -> SettingPage {
  page(
    t!("app.settings.control_center.title"),
    IconName::LayoutDashboard,
    vec![group(
      t!("app.settings.control_center.groups.control_center"),
      vec![
        item(
          t!("app.settings.control_center.time_format.title"),
          t!("app.settings.control_center.time_format.description"),
          text(
            "control_center.time_format",
            |c| c.control_center.time_format.clone(),
            |c, v| c.control_center.time_format = v,
          ),
        ),
        item(
          t!("app.settings.control_center.date_format.title"),
          t!("app.settings.control_center.date_format.description"),
          text(
            "control_center.date_format",
            |c| c.control_center.date_format.clone(),
            |c, v| c.control_center.date_format = v,
          ),
        ),
        item(
          t!("app.settings.control_center.week_starts_on.title"),
          t!("app.settings.control_center.week_starts_on.description"),
          choice(
            &[
              (Weekday::Monday, t!("app.settings.options.monday")),
              (Weekday::Sunday, t!("app.settings.options.sunday")),
            ],
            |c| c.control_center.week_start,
            |c, v| c.control_center.week_start = v,
          ),
        ),
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
            options: |cx| {
              (cx.dashboard().quick_actions().iter())
                .map(|a| Choice {
                  value: a.id.into(),
                  label: (a.title)().into(),
                })
                .collect()
            },
            get: |c| c.control_center.quick_actions.clone(),
            set: |c, v| c.control_center.quick_actions = v,
          },
        ]),
        order_item([
          OrderList {
            key: "control_center.sliders",
            title: || t!("app.settings.control_center.sliders.title"),
            description: || t!("app.settings.control_center.sliders.description"),
            options: |cx| {
              (cx.dashboard().sliders().iter())
                .map(|s| Choice {
                  value: s.id.into(),
                  label: (s.title)().into(),
                })
                .collect()
            },
            get: |c| c.control_center.sliders.clone(),
            set: |c, v| c.control_center.sliders = v,
          },
          OrderList {
            key: "control_center.cards",
            title: || t!("app.settings.control_center.cards.title"),
            description: || t!("app.settings.control_center.cards.description"),
            options: |cx| {
              (cx.dashboard().cards().iter())
                .map(|c| Choice {
                  value: c.id.into(),
                  label: (c.title)().into(),
                })
                .collect()
            },
            get: |c| c.control_center.cards.clone(),
            set: |c, v| c.control_center.cards = v,
          },
        ]),
      ],
    )],
  )
}

fn system(cx: &App) -> SettingPage {
  // the integrations' groups: the avatar from auth, night light and polling from
  // brightness and sysinfo
  let shared = [
    ("profile", t!("app.settings.system.groups.profile")),
    ("nightlight", t!("app.settings.system.groups.nightlight")),
    ("polling", t!("app.settings.system.groups.polling")),
    (
      "applies_after_a_restart",
      t!("app.settings.system.groups.applies_after_a_restart"),
    ),
  ]
  .into_iter()
  .filter_map(|(id, title)| shared_group(title, SystemSettings::ID, id, cx));
  page(
    t!("app.settings.system.title"),
    IconName::Cpu,
    shared.collect(),
  )
}

use crate::control_center::dashboard::DashboardPanel;
use corona_components::assets::toggle_mode;
use corona_config::ConfigProvider;
use corona_idle::{Idle, IdleExt};
use corona_surface::{
  control_center::ControlCenter,
  dashboard::{Changed, DashboardExt, DashboardRegistry, QuickAction, QuickActionEntry, Tile},
};
use gpui_kit::{
  App, BorrowAppContext, ClickEvent, Div, ElementId, InteractiveElement, MouseButton,
  ParentElement, SharedString, Styled, Subscription, Window,
  assets::IconName,
  base::Disableable,
  component::{
    ActiveTheme, Icon, Sizable,
    button::{Button, ButtonVariant, ButtonVariants},
  },
  div,
  prelude::FluentBuilder,
  px,
};
use rust_i18n::t;

const COLUMNS: usize = 3;

type Action = Box<dyn Fn(&mut App)>;

struct Toggle {
  icon: IconName,
  status: String,
  active: bool,
  on_click: Option<Action>,
}

/// A quick action whose tile `$build` makes, re-rendered when the entities
/// `$observe` names notify
macro_rules! action {
  (
    $name:ident, $id:literal, $title:literal, $build:ident, $page:expr,
    |$cx:ident| [$($entity:expr),* $(,)?]
  ) => {
    pub struct $name;

    impl QuickAction for $name {
      const ID: &'static str = $id;

      fn title() -> SharedString {
        t!($title).into()
      }

      fn tile(cx: &App) -> Tile {
        let toggle = $build(cx);
        Tile {
          icon: toggle.icon,
          status: toggle.status.into(),
          active: toggle.active,
          toggle: toggle.on_click,
        }
      }

      fn page() -> Option<&'static str> {
        $page
      }

      #[allow(unused_variables)]
      fn observe(changed: Changed, $cx: &mut App) -> Vec<Subscription> {
        vec![$({
          let (entity, changed) = ($entity.clone(), changed.clone());
          $cx.observe(&entity, move |_, cx| changed(cx))
        }),*]
      }
    }
  };
}

// a theme change redraws every window anyway
action!(
  DarkMode,
  "dark_mode",
  "app.dashboard.dark_mode",
  dark_mode,
  None,
  |cx| []
);
action!(
  StayAwake,
  "stay_awake",
  "app.dashboard.stay_awake",
  stay_awake,
  None,
  |cx| [cx.idle().stay_awake,]
);
/// Registers every tile, in the order the settings offer them
pub(crate) fn register(cx: &mut App) {
  cx.dashboard_mut()
    .quick_action::<DarkMode>()
    .quick_action::<StayAwake>();
}

/// The configured tiles, None for none
pub(super) fn toggles(cx: &mut gpui_kit::Context<DashboardPanel>) -> Option<Div> {
  let registry = cx.dashboard();
  let (actions, _) = DashboardRegistry::pick(
    registry.quick_actions(),
    &cx.config().control_center.quick_actions,
    |a| a.id,
  );
  if actions.is_empty() {
    return None;
  }
  let tiles: Vec<_> = (actions.into_iter())
    .map(|action| tile(action, (action.tile)(cx)))
    .collect();

  let mut rows = div().flex().flex_col().gap(px(6.));
  let mut tiles = tiles.into_iter().peekable();
  while tiles.peek().is_some() {
    let row: Vec<_> = tiles
      .by_ref()
      .take(COLUMNS)
      .map(|tile| div().flex_1().min_w_0().child(tile))
      .collect();
    rows = rows.child(div().flex().gap(px(6.)).children(row));
  }
  Some(rows)
}

fn tile(action: &QuickActionEntry, tile: Tile) -> Button {
  let page = action.page;
  let enabled = tile.toggle.is_some();

  Button::new(ElementId::Name(format!("toggle-{}", action.id).into()))
    .with_variant(match tile.active {
      true => ButtonVariant::Primary,
      false => ButtonVariant::Default,
    })
    .icon(Icon::new(tile.icon).small())
    .disabled(!enabled)
    .w_full()
    .h_auto()
    .py_1()
    .rounded_xl()
    .child(
      div()
        .flex_1()
        .flex()
        .flex_col()
        .min_w_0()
        .child(div().text_sm().truncate().child((action.title)()))
        .child(div().text_xs().opacity(0.7).truncate().child(tile.status)),
    )
    .when_some(tile.toggle, |b, toggle| {
      b.cursor_pointer()
        .on_click(move |_: &ClickEvent, _, cx| toggle(cx))
    })
    .when_some(page, |b, page| {
      b.on_mouse_down(MouseButton::Right, move |_, window: &mut Window, cx| {
        ControlCenter::navigate_id(page, window, cx)
      })
    })
}

fn on_off(on: bool) -> String {
  match on {
    true => t!("app.common.on"),
    false => t!("app.common.off"),
  }
  .into()
}

fn dark_mode(cx: &App) -> Toggle {
  let dark = cx.theme().is_dark();
  Toggle {
    icon: if dark { IconName::Moon } else { IconName::Sun },
    status: on_off(dark),
    active: dark,
    on_click: Some(Box::new(toggle_mode)),
  }
}

fn stay_awake(cx: &App) -> Toggle {
  let on = cx.idle().stay_awake(cx);
  Toggle {
    icon: IconName::Coffee,
    status: on_off(on),
    active: on,
    on_click: Some(Box::new(move |cx| {
      cx.update_global::<Idle, _>(|idle, cx| idle.set_stay_awake(!on, None, cx))
    })),
  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn on_off_labels() {
    assert_eq!(super::on_off(true), "On");
    assert_eq!(super::on_off(false), "Off");
  }
}

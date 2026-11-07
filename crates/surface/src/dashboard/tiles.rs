use corona_components::assets::toggle_mode;
use corona_config::ConfigProvider;

use crate::{
  control_center::ControlCenter,
  dashboard::{
    DashboardExt, DashboardPanel, DashboardRegistry, QuickAction, QuickActionEntry, Tile, on_off,
  },
};
use gpui_kit::{
  App, ClickEvent, Div, ElementId, InteractiveElement, MouseButton, ParentElement, SharedString,
  Styled, Window,
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

// a theme change redraws every window anyway
pub struct DarkMode;

impl QuickAction for DarkMode {
  const ID: &'static str = "dark_mode";

  fn title() -> SharedString {
    t!("app.dashboard.dark_mode").into()
  }

  fn tile(cx: &App) -> Tile {
    let dark = cx.theme().is_dark();
    Tile {
      icon: if dark { IconName::Moon } else { IconName::Sun },
      status: on_off(dark),
      active: dark,
      toggle: Some(Box::new(toggle_mode)),
    }
  }
}

/// The configured tiles, None for none
pub(super) fn tiles(cx: &mut gpui_kit::Context<DashboardPanel>) -> Option<Div> {
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
    .cursor_pointer()
    .when(!enabled, |this| this.cursor_not_allowed())
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

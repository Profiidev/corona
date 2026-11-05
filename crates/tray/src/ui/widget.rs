use std::{cell::Cell, collections::HashMap, rc::Rc};

use crate::{Orientation, TrayExt, TrayItem};
use corona_surface::bar::{BarStyle, Widget};
use gpui_kit::{
  Bounds, Context, Empty, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
  ParentElement, Pixels, Render, ScrollEvent, Styled, StyledImage, Subscription, Window,
  assets::IconName,
  base::ElementExt,
  component::{ActiveTheme, Icon, Sizable},
  div, img, px,
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use corona_components::filter::filter_regex;
use corona_utils::error::spawn_logged;

use crate::ui::menu;

const ICON_SIZE: f32 = 16.;
const SLOT_SIZE: f32 = 24.;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
  /// Regexes, case-insensitive, against an item's id and title; a match hides it
  pub blacklist: Vec<String>,
}

pub struct Tray {
  blacklist: Vec<Regex>,
  bounds: HashMap<String, Rc<Cell<Bounds<Pixels>>>>,
  /// Scrolled pixels not yet worth a step, per address and axis (true = vertical)
  scrolled: HashMap<(String, bool), f32>,
  _subscription: Subscription,
}

impl Tray {
  fn hidden(&self, item: &TrayItem) -> bool {
    hidden(&self.blacklist, &item.id, item.title.as_deref())
  }
}

/// Whether any regex matches the item's id or title
fn hidden(blacklist: &[Regex], id: &str, title: Option<&str>) -> bool {
  blacklist
    .iter()
    .any(|re| re.is_match(id) || title.is_some_and(|t| re.is_match(t)))
}

impl Widget for Tray {
  const NAME: &'static str = "tray";

  type Options = Options;

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid, options: Options) -> Self {
    let items = cx.tray().items.clone();
    Self {
      blacklist: options
        .blacklist
        .iter()
        .filter_map(|p| filter_regex(p))
        .collect(),
      bounds: HashMap::new(),
      scrolled: HashMap::new(),
      _subscription: cx.observe(&items, |this, items, cx| {
        let items = items.read(cx);
        this
          .bounds
          .retain(|address, _| items.iter().any(|i| &i.address == address));
        this
          .scrolled
          .retain(|(address, _), _| items.iter().any(|i| &i.address == address));
        cx.notify();
      }),
    }
  }
}

impl Tray {
  fn on_press(
    &mut self,
    address: &str,
    button: MouseButton,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let Some(item) = cx.tray().item(address, cx).cloned() else {
      return;
    };
    let has_menu = !item.menu.is_empty();
    let anchor = self
      .bounds
      .get(address)
      .map(|b| b.get())
      .unwrap_or_default();
    let tray = cx.tray().clone();
    // the icon's screen position, for apps that place their windows or menus from it
    let origin = window.bounds().origin;
    let (x, y) = (
      (origin.x + anchor.center().x).as_f32() as i32,
      (origin.y + anchor.center().y).as_f32() as i32,
    );

    match button {
      MouseButton::Left if item.item_is_menu || !item.can_activate => {
        if has_menu {
          menu::open(item.address, anchor, window, cx)
        } else {
          let action = tray.context_menu(address, x, y, cx);
          spawn_logged(cx, action)
        }
      }
      MouseButton::Left => {
        let action = tray.activate(address, x, y, cx);
        spawn_logged(cx, action)
      }
      MouseButton::Middle => {
        let action = tray.secondary_activate(address, x, y, cx);
        spawn_logged(cx, action)
      }
      MouseButton::Right if has_menu => menu::open(item.address, anchor, window, cx),
      MouseButton::Right => {
        let action = tray.context_menu(address, x, y, cx);
        spawn_logged(cx, action)
      }
      _ => {}
    }
  }

  fn icon(&mut self, item: &TrayItem, cx: &mut Context<Self>) -> impl IntoElement + use<> {
    let bounds = self.bounds.entry(item.address.clone()).or_default().clone();
    let address = item.address.clone();
    let hover = cx.theme().tokens.button_hover;
    let fallback = || {
      Icon::new(IconName::AppWindow)
        .with_size(px(ICON_SIZE))
        .into_any_element()
    };

    let press = |button: MouseButton| {
      let address = address.clone();
      cx.listener(move |this, _: &MouseDownEvent, window, cx| {
        this.on_press(&address, button, window, cx)
      })
    };

    div()
      .id(item.address.clone())
      .relative()
      .size(px(SLOT_SIZE))
      .flex()
      .items_center()
      .justify_center()
      .rounded_full()
      .cursor_pointer()
      .hover(|d| d.bg(hover))
      .child(match &item.icon {
        Some(path) => img(path.clone())
          .size(px(ICON_SIZE))
          .with_fallback(fallback)
          .into_any_element(),
        None => fallback(),
      })
      .on_mouse_down(MouseButton::Left, press(MouseButton::Left))
      .on_mouse_down(MouseButton::Right, press(MouseButton::Right))
      .on_mouse_down(MouseButton::Middle, press(MouseButton::Middle))
      .on_scroll(cx.listener({
        let address = address.clone();
        move |this, e: &ScrollEvent, window, cx| {
          let line = window.line_height();
          let delta = e.delta.pixel_delta(line);
          let (delta, orientation) = if delta.y.abs() >= delta.x.abs() {
            (delta.y.as_f32(), Orientation::Vertical)
          } else {
            (delta.x.as_f32(), Orientation::Horizontal)
          };
          let line = line.as_f32();
          let pending = this
            .scrolled
            .entry((
              address.clone(),
              matches!(orientation, Orientation::Vertical),
            ))
            .or_default();
          *pending += delta;
          let step = (*pending / line).trunc();
          *pending -= step * line;
          if step != 0. {
            let step = step as i32;
            let action = cx.tray().scroll(&address, step, orientation, cx);
            spawn_logged(cx, action);
          }
        }
      }))
      .on_prepaint(move |b, _, _| bounds.set(b))
  }
}

impl Render for Tray {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let items: Vec<TrayItem> = cx
      .tray()
      .list_items(cx)
      .iter()
      .filter(|item| !self.hidden(item))
      .cloned()
      .collect();
    if items.is_empty() {
      return Empty.into_any_element();
    }

    div()
      .id("tray")
      .bar_pill(window, cx)
      .px_0()
      .children(
        items
          .iter()
          .map(|item| self.icon(item, cx))
          .collect::<Vec<_>>(),
      )
      .into_any_element()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn hidden() {
    let blacklist: Vec<_> = ["^nm-applet$", "steam"]
      .iter()
      .filter_map(|p| filter_regex(p))
      .collect();
    assert!(super::hidden(&blacklist, "nm-applet", None));
    assert!(!super::hidden(&blacklist, "nm-applet-2", None));
    // titles match too, case-insensitive
    assert!(super::hidden(
      &blacklist,
      "chrome_status_icon_1",
      Some("Steam")
    ));
    assert!(!super::hidden(&blacklist, "discord", Some("Discord")));
    assert!(!super::hidden(&[], "steam", Some("steam")));
  }
}

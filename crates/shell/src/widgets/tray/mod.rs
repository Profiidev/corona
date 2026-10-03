use std::{cell::Cell, collections::HashMap, rc::Rc};

use corona_surface::bar::{BarStyle, Widget};
use corona_tray::{Orientation, TrayExt, TrayItem};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  AppContext, Bounds, Context, Empty, InteractiveElement, IntoElement, MouseButton, MouseDownEvent,
  ParentElement, Pixels, Render, ScrollWheelEvent, SharedString, Styled, StyledImage, Subscription,
  Window,
  assets::IconName,
  base::ElementExt,
  component::{ActiveTheme, Icon, Sizable},
  div, img, px,
};
use uuid::Uuid;

mod menu;

const ICON_SIZE: f32 = 16.;
const SLOT_SIZE: f32 = 24.;

pub struct Tray {
  bounds: HashMap<String, Rc<Cell<Bounds<Pixels>>>>,
  _subscription: Subscription,
}

impl Widget for Tray {
  const NAME: &'static str = "tray";

  type Options = ();

  fn init(cx: &mut Context<'_, Self>, _display_id: Uuid, _options: ()) -> Self {
    let items = cx.tray().items.clone();
    Self {
      bounds: HashMap::new(),
      _subscription: cx.observe(&items, |this, items, cx| {
        let items = items.read(cx);
        this
          .bounds
          .retain(|address, _| items.iter().any(|i| &i.address == address));
        cx.notify();
      }),
    }
  }
}

fn run(cx: &mut Context<Tray>, action: impl Future<Output = anyhow::Result<()>> + Send + 'static) {
  cx.background_spawn(async move {
    let _ = action.await.log_err();
  })
  .detach();
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

    match button {
      MouseButton::Left if (item.item_is_menu || !item.can_activate) && has_menu => {
        menu::open(item.address, anchor, window, cx)
      }
      MouseButton::Left => run(cx, tray.activate(address, 0, 0)),
      MouseButton::Middle => run(cx, tray.secondary_activate(address, 0, 0)),
      MouseButton::Right if has_menu => menu::open(item.address, anchor, window, cx),
      MouseButton::Right => run(cx, tray.context_menu(address, 0, 0)),
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
      .id(SharedString::from(item.address.clone()))
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
      .on_scroll_wheel(cx.listener({
        let address = address.clone();
        move |_, e: &ScrollWheelEvent, window, cx| {
          let delta = e.delta.pixel_delta(window.line_height());
          let (delta, orientation) = if delta.y.abs() >= delta.x.abs() {
            (delta.y.as_f32(), Orientation::Vertical)
          } else {
            (delta.x.as_f32(), Orientation::Horizontal)
          };
          if delta != 0. {
            let step = if delta > 0. { 1 } else { -1 };
            let action = cx.tray().scroll(&address, step, orientation);
            run(cx, action);
          }
        }
      }))
      .on_prepaint(move |b, _, _| bounds.set(b))
  }
}

impl Render for Tray {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let items = cx.tray().list_items(cx).to_vec();
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

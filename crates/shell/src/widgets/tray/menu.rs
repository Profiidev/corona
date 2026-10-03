use corona_tray::{MenuItem, Toggle, TrayExt};

use crate::widgets::popup::{self, ROW, SEPARATOR};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  App, AppContext, Bounds, Context, FocusHandle, Focusable, InteractiveElement, IntoElement,
  KeyDownEvent, ParentElement, Pixels, Render, SharedString, Size, StatefulInteractiveElement,
  Styled, Subscription, Window,
  assets::IconName,
  component::{ActiveTheme, Icon, Sizable},
  div,
  prelude::FluentBuilder,
  px,
};

fn spawn(cx: &mut App, action: impl Future<Output = anyhow::Result<()>> + Send + 'static) {
  cx.background_spawn(async move {
    let _ = action.await.log_err();
  })
  .detach();
}

pub fn open(address: String, anchor: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
  let tray = cx.tray().clone();
  let about = tray.about_to_show(&address, 0, cx);
  spawn(cx, about);

  let Some(item) = tray.item(&address, cx) else {
    return;
  };
  let size = size(&item.menu, false);
  popup::open(anchor, size, window, cx, |window, cx| {
    TrayMenu::new(address, window, cx)
  });
}

fn visible(entries: &[MenuItem]) -> impl Iterator<Item = &MenuItem> {
  entries.iter().filter(|e| e.visible)
}

fn size(entries: &[MenuItem], back: bool) -> Size<Pixels> {
  let rows: f32 = visible(entries)
    .map(|e| if e.separator { SEPARATOR } else { ROW })
    .sum();
  let back = if back { ROW + SEPARATOR } else { 0. };
  popup::size(rows + back)
}

struct TrayMenu {
  address: String,
  path: Vec<i32>,
  focus: FocusHandle,
  _subscription: Subscription,
}

impl TrayMenu {
  fn new(address: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
    let items = cx.tray().items.clone();
    let subscription = cx.observe_in(&items, window, |this, _, window, cx| {
      match this.entries(cx) {
        Some(entries) => window.resize(size(&entries, !this.path.is_empty())),
        None => window.remove_window(),
      }
      cx.notify();
    });
    Self {
      address,
      path: Vec::new(),
      focus: cx.focus_handle(),
      _subscription: subscription,
    }
  }

  fn entries(&self, cx: &App) -> Option<Vec<MenuItem>> {
    let mut entries = &cx.tray().item(&self.address, cx)?.menu;
    for id in &self.path {
      entries = &entries.iter().find(|e| e.id == *id)?.children;
    }
    Some(entries.clone())
  }

  fn navigate(&mut self, path: Vec<i32>, window: &mut Window, cx: &mut Context<Self>) {
    self.path = path;
    if let Some(&id) = self.path.last() {
      let about = cx.tray().about_to_show(&self.address, id, cx);
      spawn(cx, about);
    }
    if let Some(entries) = self.entries(cx) {
      window.resize(size(&entries, !self.path.is_empty()));
    }
    cx.notify();
  }

  fn entry(&self, entry: &MenuItem, cx: &Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    if entry.separator {
      return popup::separator(cx).into_any_element();
    }

    let checked = matches!(entry.toggle, Toggle::Checkmark(true) | Toggle::Radio(true));
    let toggles = entry.toggle != Toggle::None;
    let submenu = !entry.children.is_empty();
    let (id, enabled) = (entry.id, entry.enabled);
    let hover = theme.tokens.button_hover;

    popup::row(SharedString::from(format!("entry-{id}")), cx)
      .when(!enabled, |d| d.text_color(theme.muted_foreground))
      .when(enabled, |d| d.cursor_pointer().hover(|d| d.bg(hover)))
      .when(toggles, |d| {
        d.child(div().size(px(14.)).flex_none().when(checked, |d| {
          d.child(Icon::new(IconName::Check).with_size(px(14.)))
        }))
      })
      .child(
        div()
          .flex_1()
          .min_w_0()
          .truncate()
          .child(entry.label.clone()),
      )
      .when(submenu, |d| {
        d.child(Icon::new(IconName::ChevronRight).with_size(px(14.)))
      })
      .when(enabled, |d| {
        d.on_click(cx.listener(move |this, _, window, cx| {
          if submenu {
            let mut path = this.path.clone();
            path.push(id);
            this.navigate(path, window, cx);
          } else {
            let click = cx.tray().menu_click(&this.address, id, cx);
            spawn(cx, click);
            window.remove_window();
          }
        }))
      })
      .into_any_element()
  }
}

impl Focusable for TrayMenu {
  fn focus_handle(&self, _: &App) -> FocusHandle {
    self.focus.clone()
  }
}

impl Render for TrayMenu {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    let entries = self.entries(cx).unwrap_or_default();
    let hover = theme.tokens.button_hover;

    popup::frame(self, cx)
      .on_key_down(cx.listener(|this, e: &KeyDownEvent, window, cx| {
        if e.keystroke.key == "backspace" && !this.path.is_empty() {
          let mut path = this.path.clone();
          path.pop();
          this.navigate(path, window, cx);
        }
      }))
      .when(!self.path.is_empty(), |d| {
        d.child(
          popup::row("back", cx)
            .cursor_pointer()
            .hover(|d| d.bg(hover))
            .child(Icon::new(IconName::ChevronLeft).with_size(px(14.)))
            .child("Back")
            .on_click(cx.listener(|this, _, window, cx| {
              let mut path = this.path.clone();
              path.pop();
              this.navigate(path, window, cx);
            })),
        )
        .child(popup::separator(cx))
      })
      .children(visible(&entries).map(|e| self.entry(e, cx)))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn entry(separator: bool, visible: bool) -> MenuItem {
    MenuItem {
      id: 0,
      label: String::new(),
      separator,
      enabled: true,
      visible,
      toggle: Toggle::None,
      icon_name: None,
      children: Vec::new(),
    }
  }

  #[test]
  fn menu_size() {
    let entries = [entry(false, true), entry(true, true), entry(false, false)];
    assert_eq!(size(&entries, false), popup::size(ROW + SEPARATOR));
    assert_eq!(size(&entries, true), popup::size(ROW * 2. + SEPARATOR * 2.));
  }
}

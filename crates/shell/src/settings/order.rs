//! An ordered list of ids, like the control center's sidebar: rows dragged to
//! reorder them, removed, and added from the ids on offer.

use std::{borrow::Cow, cell::Cell, rc::Rc};

use corona_config::{Config, ConfigProvider};
use gpui_kit::{
  App, AppContext, Div, InteractiveElement, IntoElement, ParentElement, Pixels, Render,
  StatefulInteractiveElement, Styled, Window,
  base::ElementExt,
  component::{ActiveTheme, Sizable, setting::SettingItem},
  div, px,
};
use rust_i18n::t;

use crate::settings::{
  bar::{row_body, row_buttons},
  fields::{Choice, save, searchable},
};

/// Moves the entry at `from` in front of the one at `before`, to the end for
/// `list.len()`
pub(crate) fn move_entry(list: &mut Vec<String>, from: usize, before: usize) {
  if from >= list.len() {
    return;
  }
  let entry = list.remove(from);
  let at = if before > from { before - 1 } else { before };
  list.insert(at.min(list.len()), entry);
}

#[derive(Clone)]
struct OrderDrag {
  key: &'static str,
  from: usize,
  label: String,
  /// the dragged row's width, for a preview that looks the same
  width: Rc<Cell<Pixels>>,
}

struct DragPreview(OrderDrag);

impl Render for DragPreview {
  fn render(&mut self, _: &mut Window, cx: &mut gpui_kit::Context<Self>) -> impl IntoElement {
    let drag = &self.0;
    let (_, remove) = row_buttons(|what| format!("preview-{what}"), false);
    row_body(drag.label.clone(), None::<Div>, remove, cx)
      .w(drag.width.get())
      .opacity(0.9)
  }
}

/// One list of an [`order_item`]. `options` are the ids that may be added, with
/// their labels; an id in the list that is not one of them shows as it is written
#[derive(Clone, Copy)]
pub(super) struct OrderList {
  pub key: &'static str,
  pub title: fn() -> Cow<'static, str>,
  pub description: fn() -> Cow<'static, str>,
  pub options: fn() -> Vec<Choice>,
  pub get: fn(&Config) -> Vec<String>,
  pub set: fn(&mut Config, Vec<String>),
}

/// The lists side by side, each with its title above
pub(super) fn order_item<const N: usize>(lists: [OrderList; N]) -> SettingItem {
  SettingItem::render(move |_, window, cx| {
    let mut columns = div().flex().gap_2().w_full();
    for list in lists {
      columns = columns.child(editor(list, window, cx));
    }
    div()
      .flex()
      .flex_col()
      .gap_2()
      .w_full()
      .child(
        div()
          .text_sm()
          .text_color(cx.theme().muted_foreground)
          .child(t!("app.settings.control_center.drag_hint")),
      )
      .child(columns)
  })
  .keywords(lists.map(|l| l.key))
}

fn editor(list: OrderList, window: &mut Window, cx: &mut App) -> Div {
  let OrderList {
    key,
    title,
    description,
    options,
    get,
    set,
  } = list;
  let entries = get(cx.config());
  let choices = options();
  let theme = cx.theme();
  let (radius, accent, hover, line) = (
    theme.radius,
    theme.colors.accent,
    theme.tokens.button_hover,
    theme.colors.primary,
  );
  let edit = move |cx: &mut App, f: &dyn Fn(&mut Vec<String>)| {
    save(cx, |c| {
      let mut list = get(c);
      f(&mut list);
      set(c, list);
    })
  };
  let gap = |index: usize| {
    div()
      .id(format!("{key}-gap-{index}"))
      .h(px(8.))
      .rounded_full()
      .drag_over::<OrderDrag>(move |style, _, _, _| style.bg(line.opacity(0.5)))
      .on_drop(move |drag: &OrderDrag, _, cx| {
        if drag.key == key {
          let from = drag.from;
          edit(cx, &|l| move_entry(l, from, index));
        }
      })
  };

  let mut rows: Vec<gpui_kit::AnyElement> = Vec::new();
  for (index, id) in entries.iter().enumerate() {
    let label = choices
      .iter()
      .find(|c| &c.value == id)
      .map_or_else(|| id.clone(), |c| c.label.clone());
    let row_id = |what: &str| format!("{what}-{key}-{index}");
    let width =
      window.use_keyed_state(row_id("width"), cx, |_, _| Rc::new(Cell::new(Pixels::ZERO)));
    let width = width.read(cx).clone();
    let drag = OrderDrag {
      key,
      from: index,
      label: label.clone(),
      width: width.clone(),
    };
    let (_, remove) = row_buttons(row_id, false);
    let remove = remove.on_click(move |_, _, cx| {
      edit(cx, &|l| {
        l.remove(index);
      })
    });
    rows.push(gap(index).into_any_element());
    rows.push(
      row_body(label, None::<Div>, remove, cx)
        .id(row_id("row"))
        .cursor_grab()
        .on_prepaint(move |bounds, _, _| width.set(bounds.size.width))
        .on_drag(drag, |drag: &OrderDrag, _, _, cx| {
          cx.new(|_| DragPreview(drag.clone()))
        })
        .into_any_element(),
    );
  }

  let add = searchable(
    format!("add-{key}"),
    choices,
    None,
    t!("app.settings.control_center.add"),
    window,
    cx,
    move |id, cx| {
      edit(cx, &|l| {
        if !l.contains(&id) {
          l.push(id.clone());
        }
      })
    },
  )
  .xsmall()
  .w_full()
  .menu_width(px(220.));

  let muted = cx.theme().muted_foreground;
  div()
    .flex_1()
    .min_w_0()
    .flex()
    .flex_col()
    .gap_1()
    .child(
      div()
        .text_sm()
        .font_weight(gpui_kit::FontWeight::BOLD)
        .child(title()),
    )
    .child(div().text_xs().text_color(muted).child(description()))
    .child(
      div()
        .flex_1()
        .flex()
        .flex_col()
        .p_2()
        .rounded(radius)
        .bg(accent)
        .child(div().w_full().mb_1().child(add))
        .children(rows)
        // the rest takes drops at the end
        .child(
          div()
            .id(format!("{key}-end"))
            .min_h(px(32.))
            .flex_1()
            .rounded(radius)
            .drag_over::<OrderDrag>(move |style, _, _, _| style.bg(hover))
            .on_drop(move |drag: &OrderDrag, _, cx| {
              if drag.key == key {
                let from = drag.from;
                edit(cx, &|l| {
                  let end = l.len();
                  move_entry(l, from, end)
                });
              }
            }),
        ),
    )
}

#[cfg(test)]
mod tests {
  use super::move_entry;

  fn moved(from: usize, before: usize) -> Vec<String> {
    let mut list = ["a", "b", "c", "d"].map(String::from).to_vec();
    move_entry(&mut list, from, before);
    list
  }

  #[test]
  fn moves() {
    assert_eq!(moved(0, 2), ["b", "a", "c", "d"]);
    assert_eq!(moved(3, 1), ["a", "d", "b", "c"]);
    assert_eq!(moved(1, 4), ["a", "c", "d", "b"]);
    assert_eq!(moved(1, 1), ["a", "b", "c", "d"]);
    assert_eq!(moved(1, 2), ["a", "b", "c", "d"]);
    assert_eq!(moved(9, 0), ["a", "b", "c", "d"]);
  }
}

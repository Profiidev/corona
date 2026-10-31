//! The pieces panel pages are built of: cards, sections, list rows, tags, bars

use gpui_kit::{
  AnyElement, App, Div, ElementId, Hsla, IntoElement, ParentElement, RenderOnce, SharedString,
  Styled, Window,
  base::StyledExt,
  component::{ActiveTheme, Icon, Sizable, scroll::ScrollableElement},
  div,
  prelude::FluentBuilder,
  px,
};

use crate::components::card::CardExt;

/// The state a tag shows
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
  Ok,
  Off,
  Warn,
  Err,
}

/// Fill and text of a tag
pub fn tone(tone: Tone, cx: &App) -> (Hsla, Hsla) {
  let theme = cx.theme();
  let tinted = |c: Hsla| (c.opacity(0.18), c);
  match tone {
    Tone::Ok => tinted(theme.colors.success),
    Tone::Warn => tinted(theme.colors.warning),
    Tone::Err => tinted(theme.colors.danger),
    Tone::Off => (
      theme.foreground.opacity(0.08),
      theme.colors.muted_foreground,
    ),
  }
}

/// A plain card, its content in a column
pub fn card(cx: &App) -> Div {
  div().flex().flex_col().w_full().gap_2().p_2().card(cx)
}

/// A muted line of text
pub fn muted(text: impl Into<String>, cx: &App) -> Div {
  div()
    .text_sm()
    .truncate()
    .text_color(cx.theme().muted_foreground)
    .child(text.into())
}

/// A muted line, for empty lists
pub fn empty_note(text: impl Into<SharedString>, cx: &App) -> AnyElement {
  div()
    .flex()
    .justify_center()
    .p_2()
    .text_xs()
    .text_color(cx.theme().colors.muted_foreground)
    .child(text.into())
    .into_any_element()
}

/// A card with a title line: `title`, a muted `hint`, an optional element on the right
pub fn section(
  title: impl Into<SharedString>,
  hint: impl Into<SharedString>,
  right: Option<AnyElement>,
  rows: impl IntoIterator<Item = AnyElement>,
  cx: &App,
) -> Div {
  let (title, hint) = (title.into(), hint.into());
  let theme = cx.theme();
  div()
    .flex()
    .flex_col()
    .flex_shrink_0()
    .w_full()
    .gap_1()
    .p_2()
    .card(cx)
    .when(!title.is_empty() || right.is_some(), |d| {
      d.child(
        div()
          .flex()
          .items_center()
          .gap_2()
          .px_1()
          .child(
            div()
              .text_sm()
              .font_weight(gpui_kit::FontWeight::BOLD)
              .child(title),
          )
          .child(
            div()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .child(hint),
          )
          .child(div().flex_1())
          .children(right),
      )
    })
    .children(rows)
}

/// A titled card whose rows scroll once there are more than `scroll_after`
#[derive(IntoElement)]
pub struct ListCard {
  id: ElementId,
  title: SharedString,
  scroll_after: usize,
  empty: Option<AnyElement>,
  rows: Vec<AnyElement>,
}

impl ListCard {
  pub fn new(id: impl Into<ElementId>, title: impl Into<SharedString>) -> Self {
    Self {
      id: id.into(),
      title: title.into(),
      scroll_after: 2,
      empty: None,
      rows: Vec::new(),
    }
  }

  pub fn scroll_after(mut self, rows: usize) -> Self {
    self.scroll_after = rows;
    self
  }

  /// Shown above the rows, for an empty list
  pub fn empty(mut self, empty: Option<impl IntoElement>) -> Self {
    self.empty = empty.map(IntoElement::into_any_element);
    self
  }
}

impl ParentElement for ListCard {
  fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
    self.rows.extend(elements);
  }
}

impl RenderOnce for ListCard {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    div()
      .flex()
      .flex_col()
      .w_full()
      .when_else(
        self.rows.len() > self.scroll_after,
        |d| d.min_h(px(128.)),
        |d| d.flex_shrink_0(),
      )
      .gap_2()
      .p_2()
      .card(cx)
      .child(div().font_bold().text_sm().child(self.title))
      .children(self.empty)
      .child(
        div()
          .flex()
          .flex_col()
          .gap_1()
          .h_auto()
          .overflow_y_scrollbar()
          .id(self.id)
          .children(self.rows),
      )
  }
}

/// A row of a list: an icon, the title over a muted subtitle, then its children
#[derive(IntoElement)]
pub struct ListItem {
  icon: Icon,
  title: SharedString,
  subtitle: SharedString,
  children: Vec<AnyElement>,
}

impl ListItem {
  pub fn new(
    icon: impl Into<Icon>,
    title: impl Into<SharedString>,
    subtitle: impl Into<SharedString>,
  ) -> Self {
    Self {
      icon: icon.into(),
      title: title.into(),
      subtitle: subtitle.into(),
      children: Vec::new(),
    }
  }
}

impl ParentElement for ListItem {
  fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
    self.children.extend(elements);
  }
}

impl RenderOnce for ListItem {
  fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
    let theme = cx.theme();
    div()
      .flex()
      .w_full()
      .gap_2()
      .p_2()
      .items_center()
      .rounded_xl()
      .bg(theme.colors.background)
      .child(self.icon.small())
      .child(
        div()
          .flex()
          .flex_col()
          .flex_1()
          .min_w_0()
          .child(div().text_sm().truncate().child(self.title))
          .child(
            div()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .truncate()
              .child(self.subtitle),
          ),
      )
      .children(self.children)
  }
}

/// A tag pill in one of the tones
pub fn tag(text: impl Into<SharedString>, tone: Tone, cx: &App) -> Div {
  let (bg, fg) = self::tone(tone, cx);
  div()
    .flex_shrink_0()
    .h(px(20.))
    .px_2()
    .flex()
    .items_center()
    .rounded_full()
    .bg(bg)
    .text_color(fg)
    .text_xs()
    .child(text.into())
}

/// A plain key / value line
pub fn kv(
  key: impl Into<SharedString>,
  value: impl Into<SharedString>,
  color: Option<Hsla>,
  cx: &App,
) -> AnyElement {
  let theme = cx.theme();
  div()
    .flex()
    .gap_2()
    .px_1()
    .py(px(2.))
    .text_sm()
    .child(
      div()
        .w(px(130.))
        .flex_shrink_0()
        .text_color(theme.colors.muted_foreground)
        .child(key.into()),
    )
    .child(
      div()
        .flex_1()
        .min_w_0()
        .truncate()
        .text_color(color.unwrap_or(theme.colors.foreground))
        .child(value.into()),
    )
    .into_any_element()
}

/// A bar of `percent`, red over 85, amber over 65
pub fn usage_bar(
  percent: f32,
  left: impl Into<SharedString>,
  right: impl Into<SharedString>,
  cx: &App,
) -> Div {
  let theme = cx.theme();
  let color = match percent {
    p if p > 85. => theme.colors.danger,
    p if p > 65. => theme.colors.warning,
    _ => theme.colors.primary,
  };
  div()
    .flex()
    .flex_col()
    .gap(px(2.))
    .w_full()
    .child(
      div()
        .h(px(4.))
        .w_full()
        .rounded_full()
        .bg(theme.colors.muted)
        .child(
          div()
            .h_full()
            .rounded_full()
            .bg(color)
            .w(gpui_kit::relative(percent.clamp(0., 100.) / 100.)),
        ),
    )
    .child(
      div()
        .flex()
        .text_xs()
        .text_color(theme.colors.muted_foreground)
        .child(div().flex_1().truncate().child(left.into()))
        .child(right.into()),
    )
}

/// A dot in a state color
pub fn dot(color: Hsla) -> Div {
  div().flex_shrink_0().size(px(8.)).rounded_full().bg(color)
}

#[cfg(test)]
mod tests {
  use gpui_kit::{self as gpui, TestAppContext, assets::IconName};

  use super::*;
  use crate::test_view;

  #[gpui::test]
  fn renders(cx: &mut TestAppContext) {
    let (handle, _) = test_view::open(cx, |_, cx| {
      div()
        .child(card(cx).child(muted("muted", cx)))
        .child(section(
          "title",
          "hint",
          None,
          [kv("key", "value", None, cx), empty_note("none", cx)],
          cx,
        ))
        .child(
          ListCard::new("list", "list")
            .empty(Some(empty_note("none", cx)))
            .child(ListItem::new(IconName::Plug, "title", "subtitle").child(dot(gpui::red())))
            .children((0..3).map(|i| ListItem::new(IconName::Plug, format!("{i}"), ""))),
        )
        .child(tag("on", Tone::Ok, cx))
        .child(tag("off", Tone::Off, cx))
        .child(usage_bar(70., "left", "right", cx))
        .into_any_element()
    });
    test_view::draw(handle, cx);
  }
}

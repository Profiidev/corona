//! The pieces the panel's pages are built of: sections, rows, tags, tiles, bars

use corona_components::components::card::CardExt;
use corona_launcher::{Tone, ui::style};
use gpui_kit::{
  AnyElement, App, Div, Hsla, IntoElement, ParentElement, SharedString, Styled,
  assets::IconName,
  component::{ActiveTheme, Icon, Sizable},
  div, hsla,
  prelude::FluentBuilder,
  px,
};

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

/// A tag pill in one of the launcher's tones
pub fn tag(text: impl Into<SharedString>, tone: Tone, cx: &App) -> Div {
  let (bg, fg) = style::tone(tone, cx);
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

/// The launcher's gradient tile with an icon or a letter
pub fn tile(hue: f32, content: TileContent, size: f32) -> Div {
  let inner = match content {
    TileContent::Icon(icon) => Icon::new(icon)
      .small()
      .text_color(gpui_kit::white())
      .into_any_element(),
    TileContent::Letter(letter) => div()
      .text_sm()
      .font_weight(gpui_kit::FontWeight::BOLD)
      .text_color(gpui_kit::white())
      .child(letter)
      .into_any_element(),
  };
  div()
    .flex_shrink_0()
    .size(px(size))
    .rounded(px(size / 4.))
    .bg(style::tile(hue))
    .flex()
    .items_center()
    .justify_center()
    .child(inner)
}

pub enum TileContent {
  Icon(IconName),
  Letter(SharedString),
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
    p if p > 65. => hsla(45. / 360., 0.9, 0.6, 1.),
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

/// The drive's partitions as one bar: colored by partition, faint while off
pub fn partition_map(segments: &[(f32, f32, bool)], cx: &App) -> Div {
  let theme = cx.theme();
  let used: f32 = segments.iter().map(|s| s.0).sum();
  div()
    .flex()
    .gap(px(2.))
    .h(px(6.))
    .w_full()
    .children(segments.iter().map(|(share, hue, on)| {
      div()
        .h_full()
        .rounded_sm()
        .flex_basis(gpui_kit::relative(share.max(0.01)))
        .bg(hsla(hue / 360., 0.62, 0.55, if *on { 1. } else { 0.38 }))
    }))
    .when(used < 0.99, |d| {
      d.child(
        div()
          .h_full()
          .rounded_sm()
          .flex_basis(gpui_kit::relative(1. - used))
          .bg(theme.colors.muted),
      )
    })
}

/// A dot in a state color
pub fn dot(color: Hsla) -> Div {
  div().flex_shrink_0().size(px(8.)).rounded_full().bg(color)
}

pub fn ok_color() -> Hsla {
  hsla(150. / 360., 0.55, 0.55, 1.)
}

pub fn warn_color() -> Hsla {
  hsla(45. / 360., 0.9, 0.6, 1.)
}

/// A muted line, for empty lists
pub fn note(text: impl Into<SharedString>, cx: &App) -> AnyElement {
  div()
    .flex()
    .justify_center()
    .p_2()
    .text_xs()
    .text_color(cx.theme().colors.muted_foreground)
    .child(text.into())
    .into_any_element()
}

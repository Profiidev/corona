//! The disks pages' own pieces: row icons and the partition map

use gpui_kit::{
  App, Div, ParentElement, Styled,
  assets::IconName,
  component::{ActiveTheme, Icon},
  div, hsla,
  prelude::FluentBuilder,
  px,
};

/// A plain Lucide icon at a row's start
pub fn icon(name: IconName) -> Icon {
  Icon::new(name).flex_shrink_0()
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

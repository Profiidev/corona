use corona_surface::osd::{Osd, OsdExt};
use gpui_kit::{
  App, Context, IntoElement, ParentElement, Pixels, Render, Size, Styled, Window,
  component::{ActiveTheme, Icon, Sizable},
  div,
  prelude::FluentBuilder,
  px, size,
};
use tracing::warn;

use crate::command::Hud;

const HEIGHT: f32 = 44.;
const CHAR: f32 = 7.5;
const PADDING: f32 = 18.;

/// The pill shown after the launcher closes
pub struct HudOsd(Hud);

/// Shows `hud` on the OSD surface
pub fn show(hud: Hud, cx: &mut App) {
  if let Err(e) = cx.show_osd(HudOsd(hud)) {
    warn!("failed to show the launcher HUD: {e:#}");
  }
}

impl Osd for HudOsd {
  const NAME: &'static str = "launcher_hud";

  fn size(&self, _: &App) -> Size<Pixels> {
    let extra = (self.0.icon.is_some() || self.0.swatch.is_some()) as u8 as f32 * 24.;
    let width = self.0.text.chars().count() as f32 * CHAR + PADDING * 2. + extra;
    size(px(width.clamp(120., 560.)), px(HEIGHT))
  }
}

impl Render for HudOsd {
  fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();
    div()
      .size_full()
      .flex()
      .items_center()
      .justify_center()
      .gap_2()
      .px(px(PADDING))
      .text_size(px(13.5))
      .text_color(theme.foreground)
      .when_some(self.0.swatch, |el, color| {
        el.child(div().size(px(14.)).rounded(px(4.)).bg(color))
      })
      .when_some(self.0.icon, |el, icon| el.child(Icon::new(icon).small()))
      .child(self.0.text.clone())
  }
}

use corona_brightness::BrightnessExt;
use gpui_kit::{
  Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  base::StyledExt,
  component::{Icon, Sizable, Theme, slider::Slider, switch::Switch},
  div,
};
use rust_i18n::t;

use crate::control_center::brightness::{BrightnessPanel, card};

/// The slider's range in kelvin
pub(super) const MIN: f32 = 1000.;
pub(super) const MAX: f32 = 6500.;

impl BrightnessPanel {
  pub(super) fn nightlight(
    &self,
    theme: &Theme,
    cx: &Context<'_, Self>,
  ) -> Option<impl IntoElement> {
    let state = *cx.brightness().nightlight(cx);
    if !state.available {
      return None;
    }
    let (slider, _) = &self.nightlight_slider;
    let kelvin = match self.dragging.contains(super::NIGHTLIGHT) {
      true => slider.read(cx).value().start() as u32,
      false => state.temperature,
    };
    Some(
      card(cx).child(
        div()
          .flex()
          .flex_col()
          .w_full()
          .gap_1()
          .p_2()
          .rounded_xl()
          .bg(theme.colors.background)
          .child(
            div()
              .flex()
              .gap_2()
              .items_center()
              .child(Icon::new(IconName::Moon).small())
              .child(
                div()
                  .flex_1()
                  .text_sm()
                  .font_bold()
                  .child(t!("app.brightness.nightlight")),
              )
              .child(
                div()
                  .text_xs()
                  .text_color(theme.colors.muted_foreground)
                  .child(format!("{kelvin} K")),
              )
              .child(
                Switch::new("nightlight")
                  .checked(state.on)
                  .on_change(cx.listener(|this, on: &bool, _, cx| {
                    let result = cx.brightness().set_nightlight(*on, cx);
                    this.show_error(result, cx);
                    cx.notify();
                  })),
              ),
          )
          .child(
            div()
              .flex()
              .gap_2()
              .items_center()
              .child(Icon::new(IconName::Flame).small())
              .child(
                div()
                  .flex_1()
                  .child(Slider::new(slider).px_2().cursor_pointer()),
              )
              .child(Icon::new(IconName::Sun).small()),
          ),
      ),
    )
  }
}

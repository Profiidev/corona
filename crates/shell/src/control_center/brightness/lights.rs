use corona_brightness::{BrightnessExt, DisplayKind, Unavailable};
use corona_power::PowerExt;
use gpui_kit::{
  App, Context, IntoElement, ParentElement, Styled,
  assets::IconName,
  base::StyledExt,
  component::{Icon, Sizable, Theme, slider::Slider},
  div,
};

use crate::control_center::brightness::{BrightnessPanel, KEYBOARD, card};

pub struct Light {
  pub id: String,
  pub name: String,
  pub output: Option<String>,
  pub icon: IconName,
  pub brightness: u32,
  pub max: u32,
  pub unavailable: Option<Unavailable>,
}

impl Light {
  pub fn percent(&self) -> f32 {
    if self.max == 0 {
      return 0.;
    }
    (self.brightness as f32 / self.max as f32 * 100.).round()
  }
}

pub(super) fn displays(cx: &App) -> Vec<Light> {
  cx.brightness()
    .list_displays(cx)
    .iter()
    .map(|d| Light {
      id: d.id.clone(),
      name: d.name.clone(),
      output: d.output.clone(),
      icon: match d.kind {
        DisplayKind::Backlight => IconName::Laptop,
        DisplayKind::External => IconName::Monitor,
      },
      brightness: d.brightness,
      max: d.max,
      unavailable: d.unavailable,
    })
    .collect()
}

pub(super) fn keyboard(cx: &App) -> Option<Light> {
  let backlight = cx.power().keyboard_backlight(cx)?;
  Some(Light {
    id: KEYBOARD.into(),
    name: "Keyboard backlight".into(),
    output: None,
    icon: IconName::Keyboard,
    brightness: backlight.brightness.max(0) as u32,
    max: backlight.max.max(0) as u32,
    unavailable: None,
  })
}

fn reason(unavailable: Unavailable) -> &'static str {
  match unavailable {
    Unavailable::DdcutilDisabled => "Set enable_ddcutil under [brightness]",
    Unavailable::DdcutilMissing => "Install ddcutil",
    Unavailable::Detecting => "Detecting…",
    Unavailable::Unsupported => "No DDC/CI",
    Unavailable::Failed => "Not responding",
  }
}

impl BrightnessPanel {
  pub fn lights(
    &self,
    theme: &Theme,
    title: &'static str,
    lights: Vec<Light>,
    cx: &Context<'_, Self>,
  ) -> Option<impl IntoElement> {
    if lights.is_empty() {
      return None;
    }
    Some(
      card(cx)
        .child(div().text_sm().font_bold().child(title))
        .children(lights.iter().map(|light| self.light(theme, light, cx))),
    )
  }

  fn light(&self, theme: &Theme, light: &Light, cx: &Context<'_, Self>) -> impl IntoElement {
    let slider = self.sliders.get(&light.id).map(|(slider, _)| slider);
    let value = match slider {
      Some(slider) if self.dragging.contains(&light.id) => slider.read(cx).value().start(),
      _ => light.percent(),
    };
    let status = match light.unavailable {
      Some(unavailable) => reason(unavailable).to_string(),
      None => format!("{value:.0}%"),
    };

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
          .child(Icon::new(light.icon).small())
          .child(
            div()
              .flex_1()
              .min_w_0()
              .text_sm()
              .truncate()
              .child(light.name.clone()),
          )
          .children(light.output.clone().map(|output| {
            div()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .child(output)
          }))
          .child(
            div()
              .text_xs()
              .text_color(theme.colors.muted_foreground)
              .child(status),
          ),
      )
      .children(slider.map(|slider| {
        div()
          .flex()
          .gap_2()
          .items_center()
          .child(Icon::new(IconName::SunDim).small())
          .child(
            div().flex_1().child(
              Slider::new(slider)
                .px_2()
                .cursor_pointer()
                .disabled(light.unavailable.is_some()),
            ),
          )
          .child(Icon::new(IconName::Sun).small())
      }))
  }
}

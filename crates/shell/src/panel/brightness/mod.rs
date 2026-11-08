use corona_components::components::list::card;
use std::{collections::HashMap, pin::Pin, time::Duration};

use anyhow::Result;
use corona_brightness::{BrightnessExt, Display, DisplayKind};
use corona_components::components::{
  card::PageError,
  throttled_slider::{Moved, ThrottledSlider},
};
use corona_power::PowerExt;
use corona_surface::control_center::ControlCenterPage;
use gpui_kit::{
  AppContext, Context, Entity, IntoElement, ParentElement, Render, Styled, Subscription, Window,
  base::slider::SliderState, component::ActiveTheme, div, prelude::FluentBuilder,
};

use crate::panel::brightness::lights::{Light, displays, keyboard};
use rust_i18n::t;

mod lights;
mod nightlight;

pub fn display_name(display: &Display) -> String {
  match (&display.name, display.kind) {
    (Some(name), _) => name.clone(),
    (None, DisplayKind::Backlight) => t!("app.brightness.builtin_display").into(),
    (None, DisplayKind::External) => t!("app.brightness.external_display").into(),
  }
}

const KEYBOARD: &str = "keyboard";
const THROTTLE: Duration = Duration::from_millis(100);

pub struct BrightnessPanel {
  error: PageError,
  sliders: HashMap<String, (Entity<ThrottledSlider>, Subscription)>,
  /// Kelvin
  nightlight_slider: Entity<ThrottledSlider>,
  _subscriptions: [Subscription; 4],
}

impl ControlCenterPage for BrightnessPanel {
  const ID: &'static str = "brightness";

  fn icon() -> gpui_kit::assets::IconName {
    gpui_kit::assets::IconName::Sun
  }

  fn title() -> gpui_kit::SharedString {
    rust_i18n::t!("app.control_center.brightness").into()
  }

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let displays = cx.brightness().displays.clone();
    let keyboard = cx.power().keyboard_backlight.clone();
    let nightlight = cx.brightness().nightlight.clone();

    // the gamma thread takes only the latest, so no throttle is needed
    let slider = cx.new(|cx| {
      // max first: min clamps against the default max of 100
      let state = SliderState::new()
        .max(nightlight::MAX)
        .min(nightlight::MIN)
        .step(100.);
      ThrottledSlider::new(state, None, cx)
    });

    let subscriptions = [
      cx.observe_in(&displays, window, |this, _, window, cx| {
        this.sync_sliders(window, cx);
        cx.notify();
      }),
      cx.observe_in(&keyboard, window, |this, _, window, cx| {
        this.sync_sliders(window, cx);
        cx.notify();
      }),
      cx.observe_in(&nightlight, window, |this, _, window, cx| {
        this.sync_nightlight(window, cx);
        cx.notify();
      }),
      cx.subscribe(&slider, |this, _, Moved(kelvin), cx| {
        if let Some(kelvin) = kelvin {
          let result = cx
            .brightness()
            .set_nightlight_temperature(kelvin.round() as u32, cx)
            .map(|_| ());
          this.error.set(result);
        }
        cx.notify();
      }),
    ];

    let mut panel = Self {
      error: PageError::default(),
      sliders: HashMap::new(),
      nightlight_slider: slider,
      _subscriptions: subscriptions,
    };
    panel.sync_sliders(window, cx);
    panel.sync_nightlight(window, cx);
    panel
  }
}

impl BrightnessPanel {
  fn sync_sliders(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let lights: Vec<Light> = displays(cx).into_iter().chain(keyboard(cx)).collect();
    self
      .sliders
      .retain(|id, _| lights.iter().any(|light| &light.id == id));

    for light in lights {
      if !self.sliders.contains_key(&light.id) {
        let slider = cx.new(|cx| {
          let state = SliderState::new().min(0.).max(100.).step(1.);
          ThrottledSlider::new(state, Some(THROTTLE), cx)
        });
        let id = light.id.clone();
        let subscription = cx.subscribe(&slider, move |this, _, Moved(percent), cx| {
          if let Some(percent) = percent {
            this.set_brightness(&id, *percent, cx);
          }
          cx.notify();
        });
        self
          .sliders
          .insert(light.id.clone(), (slider, subscription));
      }
      let (slider, _) = &self.sliders[&light.id];
      slider.update(cx, |slider, cx| slider.sync(light.percent(), window, cx));
    }
  }

  fn sync_nightlight(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let kelvin = cx.brightness().nightlight(cx).temperature as f32;
    self
      .nightlight_slider
      .update(cx, |slider, cx| slider.sync(kelvin, window, cx));
  }

  fn set_brightness(&mut self, id: &str, percent: f32, cx: &mut Context<Self>) {
    let light = displays(cx)
      .into_iter()
      .chain(keyboard(cx))
      .find(|light| light.id == id);
    let Some(light) = light else {
      return;
    };
    let raw = (percent / 100. * light.max as f32).round() as u32;
    let task: Pin<Box<dyn Future<Output = Result<()>>>> = if id == KEYBOARD {
      Box::pin(cx.power().set_keyboard_brightness(raw as i32, cx))
    } else {
      Box::pin(cx.brightness().set_brightness(id, raw, cx))
    };
    cx.spawn(async move |this, cx| {
      let result = task.await;
      this
        .update(cx, |this, cx| {
          this.error.set(result);
          cx.notify();
        })
        .ok();
    })
    .detach();
  }
}

impl Render for BrightnessPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();

    div()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .when_some(
        self
          .error
          .card("brightness-error-dismiss", |this| &mut this.error, cx),
        |d, error| d.child(error),
      )
      .when_some(self.nightlight(theme, cx), |d, nightlight| {
        d.child(nightlight)
      })
      .when_some(
        self.lights(theme, t!("app.brightness.displays"), displays(cx), cx),
        |d, displays| d.child(displays),
      )
      .when_some(
        self.lights(
          theme,
          t!("app.brightness.keyboard"),
          keyboard(cx).into_iter().collect(),
          cx,
        ),
        |d, keyboard| d.child(keyboard),
      )
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn display(name: Option<&str>, kind: DisplayKind) -> Display {
    Display {
      id: "test".into(),
      output: None,
      name: name.map(Into::into),
      kind,
      brightness: 0,
      max: 100,
      unavailable: None,
    }
  }

  #[test]
  fn display_name() {
    assert_eq!(
      super::display_name(&display(Some("Dell U2720Q"), DisplayKind::Backlight)),
      "Dell U2720Q"
    );
    let builtin = super::display_name(&display(None, DisplayKind::Backlight));
    let external = super::display_name(&display(None, DisplayKind::External));
    assert!(!builtin.is_empty() && !external.is_empty());
    assert_ne!(builtin, external);
  }
}

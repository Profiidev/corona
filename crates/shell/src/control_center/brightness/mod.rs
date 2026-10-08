use std::{
  collections::{HashMap, HashSet},
  pin::Pin,
  time::Duration,
};

use anyhow::Result;
use corona_brightness::{BrightnessExt, Display, DisplayKind};
use corona_components::components::card::{CardExt, ErrorCard};
use corona_power::PowerExt;
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  App, AppContext, Context, Div, Entity, IntoElement, ParentElement, Render, Styled, Subscription,
  Window,
  base::slider::{SliderEvent, SliderState},
  component::ActiveTheme,
  div,
  prelude::FluentBuilder,
};

use crate::control_center::{
  ControlCenterPanel,
  brightness::lights::{Light, displays, keyboard},
  variants::ControlCenterType,
};
use rust_i18n::t;

mod lights;

pub(crate) fn display_name(display: &Display) -> String {
  match (&display.name, display.kind) {
    (Some(name), _) => name.clone(),
    (None, DisplayKind::Backlight) => t!("app.brightness.builtin_display").into(),
    (None, DisplayKind::External) => t!("app.brightness.external_display").into(),
  }
}

const KEYBOARD: &str = "keyboard";
const THROTTLE: Duration = Duration::from_millis(100);

pub struct BrightnessPanel {
  error: Option<String>,
  sliders: HashMap<String, (Entity<SliderState>, Subscription)>,
  dragging: HashSet<String>,
  pending: HashMap<String, f32>,
  _subscriptions: [Subscription; 2],
}

impl ControlCenterPanel for BrightnessPanel {
  const TYPE: ControlCenterType = ControlCenterType::Brightness;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let displays = cx.brightness().displays.clone();
    let keyboard = cx.power().keyboard_backlight.clone();

    let subscriptions = [
      cx.observe_in(&displays, window, |this, _, window, cx| {
        this.sync_sliders(window, cx);
        cx.notify();
      }),
      cx.observe_in(&keyboard, window, |this, _, window, cx| {
        this.sync_sliders(window, cx);
        cx.notify();
      }),
    ];

    let mut panel = Self {
      error: None,
      sliders: HashMap::new(),
      dragging: HashSet::new(),
      pending: HashMap::new(),
      _subscriptions: subscriptions,
    };
    panel.sync_sliders(window, cx);
    panel
  }
}

fn card(cx: &App) -> Div {
  div().flex().flex_col().w_full().gap_2().p_2().card(cx)
}

impl BrightnessPanel {
  fn show_error(&mut self, result: Result<()>, _: &mut Context<Self>) {
    if let Err(e) = result.log_err() {
      self.error = Some(e.to_string());
    }
  }

  fn sync_sliders(&mut self, window: &mut Window, cx: &mut Context<Self>) {
    let lights: Vec<Light> = displays(cx).into_iter().chain(keyboard(cx)).collect();
    self
      .sliders
      .retain(|id, _| lights.iter().any(|light| &light.id == id));

    for light in lights {
      if !self.sliders.contains_key(&light.id) {
        let slider = cx.new(|_| SliderState::new().min(0.).max(100.).step(1.));
        let subscription = cx.subscribe(&slider, {
          let id = light.id.clone();
          move |this, _, event: &SliderEvent, cx| match event {
            SliderEvent::Change(value) => {
              this.dragging.insert(id.clone());
              this.throttle(&id, value.start(), cx);
              cx.notify();
            }
            SliderEvent::Release(value) => {
              this.dragging.remove(&id);
              this.pending.remove(&id);
              this.set_brightness(&id, value.start(), cx);
            }
          }
        });
        self
          .sliders
          .insert(light.id.clone(), (slider, subscription));
      }
      if !self.dragging.contains(&light.id) {
        let (slider, _) = &self.sliders[&light.id];
        slider.update(cx, |state, cx| state.set_value(light.percent(), window, cx));
      }
    }
  }

  fn throttle(&mut self, id: &str, percent: f32, cx: &mut Context<Self>) {
    if self.pending.insert(id.to_string(), percent).is_some() {
      return;
    }
    let id = id.to_string();
    cx.spawn(async move |this, cx| {
      cx.background_executor().timer(THROTTLE).await;
      this
        .update(cx, |this, cx| {
          if let Some(percent) = this.pending.remove(&id) {
            this.set_brightness(&id, percent, cx);
          }
        })
        .ok();
    })
    .detach();
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
          this.show_error(result, cx);
          cx.notify();
        })
        .ok();
    })
    .detach();
  }

  fn error(&self, cx: &Context<'_, Self>) -> Option<ErrorCard> {
    let error = self.error.clone()?;
    Some(
      ErrorCard::new("brightness-error-dismiss", error).on_dismiss(cx.listener(
        |this, _, _, cx| {
          this.error = None;
          cx.notify();
        },
      )),
    )
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
      .when_some(self.error(cx), |d, error| d.child(error))
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

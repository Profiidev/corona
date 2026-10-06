use std::time::Duration;

use corona_brightness::{BrightnessExt, Display};
use corona_compositor::CompositorExt;
use corona_pipewire::{
  AudioNode, PipewireExt,
  volume::{to_linear, to_slider},
};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{
  App, AppContext, Context, Div, Entity, ParentElement, SharedString, Styled, Subscription, Window,
  assets::IconName,
  base::{
    Disableable,
    slider::{SliderEvent, SliderState},
  },
  component::{
    ActiveTheme,
    button::{Button, ButtonVariant, ButtonVariants},
    slider::Slider,
  },
  div,
  prelude::FluentBuilder,
};

use crate::{
  control_center::{
    ControlCenter,
    dashboard::{DashboardPanel, spawn_logged},
    variants::ControlCenterType,
  },
  icons::volume_icon,
};

pub(super) struct Sliders {
  output: Entity<SliderState>,
  input: Entity<SliderState>,
  brightness: Entity<SliderState>,
  dragging: bool,
  pending: Option<f32>,
  _subscriptions: [Subscription; 3],
}

const THROTTLE: Duration = Duration::from_millis(100);

fn sink(cx: &App) -> Option<&AudioNode> {
  cx.pipewire().default_sink(cx)
}

fn source(cx: &App) -> Option<&AudioNode> {
  cx.pipewire().default_source(cx)
}

/// the focused monitor's display, or the first usable one
pub(crate) fn display(cx: &App) -> Option<&Display> {
  let focused = &cx.compositor().active_monitor(cx).name;
  let displays = cx.brightness().list_displays(cx);
  let usable = || displays.iter().filter(|d| d.unavailable.is_none());
  usable()
    .find(|d| d.output.as_ref() == Some(focused))
    .or_else(|| usable().next())
}

fn set_volume(node: Option<&AudioNode>, value: f32, cx: &App) {
  let Some(node) = node else {
    return;
  };
  let channels = node.volumes.len().max(1);
  let _ = cx
    .pipewire()
    .audio()
    .set_volumes(node.id, vec![to_linear(value); channels])
    .log_err();
}

fn set_brightness(percent: f32, cx: &mut App) {
  let Some(display) = display(cx) else {
    return;
  };
  let raw = (percent / 100. * display.max as f32).round() as u32;
  let task = cx.brightness().set_brightness(&display.id, raw, cx);
  spawn_logged(cx, task);
}

impl Sliders {
  pub fn new(cx: &mut Context<DashboardPanel>) -> Self {
    let volume = || SliderState::new().min(0.).max(1.).step(0.01);
    let output = cx.new(|_| volume());
    let input = cx.new(|_| volume());
    let brightness = cx.new(|_| SliderState::new().min(0.).max(100.).step(1.));

    let subscriptions = [
      cx.subscribe(&output, |_, _, event: &SliderEvent, cx| {
        if let SliderEvent::Change(value) = event {
          set_volume(sink(cx), value.start(), cx);
        }
      }),
      cx.subscribe(&input, |_, _, event: &SliderEvent, cx| {
        if let SliderEvent::Change(value) = event {
          set_volume(source(cx), value.start(), cx);
        }
      }),
      cx.subscribe(
        &brightness,
        |this, _, event: &SliderEvent, cx| match event {
          SliderEvent::Change(value) => {
            this.sliders.dragging = true;
            this.sliders.throttle(value.start(), cx);
          }
          SliderEvent::Release(value) => {
            this.sliders.dragging = false;
            this.sliders.pending = None;
            set_brightness(value.start(), cx);
          }
        },
      ),
    ];

    Self {
      output,
      input,
      brightness,
      dragging: false,
      pending: None,
      _subscriptions: subscriptions,
    }
  }

  fn throttle(&mut self, percent: f32, cx: &mut Context<DashboardPanel>) {
    if self.pending.replace(percent).is_some() {
      return;
    }
    cx.spawn(async move |panel, cx| {
      cx.background_executor().timer(THROTTLE).await;
      let _ = panel.update(cx, |panel, cx| {
        if let Some(percent) = panel.sliders.pending.take() {
          set_brightness(percent, cx);
        }
      });
    })
    .detach();
  }

  pub fn sync(&mut self, window: &mut Window, cx: &mut Context<DashboardPanel>) {
    let values = [
      (&self.output, sink(cx).map(|n| to_slider(n.volume())), 0.005),
      (
        &self.input,
        source(cx).map(|n| to_slider(n.volume())),
        0.005,
      ),
      (
        &self.brightness,
        display(cx).map(Display::percent).filter(|_| !self.dragging),
        0.5,
      ),
    ];
    for (slider, value, epsilon) in values {
      let Some(value) = value else {
        continue;
      };
      slider.update(cx, |state, cx| {
        if (state.value().start() - value).abs() > epsilon {
          state.set_value(value, window, cx);
        }
      });
    }
  }

  pub fn render(&self, cx: &mut Context<DashboardPanel>) -> Div {
    let output = sink(cx).map(|n| (n.id, n.mute, to_slider(n.volume())));
    let input = source(cx).map(|n| (n.id, n.mute, to_slider(n.volume())));
    let brightness = display(cx).map(Display::percent);

    let mute = |id: &'static str, node: Option<(u32, bool, f32)>, icon: IconName| {
      Button::new(id)
        .icon(icon)
        .cursor_pointer()
        .disabled(node.is_none())
        .when(node.is_some_and(|(_, mute, _)| mute), |b| {
          b.with_variant(ButtonVariant::Danger)
        })
        .on_click(move |_, _, cx| {
          if let Some((id, mute, _)) = node {
            let _ = cx.pipewire().audio().set_mute(id, !mute).log_err();
          }
        })
    };

    div()
      .flex()
      .flex_col()
      .gap_1()
      .p_2()
      .rounded_xl()
      .border_1()
      .border_color(cx.theme().border)
      .bg(cx.theme().colors.accent)
      .child(row(
        mute(
          "dashboard-output-mute",
          output,
          output.map_or(IconName::VolumeOff, |(_, mute, v)| volume_icon(v, mute)),
        ),
        &self.output,
        output.map(|(_, _, v)| v * 100.),
        ControlCenterType::Audio,
        cx,
      ))
      .child(row(
        mute(
          "dashboard-input-mute",
          input,
          match input {
            Some((_, false, _)) => IconName::Mic,
            _ => IconName::MicOff,
          },
        ),
        &self.input,
        input.map(|(_, _, v)| v * 100.),
        ControlCenterType::Audio,
        cx,
      ))
      .children(brightness.map(|percent| {
        row(
          Button::new("dashboard-brightness")
            .icon(IconName::Sun)
            .ghost(),
          &self.brightness,
          Some(percent),
          ControlCenterType::Brightness,
          cx,
        )
      }))
  }
}

fn row(
  icon: Button,
  slider: &Entity<SliderState>,
  percent: Option<f32>,
  page: ControlCenterType,
  cx: &App,
) -> Div {
  let id = SharedString::from(format!("dashboard-open-{}", slider.entity_id()));
  div()
    .flex()
    .items_center()
    .gap_1()
    .child(icon)
    .child(
      div().flex_1().child(
        Slider::new(slider)
          .px_2()
          .cursor_pointer()
          .disabled(percent.is_none()),
      ),
    )
    .child(
      div()
        .w_10()
        .flex()
        .justify_end()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(percent.map_or("–".into(), |p| format!("{p:.0}%"))),
    )
    .child(
      Button::new(id)
        .icon(IconName::ChevronRight)
        .ghost()
        .cursor_pointer()
        .on_click(move |_, window, cx| ControlCenter::navigate(page, window, cx)),
    )
}

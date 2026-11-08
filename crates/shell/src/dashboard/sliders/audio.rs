//! The dashboard's output and input volume sliders

use corona_surface::{
  control_center::ControlCenterPage,
  dashboard::{Changed, DashboardSlider},
};
use corona_utils::error::ErrorLogExt;
use gpui_kit::{App, Entity, SharedString, Subscription, assets::IconName};
use rust_i18n::t;

use crate::panel::audio::{AudioPanel, icons::volume_icon};
use corona_pipewire::{
  AudioNode, PipewireExt,
  volume::{to_linear, to_slider},
};

fn sink(cx: &App) -> Option<&AudioNode> {
  cx.pipewire().default_sink(cx)
}

fn source(cx: &App) -> Option<&AudioNode> {
  cx.pipewire().default_source(cx)
}

fn set_volume(node: Option<&AudioNode>, value: f32, cx: &App) {
  let Some(node) = node else {
    return;
  };
  let _ = cx
    .pipewire()
    .audio()
    .set_level(node.id, to_linear(value))
    .log_err();
}

fn toggle_mute(node: Option<&AudioNode>, cx: &App) {
  if let Some(node) = node {
    let _ = cx
      .pipewire()
      .audio()
      .set_mute(node.id, !node.mute)
      .log_err();
  }
}

fn observe<T: 'static>(entity: &Entity<T>, changed: Changed, cx: &mut App) -> Subscription {
  cx.observe(entity, move |_, cx| changed(cx))
}

pub struct Output;

impl DashboardSlider for Output {
  const ID: &'static str = "output";

  fn title() -> SharedString {
    t!("app.dashboard.sliders.output").into()
  }

  fn icon(cx: &App) -> IconName {
    sink(cx).map_or(IconName::VolumeOff, |n| {
      volume_icon(to_slider(n.volume()), n.mute)
    })
  }

  fn value(cx: &App) -> Option<f32> {
    sink(cx).map(|n| to_slider(n.volume()))
  }

  fn set(value: f32, cx: &mut App) {
    set_volume(sink(cx), value, cx);
  }

  fn muted(cx: &App) -> Option<bool> {
    Some(sink(cx).is_some_and(|n| n.mute))
  }

  fn toggle_mute(cx: &mut App) {
    toggle_mute(sink(cx), cx);
  }

  fn page() -> Option<&'static str> {
    Some(AudioPanel::ID)
  }

  fn observe(changed: Changed, cx: &mut App) -> Vec<Subscription> {
    let sink = cx.pipewire().default_sink.clone();
    vec![observe(&sink, changed, cx)]
  }
}

pub struct Input;

impl DashboardSlider for Input {
  const ID: &'static str = "input";

  fn title() -> SharedString {
    t!("app.dashboard.sliders.input").into()
  }

  fn icon(cx: &App) -> IconName {
    match source(cx) {
      Some(n) if !n.mute => IconName::Mic,
      _ => IconName::MicOff,
    }
  }

  fn value(cx: &App) -> Option<f32> {
    source(cx).map(|n| to_slider(n.volume()))
  }

  fn set(value: f32, cx: &mut App) {
    set_volume(source(cx), value, cx);
  }

  fn muted(cx: &App) -> Option<bool> {
    Some(source(cx).is_some_and(|n| n.mute))
  }

  fn toggle_mute(cx: &mut App) {
    toggle_mute(source(cx), cx);
  }

  fn page() -> Option<&'static str> {
    Some(AudioPanel::ID)
  }

  fn observe(changed: Changed, cx: &mut App) -> Vec<Subscription> {
    let source = cx.pipewire().default_source.clone();
    vec![observe(&source, changed, cx)]
  }
}

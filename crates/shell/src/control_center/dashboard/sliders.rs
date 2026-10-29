use std::time::Duration;

use corona_brightness::{BrightnessExt, Display};
use corona_compositor::CompositorExt;
use corona_config::ConfigProvider;
use corona_pipewire::{
  AudioNode, PipewireExt,
  volume::{to_linear, to_slider},
};
use corona_surface::{
  control_center::{ControlCenter, ControlCenterPage},
  dashboard::{Changed, DashboardExt, DashboardRegistry, DashboardSlider, SliderEntry},
};
use corona_utils::error::ErrorLogExt;
use corona_utils::error::spawn_logged;
use gpui_kit::{
  App, AppContext, Context, Div, Entity, ParentElement, SharedString, Styled, Subscription, Window,
  assets::IconName,
  base::slider::{SliderEvent, SliderState},
  component::{
    ActiveTheme,
    button::{Button, ButtonVariant, ButtonVariants},
    slider::Slider,
  },
  div,
  prelude::FluentBuilder,
};
use rust_i18n::t;

use crate::{
  control_center::{AudioPanel, BrightnessPanel, dashboard::DashboardPanel},
  icons::volume_icon,
};

fn sink(cx: &App) -> Option<&AudioNode> {
  cx.pipewire().default_sink(cx)
}

fn source(cx: &App) -> Option<&AudioNode> {
  cx.pipewire().default_source(cx)
}

/// the focused monitor's display, or the first usable one
pub(crate) fn display(cx: &App) -> Option<&Display> {
  let focused = &cx.compositor().active_monitor(cx).name;
  pick_display(cx.brightness().list_displays(cx), focused)
}

fn pick_display<'d>(displays: &'d [Display], focused: &str) -> Option<&'d Display> {
  let usable = || displays.iter().filter(|d| d.unavailable.is_none());
  usable()
    .find(|d| d.output.as_deref() == Some(focused))
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

pub struct Brightness;

impl DashboardSlider for Brightness {
  const ID: &'static str = "brightness";
  // each step is a DDC/CI or sysfs write
  const THROTTLE: Option<Duration> = Some(Duration::from_millis(100));

  fn title() -> SharedString {
    t!("app.dashboard.sliders.brightness").into()
  }

  fn icon(_: &App) -> IconName {
    IconName::Sun
  }

  fn value(cx: &App) -> Option<f32> {
    display(cx).map(|d| d.percent() / 100.)
  }

  fn set(value: f32, cx: &mut App) {
    let Some(display) = display(cx) else {
      return;
    };
    let raw = (value * display.max as f32).round() as u32;
    let task = cx.brightness().set_brightness(&display.id, raw, cx);
    spawn_logged(cx, task);
  }

  fn page() -> Option<&'static str> {
    Some(BrightnessPanel::ID)
  }

  fn observe(changed: Changed, cx: &mut App) -> Vec<Subscription> {
    let displays = cx.brightness().displays.clone();
    let monitor = cx.compositor().active_monitor.clone();
    vec![
      observe(&displays, changed.clone(), cx),
      observe(&monitor, changed, cx),
    ]
  }
}

pub(crate) fn register(cx: &mut App) {
  cx.dashboard_mut()
    .slider::<Output>()
    .slider::<Input>()
    .slider::<Brightness>();
}

/// A slider's state on the open dashboard
struct Row {
  id: &'static str,
  state: Entity<SliderState>,
  /// a throttled slider follows the drag, not the backend, until released
  dragging: bool,
  pending: Option<f32>,
}

pub(super) struct Sliders {
  rows: Vec<Row>,
  _subscriptions: Vec<Subscription>,
}

fn entry<'a>(cx: &'a App, id: &str) -> Option<&'a SliderEntry> {
  cx.dashboard().sliders().iter().find(|s| s.id == id)
}

impl Sliders {
  /// A row for every registered slider, `changed` resyncs them
  pub fn new(changed: &Changed, cx: &mut Context<DashboardPanel>) -> Self {
    let entries: Vec<_> = (cx.dashboard().sliders().iter())
      .map(|s| (s.id, s.observe))
      .collect();
    let mut rows = Vec::new();
    let mut subscriptions = Vec::new();
    for (index, (id, observe)) in entries.into_iter().enumerate() {
      let state = cx.new(|_| SliderState::new().min(0.).max(1.).step(0.01));
      subscriptions.push(
        cx.subscribe(&state, move |this, _, event: &SliderEvent, cx| {
          this.sliders.on_event(index, event, cx)
        }),
      );
      subscriptions.extend(observe(changed.clone(), cx));
      rows.push(Row {
        id,
        state,
        dragging: false,
        pending: None,
      });
    }
    Self {
      rows,
      _subscriptions: subscriptions,
    }
  }

  fn on_event(&mut self, index: usize, event: &SliderEvent, cx: &mut Context<DashboardPanel>) {
    let Some(entry) = entry(cx, self.rows[index].id) else {
      return;
    };
    let (set, throttle) = (entry.set, entry.throttle);
    let Some(throttle) = throttle else {
      if let SliderEvent::Change(value) = event {
        set(value.start(), cx);
      }
      return;
    };
    let row = &mut self.rows[index];
    match event {
      SliderEvent::Change(value) => {
        row.dragging = true;
        if row.pending.replace(value.start()).is_none() {
          cx.spawn(async move |panel, cx| {
            cx.background_executor().timer(throttle).await;
            let _ = panel.update(cx, |panel, cx| {
              if let Some(value) = panel.sliders.rows[index].pending.take() {
                set(value, cx);
              }
            });
          })
          .detach();
        }
        cx.notify();
      }
      SliderEvent::Release(value) => {
        row.dragging = false;
        row.pending = None;
        set(value.start(), cx);
      }
    }
  }

  /// Moves the sliders to the backends' values, all but ones being dragged
  pub fn sync(&mut self, window: &mut Window, cx: &mut Context<DashboardPanel>) {
    for row in &self.rows {
      let value = entry(cx, row.id)
        .and_then(|e| (e.value)(cx))
        .filter(|_| !row.dragging);
      let Some(value) = value else {
        continue;
      };
      row.state.update(cx, |state, cx| {
        if (state.value().start() - value).abs() > 0.005 {
          state.set_value(value, window, cx);
        }
      });
    }
  }

  pub fn render(&self, cx: &mut Context<DashboardPanel>) -> Div {
    let (entries, _) = DashboardRegistry::pick(
      cx.dashboard().sliders(),
      &cx.config().control_center.sliders,
      |s| s.id,
    );
    let rows: Vec<_> = (entries.into_iter())
      .filter_map(|entry| {
        let row = self.rows.iter().find(|r| r.id == entry.id)?;
        let value = (entry.value)(cx)?;
        // a drag shows where it is, not where the backend got to
        let shown = if row.dragging {
          row.state.read(cx).value().start()
        } else {
          value
        };
        Some(self.row(entry, row, shown * 100., cx))
      })
      .collect();

    div()
      .flex()
      .flex_col()
      .gap_1()
      .p_2()
      .rounded_xl()
      .border_1()
      .border_color(cx.theme().border)
      .bg(cx.theme().colors.accent)
      .children(rows)
  }

  fn row(&self, entry: &SliderEntry, row: &Row, percent: f32, cx: &App) -> Div {
    let icon = Button::new(format!("dashboard-{}-icon", entry.id)).icon((entry.icon)(cx));
    let icon = match (entry.muted)(cx) {
      Some(muted) => {
        let toggle = entry.toggle_mute;
        icon
          .cursor_pointer()
          .when(muted, |b| b.with_variant(ButtonVariant::Danger))
          .on_click(move |_, _, cx| toggle(cx))
      }
      None => icon.ghost(),
    };
    div()
      .flex()
      .items_center()
      .gap_1()
      .child(icon)
      .child(
        div()
          .flex_1()
          .child(Slider::new(&row.state).px_2().cursor_pointer()),
      )
      .child(
        div()
          .w_10()
          .flex()
          .justify_end()
          .text_sm()
          .text_color(cx.theme().muted_foreground)
          .child(format!("{percent:.0}%")),
      )
      .when_some(entry.page, |d, page| {
        d.child(
          Button::new(format!("dashboard-open-{}", entry.id))
            .icon(IconName::ChevronRight)
            .ghost()
            .cursor_pointer()
            .on_click(move |_, window, cx| ControlCenter::navigate_id(page, window, cx)),
        )
      })
  }
}

#[cfg(test)]
mod tests {
  use corona_brightness::{DisplayKind, Unavailable};

  use super::*;

  fn display(id: &str, output: Option<&str>, unavailable: Option<Unavailable>) -> Display {
    Display {
      id: id.into(),
      output: output.map(Into::into),
      name: None,
      kind: DisplayKind::External,
      brightness: 50,
      max: 100,
      unavailable,
    }
  }

  fn picked(displays: &[Display], focused: &str) -> Option<String> {
    pick_display(displays, focused).map(|d| d.id.clone())
  }

  #[test]
  fn pick_display_prefers_focused() {
    let displays = [
      display("a", Some("DP-1"), None),
      display("b", Some("DP-2"), None),
    ];
    assert_eq!(picked(&displays, "DP-2").as_deref(), Some("b"));
    assert_eq!(picked(&displays, "HDMI-1").as_deref(), Some("a"));
  }

  #[test]
  fn pick_display_skips_unavailable() {
    let displays = [
      display("a", Some("DP-1"), Some(Unavailable::Detecting)),
      display("b", None, None),
      display("c", Some("DP-2"), Some(Unavailable::Failed)),
    ];
    // the focused one is unusable, so the first usable one
    assert_eq!(picked(&displays, "DP-2").as_deref(), Some("b"));
    assert_eq!(picked(&displays[..1], "DP-1"), None);
    assert_eq!(picked(&[], "DP-1"), None);
  }
}

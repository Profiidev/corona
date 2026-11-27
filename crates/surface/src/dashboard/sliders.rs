use corona_components::components::throttled_slider::{Moved, ThrottledSlider};
use corona_config::ConfigProvider;

use gpui_kit::{
  App, AppContext, Context, Div, Entity, ParentElement, SharedString, Styled, Subscription, Window,
  assets::IconName,
  base::slider::SliderState,
  component::{
    ActiveTheme,
    button::{Button, ButtonVariant, ButtonVariants},
    slider::Slider,
  },
  div,
  prelude::FluentBuilder,
};

use crate::dashboard::{Changed, DashboardExt, DashboardPanel, DashboardRegistry, SliderEntry};

/// A slider's state on the open dashboard
pub(super) struct Row {
  id: SharedString,
  pub(super) slider: Entity<ThrottledSlider>,
}

pub(super) struct Sliders {
  pub(super) rows: Vec<Row>,
  _subscriptions: Vec<Subscription>,
}

fn entry<'a>(cx: &'a App, id: &str) -> Option<&'a SliderEntry> {
  cx.dashboard().sliders().iter().find(|s| s.id == id)
}

impl Sliders {
  pub fn empty() -> Self {
    Self {
      rows: Vec::new(),
      _subscriptions: Vec::new(),
    }
  }

  /// A row for every registered slider, `changed` resyncs them
  pub fn new(changed: &Changed, cx: &mut Context<DashboardPanel>) -> Self {
    let entries: Vec<_> = (cx.dashboard().sliders().iter())
      .map(|s| (s.id.clone(), s.throttle, s.step, s.observe.clone()))
      .collect();
    let mut rows = Vec::new();
    let mut subscriptions = Vec::new();
    for (id, throttle, step, observe) in entries {
      let slider = cx.new(|cx| {
        let state = SliderState::new().min(0.).max(1.).step(step);
        ThrottledSlider::new(state, throttle, cx)
      });
      let sid = id.clone();
      subscriptions.push(cx.subscribe(&slider, move |_, _, Moved(value), cx| {
        if let (Some(value), Some(set)) = (value, entry(cx, &sid).map(|e| e.set.clone())) {
          set(*value, cx);
        }
        cx.notify();
      }));
      subscriptions.extend(observe(changed.clone(), cx));
      rows.push(Row { id, slider });
    }
    Self {
      rows,
      _subscriptions: subscriptions,
    }
  }

  /// Moves the sliders to the backends' values, all but ones being dragged
  pub fn sync(&mut self, window: &mut Window, cx: &mut Context<DashboardPanel>) {
    for row in &self.rows {
      let Some(value) = entry(cx, &row.id).and_then(|e| (e.value)(cx)) else {
        continue;
      };
      row.slider.update(cx, |slider, cx| {
        if (slider.state().read(cx).value().start() - value).abs() > 0.005 {
          slider.sync(value, window, cx);
        }
      });
    }
  }

  pub fn render(&self, cx: &mut Context<DashboardPanel>) -> Div {
    let (entries, _) = DashboardRegistry::pick(
      cx.dashboard().sliders(),
      &cx.config().control_center.sliders,
      |s| &s.id,
    );
    let rows: Vec<_> = (entries.into_iter())
      .filter_map(|entry| {
        let row = self.rows.iter().find(|r| r.id == entry.id)?;
        let value = (entry.value)(cx)?;
        // a drag shows where it is, not where the backend got to
        let shown = row.slider.read(cx).shown(value, cx);
        Some(self.row(entry, row, (entry.label)(shown, cx), cx))
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

  fn row(&self, entry: &SliderEntry, row: &Row, label: SharedString, cx: &App) -> Div {
    let icon = Button::new(format!("dashboard-{}-icon", entry.id))
      .cursor_pointer()
      .icon((entry.icon)(cx));
    let icon = match (entry.muted)(cx) {
      Some(muted) => {
        let toggle = entry.toggle_mute.clone();
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
        div().flex_1().child(
          Slider::new(row.slider.read(cx).state())
            .px_2()
            .cursor_pointer(),
        ),
      )
      .child(
        div()
          .min_w_10()
          .whitespace_nowrap()
          .flex()
          .justify_end()
          .text_sm()
          .text_color(cx.theme().muted_foreground)
          .child(label),
      )
      .when_some(entry.secondary.clone(), |d, secondary| {
        d.child(
          Button::new(format!("dashboard-open-{}", entry.id))
            .icon(IconName::ChevronRight)
            .ghost()
            .cursor_pointer()
            .on_click(move |_, window, cx| super::run_secondary(&secondary, window, cx)),
        )
      })
  }
}

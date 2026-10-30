use corona_config::ConfigProvider;

use gpui_kit::{
  App, AppContext, Context, Div, Entity, ParentElement, Styled, Subscription, Window,
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

use crate::{
  control_center::ControlCenter,
  dashboard::{Changed, DashboardExt, DashboardPanel, DashboardRegistry, SliderEntry},
};

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

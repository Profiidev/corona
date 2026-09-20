use gpui_kit::{
  App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
  Subscription, Window,
  assets::IconName,
  base::{
    FocusableExt, IndexPath,
    slider::{SliderEvent, SliderState},
  },
  component::{
    ActiveTheme,
    button::{Button, ButtonVariant, ButtonVariants},
    select::{Select, SelectEvent, SelectItem, SelectState},
    slider::Slider,
  },
  div,
};

use crate::{
  error::ErrorLogExt,
  integration::pipewire::{AudioNode, PipewireEvent, PipewireExt},
  ui::app::control_center::ControlCenterPanel,
};

pub struct AudioPanel {
  source: NodeState,
  sink: NodeState,
  sources: Vec<AudioNode>,
  sinks: Vec<AudioNode>,
  pipewire_subscription: Subscription,
}

struct NodeState {
  slider: Entity<SliderState>,
  select: Entity<SelectState<Vec<NodeSelectItem>>>,
  _slider_subscription: Subscription,
  _select_subscription: Subscription,
  node: Option<u32>,
  muted: bool,
  volume: f32,
}

#[derive(Clone, Debug)]
struct NodeSelectItem {
  id: u32,
  name: String,
}

impl SelectItem for NodeSelectItem {
  type Value = u32;

  fn title(&self) -> gpui_kit::SharedString {
    self.name.clone().into()
  }

  fn value(&self) -> &Self::Value {
    &self.id
  }
}

fn to_slider(linear: f32) -> f32 {
  linear.cbrt()
}

fn to_linear(slider: f32) -> f32 {
  slider.powi(3)
}

impl ControlCenterPanel for AudioPanel {
  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self {
    let audio = cx.pipewire().audio();
    let sources = audio.list_sources();
    let sinks = audio.list_sinks();

    let source = audio.default_source();
    let sink = audio.default_sink();

    let emitter = cx.pipewire().emitter().clone();
    let pipewire_subscription = cx.subscribe_in(&emitter, window, |this, _, event, window, cx| {
      match event {
        PipewireEvent::AudioNodeChanged(id) => {
          let audio = cx.pipewire().audio();
          let node = audio.node(*id);
          if this.source.node == Some(*id) {
            this.source.update(node.clone(), window, cx);
          }
          if this.sink.node == Some(*id) {
            this.sink.update(node, window, cx);
          }
        }
        _ => (),
      }
      cx.notify();
    });

    let source = NodeState::create(source, &sources, window, cx);
    let sink = NodeState::create(sink, &sinks, window, cx);

    Self {
      source,
      sink,
      sources,
      sinks,
      pipewire_subscription,
    }
  }
}

impl NodeState {
  fn create(
    node: Option<AudioNode>,
    options: &[AudioNode],
    window: &mut Window,
    cx: &mut App,
  ) -> Self {
    let volume = to_slider(
      node
        .as_ref()
        .and_then(|s| s.volumes.first())
        .copied()
        .unwrap_or(0.),
    );
    let slider = cx.new(|_| {
      SliderState::new()
        .min(0.)
        .max(1.)
        .step(0.01)
        .default_value(volume)
    });

    let options: Vec<NodeSelectItem> = options
      .iter()
      .map(|n| NodeSelectItem {
        id: n.id,
        name: n.nickname.clone().unwrap_or_else(|| n.description.clone()),
      })
      .collect();
    let index = node
      .as_ref()
      .and_then(|n| options.iter().position(|o| o.id == n.id))
      .map(IndexPath::new);
    let select = cx.new(|cx| SelectState::new(options, index, window, cx));

    let state = select.clone();
    let slider_subscription = cx.subscribe(&slider, move |_, event: &SliderEvent, cx| {
      let SliderEvent::Change(value) = event else {
        return;
      };

      let Some(id) = state.read(cx).selected_value().copied() else {
        return;
      };
      let audio = cx.pipewire().audio();
      let channels = audio.node(id).map_or(1, |node| node.volumes.len().max(1));

      audio
        .set_volumes(id, vec![to_linear(value.start()); channels])
        .log_err()
        .ok();
    });

    let select_subscription = cx.subscribe(
      &select,
      move |_, event: &SelectEvent<Vec<NodeSelectItem>>, cx| {
        let SelectEvent::Confirm(Some(value)) = event else {
          return;
        };

        let audio = cx.pipewire().audio();
        audio.set_default(*value).log_err().ok();
      },
    );

    Self {
      node: node.as_ref().map(|s| s.id),
      muted: node.as_ref().map(|s| s.mute).unwrap_or(false),
      slider,
      select,
      volume,
      _slider_subscription: slider_subscription,
      _select_subscription: select_subscription,
    }
  }

  fn update(&mut self, node: Option<AudioNode>, window: &mut Window, cx: &mut App) {
    self.node = node.as_ref().map(|s| s.id);
    self.muted = node.as_ref().map(|s| s.mute).unwrap_or(false);
    self.volume = to_slider(
      node
        .as_ref()
        .and_then(|s| s.volumes.first())
        .copied()
        .unwrap_or(0.),
    );
    self
      .slider
      .update(cx, |state, cx| state.set_value(self.volume, window, cx));
  }
}

impl Render for AudioPanel {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let theme = cx.theme();

    div()
      .flex()
      .flex_col()
      .size_full()
      .gap_2()
      .child(
        div()
          .w_full()
          .h_20()
          .flex()
          .flex_col()
          .gap_2()
          .p_2()
          .bg(theme.tokens.accent)
          .rounded_2xl()
          .child(
            div()
              .flex()
              .items_center()
              .gap_2()
              .child(Select::new(&self.source.select).focus_ring(false))
              .child(
                Button::new("mute_source")
                  .icon(if self.source.muted {
                    IconName::VolumeOff
                  } else if self.source.volume < f32::EPSILON {
                    IconName::VolumeX
                  } else if self.source.volume < 0.5 {
                    IconName::Volume1
                  } else {
                    IconName::Volume2
                  })
                  .with_variant(if self.source.muted {
                    ButtonVariant::Danger
                  } else {
                    ButtonVariant::Default
                  })
                  .on_click({
                    let id = self.source.node;
                    let muted = self.source.muted;
                    move |_, _, cx| {
                      let audio = cx.pipewire().audio();
                      if let Some(id) = id {
                        audio.set_mute(id, !muted).log_err().ok();
                      }
                    }
                  }),
              ),
          )
          .child(
            div()
              .flex()
              .items_center()
              .gap_1()
              .child(Slider::new(&self.source.slider).px_2())
              .child(
                div()
                  .text_sm()
                  .text_color(theme.tokens.muted_foreground)
                  .w_10()
                  .flex()
                  .justify_end()
                  .child(SharedString::from(format!(
                    "{:.0}%",
                    self.source.volume * 100.
                  ))),
              ),
          ),
      )
      .child(
        div()
          .w_full()
          .h_20()
          .flex()
          .flex_col()
          .gap_2()
          .p_2()
          .bg(theme.tokens.accent)
          .rounded_2xl()
          .child(Select::new(&self.sink.select).focus_ring(false))
          .child(Slider::new(&self.sink.slider).px_2()),
      )
      .child(
        div()
          .w_full()
          .flex_grow_1()
          .bg(theme.tokens.accent)
          .rounded_2xl(),
      )
  }
}

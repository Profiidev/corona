use std::{cell::Cell, rc::Rc};

use gpui_kit::{
  App, AppContext, Context, Div, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
  Subscription, Window,
  assets::IconName,
  base::{
    Disableable, FocusableExt, IndexPath, StyledExt,
    slider::{SliderEvent, SliderState},
  },
  component::{
    ActiveTheme, Theme,
    button::{Button, ButtonVariant, ButtonVariants},
    scroll::ScrollableElement,
    select::{Select, SelectEvent, SelectItem, SelectState},
    slider::Slider,
  },
  div,
};

use crate::{
  error::ErrorLogExt,
  integration::{
    desktop::entry::name_for_names,
    pipewire::{AudioNode, NodeType, PipewireAudio, PipewireEvent, PipewireExt},
  },
  ui::{app::control_center::ControlCenterPanel, components::window_icon::WindowIcon},
};

pub struct AudioPanel {
  source: DefaultState,
  sink: DefaultState,
  streams: Vec<StreamState>,
  sources: Vec<AudioNode>,
  sinks: Vec<AudioNode>,
  _pipewire_subscription: Subscription,
}

struct NodeState {
  slider: Entity<SliderState>,
  select: Entity<SelectState<Vec<NodeSelectItem>>>,
  _slider_subscription: Subscription,
  _select_subscription: Subscription,
  // The select picks a device or a target, never which node this row owns, so
  // the slider follows the row and not the selection.
  id: Rc<Cell<Option<u32>>>,
  selected: Option<u32>,
  default_option: bool,
  muted: bool,
  volume: f32,
}

struct DefaultState {
  state: NodeState,
  node: Option<AudioNode>,
}

struct StreamState {
  state: NodeState,
  node: AudioNode,
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

fn select_items(options: &[AudioNode], default_option: bool) -> Vec<NodeSelectItem> {
  let mut items: Vec<NodeSelectItem> = options
    .iter()
    .map(|node| NodeSelectItem {
      id: node.id,
      name: node
        .nickname
        .clone()
        .unwrap_or_else(|| node.description.clone()),
    })
    .collect();

  if default_option {
    items.insert(
      0,
      NodeSelectItem {
        id: u32::MAX,
        name: "Default".to_string(),
      },
    );
  }

  items
}

fn index_of(items: &[NodeSelectItem], selected: Option<u32>) -> Option<IndexPath> {
  let selected = selected?;
  items
    .iter()
    .position(|item| item.id == selected)
    .map(IndexPath::new)
}

fn target_of(stream: u32, cx: &App) -> u32 {
  cx.pipewire()
    .audio()
    .target(stream)
    .map_or(u32::MAX, |node| node.id)
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
    let streams = audio.list_streams();

    let source = audio.default_source();
    let sink = audio.default_sink();

    let emitter = cx.pipewire().emitter().clone();
    let pipewire_subscription = cx.subscribe_in(&emitter, window, |this, _, event, window, cx| {
      let audio = cx.pipewire().audio();
      match event {
        PipewireEvent::AudioNodeChanged(id) => {
          let node = audio.node(*id);
          if this.source.node.as_ref().map(|n| n.id) == Some(*id) {
            this.source.update(node, window, cx);
          } else if this.sink.node.as_ref().map(|n| n.id) == Some(*id) {
            this.sink.update(node, window, cx);
          } else if let Some(stream) = this.streams.iter_mut().find(|s| s.node.id == *id)
            && let Some(node) = node
          {
            stream.update(node, window, cx);
          }
        }
        PipewireEvent::AudioDefaultChanged(NodeType::Sink) => {
          let node = audio.default_sink();
          this.sink.update(node, window, cx);
        }
        PipewireEvent::AudioDefaultChanged(NodeType::Source) => {
          let node = audio.default_source();
          this.source.update(node, window, cx);
        }
        PipewireEvent::AudioTargetChanged(id) => {
          if let Some(stream) = this.streams.iter_mut().find(|s| s.node.id == *id)
            && let Some(node) = audio.node(*id)
          {
            stream.update(node, window, cx);
          }
        }
        PipewireEvent::AudioNodeAdded(id) => {
          let Some(node) = audio.node(*id) else {
            return;
          };

          match node.kind {
            NodeType::Sink => {
              this.sinks = audio.list_sinks();
              this.sink.update_options(&this.sinks, window, cx);
              for stream in this.streams.iter_mut() {
                stream.update_options(&this.sinks, window, cx);
              }
            }
            NodeType::Source => {
              this.sources = audio.list_sources();
              this.source.update_options(&this.sources, window, cx);
            }
            NodeType::Stream => {
              let stream = node.id;
              this.streams.push(StreamState::create(
                node,
                &this.sinks,
                window,
                cx,
                move |id, audio| {
                  if id == u32::MAX {
                    audio.reset_target(stream).log_err().ok();
                    return;
                  }
                  audio.set_target(stream, id).log_err().ok();
                },
              ));
              this.streams.sort_unstable_by_key(|s| s.node.id);
            }
          }
        }
        PipewireEvent::AudioNodeRemoved(id) => {
          if let Some(index) = this.sinks.iter().position(|s| s.id == *id) {
            this.sinks.remove(index);
            if this.sink.node.as_ref().map(|node| node.id) == Some(*id) {
              this.sink.update(None, window, cx);
            }
            this.sink.update_options(&this.sinks, window, cx);
            for stream in this.streams.iter_mut() {
              stream.update_options(&this.sinks, window, cx);
            }
          } else if let Some(index) = this.sources.iter().position(|s| s.id == *id) {
            this.sources.remove(index);
            if this.source.node.as_ref().map(|node| node.id) == Some(*id) {
              this.source.update(None, window, cx);
            }
            this.source.update_options(&this.sources, window, cx);
          } else if let Some(index) = this.streams.iter().position(|s| s.node.id == *id) {
            this.streams.remove(index);
          }
        }
        _ => (),
      }
      cx.notify();
    });

    let source = DefaultState::create(source, &sources, window, cx);
    let sink = DefaultState::create(sink, &sinks, window, cx);

    let streams = streams
      .into_iter()
      .map(|s| {
        let stream = s.id;
        let on_select = move |id, audio: PipewireAudio<'_>| {
          if id == u32::MAX {
            audio.reset_target(stream).log_err().ok();
            return;
          }
          audio.set_target(stream, id).log_err().ok();
        };

        StreamState::create(s, &sinks, window, cx, on_select)
      })
      .collect::<Vec<_>>();

    Self {
      source,
      sink,
      streams,
      sources,
      sinks,
      _pipewire_subscription: pipewire_subscription,
    }
  }
}

impl NodeState {
  fn create(
    node: Option<&AudioNode>,
    selected: Option<u32>,
    options: &[AudioNode],
    window: &mut Window,
    cx: &mut App,
    default_option: bool,
    on_select: impl Fn(u32, PipewireAudio<'_>) + 'static,
  ) -> Self {
    let volume = to_slider(node.and_then(|s| s.volumes.first()).copied().unwrap_or(0.));
    let slider = cx.new(|_| {
      SliderState::new()
        .min(0.)
        .max(1.)
        .step(0.01)
        .default_value(volume)
    });

    let items = select_items(options, default_option);
    let index = index_of(&items, selected);
    let select = cx.new(|cx| SelectState::new(items, index, window, cx));

    let id = Rc::new(Cell::new(node.map(|node| node.id)));
    let slider_subscription = cx.subscribe(&slider, {
      let id = id.clone();
      move |_, event: &SliderEvent, cx| {
        let SliderEvent::Change(value) = event else {
          return;
        };

        let Some(id) = id.get() else {
          return;
        };
        let audio = cx.pipewire().audio();
        let channels = audio.node(id).map_or(1, |node| node.volumes.len().max(1));

        audio
          .set_volumes(id, vec![to_linear(value.start()); channels])
          .log_err()
          .ok();
      }
    });

    let select_subscription = cx.subscribe(
      &select,
      move |_, event: &SelectEvent<Vec<NodeSelectItem>>, cx| {
        let SelectEvent::Confirm(Some(value)) = event else {
          return;
        };

        let audio = cx.pipewire().audio();
        on_select(*value, audio);
      },
    );

    Self {
      muted: node.map(|s| s.mute).unwrap_or(false),
      id,
      selected,
      default_option,
      slider,
      select,
      volume,
      _slider_subscription: slider_subscription,
      _select_subscription: select_subscription,
    }
  }

  fn update(
    &mut self,
    node: Option<&AudioNode>,
    selected: Option<u32>,
    window: &mut Window,
    cx: &mut App,
  ) {
    self.muted = node.map(|s| s.mute).unwrap_or(false);
    self.volume = to_slider(node.and_then(|s| s.volumes.first()).copied().unwrap_or(0.));
    self.id.set(node.map(|node| node.id));

    let volume = self.volume;
    self.slider.update(cx, |state, cx| {
      // The epsilon keeps a drag from fighting the echo of its own command.
      if (state.value().start() - volume).abs() > 0.005 {
        state.set_value(volume, window, cx);
      }
    });

    if self.selected != selected {
      self.selected = selected;
      self.select.update(cx, |state, cx| match selected {
        Some(value) => state.set_selected_value(&value, window, cx),
        None => state.set_selected_index(None, window, cx),
      });
    }
  }

  fn update_options(&mut self, options: &[AudioNode], window: &mut Window, cx: &mut App) {
    // set_items swaps the list without touching the selection, so a stale entry
    // would survive the device it named.
    let items = select_items(options, self.default_option);
    let index = index_of(&items, self.selected);

    self.select.update(cx, |state, cx| {
      state.set_items(items, window, cx);
      state.set_selected_index(index, window, cx);
    });
  }
}

impl StreamState {
  fn create(
    node: AudioNode,
    options: &[AudioNode],
    window: &mut Window,
    cx: &mut App,
    on_select: impl Fn(u32, PipewireAudio<'_>) + 'static,
  ) -> Self {
    let target = target_of(node.id, cx);
    let state = NodeState::create(
      Some(&node),
      Some(target),
      options,
      window,
      cx,
      true,
      on_select,
    );

    Self { state, node }
  }

  fn update(&mut self, node: AudioNode, window: &mut Window, cx: &mut App) {
    let target = target_of(node.id, cx);
    self.state.update(Some(&node), Some(target), window, cx);
    self.node = node;
  }

  fn update_options(&mut self, options: &[AudioNode], window: &mut Window, cx: &mut App) {
    self.state.update_options(options, window, cx);
  }
}

impl DefaultState {
  fn create(
    node: Option<AudioNode>,
    options: &[AudioNode],
    window: &mut Window,
    cx: &mut App,
  ) -> Self {
    let selected = node.as_ref().map(|node| node.id);
    let state = NodeState::create(
      node.as_ref(),
      selected,
      options,
      window,
      cx,
      false,
      |id, audio| {
        audio.set_default(id).log_err().ok();
      },
    );

    Self { state, node }
  }

  fn update(&mut self, node: Option<AudioNode>, window: &mut Window, cx: &mut App) {
    let selected = node.as_ref().map(|node| node.id);
    self.state.update(node.as_ref(), selected, window, cx);
    self.node = node;
  }

  fn update_options(&mut self, options: &[AudioNode], window: &mut Window, cx: &mut App) {
    self.state.update_options(options, window, cx);
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
      .child(audio_node("Input", theme, &self.source, true))
      .child(audio_node("Output", theme, &self.sink, false))
      .child(
        div()
          .w_full()
          .flex_grow_1()
          .min_h_0()
          .bg(theme.tokens.accent)
          .rounded_xl()
          .p_2()
          .child(
            div()
              .flex()
              .flex_col()
              .gap_1()
              .overflow_y_scrollbar()
              .children(
                self
                  .streams
                  .iter()
                  .map(|node| audio_stream(theme, node, false)),
              ),
          ),
      )
  }
}

fn audio_btns(state: &NodeState, node: Option<&AudioNode>, mic: bool) -> Div {
  div()
    .flex_grow_1()
    .flex()
    .items_center()
    .gap_2()
    .child(
      Select::new(&state.select)
        .focus_ring(false)
        .cursor_pointer()
        .disabled(node.is_none()),
    )
    .child(
      Button::new(format!(
        "mute_{}",
        node.map_or(if mic { -1 } else { -2 }, |n| i64::from(n.id))
      ))
      .disabled(node.is_none())
      .cursor_pointer()
      .icon(if mic {
        if state.muted {
          IconName::MicOff
        } else {
          IconName::Mic
        }
      } else {
        if state.muted {
          IconName::VolumeOff
        } else if state.volume < f32::EPSILON {
          IconName::VolumeX
        } else if state.volume < 0.5 {
          IconName::Volume1
        } else {
          IconName::Volume2
        }
      })
      .with_variant(if state.muted {
        ButtonVariant::Danger
      } else {
        ButtonVariant::Default
      })
      .on_click({
        let id = node.map(|n| n.id).unwrap_or(0);
        let muted = state.muted;
        move |_, _, cx| {
          let audio = cx.pipewire().audio();
          audio.set_mute(id, !muted).log_err().ok();
        }
      }),
    )
}

fn audio_slider(state: &NodeState, node: Option<&AudioNode>, theme: &Theme) -> Div {
  div()
    .flex()
    .items_center()
    .gap_1()
    .child(
      Slider::new(&state.slider)
        .px_2()
        .cursor_pointer()
        .disabled(node.is_none()),
    )
    .child(
      div()
        .text_sm()
        .text_color(theme.tokens.muted_foreground)
        .w_10()
        .flex()
        .justify_end()
        .child(SharedString::from(format!("{:.0}%", state.volume * 100.))),
    )
}

fn audio_controls(theme: &Theme, node: &DefaultState, mic: bool) -> Div {
  div()
    .w_full()
    .flex()
    .flex_col()
    .gap_1()
    .child(audio_btns(&node.state, node.node.as_ref(), mic))
    .child(audio_slider(&node.state, node.node.as_ref(), theme))
}

fn audio_node(title: impl IntoElement, theme: &Theme, node: &DefaultState, mic: bool) -> Div {
  div()
    .w_full()
    .flex()
    .flex_col()
    .gap_2()
    .p_2()
    .bg(theme.tokens.accent)
    .rounded_xl()
    .child(div().child(title).text_sm().font_bold())
    .child(audio_controls(theme, node, mic))
}

fn audio_stream(theme: &Theme, node: &StreamState, mic: bool) -> Div {
  let name = node
    .node
    .nickname
    .clone()
    .unwrap_or_else(|| node.node.description.clone());

  let label = node
    .node
    .nickname
    .clone()
    .or_else(|| name_for_names(node.node.app.iter().map(String::as_str)))
    .unwrap_or_else(|| node.node.description.clone());

  div()
    .w_full()
    .flex()
    .p_2()
    .items_center()
    .gap_2()
    .rounded_xl()
    .bg(theme.tokens.background)
    .child(
      WindowIcon::new(name, ("node-icon", node.node.id))
        .names(node.node.app.clone())
        .max_size_10(),
    )
    .child(
      div()
        .w_full()
        .flex()
        .flex_col()
        .child(
          div()
            .flex()
            .gap_2()
            .w_full()
            .items_center()
            .child(div().child(label.clone()).text_sm().font_bold().ml_2())
            .child(audio_btns(&node.state, Some(&node.node), mic)),
        )
        .child(audio_slider(&node.state, Some(&node.node), theme)),
    )
}

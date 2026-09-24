use std::{cell::Cell, rc::Rc};

use gpui_kit::{
  App, AppContext, Entity, Subscription, Window,
  base::slider::{SliderEvent, SliderState},
  component::select::{SelectEvent, SelectState},
};

use crate::{
  error::ErrorLogExt,
  integration::pipewire::{AudioNode, PipewireAudio, PipewireExt},
  ui::app::control_center::audio::utils::{
    NodeSelectItem, index_of, select_items, target_of, to_linear, to_slider,
  },
};

pub const DEFAULT_SINK_ID: u32 = u32::MAX;

pub struct NodeState {
  pub slider: Entity<SliderState>,
  pub select: Entity<SelectState<Vec<NodeSelectItem>>>,
  _slider_subscription: Subscription,
  _select_subscription: Subscription,
  id: Rc<Cell<Option<u32>>>,
  selected: Option<u32>,
  default_option: bool,
  pub muted: bool,
  pub volume: f32,
}

pub struct DefaultState {
  pub state: NodeState,
  pub node: Option<AudioNode>,
}

pub struct StreamState {
  pub state: NodeState,
  pub node: AudioNode,
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
  pub fn create(
    node: AudioNode,
    options: &[AudioNode],
    window: &mut Window,
    cx: &mut App,
    on_select: impl Fn(u32, PipewireAudio<'_>) + 'static,
  ) -> Self {
    let target = target_of(node.id, cx);
    let state = NodeState::create(
      Some(&node),
      Some(target.unwrap_or(DEFAULT_SINK_ID)),
      options,
      window,
      cx,
      true,
      on_select,
    );

    Self { state, node }
  }

  pub fn update(&mut self, node: AudioNode, window: &mut Window, cx: &mut App) {
    let target = target_of(node.id, cx);
    self.state.update(
      Some(&node),
      Some(target.unwrap_or(DEFAULT_SINK_ID)),
      window,
      cx,
    );
    self.node = node;
  }

  pub fn update_options(&mut self, options: &[AudioNode], window: &mut Window, cx: &mut App) {
    self.state.update_options(options, window, cx);
  }
}

impl DefaultState {
  pub fn create(
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

  pub fn update(&mut self, node: Option<AudioNode>, window: &mut Window, cx: &mut App) {
    let selected = node.as_ref().map(|node| node.id);
    self.state.update(node.as_ref(), selected, window, cx);
    self.node = node;
  }

  pub fn update_options(&mut self, options: &[AudioNode], window: &mut Window, cx: &mut App) {
    self.state.update_options(options, window, cx);
  }
}

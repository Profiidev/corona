use gpui_kit::{Context, Subscription, Window};

use crate::{
  error::ErrorLogExt,
  integration::pipewire::{AudioNode, PipewireExt},
  ui::app::control_center::audio::{AudioPanel, state::StreamState},
};

pub fn listeners(cx: &mut Context<AudioPanel>, window: &mut Window) -> [Subscription; 6] {
  let pipewire = cx.pipewire();
  let sinks = pipewire.sinks.clone();
  let sources = pipewire.sources.clone();
  let streams = pipewire.streams.clone();
  let default_sink = pipewire.default_sink.clone();
  let default_source = pipewire.default_source.clone();
  let targets = pipewire.targets.clone();

  [
    cx.observe_in(&sinks, window, |this, e, window, cx| {
      this.sinks = e.read(cx).clone();
      let node = cx.pipewire().default_sink(cx).cloned();
      this.sink.update(node, window, cx);
      this.sink.update_options(&this.sinks, window, cx);
      for stream in this.streams.iter_mut() {
        stream.update_options(&this.sinks, window, cx);
      }
      cx.notify();
    }),
    cx.observe_in(&sources, window, |this, e, window, cx| {
      this.sources = e.read(cx).clone();
      let node = cx.pipewire().default_source(cx).cloned();
      this.source.update(node, window, cx);
      this.source.update_options(&this.sources, window, cx);
      cx.notify();
    }),
    cx.observe_in(&streams, window, |this, e, window, cx| {
      reconcile_streams(this, &e.read(cx).clone(), window, cx);
      cx.notify();
    }),
    cx.observe_in(&default_sink, window, |this, e, window, cx| {
      let node = e.read(cx).clone();
      this.sink.update(node, window, cx);
      cx.notify();
    }),
    cx.observe_in(&default_source, window, |this, e, window, cx| {
      let node = e.read(cx).clone();
      this.source.update(node, window, cx);
      cx.notify();
    }),
    cx.observe_in(&targets, window, |this, _, window, cx| {
      let streams = cx.pipewire().list_streams(cx).to_vec();
      reconcile_streams(this, &streams, window, cx);
      cx.notify();
    }),
  ]
}

fn reconcile_streams(
  this: &mut AudioPanel,
  streams: &[AudioNode],
  window: &mut Window,
  cx: &mut Context<AudioPanel>,
) {
  this
    .streams
    .retain(|state| streams.iter().any(|node| node.id == state.node.id));

  for node in streams {
    match this
      .streams
      .iter_mut()
      .find(|state| state.node.id == node.id)
    {
      Some(state) => state.update(node.clone(), window, cx),
      None => {
        let stream = node.id;
        let state = StreamState::create(node.clone(), &this.sinks, window, cx, move |id, audio| {
          if id == u32::MAX {
            audio.reset_target(stream).log_err().ok();
            return;
          }
          audio.set_target(stream, id).log_err().ok();
        });
        this.streams.push(state);
      }
    }
  }

  this.streams.sort_unstable_by_key(|state| state.node.id);
}

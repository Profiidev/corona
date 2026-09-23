use gpui_kit::{Context, Entity, Subscription, Window};

use crate::{
  error::ErrorLogExt,
  integration::pipewire::{NodeType, PipewireEvent, PipewireEventEmitter, PipewireExt},
  ui::app::control_center::audio::{AudioPanel, state::StreamState},
};

pub fn listener(
  emitter: &Entity<PipewireEventEmitter>,
  cx: &mut Context<AudioPanel>,
  window: &Window,
) -> Subscription {
  cx.subscribe_in(emitter, window, |this, _, event, window, cx| {
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
  })
}

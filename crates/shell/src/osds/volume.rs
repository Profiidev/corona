use corona_pipewire::{AudioNode, PipewireExt, volume::to_slider};
use gpui_kit::{App, assets::IconName};

use crate::{
  icons::volume_icon,
  osds::{
    on_change,
    view::{LevelOsd, show},
  },
};

fn level(node: Option<&AudioNode>) -> Option<(u32, u32, bool)> {
  let node = node?;
  let percent = (to_slider(node.volume()) * 100.).round() as u32;
  Some((node.id, percent, node.mute))
}

pub fn init(cx: &mut App) {
  let pipewire = cx.pipewire();
  let (sink, source) = (
    pipewire.default_sink.clone(),
    pipewire.default_source.clone(),
  );

  on_change(
    &sink,
    cx,
    |cx| level(cx.pipewire().default_sink(cx)),
    |prev, &(id, percent, muted), cx| {
      if prev.0 == id {
        let icon = volume_icon(percent as f32 / 100., muted);
        show(
          LevelOsd {
            icon,
            label: "Volume".into(),
            percent: percent as f32,
            muted,
          },
          cx,
        );
      }
    },
  );
  on_change(
    &source,
    cx,
    |cx| level(cx.pipewire().default_source(cx)),
    |prev, &(id, percent, muted), cx| {
      if prev.0 == id {
        let icon = if muted {
          IconName::MicOff
        } else {
          IconName::Mic
        };
        show(
          LevelOsd {
            icon,
            label: "Microphone".into(),
            percent: percent as f32,
            muted,
          },
          cx,
        );
      }
    },
  );
}

use crate::{AudioNode, PipewireExt, volume::to_slider};
use gpui_kit::{App, assets::IconName};

use crate::ui::{
  icons::volume_icon,
  osd::{VolumeOsd, show},
};
use corona_surface::osd::{LevelOsd, on_change};
use rust_i18n::t;

fn level(node: Option<&AudioNode>) -> Option<(u32, u32, bool)> {
  let node = node?;
  let percent = (to_slider(node.volume()) * 100.).round() as u32;
  Some((node.id, percent, node.mute))
}

pub(super) fn init(cx: &mut App) {
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
        show::<VolumeOsd>(
          LevelOsd {
            icon,
            label: t!("app.osd.volume"),
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
        show::<VolumeOsd>(
          LevelOsd {
            icon,
            label: t!("app.privacy.microphone"),
            percent: percent as f32,
            muted,
          },
          cx,
        );
      }
    },
  );
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::NodeType;

  fn node(volumes: Vec<f32>, mute: bool) -> AudioNode {
    AudioNode {
      id: 7,
      serial: 0,
      kind: NodeType::Sink,
      name: String::new(),
      description: String::new(),
      nickname: None,
      device: None,
      profile_device: None,
      volumes,
      mute,
      app: Vec::new(),
      pid: None,
      ready: true,
    }
  }

  #[test]
  fn levels() {
    assert_eq!(level(None), None);
    assert_eq!(
      level(Some(&node(vec![0.125, 1.], true))),
      Some((7, 50, true))
    );
    assert_eq!(level(Some(&node(vec![1.], false))), Some((7, 100, false)));
    assert_eq!(level(Some(&node(vec![], false))), Some((7, 0, false)));
  }
}

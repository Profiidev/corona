use corona_pipewire::{CaptureKind, PipewireExt};
use gpui_kit::App;

use crate::{
  icons::{capture_icon, capture_label},
  osds::{
    on_change,
    view::{ToggleOsd, show},
  },
  widgets::privacy::hidden,
};

pub fn init(cx: &mut App) {
  let captures = cx.pipewire().captures.clone();
  on_change(
    &captures,
    cx,
    |cx| {
      let mut active: Vec<(CaptureKind, String)> = cx
        .pipewire()
        .list_captures(cx)
        .iter()
        .filter(|c| c.active && !hidden(c.kind, Some(&c.name), cx))
        .map(|c| (c.kind, c.name.clone()))
        .collect();
      active.sort_by_key(|(kind, name)| (*kind as u8, name.clone()));
      active.dedup();
      Some(active)
    },
    |prev, next, cx| {
      let started = next
        .iter()
        .filter(|c| !prev.contains(c))
        .max_by_key(|(_, name)| !name.is_empty());
      if let Some((kind, name)) = started {
        let state = if name.is_empty() {
          "In use".into()
        } else {
          name.clone().into()
        };
        show(
          |k| k.privacy,
          ToggleOsd {
            icon: capture_icon(*kind),
            label: capture_label(*kind).into(),
            state,
            active: true,
          },
          cx,
        );
      }
    },
  );
}

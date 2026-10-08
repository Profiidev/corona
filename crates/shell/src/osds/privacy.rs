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
use rust_i18n::t;

type Capture = (CaptureKind, String);

/// A capture in `next` that is not in `prev`, a named one when there is one
fn started<'a>(prev: &[Capture], next: &'a [Capture]) -> Option<&'a Capture> {
  next
    .iter()
    .filter(|c| !prev.contains(c))
    .max_by_key(|(_, name)| !name.is_empty())
}

pub fn init(cx: &mut App) {
  let captures = cx.pipewire().captures.clone();
  on_change(
    &captures,
    cx,
    |cx| {
      let mut active: Vec<Capture> = cx
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
      if let Some((kind, name)) = started(prev, next) {
        let state = if name.is_empty() {
          t!("app.osd.in_use")
        } else {
          name.clone().into()
        };
        show(
          |k| k.privacy,
          ToggleOsd {
            icon: capture_icon(*kind),
            label: capture_label(*kind),
            state,
            active: true,
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

  fn c(kind: CaptureKind, name: &str) -> Capture {
    (kind, name.into())
  }

  #[test]
  fn only_new_captures() {
    let mic = c(CaptureKind::Microphone, "Discord");
    assert_eq!(started(&[], std::slice::from_ref(&mic)), Some(&mic));
    assert_eq!(
      started(std::slice::from_ref(&mic), std::slice::from_ref(&mic)),
      None
    );
    assert_eq!(started(std::slice::from_ref(&mic), &[]), None);
  }

  #[test]
  fn named_capture_wins() {
    let next = [
      c(CaptureKind::Camera, ""),
      c(CaptureKind::Screen, "OBS"),
      c(CaptureKind::Microphone, ""),
    ];
    assert_eq!(started(&[], &next), Some(&next[1]));
    let unnamed = [c(CaptureKind::Camera, "")];
    assert_eq!(started(&[], &unnamed), Some(&unnamed[0]));
  }
}

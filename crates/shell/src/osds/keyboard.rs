use corona_compositor::CompositorExt;
use gpui_kit::{App, assets::IconName};

use crate::osds::{kinds::KeyboardLayoutOsd, show};
use corona_surface::osd::{ToggleOsd, on_change};
use rust_i18n::t;

pub fn init(cx: &mut App) {
  let layout = cx.compositor().keyboard_layout.clone();
  on_change(
    &layout,
    cx,
    |cx| cx.compositor().keyboard_layout(cx).map(str::to_string),
    |_, layout, cx| {
      show::<KeyboardLayoutOsd>(
        ToggleOsd {
          icon: IconName::Keyboard,
          label: t!("app.osd.keyboard_layout"),
          state: layout.clone().into(),
          active: true,
        },
        cx,
      );
    },
  );
}

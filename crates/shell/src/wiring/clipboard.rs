//! Pastes the launcher's picks through the clipboard

use corona_clipboard::paste;
use corona_compositor::CompositorExt;
use gpui_kit::{App, AppContext};

pub(super) fn init(cx: &mut App) {
  corona_launcher::paste::set_paster(
    |_, cx| {
      // the window the launcher hands the keyboard back to
      let terminal = cx
        .compositor()
        .active_window(cx)
        .is_some_and(|w| paste::is_terminal(&w.class));
      cx.background_spawn(async move {
        std::thread::sleep(std::time::Duration::from_millis(150));
        paste::paste(terminal)
      })
    },
    cx,
  );
}

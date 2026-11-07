//! Connects integrations that do not know of each other

use gpui_kit::App;

mod notifications;

pub(crate) fn wire_all(cx: &mut App) {
  notifications::init(cx);
}

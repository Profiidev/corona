use std::time::Duration;

use gpui_kit::{Context, Task, Window};

mod base;
mod state;
mod text;
mod variants;

pub use state::{TooltipExt, TooltipState};
pub use text::TextTooltip;
pub use variants::Tooltip;

/// How long the pointer rests on something before its tooltip shows
const HOVER_DELAY: Duration = Duration::from_millis(200);

/// Calls `show` once the pointer rested on the hovered element, dropping the task cancels it
pub fn after_hover_delay<V: 'static>(
  window: &Window,
  cx: &mut Context<V>,
  show: impl FnOnce(&mut V, &mut Window, &mut Context<V>) + 'static,
) -> Task<()> {
  cx.spawn_in(window, async move |e, cx| {
    cx.background_executor().timer(HOVER_DELAY).await;
    e.update_in(cx, show).ok();
  })
}

use std::{
  sync::atomic::{AtomicU64, Ordering},
  time::Duration,
};

use corona_js::TooltipHost;
use corona_surface::tooltip::{TextTooltip, TooltipExt};
use gpui_kit::{
  AnyView, App, AppContext, Bounds, Context, IntoElement, Render, Window, div, px, size,
};

/// Plugin tooltips in a bar are a popup of the bar, gpui's own would be cut off at its edge
pub fn init(cx: &mut App) {
  cx.set_global(TooltipHost(bar_tooltip));
}

/// Gone with the hover, which drops this view
struct Hover;

impl Render for Hover {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    div()
  }
}

/// The popup of the latest hover, 0 for none; an older hover's end must not close a newer popup
static CURRENT: AtomicU64 = AtomicU64::new(0);
static NEXT: AtomicU64 = AtomicU64::new(1);
const POLL: Duration = Duration::from_millis(100);

fn hide(id: u64, cx: &mut App) {
  if CURRENT
    .compare_exchange(id, 0, Ordering::Relaxed, Ordering::Relaxed)
    .is_ok()
  {
    cx.hide_tooltip::<TextTooltip>();
  }
}

fn bar_tooltip(text: &str, window: &mut Window, cx: &mut App) -> Option<AnyView> {
  let anchor = Bounds::new(window.mouse_position(), size(px(1.), px(1.)));
  cx.show_bar_tooltip(TextTooltip::new(text.to_string()), anchor, window)
    .ok()?;
  let id = NEXT.fetch_add(1, Ordering::Relaxed);
  CURRENT.store(id, Ordering::Relaxed);

  let hover = cx.new(|_| Hover);
  cx.observe_release(&hover, move |_, cx| hide(id, cx))
    .detach();
  // gpui only drops the view once the pointer moves in the window or it redraws, so a pointer
  // leaving the window straight off the widget would leave the popup up
  window
    .spawn(cx, async move |cx| {
      while CURRENT.load(Ordering::Relaxed) == id {
        cx.background_executor().timer(POLL).await;
        match cx.update(|window, _| window.is_window_hovered()) {
          Ok(true) => {}
          _ => return cx.update(|_, cx| hide(id, cx)).unwrap_or_default(),
        }
      }
    })
    .detach();
  Some(hover.into())
}

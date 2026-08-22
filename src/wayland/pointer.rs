use slint::{
  LogicalPosition,
  platform::{PointerEventButton, WindowEvent},
};
use smithay_client_toolkit::seat::pointer::{
  AxisScroll, BTN_BACK, BTN_FORWARD, BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, PointerEvent,
  PointerEventKind, PointerHandler,
};
use wayland_client::{Connection, Proxy, QueueHandle, protocol::wl_pointer};

use super::Dispatcher;

/// Logical pixels a single wheel notch scrolls, matching Slint's own winit backend.
const SCROLL_LINE_HEIGHT: f32 = 60.0;

impl PointerHandler for Dispatcher {
  fn pointer_frame(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    _pointer: &wl_pointer::WlPointer,
    events: &[PointerEvent],
  ) {
    for event in events {
      let Some(window) = self.corona.widgets.window(&event.surface.id()) else {
        continue;
      };

      // Surface-local coordinates are already logical pixels.
      let position = LogicalPosition::new(event.position.0 as f32, event.position.1 as f32);

      window.dispatch(match event.kind {
        PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
          WindowEvent::PointerMoved { position }
        }
        PointerEventKind::Leave { .. } => WindowEvent::PointerExited,
        PointerEventKind::Press { button, .. } => WindowEvent::PointerPressed {
          position,
          button: pointer_button(button),
        },
        PointerEventKind::Release { button, .. } => WindowEvent::PointerReleased {
          position,
          button: pointer_button(button),
        },
        PointerEventKind::Axis {
          horizontal,
          vertical,
          ..
        } => WindowEvent::PointerScrolled {
          position,
          delta_x: scroll_delta(&horizontal),
          delta_y: scroll_delta(&vertical),
        },
      });
    }
  }
}

fn pointer_button(button: u32) -> PointerEventButton {
  match button {
    BTN_LEFT => PointerEventButton::Left,
    BTN_RIGHT => PointerEventButton::Right,
    BTN_MIDDLE => PointerEventButton::Middle,
    BTN_BACK => PointerEventButton::Back,
    BTN_FORWARD => PointerEventButton::Forward,
    _ => PointerEventButton::Other,
  }
}

/// Wayland scrolls positive towards the user, Slint positive away from it.
fn scroll_delta(axis: &AxisScroll) -> f32 {
  if axis.discrete != 0 {
    -axis.discrete as f32 * SCROLL_LINE_HEIGHT
  } else {
    -axis.absolute as f32
  }
}

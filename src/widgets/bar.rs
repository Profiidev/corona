use std::rc::Rc;

use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};
use wayland_client::{Proxy, protocol::wl_output::WlOutput};

use crate::{
  Corona,
  adapter::{slint::SlintWindow, wayland::LayerSurfaceSpec},
  widgets::{PendingWidget, WidgetKind},
};

pub struct Bar {
  pub(super) window: Rc<SlintWindow>,
}

impl Corona {
  pub fn create_bar(&mut self, output: &WlOutput) {
    let id = output.id();
    let surface = self.wayland.create_layer_surface(LayerSurfaceSpec {
      namespace: "corona-bar".into(),
      layer: Layer::Top,
      anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
      width: 0,
      height: 30,
      exclusive_zone: 30,
      output: Some(output),
      keyboard_interactivity: KeyboardInteractivity::None,
    });

    self.widgets.pending.insert(
      id,
      PendingWidget {
        layer_surface: surface,
        kind: WidgetKind::Bar,
      },
    );
  }
}

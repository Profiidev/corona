use std::rc::Rc;

use slint::PlatformError;
use smithay_client_toolkit::shell::{
  WaylandSurface,
  wlr_layer::{Anchor, KeyboardInteractivity, Layer},
};
use wayland_client::{Proxy, protocol::wl_output::WlOutput};

use crate::{
  Corona,
  adapter::{slint::SlintWindow, wayland::LayerSurfaceSpec},
  ui::SlintComponent,
  widgets::{PendingWidget, WidgetKind},
};

pub struct Bar {
  pub(super) window: Rc<SlintWindow>,
  pub(super) component: Box<dyn SlintComponent>,
}

impl Corona {
  pub fn create_bar(
    &mut self,
    output: &WlOutput,
    init: impl FnOnce() -> Result<Box<dyn SlintComponent>, PlatformError> + 'static,
  ) {
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
      surface.wl_surface().id(),
      PendingWidget {
        layer_surface: surface,
        kind: WidgetKind::Bar,
        init: Box::new(init),
      },
    );
  }
}

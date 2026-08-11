use std::rc::Rc;

use smithay_client_toolkit::shell::{
  WaylandSurface,
  wlr_layer::{Anchor, KeyboardInteractivity, Layer},
};
use wayland_client::{Proxy, backend::ObjectId, protocol::wl_output::WlOutput};

use crate::{
  Corona,
  adapter::{slint::SlintWindow, wayland::LayerSurfaceSpec},
  widgets::{
    PendingWidget,
    init::{IntoSlintInit, SlintComponent},
  },
};

// field order is important for drop order
pub struct Widget {
  #[allow(dead_code)]
  component: Box<dyn SlintComponent>,
  pub(super) window: Rc<SlintWindow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WidgetHandle(ObjectId);

impl Widget {
  pub fn new(window: Rc<SlintWindow>, component: Box<dyn SlintComponent>) -> Self {
    Self { window, component }
  }
}

impl Corona {
  pub fn create_widget<C>(
    &mut self,
    output: &WlOutput,
    init: impl IntoSlintInit<C>,
  ) -> WidgetHandle {
    let surface = self.wayland.create_layer_surface(LayerSurfaceSpec {
      namespace: "corona-bar".into(),
      layer: Layer::Top,
      anchor: Anchor::TOP | Anchor::LEFT | Anchor::RIGHT,
      width: 0,
      height: 30,
      exclusive_zone: 30,
      output: Some(output),
      keyboard_interactivity: KeyboardInteractivity::OnDemand,
    });
    let id = surface.wl_surface().id();

    self.widgets.pending.insert(
      id.clone(),
      PendingWidget {
        layer_surface: surface,
        init: Box::new(init.into_init()),
      },
    );

    WidgetHandle(id)
  }

  pub fn destroy_widget(&mut self, handle: WidgetHandle) {
    self.widgets.destroy_widget(handle.0);
  }
}

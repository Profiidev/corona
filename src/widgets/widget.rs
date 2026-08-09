use std::{mem::ManuallyDrop, rc::Rc};

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

pub struct Widget {
  pub(super) window: Rc<SlintWindow>,
  component: ManuallyDrop<Box<dyn SlintComponent>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WidgetHandle(ObjectId);

impl Widget {
  pub fn new(window: Rc<SlintWindow>, component: Box<dyn SlintComponent>) -> Self {
    Self {
      window,
      component: ManuallyDrop::new(component),
    }
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
      keyboard_interactivity: KeyboardInteractivity::None,
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

impl Drop for Widget {
  fn drop(&mut self) {
    unsafe { ManuallyDrop::drop(&mut self.component) };
  }
}

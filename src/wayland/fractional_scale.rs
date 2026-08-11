use smithay_client_toolkit::globals::GlobalData;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, protocol::wl_surface::WlSurface};
use wayland_protocols::wp::{
  fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::{Event::PreferredScale, WpFractionalScaleV1},
  },
  viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};

use crate::Corona;

impl Dispatch<WpFractionalScaleManagerV1, GlobalData, Corona> for Corona {
  fn event(
    _: &mut Corona,
    _: &WpFractionalScaleManagerV1,
    _: <WpFractionalScaleManagerV1 as Proxy>::Event,
    _: &GlobalData,
    _: &Connection,
    _: &QueueHandle<Corona>,
  ) {
    unreachable!("WpFractionalScaleManagerV1 has no events")
  }
}

impl Dispatch<WpFractionalScaleV1, WlSurface, Corona> for Corona {
  fn event(
    state: &mut Corona,
    _: &WpFractionalScaleV1,
    event: <WpFractionalScaleV1 as Proxy>::Event,
    surface: &WlSurface,
    _: &Connection,
    _: &QueueHandle<Corona>,
  ) {
    match event {
      PreferredScale { scale } => {
        state.widgets.set_scale(&surface.id(), scale as f64 / 120.);
      }
      _ => unreachable!("WpFractionalScaleV1 should only have a preferred_scale event"),
    }
  }
}

impl Dispatch<WpViewporter, GlobalData, Corona> for Corona {
  fn event(
    _: &mut Corona,
    _: &WpViewporter,
    _: <WpViewporter as Proxy>::Event,
    _: &GlobalData,
    _: &Connection,
    _: &QueueHandle<Corona>,
  ) {
    unreachable!("WpViewporter has no events")
  }
}

impl Dispatch<WpViewport, GlobalData, Corona> for Corona {
  fn event(
    _: &mut Corona,
    _: &WpViewport,
    _: <WpViewport as Proxy>::Event,
    _: &GlobalData,
    _: &Connection,
    _: &QueueHandle<Corona>,
  ) {
    unreachable!("WpViewport has no events")
  }
}

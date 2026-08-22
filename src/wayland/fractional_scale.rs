use smithay_client_toolkit::globals::GlobalData;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, protocol::wl_surface::WlSurface};
use wayland_protocols::wp::{
  fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::{Event::PreferredScale, WpFractionalScaleV1},
  },
  viewporter::client::{wp_viewport::WpViewport, wp_viewporter::WpViewporter},
};

use super::Dispatcher;

impl Dispatch<WpFractionalScaleManagerV1, GlobalData, Dispatcher> for Dispatcher {
  fn event(
    _: &mut Dispatcher,
    _: &WpFractionalScaleManagerV1,
    _: <WpFractionalScaleManagerV1 as Proxy>::Event,
    _: &GlobalData,
    _: &Connection,
    _: &QueueHandle<Dispatcher>,
  ) {
    unreachable!("WpFractionalScaleManagerV1 has no events")
  }
}

impl Dispatch<WpFractionalScaleV1, WlSurface, Dispatcher> for Dispatcher {
  fn event(
    state: &mut Dispatcher,
    _: &WpFractionalScaleV1,
    event: <WpFractionalScaleV1 as Proxy>::Event,
    surface: &WlSurface,
    _: &Connection,
    _: &QueueHandle<Dispatcher>,
  ) {
    match event {
      PreferredScale { scale } => {
        state.set_scale(&surface.id(), scale as f64 / 120.);
      }
      _ => unreachable!("WpFractionalScaleV1 should only have a preferred_scale event"),
    }
  }
}

impl Dispatch<WpViewporter, GlobalData, Dispatcher> for Dispatcher {
  fn event(
    _: &mut Dispatcher,
    _: &WpViewporter,
    _: <WpViewporter as Proxy>::Event,
    _: &GlobalData,
    _: &Connection,
    _: &QueueHandle<Dispatcher>,
  ) {
    unreachable!("WpViewporter has no events")
  }
}

impl Dispatch<WpViewport, GlobalData, Dispatcher> for Dispatcher {
  fn event(
    _: &mut Dispatcher,
    _: &WpViewport,
    _: <WpViewport as Proxy>::Event,
    _: &GlobalData,
    _: &Connection,
    _: &QueueHandle<Dispatcher>,
  ) {
    unreachable!("WpViewport has no events")
  }
}

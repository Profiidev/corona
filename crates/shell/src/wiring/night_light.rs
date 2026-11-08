//! Tells the nightlight where the device is

use corona_brightness::BrightnessExt;
use corona_location::{LocationEvent, LocationExt};
use gpui_kit::App;

pub(super) fn init(cx: &mut App) {
  let events = cx.location().events.clone();
  cx.subscribe(&events, |_, event, cx| {
    if let LocationEvent::Changed(place) = event {
      cx.brightness().set_location(Some((place.latitude, place.longitude)));
    }
  })
  .detach();
  // the cached place is there before the first lookup
  if let Some(place) = cx.location().current(cx) {
    cx.brightness().set_location(Some((place.latitude, place.longitude)));
  }
}

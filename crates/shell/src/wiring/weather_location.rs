//! Tells the forecast where the device is

use corona_location::{LocationEvent, LocationExt};
use corona_weather::WeatherExt;
use gpui_kit::App;

pub(super) fn init(cx: &mut App) {
  let events = cx.location().events.clone();
  cx.subscribe(&events, |_, event, cx| {
    let weather = cx.weather().clone();
    match event {
      LocationEvent::Changed(place) => weather.set_location(place.clone(), cx),
      LocationEvent::Failed(error) => weather.set_location_error(error.clone(), cx),
    }
  })
  .detach();
  // the cached place is there before the first lookup
  if let Some(place) = cx.location().current(cx).cloned() {
    cx.weather().clone().set_location(place, cx);
  }
}

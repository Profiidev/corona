mod active_window;
pub(crate) mod clock;
mod control_center;
mod player;
pub(crate) mod popup;
pub(crate) mod privacy;
mod resource;
mod tray;
mod workspaces;

use active_window::ActiveWindow;
use clock::Clock;
use control_center::{
  AudioButton, BatteryButton, BluetoothButton, BrightnessButton, CalendarButton,
  ControlCenterButton, MediaButton, NetworkButton, NotificationsButton, PowerButton, SysinfoButton,
  WeatherButton,
};
use corona_surface::bar::BarExt;
use gpui_kit::App;
use player::ActivePlayer;
use privacy::Privacy;
use resource::Resource;
use tray::Tray;
use workspaces::widget::Workspaces;

pub fn register_widgets(cx: &mut App) {
  privacy::init_filter(cx);
  cx.bar_mut()
    .register::<ControlCenterButton>()
    .register::<AudioButton>()
    .register::<NetworkButton>()
    .register::<BluetoothButton>()
    .register::<PowerButton>()
    .register::<BatteryButton>()
    .register::<BrightnessButton>()
    .register::<NotificationsButton>()
    .register::<SysinfoButton>()
    .register::<WeatherButton>()
    .register::<CalendarButton>()
    .register::<MediaButton>()
    .register::<Workspaces>()
    .register::<ActiveWindow>()
    .register::<Clock>()
    .register::<ActivePlayer>()
    .register::<Resource>()
    .register::<Tray>()
    .register::<Privacy>();
}

/// `pattern` as a case-insensitive regex; a bad one is logged and matches nothing
pub(crate) fn filter_regex(pattern: &str) -> Option<regex::Regex> {
  if pattern.is_empty() {
    return None;
  }
  regex::RegexBuilder::new(pattern)
    .case_insensitive(true)
    .build()
    .inspect_err(|e| tracing::warn!("bad filter regex {pattern:?}: {e}"))
    .ok()
}

#[cfg(test)]
mod tests {
  use super::filter_regex;

  #[test]
  fn filters() {
    let re = filter_regex("noctalia|obs").unwrap();
    assert!(re.is_match("Noctalia Shell"));
    assert!(re.is_match("com.obsproject.Studio"));
    assert!(!re.is_match("firefox"));
    // nothing set hides nothing, and neither does a broken pattern
    assert!(filter_regex("").is_none());
    assert!(filter_regex("(").is_none());
  }
}

mod active_window;
pub(crate) mod clock;
pub(crate) mod control_center;
mod player;
pub(crate) mod popup;
pub(crate) mod privacy;
pub(crate) mod resource;
mod tooltip;
pub(crate) mod tray;
mod workspaces;

use active_window::ActiveWindow;
use clock::Clock;
use control_center::{
  AudioButton, BatteryButton, BluetoothButton, BrightnessButton, CalendarButton, ClipboardButton,
  ControlCenterButton, DisksButton, DisplaysButton, HotspotButton, MediaButton, NetworkButton,
  NotificationsButton, PowerButton, PrintersButton, SysinfoButton, WeatherButton,
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
    .register::<HotspotButton>()
    .register::<BluetoothButton>()
    .register::<PrintersButton>()
    .register::<DisksButton>()
    .register::<PowerButton>()
    .register::<BatteryButton>()
    .register::<BrightnessButton>()
    .register::<DisplaysButton>()
    .register::<NotificationsButton>()
    .register::<ClipboardButton>()
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

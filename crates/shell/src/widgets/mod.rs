mod active_window;
mod clock;
mod control_center;
mod player;
mod popup;
mod privacy;
mod resource;
mod tray;
mod workspaces;

use active_window::ActiveWindow;
use clock::Clock;
use control_center::{
  AudioButton, BluetoothButton, BrightnessButton, CalendarButton, ControlCenterButton, MediaButton,
  NetworkButton, NotificationsButton, PowerButton, SysinfoButton, WeatherButton,
};
use corona_surface::bar::BarExt;
use gpui_kit::App;
use player::ActivePlayer;
use privacy::Privacy;
use resource::Resource;
use tray::Tray;
use workspaces::widget::Workspaces;

pub fn register_widgets(cx: &mut App) {
  cx.bar_mut()
    .register::<ControlCenterButton>()
    .register::<AudioButton>()
    .register::<NetworkButton>()
    .register::<BluetoothButton>()
    .register::<PowerButton>()
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

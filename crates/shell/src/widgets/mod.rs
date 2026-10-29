mod active_window;
pub(crate) mod control_center;
mod player;
pub(crate) mod privacy;
mod workspaces;

use active_window::ActiveWindow;
use control_center::{
  AudioButton, BatteryButton, BluetoothButton, BrightnessButton, ClipboardButton,
  ControlCenterButton, DisksButton, DisplaysButton, HotspotButton, MediaButton, NetworkButton,
  NotificationsButton, PowerButton,
};
use corona_surface::bar::BarExt;
use gpui_kit::App;
use player::ActivePlayer;
use privacy::Privacy;
use workspaces::widget::Workspaces;

pub fn register_widgets(cx: &mut App) {
  privacy::init_filter(cx);
  cx.bar_mut()
    .register::<ControlCenterButton>()
    .register::<AudioButton>()
    .register::<NetworkButton>()
    .register::<HotspotButton>()
    .register::<BluetoothButton>()
    .register::<DisksButton>()
    .register::<PowerButton>()
    .register::<BatteryButton>()
    .register::<BrightnessButton>()
    .register::<DisplaysButton>()
    .register::<NotificationsButton>()
    .register::<ClipboardButton>()
    .register::<MediaButton>()
    .register::<Workspaces>()
    .register::<ActiveWindow>()
    .register::<ActivePlayer>()
    .register::<Privacy>();
}

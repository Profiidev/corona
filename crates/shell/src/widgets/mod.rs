mod active_window;
mod clock;
mod control_center;
mod player;
mod popup;
mod privacy;
mod resource;
mod tray;
mod workspaces;

pub use active_window::ActiveWindow;
pub use clock::Clock;
pub use control_center::{
  AudioButton, BluetoothButton, BrightnessButton, CalendarButton, ControlCenterButton, MediaButton,
  NetworkButton, NotificationsButton, PowerButton, SysinfoButton, WeatherButton,
};
pub use player::ActivePlayer;
pub use privacy::Privacy;
pub use resource::Resource;
pub use tray::Tray;
pub use workspaces::widget::Workspaces;

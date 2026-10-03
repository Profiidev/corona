mod active_window;
pub mod button;
mod clock;
mod control_center;
mod player;
mod workspaces;

pub use active_window::ActiveWindow;
pub use clock::Clock;
pub use control_center::{
  AudioButton, BluetoothButton, BrightnessButton, CalendarButton, ControlCenterButton, MediaButton,
  NetworkButton, NotificationsButton, PowerButton, SysinfoButton, WeatherButton,
};
pub use player::ActivePlayer;
pub use workspaces::widget::Workspaces;

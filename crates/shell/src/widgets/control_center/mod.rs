use corona_components::assets::icons::IconName;
use corona_surface::bar::{Button, Widget};
use gpui_kit::{Context, IntoElement, Render, Window};
use uuid::Uuid;

use crate::control_center::ControlCenter;

mod audio;
mod battery;
mod bluetooth;
mod brightness;
mod calendar;
mod media;
mod network;
mod notifications;
mod power;
mod sysinfo;
mod weather;

pub use audio::AudioButton;
pub use battery::BatteryButton;
pub use bluetooth::BluetoothButton;
pub use brightness::BrightnessButton;
pub use calendar::CalendarButton;
pub use media::MediaButton;
pub use network::NetworkButton;
pub use notifications::NotificationsButton;
pub use power::PowerButton;
pub use sysinfo::SysinfoButton;
pub use weather::WeatherButton;

pub struct ControlCenterButton;

impl Widget for ControlCenterButton {
  const NAME: &'static str = "control_center";
  type Options = ();

  fn init(_cx: &mut Context<'_, Self>, _display_id: Uuid, _options: Self::Options) -> Self {
    ControlCenterButton
  }
}

impl Render for ControlCenterButton {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    Button::<_, ControlCenter>::new(cx, "control-panel-button", IconName::Nixos)
  }
}

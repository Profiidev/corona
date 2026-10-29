use corona_components::assets::icons::IconName;
use corona_surface::bar::{Button, Widget};
use gpui_kit::{Context, IntoElement, Render, Window};
use uuid::Uuid;

use corona_surface::control_center::ControlCenter;

mod battery;
mod brightness;
mod clipboard;
pub(crate) mod disks;
mod displays;
mod hotspot;
mod network;
mod notifications;
mod power;

pub use battery::BatteryButton;
pub use brightness::BrightnessButton;
pub use clipboard::ClipboardButton;
pub use disks::DisksButton;
pub use displays::DisplaysButton;
pub use hotspot::HotspotButton;
pub use network::NetworkButton;
pub use notifications::NotificationsButton;
pub use power::PowerButton;

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

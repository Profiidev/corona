mod audio;
mod bluetooth;
mod brightness;
mod calendar;
mod dashboard;
mod layout;
mod media;
mod nav;
mod network;
mod notifications;
mod panel;
mod power;
mod sysinfo;
mod variants;
mod weather;

use corona_surface::panel::{AppPanelExt, Panel};
use gpui_kit::{AnyElement, AnyView, App, AppContext, Context, Entity, Render, Window};

pub use crate::control_center::{
  audio::AudioPanel,
  bluetooth::BluetoothPanel,
  brightness::BrightnessPanel,
  calendar::CalendarPanel,
  dashboard::DashboardPanel,
  media::MediaPanel,
  network::NetworkPanel,
  notifications::NotificationsPanel,
  panel::{ControlCenter, Standalone},
  power::PowerPanel,
  sysinfo::SysinfoPanel,
  variants::ControlCenterType,
  weather::WeatherPanel,
};

pub(crate) use crate::control_center::power::battery_icon;

pub fn register_panels(cx: &mut App) {
  cx.panel()
    .register::<ControlCenter>()
    .register::<Standalone<DashboardPanel>>()
    .register::<Standalone<AudioPanel>>()
    .register::<Standalone<NetworkPanel>>()
    .register::<Standalone<BluetoothPanel>>()
    .register::<Standalone<PowerPanel>>()
    .register::<Standalone<BrightnessPanel>>()
    .register::<Standalone<NotificationsPanel>>()
    .register::<Standalone<SysinfoPanel>>()
    .register::<Standalone<WeatherPanel>>()
    .register::<Standalone<CalendarPanel>>()
    .register::<Standalone<MediaPanel>>();
}

pub trait ControlCenterPanel: Render {
  const TYPE: ControlCenterType;
  const HEIGHT: f32 = ControlCenter::HEIGHT;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self;

  fn buttons(&mut self, _cx: &mut Context<Self>) -> Vec<AnyElement> {
    vec![]
  }

  fn handle(window: &mut Window, cx: &mut App) -> Box<dyn ControlCenterPanelHandle> {
    Box::new(cx.new(|cx| Self::init(window, cx)))
  }
}

pub trait ControlCenterPanelHandle {
  fn view(&self) -> AnyView;
  fn buttons(&self, cx: &mut App) -> Vec<AnyElement>;
}

impl<T: ControlCenterPanel> ControlCenterPanelHandle for Entity<T> {
  fn view(&self) -> AnyView {
    self.clone().into()
  }

  fn buttons(&self, cx: &mut App) -> Vec<AnyElement> {
    self.update(cx, |page, cx| page.buttons(cx))
  }
}

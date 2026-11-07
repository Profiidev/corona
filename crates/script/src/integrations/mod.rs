//! The `corona/*` script modules of the integrations

use corona_js::ScriptRegistry;

mod bluez;
mod brightness;
mod idle;
mod mpris;
mod network;
mod notifications;
mod pipewire;
mod power;
mod printing;
mod sysinfo;
mod tray;
mod weather;

pub(crate) fn register(registry: &mut ScriptRegistry) {
  registry.register::<bluez::BluetoothModule>();
  registry.register::<brightness::BrightnessModule>();
  registry.register::<idle::IdleModule>();
  registry.register::<sysinfo::SysinfoModule>();
  registry.register::<tray::TrayModule>();
  registry.register::<weather::WeatherModule>();
  registry.register::<printing::PrintingModule>();
  registry.register::<mpris::MprisModule>();
  registry.register::<network::NetworkModule>();
  registry.register::<pipewire::PipewireModule>();
  registry.register::<power::PowerModule>();
  registry
    .register::<notifications::NotificationsModule>()
    .register::<notifications::NotificationCenterModule>();
}

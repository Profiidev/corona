//! The `corona/*` script modules of the integrations

use corona_js::ScriptRegistry;

mod bluez;
mod printing;
mod sysinfo;

pub(crate) fn register(registry: &mut ScriptRegistry) {
  registry.register::<bluez::BluetoothModule>();
  registry.register::<sysinfo::SysinfoModule>();
  registry.register::<printing::PrintingModule>();
}

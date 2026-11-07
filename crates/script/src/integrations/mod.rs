//! The `corona/*` script modules of the integrations

use corona_js::ScriptRegistry;

mod bluez;

pub(crate) fn register(registry: &mut ScriptRegistry) {
  registry.register::<bluez::BluetoothModule>();
}

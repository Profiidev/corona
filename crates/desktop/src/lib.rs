pub mod apps;
pub mod entry;
pub mod script;

/// Registers `corona/desktop`, for the shell and the CLI
pub fn register_script(registry: &mut corona_js::ScriptRegistry) {
  registry.register::<script::Desktop>();
}

//! Where plugins come from and which of them run: git and directory sources,
//! installing, updating and removing plugins, and the settings they declare.

pub mod catalog;
pub mod git;
pub mod manager;
pub mod manifest;
pub mod materialize;
pub mod paths;
pub mod registry;
pub mod settings;
pub mod worker;

use corona_js::ScriptRegistry;
use serde_json::json;

/// The JSON schema of `plugin.toml`, for editors: `capabilities.corona` lists the modules
/// in `modules`
pub fn plugin_schema(modules: &ScriptRegistry) -> serde_json::Value {
  let mut schema = schemars::schema_for!(manifest::ManifestFile).to_value();
  let names: Vec<_> = (modules.all())
    .map(|m| json!({ "const": m.name, "description": m.doc }))
    .collect();
  schema["$defs"]["CapabilitiesFile"]["properties"]["corona"]["items"] = json!({ "oneOf": names });
  schema
}

/// The JSON schema of a source's `catalog.toml`, for editors
pub fn catalog_schema() -> serde_json::Value {
  schemars::schema_for!(catalog::CatalogFile).to_value()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn property<'a>(schema: &'a serde_json::Value, path: &[&str]) -> &'a serde_json::Value {
    path.iter().fold(schema, |value, key| &value[*key])
  }

  #[test]
  fn schemas_describe_the_files() {
    let mut modules = ScriptRegistry::default();
    crate::module::register(&mut modules);
    let plugin = plugin_schema(&modules);
    assert_eq!(plugin["title"], "ManifestFile");
    for key in [
      "id",
      "name",
      "widgets",
      "panels",
      "settings",
      "capabilities",
      "service",
    ] {
      assert!(property(&plugin, &["properties", key]).is_object(), "{key}");
    }
    for (def, key) in [
      ("CapabilitiesFile", "dbus"),
      ("ProcessGrantFile", "env"),
      ("NetworkGrantFile", "unix"),
    ] {
      let path = ["$defs", def, "properties", key];
      assert!(property(&plugin, &path).is_object(), "{def}.{key}");
    }
    let grants = property(
      &plugin,
      &["$defs", "CapabilitiesFile", "properties", "corona"],
    );
    let weather = (grants["items"]["oneOf"].as_array().unwrap().iter())
      .find(|m| m["const"] == "weather")
      .unwrap();
    assert!(
      weather["description"]
        .as_str()
        .unwrap()
        .contains("forecast")
    );
    let catalog = catalog_schema();
    assert_eq!(catalog["title"], "CatalogFile");
    assert!(property(&catalog, &["properties", "plugin"]).is_object());
    let entry = property(&catalog, &["$defs", "CatalogEntry"]);
    assert_eq!(
      entry["required"],
      serde_json::json!(["id", "name", "version"])
    );
    assert_eq!(entry["additionalProperties"], false);
  }
}

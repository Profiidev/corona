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

/// The JSON schema of `plugin.toml`, for editors
pub fn plugin_schema() -> serde_json::Value {
  schemars::schema_for!(manifest::ManifestFile).to_value()
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
    let plugin = plugin_schema();
    assert_eq!(plugin["title"], "ManifestFile");
    for key in [
      "id",
      "name",
      "widgets",
      "panels",
      "settings",
      "capabilities",
    ] {
      assert!(property(&plugin, &["properties", key]).is_object(), "{key}");
    }
    for (def, key) in [
      ("ProcessGrantFile", "env"),
      ("NetworkGrantFile", "unix"),
    ] {
      let path = ["$defs", def, "properties", key];
      assert!(property(&plugin, &path).is_object(), "{def}.{key}");
    }
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

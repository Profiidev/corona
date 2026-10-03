use serde::{Deserialize, Serialize};

use crate::placement::Placement;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BarConfig {
  pub placement: Placement,
  pub height: f32,
  #[serde(default)]
  pub start_widgets: Vec<WidgetConfig>,
  #[serde(default)]
  pub center_widgets: Vec<WidgetConfig>,
  #[serde(default)]
  pub end_widgets: Vec<WidgetConfig>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WidgetConfig {
  Group { group: Vec<WidgetEntry> },
  Widget(WidgetEntry),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidgetEntry {
  pub widget_type: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub options: Option<serde_json::Value>,
}

impl WidgetConfig {
  fn widget(widget_type: &str) -> Self {
    Self::Widget(WidgetEntry::new(widget_type))
  }

  fn group(widgets: impl IntoIterator<Item = WidgetEntry>) -> Self {
    Self::Group {
      group: widgets.into_iter().collect(),
    }
  }
}

impl WidgetEntry {
  fn new(widget_type: &str) -> Self {
    Self {
      widget_type: widget_type.to_string(),
      options: None,
    }
  }

  fn resource(stat: &str) -> Self {
    Self {
      widget_type: "resource".to_string(),
      options: Some(serde_json::json!({ "stat": stat })),
    }
  }
}

impl Default for BarConfig {
  fn default() -> Self {
    Self {
      placement: Placement::Top,
      height: 30.0,
      start_widgets: vec![
        WidgetConfig::widget("workspaces"),
        WidgetConfig::widget("active_window"),
      ],
      center_widgets: vec![
        WidgetConfig::widget("clock"),
        WidgetConfig::widget("player"),
      ],
      end_widgets: [
        WidgetConfig::widget("tray"),
        WidgetConfig::group(
          ["cpu", "temperature", "memory", "download", "upload", "disk"].map(WidgetEntry::resource),
        ),
      ]
      .into_iter()
      .chain(
        [
          "bluetooth",
          "network",
          "audio",
          "notifications",
          "control_center",
        ]
        .map(WidgetConfig::widget),
      )
      .collect(),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parses_groups_and_options() {
    let toml = r#"
      placement = "top"
      height = 30.0
      end_widgets = [
        { widget_type = "clock" },
        { group = [{ widget_type = "resource", options = { stat = "disk", mount = "/home", critical = 90 } }, { widget_type = "audio" }] },
      ]
    "#;
    let bar: BarConfig = config::Config::builder()
      .add_source(config::File::from_str(toml, config::FileFormat::Toml))
      .build()
      .unwrap()
      .try_deserialize()
      .unwrap();

    assert_eq!(bar.end_widgets[0], WidgetConfig::widget("clock"));
    let WidgetConfig::Group { group } = &bar.end_widgets[1] else {
      panic!("expected a group");
    };
    let options = group[0].options.as_ref().unwrap();
    assert_eq!(options["stat"], "disk");
    assert_eq!(options["mount"], "/home");
    assert_eq!(options["critical"], 90);
    assert_eq!(group[1], WidgetEntry::new("audio"));
  }
}

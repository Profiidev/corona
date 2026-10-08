use serde::{Deserialize, Serialize};

use crate::placement::Placement;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BarConfig {
  pub position: Placement,
  pub thickness: f32,
  pub background_opacity: f32,
  /// A pill behind each widget
  pub capsule: bool,
  pub widget_spacing: f32,
  /// Space before the first widget: left of a horizontal bar, top of a vertical one
  pub padding_start: f32,
  /// Space after the last widget
  pub padding_end: f32,
  pub start: Vec<WidgetConfig>,
  pub center: Vec<WidgetConfig>,
  pub end: Vec<WidgetConfig>,
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
      position: Placement::Top,
      thickness: 30.0,
      background_opacity: 1.,
      capsule: true,
      widget_spacing: 8.,
      padding_start: 5.,
      padding_end: 5.,
      start: vec![
        WidgetConfig::widget("workspaces"),
        WidgetConfig::widget("active_window"),
      ],
      center: vec![
        WidgetConfig::widget("clock"),
        WidgetConfig::widget("player"),
      ],
      end: [
        WidgetConfig::widget("privacy"),
        WidgetConfig::widget("tray"),
        WidgetConfig::group(
          ["cpu", "temperature", "memory", "download", "upload", "disk"].map(WidgetEntry::resource),
        ),
      ]
      .into_iter()
      .chain(
        [
          "battery",
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
      end = [
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

    assert_eq!(bar.end[0], WidgetConfig::widget("clock"));
    let WidgetConfig::Group { group } = &bar.end[1] else {
      panic!("expected a group");
    };
    let options = group[0].options.as_ref().unwrap();
    assert_eq!(options["stat"], "disk");
    assert_eq!(options["mount"], "/home");
    assert_eq!(options["critical"], 90);
    assert_eq!(group[1], WidgetEntry::new("audio"));
    // unset fields keep their defaults
    assert_eq!(bar.thickness, BarConfig::default().thickness);
  }

  #[test]
  fn default_bar_layout() {
    let bar = BarConfig::default();
    assert_eq!(bar.position, Placement::Top);
    let groups: Vec<_> = bar
      .end
      .iter()
      .filter_map(|w| match w {
        WidgetConfig::Group { group } => Some(group),
        WidgetConfig::Widget(_) => None,
      })
      .collect();
    assert_eq!(groups.len(), 1);
    let stats: Vec<_> = groups[0]
      .iter()
      .map(|e| {
        assert_eq!(e.widget_type, "resource");
        e.options.as_ref().unwrap()["stat"].as_str().unwrap()
      })
      .collect();
    assert_eq!(
      stats,
      ["cpu", "temperature", "memory", "download", "upload", "disk"]
    );
    assert_eq!(
      bar.end.last(),
      Some(&WidgetConfig::widget("control_center"))
    );
    // only widgets in start and center
    assert!(
      bar
        .start
        .iter()
        .chain(&bar.center)
        .all(|w| matches!(w, WidgetConfig::Widget(_)))
    );
  }

  #[test]
  fn widget_config_untagged_edges() {
    let parse = |s: &str| serde_json::from_str::<WidgetConfig>(s);
    // a group wins over a widget_type next to it
    let both = parse(r#"{"group": [], "widget_type": "clock"}"#).unwrap();
    assert_eq!(both, WidgetConfig::group([]));
    assert!(parse(r#"{"options": {}}"#).is_err());
    assert!(parse(r#"{"group": [{"options": 1}]}"#).is_err());
    assert!(parse(r#""clock""#).is_err());

    // unset options are left out, set ones kept
    let plain = serde_json::to_value(WidgetConfig::widget("clock")).unwrap();
    assert_eq!(plain, serde_json::json!({ "widget_type": "clock" }));
    let with = WidgetConfig::Widget(WidgetEntry::resource("cpu"));
    let value = serde_json::to_value(&with).unwrap();
    assert_eq!(value["options"]["stat"], "cpu");
    assert_eq!(serde_json::from_value::<WidgetConfig>(value).unwrap(), with);
  }

  #[test]
  fn default_bar_round_trips_through_toml() {
    let bar = BarConfig::default();
    let text = toml::to_string(&bar).unwrap();
    assert_eq!(toml::from_str::<BarConfig>(&text).unwrap(), bar);
  }
}

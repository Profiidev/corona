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
pub struct WidgetConfig {
  pub widget_type: String,
}

impl Default for BarConfig {
  fn default() -> Self {
    Self {
      placement: Placement::Top,
      height: 30.0,
      start_widgets: vec![
        WidgetConfig {
          widget_type: "control_center".to_string(),
        },
        WidgetConfig {
          widget_type: "workspaces".to_string(),
        },
        WidgetConfig {
          widget_type: "active_window".to_string(),
        },
      ],
      center_widgets: vec![WidgetConfig {
        widget_type: "control_center".to_string(),
      }],
      end_widgets: [
        "audio",
        "network",
        "bluetooth",
        "power",
        "brightness",
        "notifications",
        "sysinfo",
        "weather",
        "calendar",
        "media",
        "control_center",
      ]
      .into_iter()
      .map(|widget_type| WidgetConfig {
        widget_type: widget_type.to_string(),
      })
      .collect(),
    }
  }
}

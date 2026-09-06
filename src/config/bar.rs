use serde::{Deserialize, Serialize};

use crate::{bar::WidgetType, config::placement::Placement};

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

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WidgetConfig {
  pub widget_type: WidgetType,
}

impl Default for BarConfig {
  fn default() -> Self {
    Self {
      placement: Placement::Top,
      height: 30.0,
      start_widgets: vec![
        WidgetConfig {
          widget_type: WidgetType::Button,
        },
        WidgetConfig {
          widget_type: WidgetType::Button,
        },
        WidgetConfig {
          widget_type: WidgetType::Button,
        },
        WidgetConfig {
          widget_type: WidgetType::Button,
        },
        WidgetConfig {
          widget_type: WidgetType::Button,
        },
        WidgetConfig {
          widget_type: WidgetType::Button,
        },
      ],
      center_widgets: vec![WidgetConfig {
        widget_type: WidgetType::Button,
      }],
      end_widgets: vec![WidgetConfig {
        widget_type: WidgetType::Button,
      }],
    }
  }
}

use gpui_kit::{AnyView, App, AppContext, Context, Render};
use serde::{Deserialize, Serialize};

mod button;
mod control_panel;
mod workspaces;

pub trait Widget: Render {
  fn init(cx: &mut Context<'_, Self>) -> Self;

  fn view(cx: &mut App) -> AnyView {
    cx.new(Self::init).into()
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WidgetType {
  ControlPanel,
  Workspaces,
}

impl WidgetType {
  pub fn init(&self, cx: &mut App) -> AnyView {
    match self {
      WidgetType::ControlPanel => control_panel::ControlPanelButton::view(cx),
      WidgetType::Workspaces => workspaces::Workspaces::view(cx),
    }
  }
}

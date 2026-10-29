mod active_window;
pub(crate) mod control_center;
mod workspaces;

use active_window::ActiveWindow;
use control_center::{ControlCenterButton, DisksButton, DisplaysButton};
use corona_surface::bar::BarExt;
use gpui_kit::App;
use workspaces::widget::Workspaces;

pub fn register_widgets(cx: &mut App) {
  cx.bar_mut()
    .register::<ControlCenterButton>()
    .register::<DisksButton>()
    .register::<DisplaysButton>()
    .register::<Workspaces>()
    .register::<ActiveWindow>();
}

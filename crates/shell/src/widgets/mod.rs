mod active_window;
pub(crate) mod control_center;
mod workspaces;

use active_window::ActiveWindow;
use control_center::{
  BrightnessButton, ControlCenterButton, DisksButton, DisplaysButton, HotspotButton, NetworkButton,
};
use corona_surface::bar::BarExt;
use gpui_kit::App;
use workspaces::widget::Workspaces;

pub fn register_widgets(cx: &mut App) {
  cx.bar_mut()
    .register::<ControlCenterButton>()
    .register::<NetworkButton>()
    .register::<HotspotButton>()
    .register::<DisksButton>()
    .register::<BrightnessButton>()
    .register::<DisplaysButton>()
    .register::<Workspaces>()
    .register::<ActiveWindow>();
}

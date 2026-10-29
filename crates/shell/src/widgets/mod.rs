pub(crate) mod control_center;

use control_center::ControlCenterButton;
use corona_surface::bar::BarExt;
use gpui_kit::App;

pub fn register_widgets(cx: &mut App) {
  cx.bar_mut().register::<ControlCenterButton>();
}

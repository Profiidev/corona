use gpui_kit::App;

pub mod colorpicker;
pub mod drive_password;
pub mod osk;
pub mod polkit;
pub mod screen_corners;
pub mod screenshot;
pub mod switcher;
pub mod taskbar;
pub mod wallpaper;

pub fn register_commands(cx: &mut App) {
  colorpicker::commands::register_commands(cx);
  osk::register_commands(cx);
  screenshot::commands::register_commands(cx);
  switcher::commands::register_commands(cx);
}

use gpui_kit::App;

pub mod polkit;
pub mod screen_corners;
pub mod switcher;
pub mod taskbar;
pub mod wallpaper;

pub fn register_commands(cx: &mut App) {
  switcher::commands::register_commands(cx);
}

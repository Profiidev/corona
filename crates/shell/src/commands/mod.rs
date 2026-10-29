pub use corona_utils::level::parse_level;
use gpui_kit::App;

pub mod theme;

pub fn register_commands(cx: &mut App) {
  theme::register_commands(cx);
}

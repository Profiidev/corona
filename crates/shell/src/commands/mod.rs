pub use corona_utils::level::parse_level;
use gpui_kit::App;

pub mod caffeine;
pub mod theme;

pub fn register_commands(cx: &mut App) {
  caffeine::register_commands(cx);
  theme::register_commands(cx);
}

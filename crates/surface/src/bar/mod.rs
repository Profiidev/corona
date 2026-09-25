pub mod base;
mod state;
mod style;
mod widgets;

pub use state::BarState;
pub use style::BarStyle;
pub use widgets::Widget;

const BAR_NAMESPACE: &str = "corona_bar";

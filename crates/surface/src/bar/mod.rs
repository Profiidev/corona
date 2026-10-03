pub mod base;
mod button;
mod state;
mod style;
mod widgets;

pub use button::Button;
pub use state::{BarExt, BarState};
pub use style::BarStyle;
pub use widgets::Widget;

const BAR_NAMESPACE: &str = "corona_bar";

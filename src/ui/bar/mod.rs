pub mod base;
mod state;
mod style;
mod widgets;

pub use state::BarState;
pub use widgets::{Widget, WidgetType};

const BAR_NAMESPACE: &str = "corona_bar";

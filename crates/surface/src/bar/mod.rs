pub mod base;
mod button;
mod page_button;
mod state;
mod style;
mod widgets;

pub use button::Button;
pub use page_button::{IdleOptions, IdlePage, IdlePageButton, PageButton};
pub use state::{BarExt, BarState};
pub use style::BarStyle;
pub use widgets::{Widget, WidgetData};

const BAR_NAMESPACE: &str = "corona_bar";

mod align;
mod base;
mod state;
mod style;
mod variants;

pub use align::Align;
pub use base::panel_path;
pub use state::{AppPanelExt, PanelState, WdigetPanelExt};
pub use style::PanelStyle;
pub use variants::Panel;

const PANEL_NAME: &str = "corona_panel";

mod align;
mod base;
mod state;
mod style;
mod variants;

pub use state::{PanelExt, PanelState};
pub use variants::Panel;
pub use variants::control_panel::ControlPanel;

const PANEL_NAME: &str = "corona_panel";

mod base;
mod placement;
mod state;
mod widgets;

pub use placement::{Placement, PlacementStyle, PlacmentBounds};
pub use state::BarState;

const BAR_NAMESPACE: &str = "corona-bar";

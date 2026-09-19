mod audio;
mod layout;
mod nav;
mod panel;
mod variants;

use gpui_kit::{AnyElement, Render};
pub use panel::ControlCenter;

trait ControlCenterPanel: Render {
  fn buttons(&mut self) -> Vec<AnyElement> {
    vec![]
  }
}

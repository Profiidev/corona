mod audio;
mod dashboard;
mod layout;
mod nav;
mod network;
mod panel;
mod variants;

use gpui_kit::{AnyElement, AnyView, App, AppContext, Context, Entity, Render};
pub use panel::ControlCenter;

trait ControlCenterPanel: Render {
  fn init(cx: &mut Context<'_, Self>) -> Self;

  fn buttons(&mut self, _cx: &mut Context<Self>) -> Vec<AnyElement> {
    vec![]
  }

  fn handle(cx: &mut App) -> Box<dyn ControlCenterPanelHandle> {
    Box::new(cx.new(|cx| Self::init(cx)))
  }
}

trait ControlCenterPanelHandle {
  fn view(&self) -> AnyView;
  fn buttons(&self, cx: &mut App) -> Vec<AnyElement>;
}

impl<T: ControlCenterPanel> ControlCenterPanelHandle for Entity<T> {
  fn view(&self) -> AnyView {
    self.clone().into()
  }

  fn buttons(&self, cx: &mut App) -> Vec<AnyElement> {
    self.update(cx, |page, cx| page.buttons(cx))
  }
}

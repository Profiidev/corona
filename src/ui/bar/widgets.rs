use gpui_kit::{AnyView, App, AppContext, Context, Render};
use uuid::Uuid;

pub trait Widget: Render {
  fn init(cx: &mut Context<'_, Self>, display_id: Uuid) -> Self;

  fn view(cx: &mut App, display_id: Uuid) -> AnyView {
    cx.new(|cx| Self::init(cx, display_id)).into()
  }
}


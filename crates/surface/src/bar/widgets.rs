use std::sync::Arc;

use gpui_kit::{AnyView, App, AppContext, Context, Render};
use uuid::Uuid;

pub trait Widget: Render {
  const NAME: &'static str;

  fn init(cx: &mut Context<'_, Self>, display_id: Uuid) -> Self;
}

pub type WidgetInitFn = Arc<dyn Fn(&mut App, Uuid) -> AnyView>;

#[derive(Clone)]
pub struct WidgetData {
  pub name: String,
  init: WidgetInitFn,
}

impl WidgetData {
  pub fn new<W: Widget>() -> Self {
    Self {
      name: W::NAME.to_string(),
      init: Arc::new(|cx, display_id| cx.new(|cx| W::init(cx, display_id)).into()),
    }
  }

  pub fn init(&self, cx: &mut App, display_id: Uuid) -> AnyView {
    (self.init)(cx, display_id)
  }
}

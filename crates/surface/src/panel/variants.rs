use std::sync::Arc;

use gpui_kit::{AnyView, App, AppContext, Context, Render, Window};

pub trait Panel: Render {
  const NAME: &'static str;
  const WIDTH: f32;
  const HEIGHT: f32;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self;
}

pub type PanelInitFn = Arc<dyn Fn(&mut Window, &mut App) -> AnyView>;

#[derive(Clone)]
pub struct PanelData {
  pub name: String,
  pub width: f32,
  pub height: f32,
  init: PanelInitFn,
}

impl PanelData {
  pub fn new<P: Panel>() -> Self {
    Self {
      name: P::NAME.to_string(),
      width: P::WIDTH,
      height: P::HEIGHT,
      init: Arc::new(|window, cx| cx.new(|cx| P::init(window, cx)).into()),
    }
  }

  /// A panel built by `init` at runtime, like a plugin's
  pub fn from_fn(
    name: impl Into<String>,
    width: f32,
    height: f32,
    init: impl Fn(&mut Window, &mut App) -> AnyView + 'static,
  ) -> Self {
    Self {
      name: name.into(),
      width,
      height,
      init: Arc::new(init),
    }
  }

  pub fn init(&self, window: &mut Window, cx: &mut App) -> AnyView {
    (self.init)(window, cx)
  }
}

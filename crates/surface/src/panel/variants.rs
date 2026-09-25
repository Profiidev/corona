use gpui_kit::{Context, Render, Window};

pub trait Panel: Render {
  const NAME: &'static str;
  const WIDTH: f32;
  const HEIGHT: f32;

  fn init(window: &mut Window, cx: &mut Context<'_, Self>) -> Self;
}

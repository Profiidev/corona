use gpui_kit::{Context, Render};

pub trait Panel: Render {
  const NAME: &'static str;
  const WIDTH: f32;
  const HEIGHT: f32;

  fn init(cx: &mut Context<'_, Self>) -> Self;
}

use gpui_kit::{App, Pixels, Render, Size, Window};

pub trait Tooltip: Render {
  const NAME: &'static str;

  fn size(&self, window: &Window, cx: &App) -> Size<Pixels>;
}

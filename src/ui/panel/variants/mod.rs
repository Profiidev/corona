use gpui_kit::Render;

pub mod control_panel;

pub trait Panel: Render {
  const NAME: &'static str;
  const WIDTH: f32;
  const HEIGHT: f32;
}

use gpui_kit::component::Theme;

pub trait PanelStyle {
  fn panel_radius(&self) -> f32;
}

impl PanelStyle for Theme {
  fn panel_radius(&self) -> f32 {
    (self.radius * 2).as_f32()
  }
}

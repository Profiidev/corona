use gpui_kit::{App, base::IndexPath, component::select::SelectItem};

use crate::integration::pipewire::{AudioNode, PipewireExt};

#[derive(Clone, Debug)]
pub struct NodeSelectItem {
  pub id: u32,
  pub name: String,
}

impl SelectItem for NodeSelectItem {
  type Value = u32;

  fn title(&self) -> gpui_kit::SharedString {
    self.name.clone().into()
  }

  fn value(&self) -> &Self::Value {
    &self.id
  }
}

pub fn select_items(options: &[AudioNode], default_option: bool) -> Vec<NodeSelectItem> {
  let mut items: Vec<NodeSelectItem> = options
    .iter()
    .map(|node| NodeSelectItem {
      id: node.id,
      name: node
        .nickname
        .clone()
        .unwrap_or_else(|| node.description.clone()),
    })
    .collect();

  if default_option {
    items.insert(
      0,
      NodeSelectItem {
        id: u32::MAX,
        name: "Default".to_string(),
      },
    );
  }

  items
}

pub fn index_of(items: &[NodeSelectItem], selected: Option<u32>) -> Option<IndexPath> {
  let selected = selected?;
  items
    .iter()
    .position(|item| item.id == selected)
    .map(IndexPath::new)
}

pub fn target_of(stream: u32, cx: &App) -> u32 {
  cx.pipewire().target(stream, cx)
}

pub fn to_slider(linear: f32) -> f32 {
  linear.cbrt()
}

pub fn to_linear(slider: f32) -> f32 {
  slider.powi(3)
}

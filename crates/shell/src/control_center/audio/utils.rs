use gpui_kit::{App, base::IndexPath, component::select::SelectItem};

use corona_pipewire::{AudioNode, PipewireExt};

use crate::control_center::audio::state::DEFAULT_SINK_ID;
use rust_i18n::t;

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
        id: DEFAULT_SINK_ID,
        name: t!("app.audio.default").into(),
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

pub fn target_of(stream: u32, cx: &App) -> Option<u32> {
  cx.pipewire().target(stream, cx)
}

#[cfg(test)]
mod tests {
  use corona_pipewire::NodeType;

  use super::*;

  fn node(id: u32, description: &str, nickname: Option<&str>) -> AudioNode {
    AudioNode {
      id,
      serial: 0,
      kind: NodeType::Sink,
      name: format!("node-{id}"),
      description: description.into(),
      nickname: nickname.map(Into::into),
      device: None,
      profile_device: None,
      volumes: vec![],
      mute: false,
      app: vec![],
    }
  }

  #[test]
  fn select_items_names() {
    let items = select_items(
      &[node(3, "Speakers", Some("Desk")), node(1, "HDMI", None)],
      false,
    );
    let items: Vec<_> = items.iter().map(|i| (i.id, i.name.as_str())).collect();
    // nickname beats description, order is kept
    assert_eq!(items, [(3, "Desk"), (1, "HDMI")]);
  }

  #[test]
  fn select_items_default_first() {
    let items = select_items(&[node(3, "Speakers", None)], true);
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].id, DEFAULT_SINK_ID);
    assert!(!items[0].name.is_empty());
    assert_eq!(items[1].id, 3);
    assert_eq!(select_items(&[], true).len(), 1);
    assert!(select_items(&[], false).is_empty());
  }

  #[test]
  fn index_of() {
    let items = select_items(&[node(3, "a", None), node(1, "b", None)], true);
    assert_eq!(super::index_of(&items, Some(1)), Some(IndexPath::new(2)));
    assert_eq!(
      super::index_of(&items, Some(DEFAULT_SINK_ID)),
      Some(IndexPath::new(0))
    );
    assert_eq!(super::index_of(&items, Some(42)), None);
    assert_eq!(super::index_of(&items, None), None);
  }
}

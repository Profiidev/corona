use std::collections::HashMap;

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
  let mut nicks: HashMap<&str, usize> = HashMap::new();
  // every name a node could show, so a fallback can't clash with another entry
  let mut names: HashMap<&str, usize> = HashMap::new();
  for node in options {
    *nicks.entry(nick_base(node_name(node))).or_default() += 1;
    *names.entry(&node.description).or_default() += 1;
    if let Some(nick) = node.nickname.as_deref().filter(|n| *n != node.description) {
      *names.entry(nick).or_default() += 1;
    }
  }

  let mut items: Vec<NodeSelectItem> = options
    .iter()
    .map(|node| NodeSelectItem {
      id: node.id,
      name: display_name(node, &nicks, &names),
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

fn nick_base(nick: &str) -> &str {
  match nick.rsplit_once(" #") {
    Some((base, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => base,
    _ => nick,
  }
}

/// Nickname if set, description otherwise.
fn node_name(node: &AudioNode) -> &str {
  node.nickname.as_deref().unwrap_or(&node.description)
}

/// Nickname, unless another node shares its base (`X #2`); then the
/// description, as long as no other node uses that name.
fn display_name(
  node: &AudioNode,
  nicks: &HashMap<&str, usize>,
  names: &HashMap<&str, usize>,
) -> String {
  match node.nickname.as_deref() {
    Some(nick) if nicks[nick_base(nick)] > 1 && names[node.description.as_str()] == 1 => {
      node.description.clone()
    }
    _ => node_name(node).to_string(),
  }
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
      pid: None,
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
  fn select_items_ambiguous_nicks() {
    let items = select_items(
      &[
        node(1, "USB Audio Speakers", Some("USB Audio")),
        node(2, "USB Audio Front Headphones", Some("USB Audio #1")),
        node(3, "USB Audio S/PDIF Output", Some("USB Audio #2")),
        node(4, "HDMI [Acer]", Some("Acer")),
        node(5, "Other USB Audio", Some("USB Audio")),
        node(6, "Loopback", Some("Virtual")),
        node(7, "Built-in Speakers", Some("Speakers")),
        node(8, "Speakers", None),
      ],
      false,
    );
    let names: Vec<_> = items.iter().map(|i| i.name.as_str()).collect();
    // nodes sharing a visible name fall back to their description; a lone nick stays
    assert_eq!(
      names,
      [
        "USB Audio Speakers",
        "USB Audio Front Headphones",
        "USB Audio S/PDIF Output",
        "Acer",
        "Other USB Audio",
        "Virtual",
        "Built-in Speakers",
        "Speakers",
      ]
    );
  }

  #[test]
  fn select_items_ambiguous_descriptions() {
    let items = select_items(
      &[
        node(1, "USB Audio Analog Stereo", Some("USB Audio")),
        node(2, "USB Audio Analog Stereo", Some("USB Audio #1")),
        node(3, "Loopback", Some("Virtual")),
        node(4, "Loopback", Some("Virtual")),
      ],
      false,
    );
    let names: Vec<_> = items.iter().map(|i| i.name.as_str()).collect();
    // the description wouldn't tell them apart either, so the shorter nick stays
    assert_eq!(names, ["USB Audio", "USB Audio #1", "Virtual", "Virtual"]);
  }

  #[test]
  fn select_items_fallback_no_new_clash() {
    let items = select_items(
      &[
        node(1, "Speakers", Some("USB Audio")),
        node(2, "Headphones", Some("USB Audio #1")),
        node(3, "Speakers", None),
        node(4, "Desk", Some("Monitor")),
        node(5, "Monitor", Some("Monitor #1")),
      ],
      false,
    );
    let names: Vec<_> = items.iter().map(|i| i.name.as_str()).collect();
    // "Speakers" and "Monitor" are already taken, so those nicks stay
    assert_eq!(
      names,
      ["USB Audio", "Headphones", "Speakers", "Desk", "Monitor #1"]
    );
  }

  #[test]
  fn nick_base_strips_numeric_suffix() {
    assert_eq!(super::nick_base("USB Audio #12"), "USB Audio");
    assert_eq!(super::nick_base("USB Audio"), "USB Audio");
    assert_eq!(super::nick_base("Room #A"), "Room #A");
    assert_eq!(super::nick_base("Room #"), "Room #");
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

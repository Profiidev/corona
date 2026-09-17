use std::{cell::RefCell, collections::HashMap, rc::Rc};

use anyhow::{Result, bail};
use pipewire::{
  device::{Device, DeviceListener},
  metadata::{Metadata, MetadataListener},
  node::{Node, NodeInfoRef, NodeListener},
  registry::{GlobalObject, RegistryRc},
  spa::{
    param::ParamType,
    pod::{Object, Pod, Value, deserialize::PodDeserializer},
    sys,
    utils::dict::DictRef,
  },
  types::ObjectType,
};

use crate::integration::pipewire::{
  event::PipewireEvent,
  state::{AudioNode, NodeType, PipewireState},
};

#[derive(Clone, Default)]
pub struct Handles(Rc<RefCell<Proxies>>);

#[derive(Default)]
struct Proxies {
  nodes: HashMap<u32, (Node, NodeListener)>,
  devices: HashMap<u32, (Device, DeviceListener)>,
  routes: HashMap<(u32, i32), i32>,
  metadata: HashMap<u32, (Metadata, MetadataListener)>,
}

impl Handles {
  pub fn set_node_param(&self, id: u32, param: &Pod) -> Result<()> {
    let proxies = self.0.borrow();
    let Some((node, _)) = proxies.nodes.get(&id) else {
      bail!("No such node");
    };

    node.set_param(ParamType::Props, 0, param);
    Ok(())
  }

  pub fn route(&self, device: u32, profile_device: i32) -> Option<i32> {
    self
      .0
      .borrow()
      .routes
      .get(&(device, profile_device))
      .copied()
  }

  pub fn set_device_param(&self, device: u32, param: &Pod) -> Result<()> {
    let proxies = self.0.borrow();
    let Some((device, _)) = proxies.devices.get(&device) else {
      bail!("No such device");
    };

    device.set_param(ParamType::Route, 0, param);
    Ok(())
  }

  pub fn set_metadata(
    &self,
    subject: u32,
    key: &str,
    type_: Option<&str>,
    value: Option<&str>,
  ) -> Result<()> {
    let proxies = self.0.borrow();
    let Some((metadata, _)) = proxies.metadata.values().next() else {
      bail!("No default metadata");
    };

    metadata.set_property(subject, key, type_, value);
    Ok(())
  }

  fn remove(&self, id: u32) {
    let mut proxies = self.0.borrow_mut();
    proxies.nodes.remove(&id);
    proxies.metadata.remove(&id);
    if proxies.devices.remove(&id).is_some() {
      proxies.routes.retain(|(device, _), _| *device != id);
    }
  }
}

pub fn global_listener(
  registry: RegistryRc,
  handles: Handles,
  state: PipewireState,
  events: flume::Sender<PipewireEvent>,
) -> impl Fn(&GlobalObject<&DictRef>) {
  move |obj| match obj.type_ {
    ObjectType::Node => add_node(&registry, &handles, &state, &events, obj),
    ObjectType::Device => add_device(&registry, &handles, obj),
    ObjectType::Metadata => add_metadata(&registry, &handles, &state, &events, obj),
    _ => (),
  }
}

fn add_node(
  registry: &RegistryRc,
  handles: &Handles,
  state: &PipewireState,
  events: &flume::Sender<PipewireEvent>,
  obj: &GlobalObject<&DictRef>,
) {
  let Some(props) = obj.props else {
    return;
  };

  let Some(class) = props
    .get("media.class")
    .and_then(|class| class.parse().ok())
  else {
    return;
  };

  let Some(node) = AudioNode::new(obj.id, class, props) else {
    return;
  };
  state.audio.nodes.insert(obj.id, node);

  let Ok(node) = registry.bind::<Node, &DictRef>(obj) else {
    return;
  };

  let listener = node
    .add_listener_local()
    .info(node_info_listener(obj.id, state.clone(), events.clone()))
    .param(node_props_listener(obj.id, state.clone(), events.clone()))
    .register();
  node.subscribe_params(&[ParamType::Props]);

  handles
    .0
    .borrow_mut()
    .nodes
    .insert(obj.id, (node, listener));

  let _ = events.send(PipewireEvent::AudioNodeAdded(obj.id));
}

fn add_device(registry: &RegistryRc, handles: &Handles, obj: &GlobalObject<&DictRef>) {
  let Ok(device) = registry.bind::<Device, &DictRef>(obj) else {
    return;
  };

  let listener = device
    .add_listener_local()
    .param(device_route_listener(obj.id, handles.clone()))
    .register();
  device.subscribe_params(&[ParamType::Route]);

  handles
    .0
    .borrow_mut()
    .devices
    .insert(obj.id, (device, listener));
}

fn add_metadata(
  registry: &RegistryRc,
  handles: &Handles,
  state: &PipewireState,
  events: &flume::Sender<PipewireEvent>,
  obj: &GlobalObject<&DictRef>,
) {
  if obj.props.and_then(|props| props.get("metadata.name")) != Some("default") {
    return;
  }

  let Ok(metadata) = registry.bind::<Metadata, &DictRef>(obj) else {
    return;
  };

  let listener = metadata
    .add_listener_local()
    .property(metadata_property_listener(state.clone(), events.clone()))
    .register();

  handles
    .0
    .borrow_mut()
    .metadata
    .insert(obj.id, (metadata, listener));
}

fn metadata_property_listener(
  state: PipewireState,
  events: flume::Sender<PipewireEvent>,
) -> impl Fn(u32, Option<&str>, Option<&str>, Option<&str>) -> i32 {
  move |subject, key, _type, value| {
    match key {
      Some("default.audio.sink") => {
        state
          .audio
          .set_default(NodeType::Sink, value.and_then(metadata_name), &events)
      }
      Some("default.audio.source") => {
        state
          .audio
          .set_default(NodeType::Source, value.and_then(metadata_name), &events)
      }
      Some("target.object") => state.audio.set_target(subject, value.map(unquote), &events),
      _ => (),
    }
    0
  }
}

fn metadata_name(value: &str) -> Option<String> {
  let value: serde_json::Value = serde_json::from_str(value).ok()?;
  Some(value.get("name")?.as_str()?.to_string())
}

fn unquote(value: &str) -> String {
  serde_json::from_str(value).unwrap_or_else(|_| value.to_string())
}

pub fn global_remove_listener(
  handles: Handles,
  state: PipewireState,
  events: flume::Sender<PipewireEvent>,
) -> impl Fn(u32) {
  move |id| {
    handles.remove(id);

    state.audio.targets.remove(&id);

    if state.audio.nodes.remove(&id).is_some() {
      let _ = events.send(PipewireEvent::AudioNodeRemoved(id));
    }
  }
}

fn node_info_listener(
  id: u32,
  state: PipewireState,
  events: flume::Sender<PipewireEvent>,
) -> impl Fn(&NodeInfoRef) {
  move |info| {
    let Some(props) = info.props() else {
      return;
    };

    state.audio.update_props(id, props, &events);
  }
}

fn node_props_listener(
  id: u32,
  state: PipewireState,
  events: flume::Sender<PipewireEvent>,
) -> impl Fn(i32, ParamType, u32, u32, Option<&Pod>) {
  move |_seq, param_type, _index, _next, param| {
    if param_type != ParamType::Props {
      return;
    }

    let Some(obj) = parse_object(param) else {
      return;
    };

    state.audio.update_params(id, obj, &events);
  }
}

fn device_route_listener(
  device: u32,
  handles: Handles,
) -> impl Fn(i32, ParamType, u32, u32, Option<&Pod>) {
  move |_seq, param_type, _index, _next, param| {
    if param_type != ParamType::Route {
      return;
    }

    let Some(obj) = parse_object(param) else {
      return;
    };

    let mut index = None;
    let mut profile_device = None;
    for prop in obj.properties {
      match (prop.key, prop.value) {
        (sys::SPA_PARAM_ROUTE_index, Value::Int(value)) => index = Some(value),
        (sys::SPA_PARAM_ROUTE_device, Value::Int(value)) => profile_device = Some(value),
        _ => (),
      }
    }

    let (Some(index), Some(profile_device)) = (index, profile_device) else {
      return;
    };

    handles
      .0
      .borrow_mut()
      .routes
      .insert((device, profile_device), index);
  }
}

fn parse_object(param: Option<&Pod>) -> Option<Object> {
  match PodDeserializer::deserialize_any_from(param?.as_bytes()).ok()? {
    (_, Value::Object(obj)) => Some(obj),
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_default_is_named_inside_json() {
    assert_eq!(
      metadata_name(r#"{"name":"alsa_output.speaker"}"#).as_deref(),
      Some("alsa_output.speaker")
    );
    // Cleared defaults arrive as a literal null, not as a removed property.
    assert_eq!(metadata_name("null"), None);
    assert_eq!(metadata_name("alsa_output.speaker"), None);
  }

  #[test]
  fn a_target_survives_either_spelling() {
    assert_eq!(unquote("alsa_output.hdmi"), "alsa_output.hdmi");
    assert_eq!(unquote(r#""alsa_output.hdmi""#), "alsa_output.hdmi");
  }
}

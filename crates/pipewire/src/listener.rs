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

use crate::{
  capture::is_candidate,
  event::AudioEvent,
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
  events: flume::Sender<AudioEvent>,
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
  events: &flume::Sender<AudioEvent>,
  obj: &GlobalObject<&DictRef>,
) {
  let Some(props) = obj.props else {
    return;
  };

  if let Some(class) = props.get("media.class").filter(|c| is_candidate(c)) {
    add_capture(registry, handles, state, events, obj, class, props);
    return;
  }

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

  for event in state.audio.node_events(class) {
    let _ = events.send(event);
  }
}

fn add_capture(
  registry: &RegistryRc,
  handles: &Handles,
  state: &PipewireState,
  events: &flume::Sender<AudioEvent>,
  obj: &GlobalObject<&DictRef>,
  class: &str,
  props: &DictRef,
) {
  state.captures.insert(obj.id, class, props);
  state.captures.send(events);

  let Ok(node) = registry.bind::<Node, &DictRef>(obj) else {
    return;
  };
  let (id, captures, events) = (obj.id, state.captures.clone(), events.clone());
  let listener = node
    .add_listener_local()
    .info(move |info| captures.update(id, &info.state(), info.props(), &events))
    .register();
  handles
    .0
    .borrow_mut()
    .nodes
    .insert(obj.id, (node, listener));
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
  events: &flume::Sender<AudioEvent>,
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
  events: flume::Sender<AudioEvent>,
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
  events: flume::Sender<AudioEvent>,
) -> impl Fn(u32) {
  move |id| {
    handles.remove(id);

    state.audio.targets.remove(&id);
    if state.captures.remove(id, &events) {
      return;
    }

    if let Some((_, node)) = state.audio.nodes.remove(&id) {
      for event in state.audio.node_events(node.kind) {
        let _ = events.send(event);
      }
    }
  }
}

fn node_info_listener(
  id: u32,
  state: PipewireState,
  events: flume::Sender<AudioEvent>,
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
  events: flume::Sender<AudioEvent>,
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
pub(crate) mod tests {
  use pipewire::spa::pod::{Property, ValueArray};

  use super::*;
  use crate::{
    command::serialize,
    testing::{pod, pod_bytes, props},
  };

  pub(crate) fn parse(pod: &Pod) -> Option<Object> {
    parse_object(Some(pod))
  }

  /// handles that know `device` routes `profile_device` through `index`
  pub(crate) fn with_route(device: u32, profile_device: i32, index: i32) -> Handles {
    let handles = Handles::default();
    handles
      .0
      .borrow_mut()
      .routes
      .insert((device, profile_device), index);
    handles
  }

  fn route(properties: Vec<Property>) -> Vec<u8> {
    pod_bytes(Object {
      type_: sys::SPA_TYPE_OBJECT_ParamRoute,
      id: sys::SPA_PARAM_Route,
      properties,
    })
  }

  fn volume_props(volumes: Vec<f32>, mute: bool) -> Vec<u8> {
    serialize(Object {
      type_: sys::SPA_TYPE_OBJECT_Props,
      id: sys::SPA_PARAM_Props,
      properties: vec![
        Property::new(
          sys::SPA_PROP_channelVolumes,
          Value::ValueArray(ValueArray::Float(volumes)),
        ),
        Property::new(sys::SPA_PROP_mute, Value::Bool(mute)),
      ],
    })
    .unwrap()
  }

  fn sink(id: u32, name: &str, serial: &str) -> AudioNode {
    let props = props(&[("node.name", name), ("object.serial", serial)]);
    AudioNode::new(id, NodeType::Sink, props.dict()).unwrap()
  }

  #[test]
  fn objects_parse_from_pods() {
    assert_eq!(parse_object(None), None);
    // a pod that is not an object
    let int = PodSerializerBytes::int(5);
    assert_eq!(parse(pod(&int)), None);
    let bytes = volume_props(vec![0.5], true);
    let object = parse(pod(&bytes)).unwrap();
    assert_eq!(object.properties.len(), 2);
  }

  /// a bare int pod
  struct PodSerializerBytes;
  impl PodSerializerBytes {
    fn int(value: i32) -> Vec<u8> {
      pipewire::spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &Value::Int(value),
      )
      .unwrap()
      .0
      .into_inner()
    }
  }

  #[test]
  fn metadata_sets_defaults_and_pins() {
    let (tx, rx) = flume::unbounded();
    let state = PipewireState::new();
    state.audio.nodes.insert(54, sink(54, "speaker", "62"));
    let listen = metadata_property_listener(state.clone(), tx);

    assert_eq!(
      listen(
        0,
        Some("default.audio.sink"),
        Some("Spa:String:JSON"),
        Some(r#"{"name":"speaker"}"#)
      ),
      0
    );
    assert!(matches!(rx.try_recv().unwrap(), AudioEvent::DefaultSink(Some(n)) if n.id == 54));
    // cleared
    listen(0, Some("default.audio.sink"), None, None);
    assert!(matches!(
      rx.try_recv().unwrap(),
      AudioEvent::DefaultSink(None)
    ));
    // junk names clear too, a second clear is no change
    listen(0, Some("default.audio.sink"), None, Some("not json"));
    assert!(rx.try_recv().is_err());

    listen(
      0,
      Some("default.audio.source"),
      None,
      Some(r#"{"name":"mic"}"#),
    );
    assert!(matches!(
      rx.try_recv().unwrap(),
      AudioEvent::DefaultSource(None)
    ));
    assert_eq!(
      state
        .audio
        .defaults
        .get(&NodeType::Source)
        .map(|n| n.clone()),
      Some("mic".into())
    );

    // pins come quoted or bare
    listen(110, Some("target.object"), None, Some("\"speaker\""));
    assert!(matches!(rx.try_recv().unwrap(), AudioEvent::Targets(t) if t.get(&110) == Some(&54)));
    listen(111, Some("target.object"), None, Some("62"));
    assert!(matches!(rx.try_recv().unwrap(), AudioEvent::Targets(t) if t.get(&111) == Some(&54)));
    listen(111, Some("target.object"), None, None);
    assert!(matches!(rx.try_recv().unwrap(), AudioEvent::Targets(t) if t.len() == 1));

    // other keys and a cleared everything are ignored
    listen(
      0,
      Some("default.configured.audio.sink"),
      None,
      Some(r#"{"name":"x"}"#),
    );
    listen(0, None, None, None);
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn props_params_update_the_node() {
    let (tx, rx) = flume::unbounded();
    let state = PipewireState::new();
    state.audio.nodes.insert(54, sink(54, "speaker", "62"));
    let listen = node_props_listener(54, state.clone(), tx);

    let bytes = volume_props(vec![0.5, 0.25], true);
    listen(0, ParamType::Props, 0, 0, Some(pod(&bytes)));
    let node = state.audio.nodes.get(&54).unwrap().clone();
    assert_eq!(
      (node.volumes.as_slice(), node.mute),
      ([0.5, 0.25].as_slice(), true)
    );
    assert_eq!(rx.len(), 3);

    // other params, no param and junk are ignored
    let _ = rx.drain();
    listen(0, ParamType::Route, 0, 0, Some(pod(&bytes)));
    listen(0, ParamType::Props, 0, 0, None);
    let int = PodSerializerBytes::int(1);
    listen(0, ParamType::Props, 0, 0, Some(pod(&int)));
    assert!(rx.is_empty());

    // an unknown node is ignored
    let other = node_props_listener(99, state.clone(), flume::unbounded().0);
    other(0, ParamType::Props, 0, 0, Some(pod(&bytes)));
  }

  #[test]
  fn routes_are_remembered_per_device() {
    let handles = Handles::default();
    let listen = device_route_listener(7, handles.clone());
    let index = |i| Property::new(sys::SPA_PARAM_ROUTE_index, Value::Int(i));
    let device = |d| Property::new(sys::SPA_PARAM_ROUTE_device, Value::Int(d));

    listen(
      0,
      ParamType::Route,
      0,
      0,
      Some(pod(&route(vec![index(3), device(1)]))),
    );
    assert_eq!(handles.route(7, 1), Some(3));
    // a route moving to another index replaces it
    listen(
      0,
      ParamType::Route,
      0,
      0,
      Some(pod(&route(vec![device(1), index(4)]))),
    );
    assert_eq!(handles.route(7, 1), Some(4));

    // incomplete or mistyped routes and other params are ignored
    listen(0, ParamType::Route, 0, 0, Some(pod(&route(vec![index(5)]))));
    listen(
      0,
      ParamType::Route,
      0,
      0,
      Some(pod(&route(vec![device(2)]))),
    );
    let wrong = Property::new(sys::SPA_PARAM_ROUTE_index, Value::Long(1));
    listen(
      0,
      ParamType::Route,
      0,
      0,
      Some(pod(&route(vec![wrong, device(3)]))),
    );
    listen(
      0,
      ParamType::Props,
      0,
      0,
      Some(pod(&route(vec![index(6), device(4)]))),
    );
    listen(0, ParamType::Route, 0, 0, None);
    assert_eq!(handles.0.borrow().routes.len(), 1);
    assert_eq!(handles.route(8, 1), None);
  }

  #[test]
  fn removal_forgets_nodes_pins_and_captures() {
    let (tx, rx) = flume::unbounded();
    let state = PipewireState::new();
    let handles = with_route(7, 1, 3);
    state.audio.nodes.insert(54, sink(54, "speaker", "62"));
    state.audio.targets.insert(54, "x".into());
    let screen = props(&[("application.name", "OBS")]);
    state
      .captures
      .insert(200, "Stream/Output/Video", screen.dict());
    let remove = global_remove_listener(handles.clone(), state.clone(), tx);

    // a capture: only the capture list moves
    remove(200);
    assert!(matches!(rx.try_recv().unwrap(), AudioEvent::Captures(c) if c.is_empty()));
    assert!(rx.is_empty());

    // a sink: its slices move and its own pin is gone
    remove(54);
    assert!(state.audio.targets.is_empty());
    let events: Vec<_> = rx.drain().collect();
    assert!(matches!(&events[0], AudioEvent::Nodes(NodeType::Sink, n) if n.is_empty()));
    assert_eq!(events.len(), 3);

    // unknown ids are fine
    remove(12345);
    assert!(rx.is_empty());
  }

  #[test]
  fn handles_without_proxies_fail() {
    let handles = Handles::default();
    let bytes = volume_props(vec![1.], false);
    assert_eq!(
      handles
        .set_node_param(1, pod(&bytes))
        .unwrap_err()
        .to_string(),
      "No such node"
    );
    assert_eq!(
      handles
        .set_device_param(1, pod(&bytes))
        .unwrap_err()
        .to_string(),
      "No such device"
    );
    assert_eq!(
      handles
        .set_metadata(0, "k", None, None)
        .unwrap_err()
        .to_string(),
      "No default metadata"
    );
  }

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
    assert_eq!(unquote("null"), "null");
  }

  #[test]
  fn device_removal_leaves_audio_nodes_with_dangling_device_ref() {
    let (tx, _rx) = flume::unbounded();
    let state = PipewireState::new();
    let handles = with_route(7, 1, 3);
    let mut sink_node = sink(54, "speaker", "62");
    sink_node.device = Some(7);
    sink_node.profile_device = Some(1);
    state.audio.nodes.insert(54, sink_node);
    let remove = global_remove_listener(handles.clone(), state.clone(), tx);

    // When Device 7 global is removed, node 54 is untouched and retains its device reference
    remove(7);
    let node = state.audio.nodes.get(&54).unwrap();
    assert_eq!(node.device, Some(7));
    assert_eq!(node.profile_device, Some(1));
  }
}

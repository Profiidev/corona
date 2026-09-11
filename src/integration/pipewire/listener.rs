use std::{cell::RefCell, collections::HashMap, rc::Rc};

use anyhow::{Result, bail};
use pipewire::{
  device::{Device, DeviceListener},
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
  state::{AudioSink, NodeType, PipewireState},
};

#[derive(Clone, Default)]
pub struct Handles(Rc<RefCell<Proxies>>);

#[derive(Default)]
struct Proxies {
  nodes: HashMap<u32, (Node, NodeListener)>,
  devices: HashMap<u32, (Device, DeviceListener)>,
  routes: HashMap<(u32, i32), i32>,
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

  fn remove(&self, id: u32) {
    let mut proxies = self.0.borrow_mut();
    proxies.nodes.remove(&id);
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

  match class {
    NodeType::AudioSink => {
      let Some(sink) = AudioSink::new(obj.id, props) else {
        return;
      };
      state.audio.sinks.insert(obj.id, sink);
    }
    _ => return,
  }

  let Ok(node) = registry.bind::<Node, &DictRef>(obj) else {
    return;
  };

  let listener = node
    .add_listener_local()
    .info(node_info_listener(
      obj.id,
      state.clone(),
      events.clone(),
      class,
    ))
    .param(node_props_listener(
      obj.id,
      state.clone(),
      events.clone(),
      class,
    ))
    .register();
  node.subscribe_params(&[ParamType::Props]);

  handles
    .0
    .borrow_mut()
    .nodes
    .insert(obj.id, (node, listener));

  let _ = events.send(PipewireEvent::AudioSinkAdded(obj.id));
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

pub fn global_remove_listener(
  handles: Handles,
  state: PipewireState,
  events: flume::Sender<PipewireEvent>,
) -> impl Fn(u32) {
  move |id| {
    handles.remove(id);

    if state.audio.sinks.remove(&id).is_some() {
      let _ = events.send(PipewireEvent::AudioSinkRemoved(id));
    }
  }
}

fn node_info_listener(
  id: u32,
  state: PipewireState,
  events: flume::Sender<PipewireEvent>,
  class: NodeType,
) -> impl Fn(&NodeInfoRef) {
  move |info| {
    let Some(props) = info.props() else {
      return;
    };

    class.update_props(&state, id, props, &events);
  }
}

fn node_props_listener(
  id: u32,
  state: PipewireState,
  events: flume::Sender<PipewireEvent>,
  class: NodeType,
) -> impl Fn(i32, ParamType, u32, u32, Option<&Pod>) {
  move |_seq, param_type, _index, _next, param| {
    if param_type != ParamType::Props {
      return;
    }

    let Some(obj) = parse_object(param) else {
      return;
    };

    class.update(&state, id, obj, &events);
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

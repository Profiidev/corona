use std::io::Cursor;

use anyhow::{Result, bail};
use pipewire::spa::{
  pod::{Object, Pod, Property, Value, ValueArray, serialize::PodSerializer},
  sys,
};
use tracing::warn;

use crate::integration::pipewire::listener::Handles;

#[derive(Clone, Copy, Debug)]
pub enum Target {
  Node(u32),
  Route {
    node: u32,
    device: u32,
    profile_device: i32,
  },
}

#[derive(Debug)]
pub enum Command {
  SetVolumes { target: Target, volumes: Vec<f32> },
  SetMute { target: Target, mute: bool },
}

impl Command {
  pub fn execute(self, handles: &Handles) {
    let (target, property) = match self {
      Command::SetVolumes { target, volumes } => (
        target,
        Property::new(
          sys::SPA_PROP_channelVolumes,
          Value::ValueArray(ValueArray::Float(volumes)),
        ),
      ),
      Command::SetMute { target, mute } => {
        (target, Property::new(sys::SPA_PROP_mute, Value::Bool(mute)))
      }
    };

    let props = Object {
      type_: sys::SPA_TYPE_OBJECT_Props,
      id: sys::SPA_PARAM_Props,
      properties: vec![property],
    };

    let route = match target {
      Target::Route {
        device,
        profile_device,
        ..
      } => handles
        .route(device, profile_device)
        .map(|index| (device, index)),
      Target::Node(_) => None,
    };

    let result = match (target, route) {
      (Target::Route { profile_device, .. }, Some((device, index))) => {
        apply_params(route_param(index, profile_device, props), |pod| {
          handles.set_device_param(device, pod)
        })
      }
      (Target::Node(node), _) | (Target::Route { node, .. }, None) => {
        apply_params(props, |pod| handles.set_node_param(node, pod))
      }
    };

    if let Err(e) = result {
      warn!("Failed to execute pipewire command: {e}");
    }
  }
}

fn route_param(index: i32, profile_device: i32, props: Object) -> Object {
  Object {
    type_: sys::SPA_TYPE_OBJECT_ParamRoute,
    id: sys::SPA_PARAM_Route,
    properties: vec![
      Property::new(sys::SPA_PARAM_ROUTE_index, Value::Int(index)),
      Property::new(sys::SPA_PARAM_ROUTE_device, Value::Int(profile_device)),
      Property::new(sys::SPA_PARAM_ROUTE_props, Value::Object(props)),
      Property::new(sys::SPA_PARAM_ROUTE_save, Value::Bool(true)),
    ],
  }
}

fn apply_params(object: Object, to: impl FnOnce(&Pod) -> Result<()>) -> Result<()> {
  let bytes = match serialize(object) {
    Ok(bytes) => bytes,
    Err(e) => {
      bail!("Failed to serialize pipewire param: {e}");
    }
  };

  let Some(pod) = Pod::from_bytes(&bytes) else {
    bail!("Failed to build pipewire pod");
  };

  to(pod)
}

fn serialize(object: Object) -> Result<Vec<u8>> {
  Ok(
    PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(object))?
      .0
      .into_inner(),
  )
}

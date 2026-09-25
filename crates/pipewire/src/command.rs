use std::io::Cursor;

use anyhow::{Context as _, Result, bail};
use pipewire::spa::{
  pod::{Object, Pod, Property, Value, ValueArray, serialize::PodSerializer},
  sys,
};
use tracing::warn;

use crate::{listener::Handles, state::NodeType};

#[derive(Clone, Copy, Debug)]
pub enum Target {
  Node(u32),
  Route {
    node: u32,
    device: u32,
    profile_device: i32,
  },
}

impl Target {
  fn node(self) -> u32 {
    match self {
      Target::Node(node) | Target::Route { node, .. } => node,
    }
  }

  fn route(self, handles: &Handles) -> Option<(u32, i32, i32)> {
    let Target::Route {
      device,
      profile_device,
      ..
    } = self
    else {
      return None;
    };

    Some((
      device,
      handles.route(device, profile_device)?,
      profile_device,
    ))
  }
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug)]
pub enum Command {
  SetVolumes { target: Target, volumes: Vec<f32> },
  SetMute { target: Target, mute: bool },
  SetDefault { kind: NodeType, name: String },
  SetTarget { node: u32, name: Option<String> },
}

impl Command {
  pub fn execute(self, handles: &Handles) {
    let result = match self {
      Command::SetVolumes { target, volumes } => set_prop(
        handles,
        target,
        Property::new(
          sys::SPA_PROP_channelVolumes,
          Value::ValueArray(ValueArray::Float(volumes)),
        ),
      ),
      Command::SetMute { target, mute } => set_prop(
        handles,
        target,
        Property::new(sys::SPA_PROP_mute, Value::Bool(mute)),
      ),
      Command::SetDefault { kind, name } => set_default(handles, kind, &name),
      Command::SetTarget { node, name } => {
        handles.set_metadata(node, "target.object", None, name.as_deref())
      }
    };

    if let Err(e) = result {
      warn!("Failed to execute pipewire command: {e}");
    }
  }
}

fn set_prop(handles: &Handles, target: Target, property: Property) -> Result<()> {
  let props = Object {
    type_: sys::SPA_TYPE_OBJECT_Props,
    id: sys::SPA_PARAM_Props,
    properties: vec![property],
  };

  match target.route(handles) {
    Some((device, index, profile_device)) => {
      write_pod(route_param(index, profile_device, props), |pod| {
        handles.set_device_param(device, pod)
      })
    }
    None => write_pod(props, |pod| handles.set_node_param(target.node(), pod)),
  }
}

fn set_default(handles: &Handles, kind: NodeType, name: &str) -> Result<()> {
  let key = match kind {
    NodeType::Sink => "default.configured.audio.sink",
    NodeType::Source => "default.configured.audio.source",
    NodeType::Stream => bail!("Only a sink or a source can be the default"),
  };

  let value = serde_json::json!({ "name": name }).to_string();
  handles.set_metadata(0, key, Some("Spa:String:JSON"), Some(&value))
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

fn write_pod(object: Object, write: impl FnOnce(&Pod) -> Result<()>) -> Result<()> {
  let bytes = serialize(object).context("Failed to serialize pipewire param")?;
  let pod = Pod::from_bytes(&bytes).context("Failed to build pipewire pod")?;

  write(pod)
}

fn serialize(object: Object) -> Result<Vec<u8>> {
  Ok(
    PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(object))?
      .0
      .into_inner(),
  )
}

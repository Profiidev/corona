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

pub(crate) fn serialize(object: Object) -> Result<Vec<u8>> {
  Ok(
    PodSerializer::serialize(Cursor::new(Vec::new()), &Value::Object(object))?
      .0
      .into_inner(),
  )
}

#[cfg(test)]
mod tests {
  use pipewire::spa::pod::Value;

  use super::*;
  use crate::{
    listener::tests::with_route,
    testing::{pod, pod_bytes},
  };

  fn props(property: Property) -> Object {
    Object {
      type_: sys::SPA_TYPE_OBJECT_Props,
      id: sys::SPA_PARAM_Props,
      properties: vec![property],
    }
  }

  #[test]
  fn targets_name_their_node() {
    assert_eq!(Target::Node(5).node(), 5);
    let route = Target::Route {
      node: 6,
      device: 7,
      profile_device: 1,
    };
    assert_eq!(route.node(), 6);
    let handles = Handles::default();
    assert_eq!(Target::Node(5).route(&handles), None);
    // a device without that route yet: write the node
    assert_eq!(route.route(&handles), None);
    let handles = with_route(7, 1, 3);
    assert_eq!(route.route(&handles), Some((7, 3, 1)));
  }

  #[test]
  fn route_params_wrap_the_props() {
    let inner = props(Property::new(sys::SPA_PROP_mute, Value::Bool(true)));
    let bytes = pod_bytes(route_param(3, 1, inner.clone()));
    let parsed = crate::listener::tests::parse(pod(&bytes)).unwrap();
    assert_eq!(parsed.type_, sys::SPA_TYPE_OBJECT_ParamRoute);
    assert_eq!(parsed.id, sys::SPA_PARAM_Route);
    let values: Vec<_> = parsed
      .properties
      .into_iter()
      .map(|p| (p.key, p.value))
      .collect();
    assert_eq!(
      values,
      [
        (sys::SPA_PARAM_ROUTE_index, Value::Int(3)),
        (sys::SPA_PARAM_ROUTE_device, Value::Int(1)),
        (sys::SPA_PARAM_ROUTE_props, Value::Object(inner)),
        // saved, so the volume survives a restart
        (sys::SPA_PARAM_ROUTE_save, Value::Bool(true)),
      ]
    );
  }

  #[test]
  fn props_go_to_the_route_or_the_node() {
    let mute = || Property::new(sys::SPA_PROP_mute, Value::Bool(true));
    let route = Target::Route {
      node: 6,
      device: 7,
      profile_device: 1,
    };
    // the errors tell which proxy would have been written
    let none = Handles::default();
    assert_eq!(
      set_prop(&none, route, mute()).unwrap_err().to_string(),
      "No such node"
    );
    let routed = with_route(7, 1, 3);
    assert_eq!(
      set_prop(&routed, route, mute()).unwrap_err().to_string(),
      "No such device"
    );
    assert_eq!(
      set_prop(&routed, Target::Node(6), mute())
        .unwrap_err()
        .to_string(),
      "No such node"
    );
  }

  #[test]
  fn defaults_are_sinks_or_sources() {
    let handles = Handles::default();
    assert_eq!(
      set_default(&handles, NodeType::Stream, "x")
        .unwrap_err()
        .to_string(),
      "Only a sink or a source can be the default"
    );
    for kind in [NodeType::Sink, NodeType::Source] {
      assert_eq!(
        set_default(&handles, kind, "x").unwrap_err().to_string(),
        "No default metadata"
      );
    }
  }

  #[test]
  fn failed_commands_only_log() {
    let handles = Handles::default();
    for command in [
      Command::SetVolumes {
        target: Target::Node(1),
        volumes: vec![0.5, 0.5],
      },
      Command::SetMute {
        target: Target::Node(1),
        mute: true,
      },
      Command::SetDefault {
        kind: NodeType::Sink,
        name: "x".into(),
      },
      Command::SetTarget {
        node: 1,
        name: None,
      },
    ] {
      command.execute(&handles);
    }
  }

  #[test]
  fn serializes_every_value() {
    let object = props(Property::new(
      sys::SPA_PROP_channelVolumes,
      Value::ValueArray(ValueArray::Float(vec![0.25, 1.5])),
    ));
    let bytes = pod_bytes(object.clone());
    assert_eq!(crate::listener::tests::parse(pod(&bytes)), Some(object));
    write_pod(
      props(Property::new(sys::SPA_PROP_mute, Value::Bool(false))),
      |pod| {
        assert!(pod.is_object());
        Ok(())
      },
    )
    .unwrap();
  }
}

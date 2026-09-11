use std::io::Cursor;

use anyhow::Result;
use pipewire::spa::{
  param::ParamType,
  pod::{Object, Pod, Property, Value, ValueArray, serialize::PodSerializer},
  sys,
};
use tracing::warn;

use crate::integration::pipewire::listener::Handles;

pub enum Command {
  SetVolumes { id: u32, volumes: Vec<f32> },
  SetMute { id: u32, mute: bool },
}

impl Command {
  pub fn execute(self, handles: &Handles) {
    let id = self.id();

    let handles = handles.borrow();
    let Some(handle) = handles.get(&id) else {
      warn!("Node {id} is gone, dropping pipewire command");
      return;
    };

    let bytes = match self.param() {
      Ok(bytes) => bytes,
      Err(e) => {
        warn!("Failed to serialize pipewire param: {e}");
        return;
      }
    };

    let Some(pod) = Pod::from_bytes(&bytes) else {
      warn!("Failed to build pipewire pod");
      return;
    };

    handle.node.set_param(ParamType::Props, 0, pod);
  }

  fn id(&self) -> u32 {
    match self {
      Command::SetVolumes { id, .. } | Command::SetMute { id, .. } => *id,
    }
  }

  fn param(self) -> Result<Vec<u8>> {
    let property = match self {
      Command::SetVolumes { volumes, .. } => Property::new(
        sys::SPA_PROP_channelVolumes,
        Value::ValueArray(ValueArray::Float(volumes)),
      ),
      Command::SetMute { mute, .. } => Property::new(sys::SPA_PROP_mute, Value::Bool(mute)),
    };

    let value = Value::Object(Object {
      type_: sys::SPA_TYPE_OBJECT_Props,
      id: sys::SPA_PARAM_Props,
      properties: vec![property],
    });

    Ok(
      PodSerializer::serialize(Cursor::new(Vec::new()), &value)?
        .0
        .into_inner(),
    )
  }
}

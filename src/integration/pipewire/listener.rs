use pipewire::{
  node::Node,
  registry::{GlobalObject, RegistryRc},
  spa::{
    param::ParamType,
    pod::{Pod, Value, ValueArray, deserialize::PodDeserializer},
    sys,
    utils::dict::DictRef,
  },
  types::ObjectType,
};

use crate::integration::pipewire::state::{AudioSink, PipewireState};

pub fn global_listener(
  registry: RegistryRc,
  state: PipewireState,
) -> impl Fn(&GlobalObject<&DictRef>) {
  move |obj| {
    if obj.type_ != ObjectType::Node {
      return;
    }

    let Some(props) = obj.props else {
      return;
    };

    let Some(class) = props.get("media.class") else {
      return;
    };

    let Ok(node) = registry.bind::<Node, &DictRef>(obj) else {
      return;
    };
    
    let listener = node
      .add_listener_local()
      .param(param_listener(obj.id, state.clone()))
      .register();
    node.subscribe_params(&[ParamType::Props]);

    match class {
      "Audio/Sink" => {
        if let Some(sink) = AudioSink::new(obj.id, props, node, listener) {
          state.audio.sinks.insert(obj.id, sink);
        }
      }
      "Audio/Source" => {}
      _ => (),
    }
  }
}

fn param_listener(
  id: u32,
  state: PipewireState,
) -> impl Fn(i32, ParamType, u32, u32, Option<&Pod>) {
  move |_seq, param_type, _index, _next, param| {
    if param_type != ParamType::Props {
      return;
    }

    let Some(obj) = param
      .and_then(|pod| PodDeserializer::deserialize_any_from(pod.as_bytes()).ok())
      .and_then(|(_, value)| match value {
        Value::Object(obj) => Some(obj),
        _ => None,
      })
    else {
      return;
    };

    let Some(mut sink) = state.audio.sinks.get_mut(&id) else {
      return;
    };

    for prop in obj.properties {
      match (prop.key, prop.value) {
        (sys::SPA_PROP_channelVolumes, Value::ValueArray(ValueArray::Float(volume))) => {
          sink.volumes = volume;
        }
        (sys::SPA_PROP_mute, Value::Bool(mute)) => {
          sink.mute = mute;
        }
        _ => (),
      }
    }
  }
}

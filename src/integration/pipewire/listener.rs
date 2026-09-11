use std::{cell::RefCell, collections::HashMap, rc::Rc};

use pipewire::{
  node::{Node, NodeListener},
  registry::{GlobalObject, RegistryRc},
  spa::{
    param::ParamType,
    pod::{Pod, Value, deserialize::PodDeserializer},
    utils::dict::DictRef,
  },
  types::ObjectType,
};

use crate::integration::pipewire::{
  event::PipewireEvent,
  state::{AudioSink, NodeType, PipewireState},
};

pub type Handles = Rc<RefCell<HashMap<u32, NodeHandle>>>;

pub struct NodeHandle {
  pub node: Node,
  _listener: NodeListener,
}

pub fn global_listener(
  registry: RegistryRc,
  handles: Handles,
  state: PipewireState,
  events: flume::Sender<PipewireEvent>,
) -> impl Fn(&GlobalObject<&DictRef>) {
  move |obj| {
    if obj.type_ != ObjectType::Node {
      return;
    }

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
      .param(param_listener(obj.id, state.clone(), events.clone(), class))
      .register();
    node.subscribe_params(&[ParamType::Props]);

    handles.borrow_mut().insert(
      obj.id,
      NodeHandle {
        node,
        _listener: listener,
      },
    );

    let _ = events.send(PipewireEvent::AudioSinkAdded(obj.id));
  }
}

pub fn global_remove_listener(
  handles: Handles,
  state: PipewireState,
  events: flume::Sender<PipewireEvent>,
) -> impl Fn(u32) {
  move |id| {
    if state.audio.sinks.remove(&id).is_none() {
      return;
    }

    handles.borrow_mut().remove(&id);
    let _ = events.send(PipewireEvent::AudioSinkRemoved(id));
  }
}

fn param_listener(
  id: u32,
  state: PipewireState,
  events: flume::Sender<PipewireEvent>,
  class: NodeType,
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

    class.update(&state, id, obj, &events);
  }
}

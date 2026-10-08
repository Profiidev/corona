//! Helpers for tests: properties, pods and a command channel read back on a
//! local loop, none of which needs a running PipeWire.

use std::{cell::RefCell, rc::Rc, time::Duration};

use pipewire::{
  channel::AttachedReceiver,
  loop_::Timeout,
  main_loop::MainLoopRc,
  properties::{PropertiesBox, properties},
  spa::pod::{Object, Pod},
};

use crate::command::{Command, serialize};

pub(crate) fn props(pairs: &[(&str, &str)]) -> PropertiesBox {
  let mut props = properties! { "corona.test" => "1" };
  for (key, value) in pairs {
    props.insert(*key, *value);
  }
  props
}

pub(crate) fn pod_bytes(object: Object) -> Vec<u8> {
  serialize(object).unwrap()
}

pub(crate) fn pod(bytes: &[u8]) -> &Pod {
  Pod::from_bytes(bytes).unwrap()
}

/// the receiving end of the commands the shell sends to the PipeWire thread
pub(crate) struct Commands {
  main_loop: MainLoopRc,
  received: Rc<RefCell<Vec<Command>>>,
  _attached: Option<AttachedReceiver<'static, Command>>,
}

impl Commands {
  pub(crate) fn new() -> (pipewire::channel::Sender<Command>, Self) {
    pipewire::init();
    let main_loop = MainLoopRc::new(None).unwrap();
    let (tx, rx) = pipewire::channel::channel();
    let received = Rc::<RefCell<Vec<Command>>>::default();
    let sink = received.clone();
    // the loop lives as long as the receiver, both are dropped together
    let loop_: &'static pipewire::loop_::Loop =
      unsafe { &*(main_loop.loop_() as *const pipewire::loop_::Loop) };
    let attached = rx.attach(loop_, move |command| sink.borrow_mut().push(command));
    (
      tx,
      Self {
        main_loop,
        received,
        _attached: Some(attached),
      },
    )
  }

  pub(crate) fn take(&self) -> Vec<Command> {
    while self
      .main_loop
      .loop_()
      .iterate(Timeout::Finite(Duration::from_millis(1)))
      > 0
    {}
    self.received.take()
  }
}

impl Drop for Commands {
  fn drop(&mut self) {
    self._attached.take();
  }
}

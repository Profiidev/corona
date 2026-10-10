use std::{
  cell::RefCell,
  collections::{HashMap, VecDeque},
  rc::Rc,
  time::Duration,
};

use anyhow::{Context as _, Result, anyhow, bail};
use corona_macros::named;
use gpui_kit::{App, AppContext, Entity};
use gpui_shell::HostModule;
use serde::Serialize;
use serde_json::{Map, Value};
use ts_rs::TS;

use crate::{
  host_fn::{Cx, Module},
  module::{Subscribe, Subscriptions, Updates, watch},
};

/// How long `call` waits for the service to answer
const CALL_TIMEOUT: Duration = Duration::from_secs(30);
/// Notification actions kept for a plugin that is not waiting for one
const ACTION_BUFFER: usize = 16;

/// A `call` for the service to answer with `reply` or `fail`.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct Call {
  pub id: u64,
  pub method: String,
  pub args: Value,
}

/// An action the user picked on one of the plugin's notifications.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ActionEvent {
  /// The notification's id, as `notify` returned it.
  pub id: u32,
  pub key: String,
}

type Answer = Result<Value, String>;

/// Calls into the plugin's service, and the notification actions for it
#[derive(Default)]
pub struct Calls {
  /// To the running service's `nextCall`, made anew as it starts
  service: Option<flume::Sender<Call>>,
  next_id: u64,
  pending: HashMap<u64, flume::Sender<Answer>>,
  actions: VecDeque<ActionEvent>,
  /// The `nextAction`s waiting, by the load that made them
  action_waiters: Vec<(u64, flume::Sender<ActionEvent>)>,
  next_load: u64,
}

impl Calls {
  fn call(&mut self, method: String, args: Value) -> Result<flume::Receiver<Answer>> {
    let id = self.next_id;
    let call = Call { id, method, args };
    let sent = self.service.as_ref().map(|service| service.send(call));
    if !matches!(sent, Some(Ok(()))) {
      bail!("plugin has no running service");
    }
    self.next_id += 1;
    // callers that gave up
    self.pending.retain(|_, tx| !tx.is_disconnected());
    let (tx, rx) = flume::bounded(1);
    self.pending.insert(id, tx);
    Ok(rx)
  }

  fn answer(&mut self, id: u64, answer: Answer) -> Result<()> {
    let tx = self
      .pending
      .remove(&id)
      .ok_or_else(|| anyhow!("no call {id} is waiting"))?;
    // the caller timed out meanwhile
    tx.send(answer).ok();
    Ok(())
  }

  /// The calls for a service that starts; the last one's `nextCall`s end
  fn start_service(&mut self) -> flume::Receiver<Call> {
    let (tx, rx) = flume::unbounded();
    self.service = Some(tx);
    rx
  }

  fn stop_service(&mut self) {
    self.service = None;
    for (_, tx) in self.pending.drain() {
      tx.send(Err("service stopped".into())).ok();
    }
  }

  fn next_action(&mut self, load: u64) -> flume::Receiver<ActionEvent> {
    let (tx, rx) = flume::bounded(1);
    match self.actions.pop_front() {
      Some(action) => drop(tx.send(action)),
      None => self.action_waiters.push((load, tx)),
    }
    rx
  }

  /// To every instance waiting; kept for the next one when none is.
  fn push_action(&mut self, action: ActionEvent) {
    let delivered = self
      .action_waiters
      .drain(..)
      .filter(|(_, tx)| tx.send(action.clone()).is_ok())
      .count();
    if delivered == 0 {
      self.actions.push_back(action);
      if self.actions.len() > ACTION_BUFFER {
        self.actions.pop_front();
      }
    }
  }
}

/// What all instances of one plugin share: its state and the calls into its
/// service
#[derive(Clone)]
pub struct Hub {
  state: Entity<Map<String, Value>>,
  calls: Rc<RefCell<Calls>>,
}

impl Hub {
  pub fn new(cx: &mut App) -> Self {
    Self {
      state: cx.new(|_| Map::new()),
      calls: Rc::default(),
    }
  }

  /// The calls for the service that starts, for the service's [`module`]
  pub fn start_service(&self) -> flume::Receiver<Call> {
    self.calls.borrow_mut().start_service()
  }

  /// Fails every call still waiting for the service
  pub fn stop_service(&self) {
    self.calls.borrow_mut().stop_service();
  }

  /// Asks the service, answers within [`CALL_TIMEOUT`]
  pub fn call(
    &self,
    method: String,
    args: Value,
    cx: &App,
  ) -> Result<impl Future<Output = Result<Value>> + Send + use<>> {
    let rx = self.calls.borrow_mut().call(method, args)?;
    let timer = cx.background_executor().timer(CALL_TIMEOUT);
    Ok(futures_lite::future::or(
      async move {
        match rx.recv_async().await {
          Ok(answer) => answer.map_err(|e| anyhow!(e)),
          Err(_) => Err(anyhow!("service stopped")),
        }
      },
      async move {
        timer.await;
        Err(anyhow!("the service did not answer in time"))
      },
    ))
  }

  /// An id for [`Self::next_action`] and [`Self::stop_actions`]
  pub fn load(&self) -> u64 {
    let mut calls = self.calls.borrow_mut();
    calls.next_load += 1;
    calls.next_load
  }

  pub fn next_action(&self, load: u64) -> flume::Receiver<ActionEvent> {
    self.calls.borrow_mut().next_action(load)
  }

  /// Wakes the `nextAction`s of a load that is gone: gpui-shell never drops
  /// their futures, and a dead waiter would swallow the next action
  pub fn stop_actions(&self, load: u64) {
    (self.calls.borrow_mut().action_waiters).retain(|(l, _)| *l != load);
  }

  pub fn push_action(&self, action: ActionEvent) {
    self.calls.borrow_mut().push_action(action);
  }
}

/// `corona/plugin`: state shared by all of the plugin's views and its
/// service, and calls from the views into the service. Every plugin has it;
/// `calls` is the service's, `None` in a view. Only views have `call`, only
/// the service `nextCall`, `reply` and `fail`, but both are typed with all of
/// them: they share the plugin's `gpui-kit.d.ts`.
pub fn module(
  hub: &Hub,
  calls: Option<flume::Receiver<Call>>,
  subs: &mut Vec<Subscribe>,
) -> HostModule {
  let reads = Subscriptions::default();
  subs.push(watch(&reads, Updates::Plugin, hub.state.clone()));
  let service = calls.is_some();
  let (call, reply, fail) = (hub.clone(), hub.calls.clone(), hub.calls.clone());

  let call = named!(
    "call",
    /// Views only: calls `method` of the plugin's service, which answers
    /// with `reply` or `fail`. Fails when there is no service, or after 30
    /// seconds.
    move |cx: &mut App, method: String, args: Option<Value>| {
      call.call(method, args.unwrap_or_default(), cx)
    }
  );
  let next = named!(
    "nextCall",
    /// The service only: the next call to answer, once one comes.
    move || -> Result<_> {
      let calls = (calls.clone()).context("only the plugin's service takes calls")?;
      Ok(async move { calls.recv_async().await.map_err(|_| anyhow!("service stopped")) })
    }
  );
  let reply = named!(
    "reply",
    /// The service only: answers call `id` with `value`.
    move |id: u64, value: Value| reply.borrow_mut().answer(id, Ok(value)).err()
  );
  let fail = named!(
    "fail",
    /// The service only: answers call `id` with an error.
    move |id: u64, message: String| fail.borrow_mut().answer(id, Err(message)).err()
  );

  let module = state(hub, reads);
  match service {
    true => module.declare(call).func(next).func(reply).func(fail),
    false => module.func(call).declare(next).declare(reply).declare(fail),
  }
  .into()
}

/// `getState` and `setState`; `reads` records what a view read
fn state(hub: &Hub, reads: Subscriptions) -> Module {
  let (get, set) = (hub.state.clone(), hub.state.clone());

  Module::new("corona/plugin")
    .func(named!(
      "getState",
      /// The state under `key`, null when unset. A view that reads it renders
      /// again as it changes, in every view of the plugin.
      move |cx: Cx, key: String| {
        reads.record(Updates::Plugin);
        get.read(&cx).get(&key).cloned().unwrap_or(Value::Null)
      }
    ))
    .func(named!(
      "setState",
      /// Sets the state under `key` for all of the plugin's views; null removes it.
      move |cx: &mut App, key: String, value: Value| {
        set.update(cx, |state, cx| {
          let changed = match value {
            Value::Null => state.remove(&key).is_some(),
            value => state.insert(key, value.clone()) != Some(value),
          };
          if changed {
            cx.notify();
          }
        })
      }
    ))
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  #[test]
  fn calls_go_to_the_running_service() {
    let mut calls = Calls::default();
    let error = calls.call("a".into(), json!(1)).unwrap_err();
    assert_eq!(error.to_string(), "plugin has no running service");

    let service = calls.start_service();
    let first = calls.call("a".into(), json!(1)).unwrap();
    let second = calls.call("b".into(), Value::Null).unwrap();
    assert_eq!(service.try_recv().unwrap().method, "a");
    let call = service.try_recv().unwrap();
    assert_eq!((call.id, call.method.as_str()), (1, "b"));

    calls.answer(0, Ok(json!("one"))).unwrap();
    calls.answer(1, Err("no".into())).unwrap();
    assert_eq!(first.try_recv().unwrap(), Ok(json!("one")));
    assert_eq!(second.try_recv().unwrap(), Err("no".into()));
    assert!(calls.answer(1, Ok(Value::Null)).is_err());

    // a service that went without stopping takes no calls
    drop(service);
    assert!(calls.call("c".into(), Value::Null).is_err());
  }

  #[test]
  fn a_restart_ends_the_old_calls() {
    let mut calls = Calls::default();
    let old = calls.start_service();
    let new = calls.start_service();
    assert!(old.is_disconnected() && old.recv().is_err());
    calls.call("a".into(), Value::Null).unwrap();
    assert_eq!(new.try_recv().unwrap().method, "a");
  }

  #[test]
  fn stopping_fails_what_waits() {
    let mut calls = Calls::default();
    let service = calls.start_service();
    let waiting = calls.call("a".into(), Value::Null).unwrap();
    // a caller that gave up is forgotten
    drop(calls.call("b".into(), Value::Null).unwrap());
    calls.call("c".into(), Value::Null).unwrap();
    assert_eq!(calls.pending.len(), 2);

    calls.stop_service();
    assert_eq!(waiting.try_recv().unwrap(), Err("service stopped".into()));
    assert!(calls.pending.is_empty());
    // its `nextCall` ends once the queue is read
    assert_eq!(service.drain().count(), 3);
    assert!(service.is_disconnected());
    assert!(calls.call("d".into(), Value::Null).is_err());
  }

  #[test]
  fn actions_go_to_every_waiter_or_are_kept() {
    let mut calls = Calls::default();
    let action = |id| ActionEvent {
      id,
      key: "default".into(),
    };
    let (a, b) = (calls.next_action(1), calls.next_action(2));
    calls.push_action(action(1));
    assert_eq!(a.try_recv().unwrap().id, 1);
    assert_eq!(b.try_recv().unwrap().id, 1);

    for id in 0..20 {
      calls.push_action(action(id));
    }
    assert_eq!(calls.actions.len(), ACTION_BUFFER);
    assert_eq!(calls.next_action(1).try_recv().unwrap().id, 4);

    // a gone load's waiter is woken, the action kept
    calls.actions.clear();
    let gone = calls.next_action(3);
    calls.action_waiters.retain(|(l, _)| *l != 3);
    assert!(gone.try_recv().is_err() && gone.is_disconnected());
    calls.push_action(action(30));
    assert_eq!(calls.actions.back().unwrap().id, 30);
  }
}

use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

use anyhow::{Result, anyhow, bail};
use corona_js::{
  Cx, Data, Json, Listener, ModuleBuilder, Opt, Throws, Unsubscribe, host_module, track,
};
use gpui_kit::{App, AppContext, Entity, EventEmitter, Subscription};
use serde::Serialize;
use serde_json::{Map, Value};
use specta::{Type, specta};

use crate::{
  launcher::Port,
  module::settings::{Change, Settings},
  plugin::settings::Setting,
};

/// How long `call` waits for the service to answer
const CALL_TIMEOUT: Duration = Duration::from_secs(30);

/// A `call` for the service to answer with `reply` or `fail`.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
pub struct Call {
  pub id: u32,
  pub method: String,
  pub args: Value,
}

/// An action the user picked on one of the plugin's notifications.
#[derive(Clone, Debug, PartialEq, Serialize, Type)]
pub struct ActionEvent {
  /// The notification's id, as `notify` returned it.
  pub id: u32,
  pub key: String,
  /// The text typed, for key `inline-reply`.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub text: Option<String>,
}

/// Emits the actions picked on the plugin's notifications
pub struct Actions;

impl EventEmitter<ActionEvent> for Actions {}

type Answer = Result<Value, String>;

/// Calls into the plugin's service
#[derive(Default)]
pub struct Calls {
  /// To the running service's `onCall`, made anew as it starts
  service: Option<flume::Sender<Call>>,
  /// Counts the starts, so a stopped run cannot stop a later one
  run: u32,
  next_id: u32,
  pending: HashMap<u32, flume::Sender<Answer>>,
}

impl Calls {
  fn call(&mut self, method: String, args: Value) -> Result<flume::Receiver<Answer>> {
    let id = self.next_id;
    let call = Call { id, method, args };
    let sent = self.service.as_ref().map(|service| service.send(call));
    if !matches!(sent, Some(Ok(()))) {
      bail!("plugin has no running service");
    }
    self.next_id = self.next_id.wrapping_add(1);
    // callers that gave up
    self.pending.retain(|_, tx| !tx.is_disconnected());
    let (tx, rx) = flume::bounded(1);
    self.pending.insert(id, tx);
    Ok(rx)
  }

  fn answer(&mut self, id: u32, answer: Answer) -> Result<()> {
    let tx = self
      .pending
      .remove(&id)
      .ok_or_else(|| anyhow!("no call {id} is waiting"))?;
    // the caller timed out meanwhile
    tx.send(answer).ok();
    Ok(())
  }

  /// The calls for a service that starts, and its run; the last run's end and fail
  fn start_service(&mut self) -> (flume::Receiver<Call>, u32) {
    self.stop_service(self.run);
    self.run += 1;
    let (tx, rx) = flume::unbounded();
    self.service = Some(tx);
    (rx, self.run)
  }

  /// Fails the calls waiting for `run`, if it is the last one
  fn stop_service(&mut self, run: u32) {
    if run != self.run {
      return;
    }
    self.service = None;
    for (_, tx) in self.pending.drain() {
      tx.send(Err("service stopped".into())).ok();
    }
  }
}

/// What all instances of one plugin share: its state, settings, notification actions and
/// the calls into its service
#[derive(Clone)]
pub struct Hub {
  state: Entity<Map<String, Value>>,
  calls: Rc<RefCell<Calls>>,
  pub settings: Entity<Settings>,
  pub actions: Entity<Actions>,
  /// What the launcher asks the service and shows of it
  pub launcher: Port,
}

impl Hub {
  pub fn new(id: &str, settings: &[Setting], cx: &mut App) -> Self {
    Self {
      state: cx.new(|_| Map::new()),
      calls: Rc::default(),
      settings: Settings::new(id, settings, cx),
      actions: cx.new(|_| Actions),
      launcher: Port::new(id),
    }
  }

  pub fn secret_changed(&self, key: String, cx: &mut App) {
    self
      .settings
      .update(cx, |_, cx| cx.emit(Change::secret(key)));
  }

  /// The calls for the service that starts, and its run
  pub fn start_service(&self) -> (flume::Receiver<Call>, u32) {
    self.calls.borrow_mut().start_service()
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

  pub fn push_action(&self, action: ActionEvent, cx: &mut App) {
    self.actions.update(cx, |_, cx| cx.emit(action));
  }
}

/// Fails the calls still waiting for a run of the service once its runtime is gone
struct StopGuard(Hub, u32);

impl Drop for StopGuard {
  fn drop(&mut self) {
    self.0.calls.borrow_mut().stop_service(self.1);
  }
}

/// What the service's runtime keeps: the calls it takes
struct Service {
  calls: flume::Receiver<Call>,
  _stop: StopGuard,
}

/// Views only: calls `method` of the plugin's service, which answers with `reply` or
/// `fail`. Fails when there is no service, or after 30 seconds.
#[specta]
async fn call(
  cx: Cx,
  hub: Data<Hub>,
  method: String,
  args: Opt<Json<Value>>,
) -> Throws<Json<Value>> {
  let args = args.0.map(|a| a.0).unwrap_or_default();
  let answer = match hub.call(method, args, &cx) {
    Ok(answer) => answer.await,
    Err(e) => Err(e),
  };
  answer.map(Json).into()
}

/// The service only: calls `cb` with each call to answer.
#[specta]
fn on_call(cx: Cx, service: Data<Service>, cb: Listener<Json<Call>>) -> Unsubscribe {
  let calls = service.calls.clone();
  let listener = cb.clone();
  let task = cx.spawn(async move |cx| {
    while let Ok(call) = calls.recv_async().await {
      cx.update(|cx| listener.call(cx, Json(call)));
    }
  });
  cb.keep(Subscription::new(move || drop(task)))
}

/// The service only: answers call `id` with `value`.
#[specta]
fn reply(hub: Data<Hub>, id: u32, value: Json<Value>) -> Throws<()> {
  hub.calls.borrow_mut().answer(id, Ok(value.0)).into()
}

/// The service only: answers call `id` with an error.
#[specta]
fn fail(hub: Data<Hub>, id: u32, message: String) -> Throws<()> {
  hub.calls.borrow_mut().answer(id, Err(message)).into()
}

/// The state under `key`, null when unset. A view that reads it renders again as it
/// changes, in every view of the plugin.
#[specta]
fn get_state(cx: Cx, hub: Data<Hub>, key: String) -> Json<Value> {
  let state = &hub.state;
  track(state);
  Json(state.read(&cx).get(&key).cloned().unwrap_or_default())
}

/// Sets the state under `key` for all of the plugin's views; null removes it.
#[specta]
fn set_state(mut cx: Cx, hub: Data<Hub>, key: String, value: Json<Value>) {
  hub.state.update(&mut *cx, |state, cx| {
    let changed = match value.0 {
      Value::Null => state.remove(&key).is_some(),
      value => state.insert(key, value.clone()) != Some(value),
    };
    if changed {
      cx.notify();
    }
  })
}

/// `corona/plugin`: state shared by all of the plugin's views and its service, and calls
/// from the views into the service. Every plugin has it; a `service` takes the calls. Only views have `call`, only the service `onCall`, `reply` and `fail`, but
/// both are typed with all of them: they share the plugin's `corona.d.ts`.
pub fn module(hub: &Hub, service: bool, m: &mut ModuleBuilder) {
  m.provide(hub.clone());
  match service {
    true => {
      let (calls, run) = hub.start_service();
      m.provide(Service {
        calls,
        _stop: StopGuard(hub.clone(), run),
      });
      // in the view's order, so both write the same types
      host_module!(m, { sync: [get_state, set_state], declare: [call] });
      host_module!(m, { sync: [on_call, reply, fail] });
    }
    false => host_module!(m, {
      sync: [get_state, set_state],
      async: [call],
      declare: [on_call, reply, fail],
    }),
  }
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

    let (service, _) = calls.start_service();
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
    let (old, old_run) = calls.start_service();
    let waiting = calls.call("a".into(), Value::Null).unwrap();
    let (new, _) = calls.start_service();
    assert!(old.recv().is_ok() && old.is_disconnected() && old.recv().is_err());
    assert_eq!(waiting.try_recv().unwrap(), Err("service stopped".into()));
    // the old run stopping late leaves the new one
    calls.stop_service(old_run);
    calls.call("b".into(), Value::Null).unwrap();
    assert_eq!(new.try_recv().unwrap().method, "b");
  }

  #[test]
  fn stopping_fails_what_waits() {
    let mut calls = Calls::default();
    let (service, run) = calls.start_service();
    let waiting = calls.call("a".into(), Value::Null).unwrap();
    // a caller that gave up is forgotten
    drop(calls.call("b".into(), Value::Null).unwrap());
    calls.call("c".into(), Value::Null).unwrap();
    assert_eq!(calls.pending.len(), 2);

    calls.stop_service(run);
    assert_eq!(waiting.try_recv().unwrap(), Err("service stopped".into()));
    assert!(calls.pending.is_empty());
    // its calls end once the queue is read
    assert_eq!(service.drain().count(), 3);
    assert!(service.is_disconnected());
    assert!(calls.call("d".into(), Value::Null).is_err());
  }
}

use std::{
  cell::RefCell,
  collections::{HashMap, VecDeque},
  rc::Rc,
  time::Duration,
};

use anyhow::{Result, anyhow, bail};
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

/// A `call` for the service to answer with `reply` or `fail`.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct Call {
  pub id: u64,
  pub method: String,
  pub args: Value,
}

type Answer = Result<Value, String>;

/// Calls into the plugin's service
#[derive(Default)]
pub struct Calls {
  service: bool,
  next_id: u64,
  queue: VecDeque<Call>,
  /// The service's `nextCall`, while it waits
  waiter: Option<flume::Sender<Call>>,
  pending: HashMap<u64, flume::Sender<Answer>>,
}

impl Calls {
  fn call(&mut self, method: String, args: Value) -> Result<flume::Receiver<Answer>> {
    if !self.service {
      bail!("plugin has no running service");
    }
    // callers that gave up
    self.pending.retain(|_, tx| !tx.is_disconnected());
    let id = self.next_id;
    self.next_id += 1;
    let (tx, rx) = flume::bounded(1);
    self.pending.insert(id, tx);
    let call = Call { id, method, args };
    match self.waiter.take() {
      Some(waiter) => {
        if let Err(flume::SendError(call)) = waiter.send(call) {
          self.queue.push_back(call);
        }
      }
      None => self.queue.push_back(call),
    }
    Ok(rx)
  }

  /// The next call, now or once one comes. A second wait replaces the first.
  fn next_call(&mut self) -> flume::Receiver<Call> {
    let (tx, rx) = flume::bounded(1);
    match self.queue.pop_front() {
      Some(call) => drop(tx.send(call)),
      None => self.waiter = Some(tx),
    }
    rx
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

  fn stop_service(&mut self) {
    self.service = false;
    self.waiter = None;
    self.queue.clear();
    for (_, tx) in self.pending.drain() {
      tx.send(Err("service stopped".into())).ok();
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

  pub fn start_service(&self) {
    self.calls.borrow_mut().service = true;
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
}

/// `corona/plugin`: state shared by all of the plugin's views, and calls into
/// its service. Every plugin has it; `service` is whether this is the service.
pub fn module(hub: &Hub, service: bool, subs: &mut Vec<Subscribe>) -> HostModule {
  let reads = Subscriptions::default();
  subs.push(watch(&reads, Updates::Plugin, hub.state.clone()));
  let (get, set, call, next, reply, fail) = (
    hub.state.clone(),
    hub.state.clone(),
    hub.clone(),
    hub.calls.clone(),
    hub.calls.clone(),
    hub.calls.clone(),
  );

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
    .func(named!(
      "call",
      /// Calls `method` of the plugin's service, which answers with `reply`
      /// or `fail`. Fails when there is no service, or after 30 seconds.
      move |cx: &mut App, method: String, args: Option<Value>| {
        call.call(method, args.unwrap_or_default(), cx)
      }
    ))
    .func(named!(
      "nextCall",
      /// The service only: the next call to answer, once one comes.
      move || -> Result<_> {
        if !service {
          bail!("only the plugin's service takes calls");
        }
        let rx = next.borrow_mut().next_call();
        Ok(async move { rx.recv_async().await.map_err(|_| anyhow!("service stopped")) })
      }
    ))
    .func(named!(
      "reply",
      /// Answers call `id` with `value`.
      move |id: u64, value: Value| reply.borrow_mut().answer(id, Ok(value)).err()
    ))
    .func(named!(
      "fail",
      /// Answers call `id` with an error.
      move |id: u64, message: String| fail.borrow_mut().answer(id, Err(message)).err()
    ))
    .into()
}

#[cfg(test)]
mod tests {
  use serde_json::json;

  use super::*;

  #[test]
  fn calls_wait_for_the_service() {
    let mut calls = Calls::default();
    assert!(calls.call("a".into(), json!(1)).is_err());

    calls.service = true;
    let first = calls.call("a".into(), json!(1)).unwrap();
    // queued until the service asks
    let next = calls.next_call();
    assert_eq!(next.try_recv().unwrap().method, "a");

    // a waiting service gets it at once; a new wait replaces the old one
    let old = calls.next_call();
    let waiting = calls.next_call();
    assert!(old.try_recv().is_err());
    let second = calls.call("b".into(), Value::Null).unwrap();
    let call = waiting.try_recv().unwrap();
    assert_eq!((call.id, call.method.as_str()), (1, "b"));

    calls.answer(0, Ok(json!("one"))).unwrap();
    calls.answer(1, Err("no".into())).unwrap();
    assert_eq!(first.try_recv().unwrap(), Ok(json!("one")));
    assert_eq!(second.try_recv().unwrap(), Err("no".into()));
    assert!(calls.answer(1, Ok(Value::Null)).is_err());
  }

  #[test]
  fn stopping_fails_what_waits() {
    let mut calls = Calls {
      service: true,
      ..Default::default()
    };
    let queued = calls.call("a".into(), Value::Null).unwrap();
    // a caller that gave up is forgotten
    drop(calls.call("b".into(), Value::Null).unwrap());
    calls.call("c".into(), Value::Null).unwrap();
    assert_eq!(calls.pending.len(), 2);

    calls.stop_service();
    assert_eq!(queued.try_recv().unwrap(), Err("service stopped".into()));
    assert!(calls.queue.is_empty() && calls.pending.is_empty());
    assert!(calls.call("d".into(), Value::Null).is_err());
  }
}

//! A page a handler pushed, and the rows it keeps up to date

use std::{
  cell::{Cell, RefCell},
  rc::{Rc, Weak},
};

use corona_launcher::kit::Notify;
use gpui_kit::App;
use serde_json::json;

use super::{
  Done, Port,
  spec::{PageSpec, Row},
};

pub struct Page {
  pub spec: PageSpec,
  pub port: Port,
  me: Weak<Page>,
  rows: RefCell<Vec<Row>>,
  loading: Cell<bool>,
  closed: Cell<bool>,
  on_query: Cell<Option<u32>>,
  on_filter: Cell<Option<u32>>,
  /// What the page registered: its spec's and `onQuery`'s, and its rows'
  spec_handlers: RefCell<Vec<u32>>,
  row_handlers: RefCell<Vec<u32>>,
  /// The query last told to the plugin, which answers it for `on_query`
  asked: RefCell<Option<String>>,
  /// The filter option last told to the plugin
  told_filter: Cell<usize>,
  pub filter: Rc<Cell<usize>>,
  observers: RefCell<Vec<(u32, Notify)>>,
  next_observer: Cell<u32>,
}

impl Page {
  pub fn new(
    spec: PageSpec,
    port: Port,
    spec_handlers: Vec<u32>,
    row_handlers: Vec<u32>,
  ) -> Rc<Self> {
    Rc::new_cyclic(|me| Self {
      rows: RefCell::new(spec.rows().to_vec()),
      spec,
      port,
      me: me.clone(),
      loading: Cell::new(false),
      closed: Cell::new(false),
      on_query: Cell::new(None),
      on_filter: Cell::new(None),
      spec_handlers: RefCell::new(spec_handlers),
      row_handlers: RefCell::new(row_handlers),
      asked: RefCell::default(),
      told_filter: Cell::new(0),
      filter: Rc::default(),
      observers: RefCell::default(),
      next_observer: Cell::new(0),
    })
  }

  /// The plugin answers the query itself, so the shell does not filter
  pub fn filters(&self) -> bool {
    self.on_query.get().is_some()
  }

  pub fn loading(&self) -> bool {
    self.loading.get()
  }

  pub fn rows(&self) -> Vec<Row> {
    self.rows.borrow().clone()
  }

  /// `handler`, the plugin's `onQuery` fn, answers each typed query with rows
  pub fn set_on_query(&self, handler: u32) {
    self.on_query.set(Some(handler));
    self.spec_handlers.borrow_mut().push(handler);
  }

  pub fn set_on_filter(&self, handler: u32) {
    self.on_filter.set(Some(handler));
    self.spec_handlers.borrow_mut().push(handler);
  }

  pub fn set_loading(&self, loading: bool, cx: &mut App) {
    self.loading.set(loading);
    self.notify(cx);
  }

  /// Replaces the rows; `handlers` are the ones they registered. Rows that answer a query
  /// the page has since moved on from, or for a closed page, are dropped.
  pub fn set_rows(&self, rows: Vec<Row>, handlers: Vec<u32>, query: Option<&str>, cx: &mut App) {
    let stale = self.closed.get() || (query.is_some() && self.asked.borrow().as_deref() != query);
    if stale {
      self.port.release(&handlers);
      return;
    }
    let old = self.row_handlers.replace(handlers);
    self.port.release(&old);
    *self.rows.borrow_mut() = rows;
    self.loading.set(false);
    self.notify(cx);
  }

  /// Tells the plugin what changed since it last heard: the query, for a page that answers
  /// it, and the filter option
  pub fn sync(&self, query: &str) {
    if let Some(handler) = self.on_query.get() {
      let mut asked = self.asked.borrow_mut();
      if asked.as_deref() != Some(query) {
        *asked = Some(query.to_string());
        self.loading.set(true);
        let (page, typed) = (self.me.clone(), query.to_string());
        let done: Done = Box::new(move |answer, cx| {
          let (Some(page), Ok(answer)) = (page.upgrade(), answer) else {
            return;
          };
          match serde_json::from_str(&answer.json) {
            Ok(rows) => page.set_rows(rows, answer.handlers, Some(&typed), cx),
            Err(_) => page.port.release(&answer.handlers),
          }
        });
        self.port.call(handler, json!(query), done);
      }
    }
    let selected = self.filter.get();
    if selected != self.told_filter.get() {
      self.told_filter.set(selected);
      if let Some(handler) = self.on_filter.get() {
        let page = self.me.clone();
        let done: Done = Box::new(move |answer, cx| {
          let (Some(page), Ok(answer)) = (page.upgrade(), answer) else {
            return;
          };
          // rows, or nothing when the plugin set them itself
          match serde_json::from_str(&answer.json) {
            Ok(rows) => page.set_rows(rows, answer.handlers, None, cx),
            Err(_) => page.port.release(&answer.handlers),
          }
        });
        self.port.call(handler, json!(selected), done);
      }
    }
  }

  /// Empties the page and frees its handlers
  pub fn close(&self) {
    self.closed.set(true);
    self.rows.borrow_mut().clear();
    self.loading.set(false);
    let handlers = [self.spec_handlers.take(), self.row_handlers.take()].concat();
    self.port.release(&handlers);
  }

  /// Calls `notify` when the rows change; returns what stops it
  pub fn observe(&self, notify: Notify) -> u32 {
    let id = self.next_observer.get();
    self.next_observer.set(id + 1);
    self.observers.borrow_mut().push((id, notify));
    id
  }

  pub fn unobserve(&self, id: u32) {
    self.observers.borrow_mut().retain(|(i, _)| *i != id);
  }

  fn notify(&self, cx: &mut App) {
    let notifies: Vec<Notify> = self
      .observers
      .borrow()
      .iter()
      .map(|(_, n)| n.clone())
      .collect();
    for notify in notifies {
      notify(cx);
    }
  }
}

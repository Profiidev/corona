//! Launcher commands and pages of plugins: `[commands]` of the manifest in the root search,
//! and what the service's handlers answer with, from `corona/launcher`.

mod page;
mod source;
mod spec;
mod view;

use std::{
  cell::RefCell,
  collections::HashMap,
  path::{Path, PathBuf},
  rc::Rc,
  time::Duration,
};

use anyhow::{Result, anyhow};
use corona_js::Watches;
use gpui_kit::App;
use serde_json::Value;

pub use page::Page;
pub use source::{init, remove, set_commands};
pub use spec::{Arg, DetailSpec, FormSpec, ListSpec, PageSpec, Reply, Row};
#[cfg(test)]
pub(crate) use view::outcome;

/// How long a handler has to answer
const TIMEOUT: Duration = Duration::from_secs(30);

/// What a handler returned, as JSON, and the ids of the functions in it that it registered
pub struct Answer {
  pub json: String,
  pub handlers: Vec<u32>,
}

/// Called in the plugin's runtime with the answer of a [`Call`]
pub type Done = Box<dyn FnOnce(Result<Answer, String>, &mut App)>;

/// What the plugin's service is asked: call handler `handler` with `arg`, JSON
pub struct Call {
  pub handler: u32,
  pub arg: String,
  pub done: Done,
}

/// One plugin's side of the launcher, shared by what it registers and by the service's
/// runtime. It outlives restarts of the service, which drop its pages. It holds no JS value:
/// the handlers are in the runtime's [`Watches`].
#[derive(Clone)]
pub struct Port(Rc<RefCell<Inner>>);

#[derive(Default)]
struct Inner {
  id: String,
  dir: PathBuf,
  calls: Option<flume::Sender<Call>>,
  watches: Option<Watches>,
  /// Counts the starts, so a stopped run cannot stop a later one
  run: u32,
  next_page: u32,
  /// The handlers of `onRun`, by command name
  commands: HashMap<String, u32>,
  pages: HashMap<u32, Rc<Page>>,
}

impl Port {
  pub fn new(id: &str) -> Self {
    Self(Rc::new(RefCell::new(Inner {
      id: id.into(),
      ..Inner::default()
    })))
  }

  pub fn id(&self) -> String {
    self.0.borrow().id.clone()
  }

  /// The service starts: the calls it takes, and its run
  pub fn start(&self, dir: &Path, watches: Watches) -> (flume::Receiver<Call>, u32) {
    let (tx, rx) = flume::unbounded();
    let (pages, run) = {
      let mut inner = self.0.borrow_mut();
      inner.run += 1;
      inner.dir = dir.to_path_buf();
      inner.calls = Some(tx);
      inner.watches = Some(watches);
      inner.commands.clear();
      (inner.pages.drain().collect::<Vec<_>>(), inner.run)
    };
    pages.iter().for_each(|(_, page)| page.close());
    (rx, run)
  }

  /// The service of `run` is gone: its calls fail, its pages empty
  pub fn stop(&self, run: u32) {
    let pages = {
      let mut inner = self.0.borrow_mut();
      if inner.run != run {
        return;
      }
      inner.calls = None;
      inner.commands.clear();
      inner.pages.drain().collect::<Vec<_>>()
    };
    pages.iter().for_each(|(_, page)| page.close());
  }

  pub fn dir(&self) -> PathBuf {
    self.0.borrow().dir.clone()
  }

  /// The handler that runs command `name`; the one it replaces
  pub fn register_command(&self, name: String, handler: u32) -> Option<u32> {
    self.0.borrow_mut().commands.insert(name, handler)
  }

  pub fn command(&self, name: &str) -> Option<u32> {
    self.0.borrow().commands.get(name).copied()
  }

  /// Frees handlers of the plugin's runtime
  pub fn release(&self, handlers: &[u32]) {
    let watches = self.0.borrow().watches.clone();
    if let Some(watches) = watches {
      handlers.iter().for_each(|id| watches.release_handler(*id));
    }
  }

  /// Calls handler `handler` without waiting; `done` runs with what it answers, unless the
  /// service stops first
  pub fn call(&self, handler: u32, arg: Value, done: Done) {
    let calls = self.0.borrow().calls.clone();
    if let Some(calls) = calls {
      calls
        .send(Call {
          handler,
          arg: arg.to_string(),
          done,
        })
        .ok();
    }
  }

  /// Calls handler `handler`, answers within [`TIMEOUT`]
  pub fn request(
    &self,
    handler: u32,
    arg: Value,
    cx: &App,
  ) -> Result<impl Future<Output = Result<Value>> + Send + use<>> {
    if self.0.borrow().calls.is_none() {
      return Err(anyhow!("plugin has no running service"));
    }
    let (tx, rx) = flume::bounded(1);
    let port = self.clone();
    self.call(
      handler,
      arg,
      Box::new(move |answer, _| {
        let answer = answer.and_then(|a| {
          port.release(&a.handlers);
          serde_json::from_str::<Value>(&a.json).map_err(|e| e.to_string())
        });
        tx.send(answer).ok();
      }),
    );
    let timer = cx.background_executor().timer(TIMEOUT);
    Ok(futures_lite::future::or(
      async move {
        match rx.recv_async().await {
          Ok(answer) => answer.map_err(|e| anyhow!(e)),
          Err(_) => Err(anyhow!("service stopped")),
        }
      },
      async move {
        timer.await;
        Err(anyhow!("the plugin did not answer in time"))
      },
    ))
  }

  /// Opens a page; `handlers` are the ones its spec and rows registered
  pub fn open_page(&self, spec: PageSpec, spec_handlers: Vec<u32>, row_handlers: Vec<u32>) -> u32 {
    let page = Page::new(spec, self.clone(), spec_handlers, row_handlers);
    let mut inner = self.0.borrow_mut();
    inner.next_page += 1;
    let id = inner.next_page;
    inner.pages.insert(id, page);
    id
  }

  pub fn page(&self, id: u32) -> Option<Rc<Page>> {
    self.0.borrow().pages.get(&id).cloned()
  }

  pub fn close_page(&self, id: u32) {
    let page = self.0.borrow_mut().pages.remove(&id);
    if let Some(page) = page {
      page.close();
    }
  }
}

#[cfg(test)]
mod tests {
  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;

  #[gpui::test]
  fn a_stopped_service_fails_requests_and_empties_pages(cx: &mut TestAppContext) {
    let port = Port::new("p");
    assert!(cx.update(|cx| port.request(1, Value::Null, cx)).is_err());

    let (calls, run) = port.start(Path::new("/p"), Watches::default());
    let waiting = cx.update(|cx| port.request(1, Value::Null, cx)).unwrap();
    assert_eq!(calls.try_recv().unwrap().handler, 1);
    let spec = PageSpec::List(ListSpec {
      title: "T".into(),
      placeholder: None,
      rows: vec![Default::default()],
      grid: None,
      preview: false,
      filter: None,
      empty: None,
      query: None,
    });
    let id = port.open_page(spec, vec![], vec![]);
    let page = port.page(id).unwrap();
    assert_eq!(page.rows().len(), 1);

    // a restart ends the old calls and pages
    let (_new, new_run) = port.start(Path::new("/p"), Watches::default());
    assert!(futures::executor::block_on(waiting).is_err());
    assert!(page.rows().is_empty() && port.page(id).is_none());
    // the old run stopping late leaves the new one
    port.stop(run);
    assert!(cx.update(|cx| port.request(1, Value::Null, cx)).is_ok());

    port.stop(new_run);
    assert!(cx.update(|cx| port.request(1, Value::Null, cx)).is_err());
  }
}

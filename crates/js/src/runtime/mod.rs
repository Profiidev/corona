//! One plugin runtime: host modules, the kit, the stdlib, plugin files (TypeScript
//! stripped), the task that drives its futures, and what it keeps for its views.

use std::{
  any::Any,
  cell::{Cell, RefCell},
  collections::HashMap,
  path::{Path, PathBuf},
  rc::{Rc, Weak},
  sync::Arc,
};

use anyhow::{Result, anyhow};
use gpui_kit::{App, Entity, EntityId, ListState, Subscription, Task};
use rquickjs::{
  AsyncContext, AsyncRuntime, Ctx, Error, Exception, Function, JsLifetime, Module, Persistent,
  Value,
  loader::{ImportAttributes, Loader, Resolver},
  module::Evaluated,
};

use crate::{Enter, ModuleBuilder, PluginRef, ScriptView, Watches, now, stdlib, view::kit, with};
use module::{HostLoader, HostModules};
use plugin::PluginData;

pub(crate) mod caps;
pub(crate) mod host;
pub(crate) mod module;
pub(crate) mod plugin;
pub(crate) mod registry;
pub(crate) mod reload;
pub(crate) mod scope;
pub(crate) mod service;
pub(crate) mod ts;
pub(crate) mod watch;

/// What a runtime evaluates first
pub(crate) enum Main<'a> {
  /// A source without a file, named `main`; it cannot import files
  Source(&'a str),
  /// A file of the plugin
  File(&'a Path),
}

/// Handlers of one render and, for a dialog or a list, the fn that renders it
#[derive(Default)]
pub(crate) struct Slot {
  pub(crate) content: Option<Persistent<Function<'static>>>,
  pub(crate) handlers: Vec<Persistent<Function<'static>>>,
  /// Belongs to the last render of slot `owner`, dropped by its next
  pub(crate) transient: bool,
  /// The slot whose render kept it
  pub(crate) owner: u64,
  /// The [`Frame`] generation of the render whose `handlers` these are
  pub(crate) generation: u32,
}

/// One plugin's JS runtime and what it keeps for its views.
pub(crate) struct Runtime {
  pub(crate) slots: RefCell<HashMap<(EntityId, u64), Slot>>,
  next_slot: Cell<u64>,
  pub(crate) children: RefCell<HashMap<u32, Entity<ScriptView>>>,
  pub(crate) next_child: Cell<u32>,
  pub(crate) lists: RefCell<HashMap<(EntityId, String), ListState>>,
  /// Native component states a view keeps by id: a `Select`'s, a `DataTable`'s, ..
  keyed: RefCell<HashMap<(EntityId, String), Box<dyn Any>>>,
  /// The view classes `DockArea.register_panel` rebuilds panels from, by panel name
  pub(crate) panels: RefCell<HashMap<String, Persistent<Function<'static>>>>,
  /// What retained states' `.on` subscribed, and the view they did it for
  subs: RefCell<HashMap<u32, (Option<EntityId>, Subscription)>>,
  pub(crate) watches: Watches,
  /// The process groups of its children, ended with it
  pub(crate) groups: Arc<stdlib::ChildGroups>,
  /// The id of the plugin it runs
  pub(crate) plugin_id: String,
  /// Unique per runtime, tags the key bindings it installs
  pub(crate) owner: u64,
  _drive: Task<()>,
  // last: `Persistent` handles must be freed before the runtime
  ctx: AsyncContext,
}

// `JS_FreeRuntime` aborts on live handles, see the field order
impl Drop for Runtime {
  fn drop(&mut self) {
    self.slots.get_mut().clear();
    self.keyed.get_mut().clear();
    self.panels.get_mut().clear();
    self.subs.get_mut().clear();
    self.watches.clear();
    self.groups.terminate();
  }
}

thread_local! {
  static NEXT_OWNER: Cell<u64> = const { Cell::new(0) };
}

/// The context's [`Runtime`], in its userdata
struct RuntimeRef(Weak<Runtime>);

unsafe impl<'js> JsLifetime<'js> for RuntimeRef {
  type Changed<'to> = RuntimeRef;
}

/// The runtime JS runs in
pub(crate) fn runtime(ctx: &Ctx<'_>) -> rquickjs::Result<Rc<Runtime>> {
  ctx
    .userdata::<RuntimeRef>()
    .and_then(|r| r.0.upgrade())
    .ok_or_else(|| Exception::throw_internal(ctx, "the runtime is gone"))
}

impl Runtime {
  pub(crate) fn ctx(&self) -> &AsyncContext {
    &self.ctx
  }

  /// A new id, unique in this runtime
  pub(crate) fn slot_id(&self) -> u64 {
    self.next_slot.set(self.next_slot.get() + 1);
    self.next_slot.get()
  }

  /// Keeps `sub` until [`Self::unkeep`], `view` is dropped or the runtime is freed.
  pub(crate) fn keep(&self, view: Option<EntityId>, sub: Subscription) -> u32 {
    let id = self.slot_id() as u32;
    self.subs.borrow_mut().insert(id, (view, sub));
    id
  }

  pub(crate) fn unkeep(&self, id: u32) {
    let sub = self.subs.borrow_mut().remove(&id);
    drop(sub);
  }

  /// Keeps `content` to render a dialog, a sheet or list items of `view`; returns its key.
  pub(crate) fn slot(&self, view: EntityId, content: Function<'_>, transient: bool) -> u64 {
    let key = self.slot_id() << 32;
    let content = Some(Persistent::save(&content.ctx().clone(), content));
    let slot = Slot {
      content,
      handlers: Vec::new(),
      transient,
      owner: scope::try_frame().map_or(0, |f| f.slot),
      generation: 0,
    };
    self.slots.borrow_mut().insert((view, key), slot);
    key
  }

  /// Drops `view`'s slots that `pick` names, and what their renders kept: per-item
  /// handlers and the transient slots they made, nested ones too. ponytail: a scan of all
  /// slots per call; index them by owner if views keep thousands
  pub(crate) fn drop_slots(&self, view: EntityId, pick: impl Fn(u64, &Slot) -> bool) {
    let mut prefixes = std::collections::HashSet::new();
    loop {
      let gone: Vec<_> = (self.slots.borrow_mut())
        .extract_if(|&(v, key), s| {
          v == view
            && (pick(key, s)
              || prefixes.contains(&(key >> 32))
              || (s.transient && prefixes.contains(&(s.owner >> 32))))
        })
        .collect();
      if gone.is_empty() {
        break;
      }
      // the main render, slot 0, has no prefix of its own
      prefixes.extend(
        gone
          .iter()
          .map(|((_, key), _)| key >> 32)
          .filter(|p| *p != 0),
      );
    }
  }

  /// Drops `view`'s slots `keys`, with what their renders kept.
  pub(crate) fn unslot(&self, view: EntityId, keys: &[u64]) {
    self.drop_slots(view, |key, _| keys.contains(&key));
  }

  /// Everything kept for `view`, dropped with it
  pub(crate) fn forget_view(&self, view: EntityId) {
    let slots: Vec<_> = (self.slots.borrow_mut())
      .extract_if(|(v, _), _| *v == view)
      .collect();
    let lists: Vec<_> = (self.lists.borrow_mut())
      .extract_if(|(v, _), _| *v == view)
      .collect();
    let keyed: Vec<_> = (self.keyed.borrow_mut())
      .extract_if(|(v, _), _| *v == view)
      .collect();
    let subs: Vec<_> = (self.subs.borrow_mut())
      .extract_if(|_, (v, _)| *v == Some(view))
      .collect();
    // freed out of the borrows: a finalizer they run may come back here
    drop((slots, lists, keyed, subs));
  }

  /// What `view` keeps under `id`, made by `make` the first time.
  pub(crate) fn keyed<T: Clone + 'static>(
    &self,
    view: EntityId,
    id: String,
    make: impl FnOnce() -> T,
  ) -> T {
    let key = (view, id);
    if let Some(t) = self
      .keyed
      .borrow()
      .get(&key)
      .and_then(|b| b.downcast_ref::<T>())
    {
      return t.clone();
    }
    let t = make();
    self.keyed.borrow_mut().insert(key, Box::new(t.clone()));
    t
  }

  /// Replaces what `view` keeps under `id`.
  pub(crate) fn set_keyed<T: 'static>(&self, view: EntityId, id: String, value: T) {
    self.keyed.borrow_mut().insert((view, id), Box::new(value));
  }

  /// The list state named `id` of `view`, with `count` items
  pub(crate) fn list(&self, view: EntityId, id: String, count: usize) -> ListState {
    let mut lists = self.lists.borrow_mut();
    let state = lists
      .entry((view, id))
      .or_insert_with(|| ListState::new(count, gpui_kit::ListAlignment::Top, gpui_kit::px(200.)));
    // splice keeps the scroll position and measured heights; `reset` drops both
    let old = state.item_count();
    if count > old {
      state.splice(old..old, count - old);
    } else if count < old {
      state.splice(count..old, 0);
    }
    state.clone()
  }
}

/// Drops `view`'s slots `keys` when it is dropped: an overlay's builder holds it, so they
/// are freed however the overlay goes away
pub(crate) struct SlotGuard {
  rt: Weak<Runtime>,
  view: EntityId,
  keys: Vec<u64>,
}

impl SlotGuard {
  pub(crate) fn new(
    rt: &Rc<Runtime>,
    view: EntityId,
    keys: impl IntoIterator<Item = Option<u64>>,
  ) -> Self {
    Self {
      rt: Rc::downgrade(rt),
      view,
      keys: keys.into_iter().flatten().collect(),
    }
  }
}

impl Drop for SlotGuard {
  fn drop(&mut self) {
    if let Some(rt) = self.rt.upgrade() {
      rt.unslot(self.view, &self.keys);
    }
  }
}

/// A runtime for `plugin` with `modules`, subscribing through `watches`, driven until it is
/// dropped
pub(crate) fn new(
  plugin: Rc<PluginRef>,
  modules: Vec<ModuleBuilder>,
  watches: Watches,
  cx: &App,
) -> Result<Rc<Runtime>> {
  let modules = HostModules::new(modules);
  let mut builtin = stdlib::resolver();
  for name in kit::NAMES.iter().copied().chain(modules.names()) {
    builtin.add_module(name);
  }
  let files = Files(plugin.dir.canonicalize().ok());
  let loader = (HostLoader, kit::loader(stdlib::loader()), files.clone());
  let rt = AsyncRuntime::new()?;
  now(rt.set_loader((builtin, files), loader));
  now(rt.set_host_promise_rejection_tracker(Some(Box::new(rejected))));
  let ctx = now(AsyncContext::full(&rt))?;
  watches.set_context(ctx.clone());
  let runtime = Rc::new(Runtime {
    slots: Default::default(),
    next_slot: Cell::new(0),
    children: Default::default(),
    next_child: Cell::new(0),
    lists: Default::default(),
    keyed: Default::default(),
    panels: Default::default(),
    subs: Default::default(),
    watches,
    groups: Default::default(),
    plugin_id: plugin.id.clone(),
    owner: NEXT_OWNER.with(|n| {
      n.set(n.get() + 1);
      n.get()
    }),
    _drive: drive(&rt, cx),
    ctx: ctx.clone(),
  });
  let setup = with(&ctx, |ctx| -> rquickjs::Result<()> {
    ctx.store_userdata(modules).map_err(|_| Error::Unknown)?;
    ctx
      .store_userdata(PluginData(plugin.clone()))
      .map_err(|_| Error::Unknown)?;
    ctx
      .store_userdata(RuntimeRef(Rc::downgrade(&runtime)))
      .map_err(|_| Error::Unknown)?;
    kit::install(&ctx)?;
    stdlib::install(&ctx, &plugin)
  });
  setup.map_err(|e| anyhow!("runtime setup: {e}"))?;
  Ok(runtime)
}

/// Drives async host fns, timers and the promise jobs they resolve, inside a scope of the
/// app, until the task is dropped.
fn drive(rt: &AsyncRuntime, cx: &App) -> Task<()> {
  let mut drive = Box::pin(rt.drive());
  cx.spawn(async move |cx| {
    std::future::poll_fn(|task| {
      cx.update(|cx| {
        let _enter = Enter::new(cx, None, None);
        drive.as_mut().poll(task)
      })
    })
    .await
  })
}

/// Evaluates `main` and waits for its top level
pub(crate) fn eval<'js>(
  ctx: &Ctx<'js>,
  main: Main<'_>,
) -> rquickjs::Result<Module<'js, Evaluated>> {
  let (name, source) = match main {
    Main::Source(source) => ("main".to_string(), source.to_string()),
    Main::File(path) => {
      let path = path
        .canonicalize()
        .map_err(|e| Error::new_loading_message(path.display().to_string(), e.to_string()))?;
      let name = path.display().to_string();
      let source = read(&path).map_err(|e| Error::new_loading_message(&name, e.to_string()))?;
      (name, source)
    }
  };
  let (module, done) = Module::declare(ctx.clone(), name, source)?.eval()?;
  done.finish::<()>()?;
  Ok(module)
}

/// A source file, types stripped from TypeScript
fn read(path: &Path) -> Result<String> {
  let source = std::fs::read_to_string(path)?;
  match ts::is_ts(path) {
    true => ts::strip(path, &source),
    false => Ok(source),
  }
}

/// Resolves and loads relative imports among the files of a plugin directory; nothing
/// outside it, also not through a link.
#[derive(Clone)]
struct Files(Option<PathBuf>);

impl Files {
  /// The file `name` names from module `base`: `.js` wins over `.ts` without an extension,
  /// and a missing `x.js` falls back to `x.ts`
  fn find(&self, base: &str, name: &str) -> Option<PathBuf> {
    let root = self.0.as_ref()?;
    if !(name.starts_with("./") || name.starts_with("../")) {
      return None;
    }
    let joined = Path::new(base).parent()?.join(name);
    let candidates = match joined.extension().and_then(|e| e.to_str()) {
      Some("js") | Some("mjs") => vec![joined.clone(), joined.with_extension("ts")],
      Some("ts") | Some("mts") => vec![joined.clone()],
      _ => ["js", "ts"]
        .iter()
        .map(|ext| PathBuf::from(format!("{}.{ext}", joined.display())))
        .collect(),
    };
    candidates
      .into_iter()
      .filter_map(|c| c.canonicalize().ok())
      .find(|c| c.is_file() && c.starts_with(root))
  }
}

impl Resolver for Files {
  fn resolve<'js>(
    &mut self,
    _: &Ctx<'js>,
    base: &str,
    name: &str,
    _: Option<ImportAttributes<'js>>,
  ) -> rquickjs::Result<String> {
    match self.find(base, name) {
      Some(path) => Ok(path.display().to_string()),
      None => Err(Error::new_resolving(base, name)),
    }
  }
}

impl Loader for Files {
  fn load<'js>(
    &mut self,
    ctx: &Ctx<'js>,
    name: &str,
    _: Option<ImportAttributes<'js>>,
  ) -> rquickjs::Result<Module<'js>> {
    let path = Path::new(name);
    if !self.0.as_ref().is_some_and(|root| path.starts_with(root)) {
      return Err(Error::new_loading(name));
    }
    let source = read(path).map_err(|e| Error::new_loading_message(name, format!("{e:#}")))?;
    Module::declare(ctx.clone(), name, source)
  }
}

/// Logs a promise rejected without a handler. ponytail: logged when it rejects, also if a
/// handler is attached a tick later; keep and report them after the jobs ran if that is noisy
fn rejected<'js>(_: Ctx<'js>, _promise: Value<'js>, reason: Value<'js>, handled: bool) {
  if handled {
    return;
  }
  match reason.as_exception() {
    Some(e) => tracing::warn!("unhandled promise rejection: {e}"),
    None => tracing::warn!("unhandled promise rejection: {reason:?}"),
  }
}

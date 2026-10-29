//! Plugin views: `class X extends View { init(props, cx); update?(props); render(cx) }`, the
//! `cx` their methods get, and retained child views (`cx.new(Class, props)`).
//!
//! All views of a plugin share one [`Runtime`]. JS objects never own Rust things that hold
//! the runtime (that would be a cycle QuickJS cannot free): child views, handlers and slot
//! renderers live in the runtime, JS holds their keys.

use std::{
  cell::{Cell, RefCell},
  collections::HashMap,
  path::Path,
  rc::Rc,
  time::Duration,
};

use anyhow::anyhow;
use gpui_kit::{
  AnyElement, AppContext, ClipboardItem, Context, Entity, EntityId, IntoElement, KeyBinding,
  KeyBindingContextPredicate, Render, Subscription, WeakEntity, Window,
};
use rquickjs::{
  CatchResultExt, Class, Ctx, Exception, FromJs, Function, IntoJs, JsLifetime, Object, Persistent,
  Promise, Result, Value,
  class::Trace,
  function::{Args, Constructor, This},
};

use crate::{
  Child, Enter, Frame, ModuleBuilder, Named, Opt, PluginRef, Watches, app,
  events::ScriptAction,
  plugin::plugin,
  runtime::{self, Main, Runtime, Slot, runtime},
  scope::{try_frame, try_window},
  with,
};

thread_local! {
  /// The views JS runs for, innermost last
  static CURRENT: RefCell<Vec<WeakEntity<ScriptView>>> = const { RefCell::new(Vec::new()) };
}

/// The view whose init, render or handler is running
pub(crate) fn current_view() -> Option<WeakEntity<ScriptView>> {
  CURRENT.with(|c| c.borrow().last().cloned())
}

struct Current;

impl Current {
  fn new(view: WeakEntity<ScriptView>) -> Self {
    CURRENT.with(|c| c.borrow_mut().push(view));
    Current
  }
}

impl Drop for Current {
  fn drop(&mut self) {
    CURRENT.with(|c| c.borrow_mut().pop());
  }
}

/// A plugin view, rendered through the [`Frame`] arena.
pub struct ScriptView {
  instance: Persistent<Object<'static>>,
  /// The entities the last render read
  observed: HashMap<EntityId, Subscription>,
  id: EntityId,
  /// Owns the runtime's child views
  root: bool,
  #[cfg(test)]
  pub(crate) text: String,
  #[cfg(test)]
  pub(crate) renders: u32,
  // last: frees the instance before the runtime
  rt: Rc<Runtime>,
}

impl ScriptView {
  /// Evaluates `source`, whose default export extends `View`, and calls its `init`. The
  /// module can import `gpui-kit`, `gpui-base`, `gpui-component`, the stdlib and `modules`,
  /// which subscribe through `watches`. It runs with no plugin's grant.
  pub fn new(
    source: &str,
    modules: Vec<ModuleBuilder>,
    watches: Watches,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> anyhow::Result<Self> {
    let plugin = Rc::new(PluginRef::default());
    Self::start(Main::Source(source), plugin, modules, watches, window, cx)
  }

  /// Like [`new`](Self::new) for the file `entry` of `plugin`, which may import its other
  /// files.
  pub fn load(
    entry: &Path,
    plugin: Rc<PluginRef>,
    modules: Vec<ModuleBuilder>,
    watches: Watches,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> anyhow::Result<Self> {
    Self::start(Main::File(entry), plugin, modules, watches, window, cx)
  }

  fn start(
    main: Main,
    plugin: Rc<PluginRef>,
    modules: Vec<ModuleBuilder>,
    watches: Watches,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> anyhow::Result<Self> {
    let rt = runtime::new(plugin, modules, watches, cx)?;
    let ctx = rt.ctx().clone();

    let view = cx.entity().downgrade();
    let id = view.entity_id();
    let _enter = Enter::new(cx, Some(window), None);
    let _current = Current::new(view.clone());
    let instance = with(&ctx, |ctx| {
      let run = || {
        let module = runtime::eval(&ctx, main)?;
        let class: Constructor = module.get("default")?;
        let props = Value::new_undefined(ctx.clone());
        construct(&ctx, class, props, view)
      };
      run().catch(&ctx).map_err(|e| anyhow!("{e}"))
    })?;

    Ok(Self::from_parts(id, instance, rt, true))
  }

  fn from_parts(
    id: EntityId,
    instance: Persistent<Object<'static>>,
    rt: Rc<Runtime>,
    root: bool,
  ) -> Self {
    Self {
      id,
      instance,
      observed: HashMap::new(),
      root,
      #[cfg(test)]
      text: String::new(),
      #[cfg(test)]
      renders: 0,
      rt,
    }
  }

  /// Calls into JS for this view, in a scope of `window` and `cx`; logs what it throws.
  fn call<R>(
    &mut self,
    window: Option<&mut Window>,
    frame: Option<&mut Frame>,
    cx: &mut Context<Self>,
    f: impl for<'js> FnOnce(&Ctx<'js>, Object<'js>) -> Result<R>,
  ) -> anyhow::Result<R> {
    let _current = Current::new(cx.entity().downgrade());
    let _enter = Enter::new(cx, window, frame);
    let instance = self.instance.clone();
    with(self.rt.ctx(), |ctx| {
      let run = || f(&ctx, instance.restore(&ctx)?);
      run().catch(&ctx).map_err(|e| anyhow!("{e}"))
    })
  }

  /// The JS object of the view
  pub(crate) fn instance(&self) -> &Persistent<Object<'static>> {
    &self.instance
  }

  #[cfg(test)]
  pub(crate) fn runtime(&self) -> std::rc::Weak<Runtime> {
    Rc::downgrade(&self.rt)
  }

  /// Evaluates `code` in the view's runtime, as a string
  #[cfg(test)]
  pub(crate) fn eval(&self, code: &str) -> String {
    with(self.rt.ctx(), |ctx| {
      ctx
        .eval::<rquickjs::Coerced<String>, _>(code)
        .catch(&ctx)
        .map(|s| s.0)
        .unwrap_or_else(|e| format!("eval failed: {e}"))
    })
  }

  /// Calls handler `index` of render `slot` with `payload` and the `cx`.
  pub(crate) fn dispatch<P: for<'js> IntoJs<'js>>(
    &mut self,
    slot: u64,
    index: usize,
    payload: P,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) {
    let view = cx.entity().downgrade();
    let handler = (self.rt.slots.borrow())
      .get(&(view.entity_id(), slot))
      .and_then(|s| s.handlers.get(index).cloned());
    let Some(handler) = handler else {
      return;
    };
    let call = self.call(Some(window), None, cx, |ctx, _| {
      handler
        .restore(ctx)?
        .call::<_, ()>((payload, ViewCx { view }))
    });
    if let Err(e) = call {
      tracing::warn!("event handler: {e}");
    }
  }

  /// Runs `f` in a new frame for `slot`, keeping the handlers it registers.
  fn render_slot<R>(
    &mut self,
    slot: u64,
    window: &mut Window,
    cx: &mut Context<Self>,
    f: impl for<'js> FnOnce(&Ctx<'js>, Object<'js>, &mut Frame) -> Result<R>,
  ) -> anyhow::Result<R> {
    let id = cx.entity_id();
    if slot == 0 {
      self
        .rt
        .slots
        .borrow_mut()
        .retain(|(v, _), s| *v != id || !s.transient);
    }
    let mut frame = Frame::new(cx.entity().downgrade(), slot);
    let out = self.call(Some(window), Some(&mut frame), cx, |ctx, instance| {
      f(ctx, instance, try_frame().expect("frame set"))
    });
    #[cfg(test)]
    if slot == 0 {
      self.text = std::mem::take(&mut frame.text);
      self.renders += 1;
    }
    let handlers = std::mem::take(&mut frame.handlers);
    let transient = slot != 0 && slot & 0xffff_ffff != 0;
    let mut slots = self.rt.slots.borrow_mut();
    let entry = slots.entry((id, slot)).or_insert_with(|| Slot {
      transient,
      ..Default::default()
    });
    entry.handlers = handlers;
    drop(slots);
    if slot == 0 {
      self.observed.retain(|id, _| frame.reads.contains_key(id));
    }
    for (id, observe) in frame.reads {
      self.observed.entry(id).or_insert_with(|| observe(cx));
    }
    out
  }

  /// Renders slot `key`'s content fn, called with `args` and the `cx`: a dialog's, or item
  /// `index` of a list (whose handlers are kept per item).
  pub(crate) fn render_content<R>(
    &mut self,
    key: u64,
    index: Option<usize>,
    window: &mut Window,
    cx: &mut Context<Self>,
    args: impl for<'js> FnOnce(&mut Args<'js>) -> Result<()>,
    read: impl for<'js> FnOnce(&Ctx<'js>, &mut Frame, Value<'js>) -> Result<R>,
  ) -> anyhow::Result<R> {
    let id = cx.entity_id();
    let content = (self.rt.slots.borrow())
      .get(&(id, key))
      .and_then(|s| s.content.clone())
      .ok_or_else(|| anyhow!("the content is gone"))?;
    let view = cx.entity().downgrade();
    let slot = key | index.map_or(0, |i| i as u64 + 1);
    self.render_slot(slot, window, cx, |ctx, _, frame| {
      let mut call = Args::new_unsized(ctx.clone());
      args(&mut call)?;
      call.push_arg(ViewCx { view })?;
      let value = content.restore(ctx)?.call_arg::<Value>(call)?;
      read(ctx, frame, value)
    })
  }
}

/// Constructs `class` with `props`, calls its `init(props, cx)`
pub(crate) fn construct<'js>(
  ctx: &Ctx<'js>,
  class: Constructor<'js>,
  props: Value<'js>,
  view: WeakEntity<ScriptView>,
) -> Result<Persistent<Object<'static>>> {
  let instance: Object = class.construct((props.clone(),))?;
  if let Ok(init) = instance.get::<_, Function>("init") {
    init.call::<_, ()>((This(instance.clone()), props, ViewCx { view }))?;
  }
  Ok(Persistent::save(ctx, instance))
}

impl Render for ScriptView {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let view = cx.entity().downgrade();
    self
      .render_slot(0, window, cx, |ctx, instance, frame| {
        let render: Function = instance.get("render")?;
        let root: Child = render.call((This(instance), ViewCx { view }))?;
        frame.child(ctx, root)
      })
      .unwrap_or_else(|e| format!("render failed: {e}").into_any_element())
  }
}

impl Drop for ScriptView {
  fn drop(&mut self) {
    let id = self.id;
    self.rt.slots.borrow_mut().retain(|(v, _), _| *v != id);
    self.rt.lists.borrow_mut().retain(|(v, _), _| *v != id);
    if self.root {
      // breaks the cycle runtime -> child view -> runtime
      let children = std::mem::take(&mut *self.rt.children.borrow_mut());
      drop(children);
    }
  }
}

/// A retained child view from `cx.new(Class, props)`
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Entity")]
pub struct ViewHandle {
  #[qjs(skip_trace)]
  id: u32,
}

impl Named for ViewHandle {
  const TS: &'static str = "Entity";
}

impl ViewHandle {
  pub(crate) fn entity(&self, ctx: &Ctx<'_>) -> Result<Entity<ScriptView>> {
    let rt = runtime(ctx)?;
    let children = rt.children.borrow();
    children
      .get(&self.id)
      .cloned()
      .ok_or_else(|| Exception::throw_type(ctx, "the view was released"))
  }

  pub(crate) fn id(&self) -> u32 {
    self.id
  }

  /// The child as an element
  pub(crate) fn element(&self, ctx: &Ctx<'_>) -> Result<AnyElement> {
    Ok(self.entity(ctx)?.into_any_element())
  }
}

#[rquickjs::methods]
impl ViewHandle {
  /// Hands the child new props: runs its `update(props)` and renders it again.
  pub fn set_props<'js>(&self, ctx: Ctx<'js>, props: Opt<Value<'js>>) -> Result<()> {
    let entity = self.entity(&ctx)?;
    let props = props.0.unwrap_or_else(|| Value::new_undefined(ctx.clone()));
    entity.update(app(), |view, cx| {
      let _current = Current::new(cx.entity().downgrade());
      let instance = view.instance.clone().restore(&ctx)?;
      if let Ok(update) = instance.get::<_, Function>("update") {
        update.call::<_, ()>((This(instance), props))?;
      }
      cx.notify();
      Ok(())
    })
  }

  /// Drops the child view; false if it was released already.
  pub fn release(&self, ctx: Ctx<'_>) -> Result<bool> {
    let rt = runtime(&ctx)?;
    let removed = rt.children.borrow_mut().remove(&self.id);
    Ok(removed.is_some())
  }
}

/// A retained child view of `class`, owned by the runtime: `cx.new(Class, props)`
pub(crate) fn new_child<'js>(
  ctx: &Ctx<'js>,
  class: Constructor<'js>,
  props: Value<'js>,
) -> Result<(ViewHandle, Entity<ScriptView>)> {
  let rt = runtime(ctx)?;
  let mut error = None;
  let entity = app().new(|cx: &mut Context<ScriptView>| {
    let view = cx.entity().downgrade();
    let _current = Current::new(view.clone());
    let instance = construct(ctx, class, props, view).unwrap_or_else(|e| {
      error = Some(e);
      Persistent::save(ctx, Object::new(ctx.clone()).expect("object"))
    });
    ScriptView::from_parts(cx.entity_id(), instance, rt.clone(), false)
  });
  if let Some(e) = error {
    return Err(e);
  }
  let id = rt.next_child.get() + 1;
  rt.next_child.set(id);
  rt.children.borrow_mut().insert(id, entity.clone());
  Ok((ViewHandle { id }, entity))
}

/// A focus target the script keeps, from `cx.focus_handle()`
#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct FocusHandle {
  #[qjs(skip_trace)]
  handle: Option<gpui_kit::FocusHandle>,
}

impl Named for FocusHandle {
  const TS: &'static str = "FocusHandle";
}

impl FocusHandle {
  pub(crate) fn handle(&self, ctx: &Ctx<'_>) -> Result<gpui_kit::FocusHandle> {
    (self.handle.clone()).ok_or_else(|| Exception::throw_type(ctx, "the focus handle was released"))
  }
}

/// The window of the current call, or a JS error
pub(crate) fn window_or_throw(ctx: &Ctx<'_>) -> Result<&'static mut Window> {
  try_window().ok_or_else(|| {
    Exception::throw_type(
      ctx,
      "no window here: call this from render or an event handler",
    )
  })
}

#[rquickjs::methods]
impl FocusHandle {
  /// Moves the keyboard onto the element tracking this handle.
  pub fn focus(&self, ctx: Ctx<'_>) -> Result<()> {
    let handle = self.handle(&ctx)?;
    handle.focus(window_or_throw(&ctx)?, app());
    Ok(())
  }

  /// Whether the element tracking this handle has the keyboard.
  pub fn is_focused(&self) -> bool {
    match (&self.handle, try_window()) {
      (Some(handle), Some(window)) => handle.is_focused(window),
      _ => false,
    }
  }

  pub fn release(&mut self) -> bool {
    self.handle.take().is_some()
  }
}

/// One entry of `cx.bind_keys`
pub struct Binding {
  keystroke: String,
  action: String,
  context: Option<String>,
}

impl<'js> FromJs<'js> for Binding {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    let o = Object::from_js(ctx, value)?;
    Ok(Binding {
      keystroke: o.get("keystroke")?,
      action: o.get("action")?,
      context: o.get("context")?,
    })
  }
}

/// An absolute `http`/`https` URL with a host
fn web_url(url: &str) -> bool {
  let rest = url
    .strip_prefix("https://")
    .or_else(|| url.strip_prefix("http://"));
  let host = rest.and_then(|r| r.split(['/', '?', '#']).next());
  host.is_some_and(|h| !h.is_empty()) && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// The `cx` a view's methods and handlers get
#[derive(Trace, JsLifetime, Clone)]
#[rquickjs::class(rename = "Context")]
pub struct ViewCx {
  #[qjs(skip_trace)]
  view: WeakEntity<ScriptView>,
}

impl ViewCx {
  pub(crate) fn for_view(view: WeakEntity<ScriptView>) -> Self {
    Self { view }
  }

  /// Runs `cb(cx)` after `ms`, and every `ms` after that when `repeat`. Returns the canceller.
  fn schedule<'js>(
    &self,
    ctx: &Ctx<'js>,
    ms: f64,
    cb: Function<'js>,
    repeat: bool,
  ) -> Result<Object<'js>> {
    let stop = Rc::new(Cell::new(false));
    let executor = app().background_executor().clone();
    let (cx, stopped, js) = (self.clone(), stop.clone(), ctx.clone());
    let every = Duration::from_secs_f64(ms.max(0.) / 1000.);
    ctx.spawn(async move {
      loop {
        executor.timer(every).await;
        if stopped.get() {
          break;
        }
        if let Err(e) = cb.call::<_, ()>((cx.clone(),)).catch(&js) {
          tracing::warn!("timer callback: {e}");
        }
        if !repeat {
          break;
        }
      }
    });
    let handle = Object::new(ctx.clone())?;
    handle.set(
      "cancel",
      Function::new(ctx.clone(), move || stop.set(true))?,
    )?;
    Ok(handle)
  }
}

#[rquickjs::methods]
impl ViewCx {
  /// Renders the view again, or the retained child `target`. Throws while rendering.
  pub fn notify<'js>(&self, ctx: Ctx<'js>, target: Opt<Class<'js, ViewHandle>>) -> Result<()> {
    if try_frame().is_some() {
      return Err(Exception::throw_type(
        &ctx,
        "cx.notify() while rendering would loop",
      ));
    }
    match target.0 {
      Some(target) => app().notify(target.borrow().entity(&ctx)?.entity_id()),
      None => app().notify(self.view.entity_id()),
    }
    Ok(())
  }

  /// Installs key bindings `{ keystroke, action, context? }`, all or none; returns how many.
  pub fn bind_keys(&self, ctx: Ctx<'_>, bindings: Vec<Binding>) -> Result<u32> {
    let bindings = bindings
      .into_iter()
      .map(|b| {
        let context = (b.context.as_deref())
          .map(KeyBindingContextPredicate::parse)
          .transpose()
          .map_err(|e| anyhow!("context of {}: {e}", b.keystroke))?;
        KeyBinding::load(
          &b.keystroke,
          Box::new(ScriptAction(b.action.into())),
          context.map(Rc::new),
          false,
          None,
          &gpui_kit::DummyKeyboardMapper,
        )
        .map_err(|e| anyhow!("{}: {e}", b.keystroke))
      })
      .collect::<anyhow::Result<Vec<_>>>()
      .map_err(|e| Exception::throw_type(&ctx, &e.to_string()))?;
    let count = bindings.len() as u32;
    app().bind_keys(bindings);
    Ok(count)
  }

  /// Stops this event reaching the handlers further out.
  pub fn stop_propagation(&self) {
    app().stop_propagation();
  }

  /// Undoes a `stop_propagation()` of the same dispatch.
  pub fn propagate(&self) {
    app().propagate();
  }

  /// `"render"`, `"event"` (with a window) or `"task"`
  pub fn phase(&self) -> &'static str {
    match (try_frame(), try_window()) {
      (Some(_), _) => "render",
      (None, Some(_)) => "event",
      (None, None) => "task",
    }
  }

  /// The theme's semantic tokens.
  pub fn theme<'js>(&self, ctx: Ctx<'js>) -> Result<Object<'js>> {
    crate::theme::theme_object(&ctx)
  }

  /// Opens an absolute `http`/`https` URL with the system's handler.
  pub fn open_url(&self, ctx: Ctx<'_>, url: String) -> Result<()> {
    if !web_url(&url) {
      return Err(Exception::throw_type(
        &ctx,
        &format!("not an http(s) URL: {url}"),
      ));
    }
    app().open_url(&url);
    Ok(())
  }

  /// The clipboard's text, if it holds text.
  pub fn read_from_clipboard(&self, ctx: Ctx<'_>) -> Result<Option<String>> {
    if !plugin(&ctx)?.capabilities.is_clipboard_readable() {
      return Err(Exception::throw_type(
        &ctx,
        "reading the clipboard is not granted; set capabilities.clipboard.read to true",
      ));
    }
    Ok(app().read_from_clipboard().and_then(|item| item.text()))
  }

  pub fn write_to_clipboard(&self, ctx: Ctx<'_>, text: String) -> Result<()> {
    if !plugin(&ctx)?.capabilities.is_clipboard_writable() {
      return Err(Exception::throw_type(
        &ctx,
        "writing the clipboard is not granted; set capabilities.clipboard.write to true",
      ));
    }
    app().write_to_clipboard(ClipboardItem::new_string(text));
    Ok(())
  }

  /// A focus target to keep on the view and hand to `track_focus`. Not in `render`.
  pub fn focus_handle(&self, ctx: Ctx<'_>) -> Result<FocusHandle> {
    if try_frame().is_some() {
      return Err(Exception::throw_type(
        &ctx,
        "cx.focus_handle() in render makes a new one every frame",
      ));
    }
    Ok(FocusHandle {
      handle: Some(app().focus_handle()),
    })
  }

  /// Creates a retained child view; mount it with `.child(entity)`. Not in `render`.
  #[qjs(rename = "new")]
  pub fn new_view<'js>(
    &self,
    ctx: Ctx<'js>,
    class: Constructor<'js>,
    props: Opt<Value<'js>>,
  ) -> Result<ViewHandle> {
    if try_frame().is_some() {
      return Err(Exception::throw_type(
        &ctx,
        "cx.new() in render; create child views in init or a handler",
      ));
    }
    let props = props.0.unwrap_or_else(|| Value::new_undefined(ctx.clone()));
    Ok(new_child(&ctx, class, props)?.0)
  }

  /// Calls `body(cx)` and returns what it returns: the promise of an async body.
  pub fn spawn<'js>(&self, body: Function<'js>) -> Result<Value<'js>> {
    body.call((self.clone(),))
  }

  /// Resolves after `ms` milliseconds.
  pub fn sleep<'js>(&self, ctx: Ctx<'js>, ms: Opt<f64>) -> Result<Promise<'js>> {
    let timer = app()
      .background_executor()
      .timer(Duration::from_secs_f64(ms.0.unwrap_or(0.).max(0.) / 1000.));
    Promise::wrap_future(&ctx, timer)
  }

  /// One-shot and repeating callbacks.
  #[qjs(get)]
  pub fn timer<'js>(&self, ctx: Ctx<'js>) -> Result<Object<'js>> {
    let timer = Object::new(ctx.clone())?;
    let cx = self.clone();
    timer.set(
      "after",
      Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>, ms: f64, cb: Function<'js>| cx.schedule(&ctx, ms, cb, false),
      )?,
    )?;
    let cx = self.clone();
    timer.set(
      "every",
      Function::new(
        ctx.clone(),
        move |ctx: Ctx<'js>, ms: f64, cb: Function<'js>| cx.schedule(&ctx, ms, cb, true),
      )?,
    )?;
    Ok(timer)
  }
}

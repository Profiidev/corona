//! `corona/launcher`: answers the plugin's `[commands]` and builds the pages they push.
//!
//! The plugin's functions stay in its runtime's [`Watches`] as handlers; the launcher calls
//! them by id and gets JSON back.

use std::{cell::Cell, marker::PhantomData, path::PathBuf};

use corona_js::{
  Cx, Data, Handler, ModuleBuilder, PluginRef, ScriptModule, Spec, Throws, Watches, encode,
  host_module,
  rquickjs::{Ctx, Function, IntoJs, Object, Result, Value, object::Property},
};
use gpui_kit::{App, Subscription};
use specta::{Type, Types, datatype::DataType, specta};
use specta_typescript::{Typescript, define, primitives};

use crate::{
  ScriptManager,
  launcher::{Answer, Arg, Call, DetailSpec, Done, FormSpec, ListSpec, PageSpec, Port, Reply, Row},
};

pub struct Launcher;

impl ScriptModule for Launcher {
  const NAME: &'static str = "launcher";
  const DOC: &'static str =
    "Answer the plugin's launcher commands and push pages of rows, forms and details";

  fn module(plugin: &PluginRef, m: &mut ModuleBuilder, cx: &mut App) {
    let port = ScriptManager::hub(&plugin.id, cx).launcher.clone();
    build(port, plugin.dir.clone(), m);
  }
}

/// What one runtime of the plugin shares
struct Module {
  port: Port,
  watches: Watches,
  dir: PathBuf,
  started: Cell<bool>,
}

fn build(port: Port, dir: PathBuf, m: &mut ModuleBuilder) {
  m.provide(Module {
    port,
    watches: m.watches().clone(),
    dir,
    started: Cell::new(false),
  });
  host_module!(m, { sync: [on_run, list, form, detail] });
}

/// Stops the service's run of the port with the runtime
struct Stop(Port, u32);

impl Drop for Stop {
  fn drop(&mut self) {
    self.0.stop(self.1);
  }
}

impl Module {
  /// The first use takes the calls of the port, until the runtime ends
  fn start(&self) {
    if self.started.replace(true) {
      return;
    }
    let (calls, run) = self.port.start(&self.dir, self.watches.clone());
    let stop = Stop(self.port.clone(), run);
    let watches = self.watches.clone();
    let cx = Cx;
    let task = cx.spawn(async move |cx| {
      while let Ok(Call { handler, arg, done }) = calls.recv_async().await {
        let watches = watches.clone();
        cx.update(|cx| {
          let w = watches.clone();
          watches.call_handler(handler, &arg, cx, move |ctx, result, cx| {
            settle(&w, ctx, result, done, cx)
          })
        });
      }
    });
    self.watches.hold(Subscription::new(move || {
      drop(task);
      drop(stop);
    }));
  }

  /// Opens a page; `handlers` are all the functions its spec registered
  fn open<K: Kind>(
    &self,
    spec: PageSpec,
    mut handlers: Vec<u32>,
    module: &Data<Module>,
  ) -> Handle<K> {
    let rows: Vec<u32> = spec.rows().iter().flat_map(Row::handlers).collect();
    handlers.retain(|h| !rows.contains(h));
    let id = self.port.open_page(spec, handlers, rows);
    Handle {
      module: module.clone(),
      id,
      _kind: PhantomData,
    }
  }
}

/// Encodes what a handler returned and hands it to `done`
fn settle<'js>(
  watches: &Watches,
  ctx: &Ctx<'js>,
  result: std::result::Result<Value<'js>, String>,
  done: Done,
  cx: &mut App,
) {
  let answer = result.and_then(|value| {
    let sink = std::rc::Rc::default();
    let json = encode(ctx, watches, value, &sink).map_err(|e| e.to_string())?;
    Ok(Answer {
      json,
      handlers: sink.take(),
    })
  });
  done(answer, cx);
}

/// Answers the command `name` of `plugin.toml`. `handler` gets the typed arguments in the
/// order of the command's `args`, `null` for a skipped optional one, and returns what the
/// launcher does next. Service only.
#[specta]
fn on_run(module: Data<Module>, name: String, handler: Handler<Vec<Option<Arg>>, Reply>) {
  module.start();
  if let Some(old) = module.port.register_command(name, handler.id) {
    module.watches.release_handler(old);
  }
}

/// A page of rows, to push with `{ push: page }`.
#[specta]
fn list(module: Data<Module>, spec: Spec<ListSpec>) -> Throws<Handle<ListKind>> {
  module.start();
  Throws(Ok(module.open(
    PageSpec::List(spec.value),
    spec.handlers,
    &module,
  )))
}

/// A page with fields to fill in, to push with `{ push: page }`.
#[specta]
fn form(module: Data<Module>, spec: Spec<FormSpec>) -> Throws<Handle<FormKind>> {
  module.start();
  Throws(Ok(module.open(
    PageSpec::Form(spec.value),
    spec.handlers,
    &module,
  )))
}

/// A page of text with key-value rows and actions, to push with `{ push: page }`.
#[specta]
fn detail(module: Data<Module>, spec: Spec<DetailSpec>) -> Throws<Handle<DetailKind>> {
  module.start();
  Throws(Ok(module.open(
    PageSpec::Detail(spec.value),
    spec.handlers,
    &module,
  )))
}

/// The kind of a page, for the type of its handle
trait Kind {
  /// Has the methods of a list
  const LIST: bool;
  fn ts(types: &mut Types) -> String;
}

struct ListKind;
struct FormKind;
struct DetailKind;

/// `Row[]` as TypeScript
fn rows_ts(types: &mut Types) -> String {
  let dt = <Vec<Row>>::definition(types);
  primitives::reference(&Typescript::default(), types, &dt).expect("host type")
}

impl Kind for ListKind {
  const LIST: bool = true;
  fn ts(types: &mut Types) -> String {
    let rows = rows_ts(types);
    format!(
      "{{\n    /** Identifies the page. */\n    id: number;\n    /** Replaces the rows; the launcher filters them by the typed query. */\n    setRows(rows: {rows}): void;\n    /** Shows the loading bar. */\n    setLoading(loading: boolean): void;\n    /** The plugin ranks and filters: `fn` answers each typed query with rows. */\n    onQuery(fn: (query: string) => {rows} | Promise<{rows}>): void;\n    /** Runs when another option of the filter dropdown is picked; return the rows or call `setRows`. */\n    onFilter(fn: (index: number) => {rows} | void | Promise<{rows} | void>): void;\n    /** Frees the page's handlers and rows. */\n    close(): void;\n  }}"
    )
  }
}

impl Kind for FormKind {
  const LIST: bool = false;
  fn ts(_: &mut Types) -> String {
    "{ id: number; /** Frees the page's handlers. */ close(): void }".into()
  }
}

impl Kind for DetailKind {
  const LIST: bool = false;
  fn ts(_: &mut Types) -> String {
    "{ id: number; /** Frees the page's handlers. */ close(): void }".into()
  }
}

/// What a plugin holds for its page
struct Handle<K> {
  module: Data<Module>,
  id: u32,
  _kind: PhantomData<K>,
}

impl<K: Kind> Type for Handle<K> {
  fn definition(types: &mut Types) -> DataType {
    DataType::Reference(define(K::ts(types)))
  }
}

impl<'js, K: Kind> IntoJs<'js> for Handle<K> {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    handle(ctx, &self.module, self.id, K::LIST).map(Object::into_value)
  }
}

/// The object a plugin holds for its page. Its methods are not enumerable, so the page
/// stays `{ id }` in a reply.
fn handle<'js>(ctx: &Ctx<'js>, module: &Data<Module>, id: u32, list: bool) -> Result<Object<'js>> {
  let handle = Object::new(ctx.clone())?;
  handle.set("id", id)?;
  let method = |name: &str, f: Function<'js>| handle.prop(name, Property::from(f));

  let m = module.clone();
  method(
    "close",
    Function::new(ctx.clone(), move || m.port.close_page(id))?,
  )?;
  if !list {
    return Ok(handle);
  }

  let m = module.clone();
  method(
    "setRows",
    Function::new(ctx.clone(), move |rows: Spec<Vec<Row>>| {
      if let Some(page) = m.port.page(id) {
        page.set_rows(rows.value, rows.handlers, None, &mut Cx);
      } else {
        m.port.release(&rows.handlers);
      }
    })?,
  )?;

  let m = module.clone();
  method(
    "setLoading",
    Function::new(ctx.clone(), move |loading: bool| {
      if let Some(page) = m.port.page(id) {
        page.set_loading(loading, &mut Cx);
      }
    })?,
  )?;

  let m = module.clone();
  method(
    "onQuery",
    Function::new(ctx.clone(), move |f: Handler<String, Vec<Row>>| {
      match m.port.page(id) {
        Some(page) => page.set_on_query(f.id),
        None => m.watches.release_handler(f.id),
      }
    })?,
  )?;

  let m = module.clone();
  method(
    "onFilter",
    Function::new(ctx.clone(), move |f: Handler<u32, Vec<Row>>| {
      match m.port.page(id) {
        Some(page) => page.set_on_filter(f.id),
        None => m.watches.release_handler(f.id),
      }
    })?,
  )?;
  Ok(handle)
}

#[cfg(test)]
mod tests {
  use gpui_kit::{self as gpui, TestAppContext};
  use serde_json::{Value, json};

  use crate::{launcher::Port, module::harness};

  /// Runs `body` once in a view of the module over `port`
  fn run(cx: &mut TestAppContext, port: &Port, body: &str) -> harness::View {
    let body = format!("if (!globalThis.started) {{ globalThis.started = true; {body} }}");
    let port = port.clone();
    let (view, cx) = harness::view(cx, "corona/launcher", &body, move |plugin, m, _| {
      super::build(port, plugin.dir.clone(), m)
    });
    cx.run_until_parked();
    view
  }

  fn ask(cx: &mut TestAppContext, port: &Port, handler: u32, args: Value) -> anyhow::Result<Value> {
    let answer = cx.update(|cx| port.request(handler, args, cx)).unwrap();
    cx.run_until_parked();
    futures::executor::block_on(answer)
  }

  fn command(
    cx: &mut TestAppContext,
    port: &Port,
    name: &str,
    args: Value,
  ) -> anyhow::Result<Value> {
    let handler = port.command(name).expect("command not registered");
    ask(cx, port, handler, args)
  }

  /// The declarations plugins get
  fn declarations() -> String {
    let watches = corona_js::Watches::default();
    let mut m = corona_js::ModuleBuilder::new("corona/launcher", &watches);
    super::build(Port::new("p"), "/p".into(), &mut m);
    corona_js::d_ts([&m])
  }

  #[test]
  fn the_declarations_are_generated_from_the_types() {
    let dts = declarations();
    for expected in [
      "declare module \"corona/launcher\"",
      "export function list(spec: ListSpec): {",
      "export function form(spec: FormSpec):",
      "export function detail(spec: DetailSpec):",
      "export function onRun(name: string, handler: ((arg: (Arg | null)[]) => Reply | void | Promise<Reply | void>)): void;",
      // a function in a spec is typed as one
      "run: (() => Reply | void | Promise<Reply | void>),",
      "onSubmit: ((arg: { [key in string]: string }) =>",
      // an optional handler is optional, not part of the return type
      "onCancel?: (() => Reply | void | Promise<Reply | void>) | null,",
      "export type Row = {",
      "metadata?: ([string, string])[],",
      "setRows(rows: Row[]): void;",
      // docs become JSDoc
      "A page of rows, searched by the search bar.",
    ] {
      assert!(dts.contains(expected), "missing {expected:?} in\n{dts}");
    }
  }

  #[gpui::test]
  fn a_reply_with_two_keys_is_an_error(cx: &mut TestAppContext) {
    let port = Port::new("p");
    let _view = run(
      cx,
      &port,
      r#"m.onRun("both", () => ({ pop: true, keepOpen: true }));"#,
    );
    let answer = command(cx, &port, "both", json!([])).unwrap();
    let outcome = crate::launcher::outcome(Ok(answer), &port);
    let corona_launcher::Outcome::Toast(toast) = outcome else {
      panic!("a reply with two keys must fail")
    };
    assert_eq!(toast.kind, corona_launcher::ToastKind::Failure);
  }

  #[gpui::test]
  fn a_command_handler_answers_with_a_hud(cx: &mut TestAppContext) {
    let port = Port::new("p");
    let _view = run(
      cx,
      &port,
      r#"m.onRun("go", async (args) => ({ hud: { text: "hi " + args[0] } }));"#,
    );
    let answer = command(cx, &port, "go", json!(["x"])).unwrap();
    assert_eq!(answer, json!({ "hud": { "text": "hi x" } }));
  }

  #[gpui::test]
  fn a_handler_that_throws_fails_the_request(cx: &mut TestAppContext) {
    let port = Port::new("p");
    let _view = run(
      cx,
      &port,
      r#"m.onRun("go", () => { throw new Error("no"); });"#,
    );
    let error = command(cx, &port, "go", json!([])).unwrap_err();
    assert!(error.to_string().contains("no"), "{error}");
    // a handler that was never registered
    assert!(port.command("missing").is_none());
  }

  #[gpui::test]
  fn a_pushed_page_holds_rows_and_runs_their_actions(cx: &mut TestAppContext) {
    let port = Port::new("p");
    let _view = run(
      cx,
      &port,
      r#"m.onRun("recent", () => ({ push: m.list({
        title: "Recent",
        rows: [{ id: "a", title: "A", actions: [{ title: "Open", run: () => ({ toast: { title: "opened" } }) }] }],
      }) }));"#,
    );
    let answer = command(cx, &port, "recent", json!([])).unwrap();
    // the page stays `{ id }`: its methods are not part of the reply
    let page = answer["push"]["id"].as_u64().unwrap() as u32;
    assert_eq!(answer, json!({ "push": { "id": page } }));

    let page = port.page(page).unwrap();
    let rows = page.rows();
    assert_eq!((rows.len(), rows[0].title.as_str()), (1, "A"));
    let answer = ask(cx, &port, rows[0].actions[0].run.id, json!({})).unwrap();
    assert_eq!(answer, json!({ "toast": { "title": "opened" } }));
  }

  #[gpui::test]
  fn set_rows_and_close_update_the_page(cx: &mut TestAppContext) {
    let port = Port::new("p");
    let _view = run(
      cx,
      &port,
      r#"globalThis.page = m.list({ title: "T" });
      m.onRun("fill", () => { globalThis.page.setRows([{ id: "b", title: "B", actions: [{ title: "x", run: () => {} }] }]); });
      m.onRun("close", () => { globalThis.page.close(); });"#,
    );
    let page = port.page(1).unwrap();
    assert!(page.rows().is_empty());
    command(cx, &port, "fill", json!([])).unwrap();
    assert_eq!(page.rows()[0].id, "b");
    // the row's handler is there until the rows change or the page closes
    let run = page.rows()[0].actions[0].run.id;
    assert!(ask(cx, &port, run, json!({})).is_ok());
    command(cx, &port, "close", json!([])).unwrap();
    assert!(page.rows().is_empty() && port.page(1).is_none());
    assert!(ask(cx, &port, run, json!({})).is_err());
  }

  #[gpui::test]
  fn new_rows_free_the_old_handlers(cx: &mut TestAppContext) {
    let port = Port::new("p");
    let _view = run(
      cx,
      &port,
      r#"globalThis.page = m.list({ title: "T", rows: [{ id: "a", title: "A", actions: [{ title: "x", run: () => 1 }] }] });
      m.onRun("swap", () => { globalThis.page.setRows([]); });"#,
    );
    let page = port.page(1).unwrap();
    let old = page.rows()[0].actions[0].run.id;
    assert!(ask(cx, &port, old, json!({})).is_ok());
    command(cx, &port, "swap", json!([])).unwrap();
    assert!(ask(cx, &port, old, json!({})).is_err());
  }

  #[gpui::test]
  fn on_query_asks_the_plugin_and_drops_stale_rows(cx: &mut TestAppContext) {
    let port = Port::new("p");
    let _view = run(
      cx,
      &port,
      r#"const page = m.list({ title: "T" });
      page.onQuery(async (q) => [{ id: q, title: q, actions: [] }]);"#,
    );
    let page = port.page(1).unwrap();
    assert!(page.filters());
    page.sync("ab");
    cx.run_until_parked();
    assert_eq!(page.rows()[0].id, "ab");
    // rows for a query the page has since moved on from are dropped
    page.sync("abc");
    cx.update(|cx| page.set_rows(vec![Default::default()], vec![], Some("ab"), cx));
    assert_eq!(page.rows()[0].id, "ab");
  }

  #[gpui::test]
  fn a_bad_spec_throws_into_the_plugin(cx: &mut TestAppContext) {
    let port = Port::new("p");
    let _view = run(
      cx,
      &port,
      r#"let message;
      try { m.list({ title: "T", rows: [{ id: "a", title: "A", actions: [{ title: "no run" }] }] }); }
      catch (e) { message = e.message; }
      m.onRun("probe", () => ({ toast: { title: String(message) } }));"#,
    );
    let answer = command(cx, &port, "probe", json!([])).unwrap();
    assert!(
      answer["toast"]["title"].as_str().unwrap().contains("run"),
      "{answer}"
    );
    assert!(port.page(1).is_none());
  }
}

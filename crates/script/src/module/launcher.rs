//! `corona/launcher`: answers the plugin's `[commands]` and builds the pages they push.
//!
//! The plugin's functions stay in its runtime's [`Watches`] as handlers; the launcher calls
//! them by id and gets JSON back.

use std::{cell::Cell, cell::RefCell, path::PathBuf, rc::Rc};

use corona_js::{
  Cx, ModuleBuilder, PluginRef, ScriptModule, Watches,
  rquickjs::{Ctx, Exception, Function, Object, Result, Value, object::Property},
};
use gpui_kit::{App, Subscription};

use crate::{
  ScriptManager,
  launcher::{Answer, Call, Done, PageSpec, Port},
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
  let module = Rc::new(Module {
    port,
    watches: m.watches().clone(),
    dir,
    started: Cell::new(false),
  });
  m.declarations(include_str!("launcher.d.ts"));
  let this = module.clone();
  m.value("onRun", move |ctx| on_run(ctx, &this));
  for kind in ["list", "form", "detail"] {
    let this = module.clone();
    m.value(kind, move |ctx| page_fn(ctx, &this, kind));
  }
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
    let sink = Rc::default();
    let json = encode(ctx, watches, value, &sink, false).map_err(|e| e.to_string())?;
    Ok(Answer {
      json,
      handlers: sink.take(),
    })
  });
  done(answer, cx);
}

/// `value` as JSON. JSON has no functions: each one is registered as a handler and written
/// as its id, and the id is added to `sink`. `rows` is left out when `drop_rows`, to be
/// encoded on its own.
fn encode<'js>(
  ctx: &Ctx<'js>,
  watches: &Watches,
  value: Value<'js>,
  sink: &Rc<RefCell<Vec<u32>>>,
  drop_rows: bool,
) -> Result<String> {
  let (watches, sink) = (watches.clone(), sink.clone());
  let replacer = Function::new(
    ctx.clone(),
    move |ctx: Ctx<'js>, key: String, value: Value<'js>| -> Result<Value<'js>> {
      if drop_rows && key == "rows" {
        return Ok(Value::new_undefined(ctx));
      }
      let Some(f) = value.as_function() else {
        return Ok(value);
      };
      let id = watches.register_handler(f.clone());
      sink.borrow_mut().push(id);
      Ok(Value::new_int(ctx, id as i32))
    },
  )?;
  let stringify: Function = ctx.globals().get::<_, Object>("JSON")?.get("stringify")?;
  let text: Option<String> = stringify.call((value, replacer))?;
  Ok(text.unwrap_or_else(|| "null".into()))
}

fn throw(ctx: &Ctx<'_>, error: impl std::fmt::Display) -> corona_js::rquickjs::Error {
  Exception::throw_message(ctx, &error.to_string())
}

/// `onRun(name, handler)`
fn on_run<'js>(ctx: &Ctx<'js>, module: &Rc<Module>) -> Result<Value<'js>> {
  let module = module.clone();
  let f = Function::new(ctx.clone(), move |name: String, handler: Function<'js>| {
    module.start();
    let id = module.watches.register_handler(handler);
    if let Some(old) = module.port.register_command(name, id) {
      module.watches.release_handler(old);
    }
  })?;
  Ok(f.into_value())
}

/// `list(spec)`, `form(spec)` or `detail(spec)`
fn page_fn<'js>(ctx: &Ctx<'js>, module: &Rc<Module>, kind: &'static str) -> Result<Value<'js>> {
  let module = module.clone();
  let f = Function::new(
    ctx.clone(),
    move |ctx: Ctx<'js>, spec: Object<'js>| -> Result<Object<'js>> {
      module.start();
      let rows: Value = spec.get("rows")?;
      let (spec_sink, row_sink) = (Rc::default(), Rc::default());
      let open = || -> std::result::Result<u32, String> {
        let json = encode(
          &ctx,
          &module.watches,
          spec.clone().into_value(),
          &spec_sink,
          true,
        )
        .map_err(|e| e.to_string())?;
        let mut spec: PageSpec = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        spec.kind = kind.into();
        if !rows.is_undefined() {
          let json = encode(&ctx, &module.watches, rows.clone(), &row_sink, false)
            .map_err(|e| e.to_string())?;
          spec.rows = serde_json::from_str(&json).map_err(|e| e.to_string())?;
        }
        Ok(
          module
            .port
            .open_page(spec, spec_sink.take(), row_sink.take()),
        )
      };
      match open() {
        Ok(id) => handle(&ctx, &module, id),
        Err(e) => {
          module.port.release(&spec_sink.take());
          module.port.release(&row_sink.take());
          Err(throw(&ctx, e))
        }
      }
    },
  )?;
  Ok(f.into_value())
}

/// The object a plugin holds for its page. Its methods are not enumerable, so the page
/// stays `{ id }` in a reply.
fn handle<'js>(ctx: &Ctx<'js>, module: &Rc<Module>, id: u32) -> Result<Object<'js>> {
  let handle = Object::new(ctx.clone())?;
  handle.set("id", id)?;
  let method = |name: &str, f: Function<'js>| handle.prop(name, Property::from(f));

  let m = module.clone();
  method(
    "setRows",
    Function::new(
      ctx.clone(),
      move |ctx: Ctx<'js>, rows: Value<'js>| -> Result<()> {
        let Some(page) = m.port.page(id) else {
          return Ok(());
        };
        let sink = Rc::default();
        let json = encode(&ctx, &m.watches, rows, &sink, false)?;
        match serde_json::from_str(&json) {
          Ok(rows) => page.set_rows(rows, sink.take(), None, &mut Cx),
          Err(e) => {
            m.port.release(&sink.take());
            return Err(throw(&ctx, e));
          }
        }
        Ok(())
      },
    )?,
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
    Function::new(ctx.clone(), move |f: Function<'js>| {
      let handler = m.watches.register_handler(f);
      match m.port.page(id) {
        Some(page) => page.set_on_query(handler),
        None => m.watches.release_handler(handler),
      }
    })?,
  )?;

  let m = module.clone();
  method(
    "onFilter",
    Function::new(ctx.clone(), move |f: Function<'js>| {
      let handler = m.watches.register_handler(f);
      match m.port.page(id) {
        Some(page) => page.set_on_filter(handler),
        None => m.watches.release_handler(handler),
      }
    })?,
  )?;

  let m = module.clone();
  method(
    "close",
    Function::new(ctx.clone(), move || m.port.close_page(id))?,
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
    let answer = ask(cx, &port, rows[0].actions[0].run, json!({})).unwrap();
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
    let run = page.rows()[0].actions[0].run;
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
    let old = page.rows()[0].actions[0].run;
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

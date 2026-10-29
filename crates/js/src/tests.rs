use std::{
  io,
  sync::{Arc, Mutex},
  time::Duration,
};

use gpui_kit::{
  self as gpui, App, AppContext, Entity, Global, Modifiers, TestAppContext, VisualTestContext,
  point, px,
};
use rquickjs::{
  AsyncContext, AsyncRuntime, IntoJs, Module,
  loader::{BuiltinResolver, ModuleLoader},
};
use specta::{Type, specta};

use crate::{
  Cx, Glob, GpuiKit, ModuleBuilder, Opt, PluginRef, ScriptExt, ScriptModule, ScriptRegistry,
  ScriptView, Throws, Watches, d_ts,
  module::{HostLoader, HostModules},
  now, with,
};

mod components_data;
mod components_forms;
mod components_navigation;
mod components_overlays;
mod elements;

pub(crate) use elements::FAMILY;

#[derive(Default)]
struct Counter(u32);

impl Global for Counter {}

/// Adds `by` to the shared counter. Throws when it would pass 100.
#[specta]
fn add(mut cx: Cx, by: u32) -> Throws<u32> {
  let counter = &mut cx.default_global::<Counter>().0;
  let next = *counter + by;
  if next > 100 {
    return Err::<u32, _>("counter past 100").into();
  }
  *counter = next;
  Ok::<_, &str>(next).into()
}

/// Resolves with `value` doubled after `ms` milliseconds.
#[specta]
async fn double_later(cx: Cx, ms: u32, value: f64) -> f64 {
  let timer = cx
    .background_executor()
    .timer(Duration::from_millis(ms.into()));
  timer.await;
  value * 2.0
}

/// Rejects when `fail`.
#[specta]
async fn check(fail: bool) -> Throws<u32> {
  match fail {
    true => Err("check failed"),
    false => Ok(1),
  }
  .into()
}

/// A corner of a shape.
#[derive(Type, IntoJs)]
struct Point {
  /// Pixels from the left.
  x: f64,
  y: f32,
}

#[derive(Type, IntoJs)]
struct Shape {
  name: String,
  corners: Vec<Point>,
  scale: Option<f64>,
}

/// A square named `name`, `scale` times the counter wide.
#[specta]
fn square(counter: Glob<Counter>, name: String, scale: Opt<f64>) -> Shape {
  let size = counter.0 as f64 * scale.0.unwrap_or(1.);
  let corners = [(0., 0.), (size, 0.), (size, size), (0., size)];
  Shape {
    name,
    corners: corners.map(|(x, y)| Point { x, y: y as f32 }).into(),
    scale: scale.0,
  }
}

/// Only in another variant of the module.
#[specta]
fn hidden() -> u32 {
  0
}

/// What the `count` signal watches, and an entity no script reads
struct Models {
  count: Entity<u32>,
  other: Entity<u32>,
}

impl Global for Models {}

struct Test;

impl ScriptModule for Test {
  const NAME: &str = "test";
  const DOC: &str = "Host fns for tests.";

  fn module(_: &PluginRef, m: &mut ModuleBuilder, _: &mut App) {
    crate::host_module!(m, {
      sync: [add, square],
      async: [double_later, check],
      declare: [hidden],
    });
    m.watch(
      "count",
      "The watched count.",
      |cx| cx.global::<Models>().count.clone(),
      |cx| *cx.global::<Models>().count.read(cx),
    );
  }
}

/// The modules a test plugin is granted
fn modules(watches: &Watches, cx: &mut App) -> Vec<ModuleBuilder> {
  cx.script_mut().register::<Test>();
  let plugin = PluginRef {
    id: "test".into(),
    name: "Test".into(),
    ..Default::default()
  };
  let test = cx.global::<ScriptRegistry>().get(Test::NAME).unwrap();
  vec![test.build(&plugin, watches, cx)]
}

const VIEW: &str = r#"
import { View, div } from "gpui-kit";
import { add, doubleLater, square } from "corona/test";

export default class Main extends View {
  init(_props, cx) {
    this.count = 0;
    this.answer = "pending";
    try {
      add(1000);
    } catch (error) {
      this.error = error.message;
    }
    doubleLater(10, 21).then((answer) => {
      this.answer = "answer:" + answer;
      cx.notify();
    });
  }

  render(_cx) {
    const corner = square("s", 2).corners[2];
    return div().size_full().flex().gap(4).bg(0x202020)
      .child(`count:${this.count}|${this.answer}|${this.error}|${corner.x}`)
      .on_click((_event, cx) => {
        this.count = add(1);
        cx.notify();
      });
  }
}
"#;

/// Counts its renders, shows `count` and what its subscription saw until it stopped at 2
const WATCH: &str = r#"
import { View, div } from "gpui-kit";
import { count } from "corona/test";

let renders = 0;

export default class Main extends View {
  init(_props, _cx) {
    this.seen = [];
    this.stop = count.subscribe((value) => {
      this.seen.push(value);
      if (value >= 2) this.stop();
    });
  }

  render(_cx) {
    renders++;
    return div().child(`renders:${renders}|count:${count()}|seen:${this.seen.join(",")}`);
  }
}
"#;

fn view<'a>(
  cx: &'a mut TestAppContext,
  source: &'static str,
) -> (Entity<ScriptView>, &'a mut VisualTestContext) {
  cx.update(|cx| {
    let models = Models {
      count: cx.new(|_| 0),
      other: cx.new(|_| 0),
    };
    cx.set_global(models);
  });
  let (view, cx) = cx.add_window_view(|window, cx| {
    let watches = Watches::default();
    let modules = modules(&watches, cx);
    ScriptView::new(source, modules, watches, window, cx).unwrap()
  });
  cx.run_until_parked();
  (view, cx)
}

/// `source` mounted in a gpui-component `Root`, which hosts dialogs, sheets and toasts
fn rooted<'a>(
  cx: &'a mut TestAppContext,
  source: &'static str,
) -> (Entity<ScriptView>, &'a mut VisualTestContext) {
  cx.update(gpui::init);
  let (root, cx) = cx.add_window_view(|window, cx| {
    let watches = Watches::default();
    let modules = modules(&watches, cx);
    let view = cx.new(|cx| ScriptView::new(source, modules, watches, window, cx).unwrap());
    let content = cx.new(|_| crate::ScriptRoot::new(view));
    gpui::base::Root::new(content, window, cx)
  });
  cx.run_until_parked();
  let content = root.read_with(cx, |root, _| root.view().clone());
  let content = content.downcast::<crate::ScriptRoot>().ok().unwrap();
  let view = content.read_with(cx, |root, _| root.view().clone());
  (view, cx)
}

/// What `code` evaluates to in the view's context, as a string
fn eval(view: &Entity<ScriptView>, cx: &mut VisualTestContext, code: &str) -> String {
  view.read_with(cx, |view, _| view.eval(code))
}

fn text(view: &Entity<ScriptView>, cx: &mut VisualTestContext) -> String {
  view.read_with(cx, |view, _| view.text.clone())
}

/// Sets the model `pick` returns and notifies its observers.
fn set(cx: &mut VisualTestContext, pick: fn(&Models) -> &Entity<u32>, value: u32) {
  cx.update(|_, cx| {
    let entity = pick(cx.global::<Models>()).clone();
    entity.update(cx, |n, cx| {
      *n = value;
      cx.notify();
    });
  });
  cx.run_until_parked();
}

#[gpui::test]
fn async_host_fn_resolves(cx: &mut TestAppContext) {
  let (view, cx) = view(cx, VIEW);
  assert_eq!(text(&view, cx), "count:0|pending|counter past 100|0");

  cx.executor().advance_clock(Duration::from_millis(10));
  cx.run_until_parked();
  assert_eq!(text(&view, cx), "count:0|answer:42|counter past 100|0");
}

#[gpui::test]
fn click_notifies_and_rerenders(cx: &mut TestAppContext) {
  let (view, cx) = view(cx, VIEW);
  cx.simulate_click(point(px(100.), px(100.)), Modifiers::none());
  cx.run_until_parked();
  assert_eq!(text(&view, cx), "count:1|pending|counter past 100|2");

  cx.simulate_click(point(px(100.), px(100.)), Modifiers::none());
  cx.run_until_parked();
  assert_eq!(text(&view, cx), "count:2|pending|counter past 100|4");
}

#[gpui::test]
fn rerenders_only_on_read_entities(cx: &mut TestAppContext) {
  let (view, cx) = view(cx, WATCH);
  assert_eq!(text(&view, cx), "renders:1|count:0|seen:");

  set(cx, |m| &m.other, 5);
  assert_eq!(text(&view, cx), "renders:1|count:0|seen:");

  set(cx, |m| &m.count, 1);
  assert_eq!(text(&view, cx), "renders:2|count:1|seen:1");
}

#[gpui::test]
fn subscription_stops_after_unsubscribe(cx: &mut TestAppContext) {
  let (view, cx) = view(cx, WATCH);
  for value in 1..=3 {
    set(cx, |m| &m.count, value);
  }
  assert_eq!(text(&view, cx), "renders:4|count:3|seen:1,2");
}

/// Collects what is logged
#[derive(Clone, Default)]
struct Logs(Arc<Mutex<Vec<u8>>>);

impl io::Write for Logs {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    self.0.lock().unwrap().extend_from_slice(buf);
    Ok(buf.len())
  }

  fn flush(&mut self) -> io::Result<()> {
    Ok(())
  }
}

#[gpui::test]
fn unhandled_rejection_is_logged(cx: &mut TestAppContext) {
  let logs = Logs::default();
  let writer = logs.clone();
  let subscriber = tracing_subscriber::fmt()
    .with_ansi(false)
    .with_writer(move || writer.clone())
    .finish();
  let _guard = tracing::subscriber::set_default(subscriber);

  const SOURCE: &str = r#"
import { View, div } from "gpui-kit";
import { check } from "corona/test";

export default class Main extends View {
  init(_props, _cx) {
    check(false).then(() => {});
    check(true).catch(() => {});
    check(true).then(() => {});
  }

  render(_cx) {
    return div();
  }
}
"#;
  let (_view, _cx) = view(cx, SOURCE);
  let logs = String::from_utf8(logs.0.lock().unwrap().clone()).unwrap();
  assert_eq!(
    logs.matches("unhandled promise rejection").count(),
    1,
    "{logs}"
  );
  assert!(logs.contains("check failed"), "{logs}");
}

#[gpui::test]
fn registry_finds_modules_by_name(cx: &mut TestAppContext) {
  cx.update(|cx| {
    cx.script_mut().register::<Test>();
    let registry = cx.global::<ScriptRegistry>();
    let names: Vec<_> = registry.all().map(|m| (m.name, m.doc)).collect();
    assert_eq!(names, [("test", "Host fns for tests.")]);
    assert!(registry.get("test").is_some());
    assert!(registry.get("missing").is_none());
  });
}

#[gpui::test]
fn d_ts_unchanged(cx: &mut TestAppContext) {
  let d_ts = cx.update(|cx| GpuiKit::d_ts() + &d_ts(&modules(&Watches::default(), cx)));
  if std::env::var_os("CORONA_REGEN_DTS").is_some() {
    std::fs::write(
      concat!(env!("CARGO_MANIFEST_DIR"), "/src/tests.d.ts"),
      &d_ts,
    )
    .unwrap();
  }
  assert_eq!(
    d_ts,
    include_str!("tests.d.ts"),
    "typings changed, regenerate with CORONA_REGEN_DTS=1"
  );
}

#[test]
#[should_panic(expected = "host fn called outside init/render/dispatch")]
fn host_fn_outside_enter_panics() {
  let mut m = ModuleBuilder::new("corona/test", &Watches::default());
  crate::host_module!(&mut m, { sync: [add] });
  let rt = AsyncRuntime::new().unwrap();
  let resolver = BuiltinResolver::default().with_module("corona/test");
  now(rt.set_loader(resolver, (HostLoader, ModuleLoader::default())));
  let ctx = now(AsyncContext::full(&rt)).unwrap();
  with(&ctx, |ctx| {
    let _ = ctx.store_userdata(HostModules::new(vec![m]));
    // a panic in module top-level code only rejects the module promise, so call from a script
    let source = r#"import { add } from "corona/test"; globalThis.add = add;"#;
    Module::evaluate(ctx.clone(), "probe", source)
      .unwrap()
      .finish::<()>()
      .unwrap();
    let _ = ctx.eval::<(), _>("add(1)");
  });
}

#[gpui::test]
fn closing_frees_handles_before_runtime(cx: &mut TestAppContext) {
  let (view, cx) = view(cx, WATCH);
  let weak = view.downgrade();
  drop(view);
  cx.update(|window, _| window.remove_window());
  cx.run_until_parked();
  assert!(weak.upgrade().is_none());
}

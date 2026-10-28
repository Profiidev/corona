use std::time::Duration;

use gpui_kit::{
  self as gpui, Entity, Global, Modifiers, TestAppContext, VisualTestContext, point, px,
};
use rquickjs::{
  AsyncContext, AsyncRuntime, Module,
  loader::{BuiltinResolver, ModuleLoader},
};
use specta::specta;

use crate::{Cx, GpuiKit, ScriptView, Throws, now, with};

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

crate::host_module!(TestApi = "corona/test" { sync: [add], async: [double_later] });

const VIEW: &str = r#"
import { View, div } from "gpui-kit";
import { add, double_later } from "corona/test";

export default class Main extends View {
  init(_props, cx) {
    this.count = 0;
    this.answer = "pending";
    try {
      add(1000);
    } catch (error) {
      this.error = error.message;
    }
    double_later(10, 21).then((answer) => {
      this.answer = "answer:" + answer;
      cx.notify();
    });
  }

  render(_cx) {
    return div().size_full().flex().gap(4).bg(0x202020)
      .child(`count:${this.count}|${this.answer}|${this.error}`)
      .on_click((_event, cx) => {
        this.count = add(1);
        cx.notify();
      });
  }
}
"#;

fn view(cx: &mut TestAppContext) -> (Entity<ScriptView>, &mut VisualTestContext) {
  let (view, cx) = cx.add_window_view(|window, cx| {
    let (mut resolver, mut loader) = (BuiltinResolver::default(), ModuleLoader::default());
    TestApi::install(&mut resolver, &mut loader);
    ScriptView::new(VIEW, resolver, loader, window, cx).unwrap()
  });
  cx.run_until_parked();
  (view, cx)
}

fn text(view: &Entity<ScriptView>, cx: &mut VisualTestContext) -> String {
  view.read_with(cx, |view, _| view.text.clone())
}

#[gpui::test]
fn async_host_fn_resolves(cx: &mut TestAppContext) {
  let (view, cx) = view(cx);
  assert_eq!(text(&view, cx), "count:0|pending|counter past 100");

  cx.executor().advance_clock(Duration::from_millis(10));
  cx.run_until_parked();
  assert_eq!(text(&view, cx), "count:0|answer:42|counter past 100");
}

#[gpui::test]
fn click_notifies_and_rerenders(cx: &mut TestAppContext) {
  let (view, cx) = view(cx);
  cx.simulate_click(point(px(100.), px(100.)), Modifiers::none());
  cx.run_until_parked();
  assert_eq!(text(&view, cx), "count:1|pending|counter past 100");

  cx.simulate_click(point(px(100.), px(100.)), Modifiers::none());
  cx.run_until_parked();
  assert_eq!(text(&view, cx), "count:2|pending|counter past 100");
}

#[test]
fn d_ts_unchanged() {
  let d_ts = GpuiKit::d_ts() + &TestApi::d_ts();
  assert_eq!(
    d_ts,
    include_str!("tests.d.ts"),
    "typings changed, update src/tests.d.ts"
  );
}

#[test]
#[should_panic(expected = "host fn called outside init/render/dispatch")]
fn host_fn_outside_enter_panics() {
  let rt = AsyncRuntime::new().unwrap();
  let (mut resolver, mut loader) = (BuiltinResolver::default(), ModuleLoader::default());
  TestApi::install(&mut resolver, &mut loader);
  now(rt.set_loader(resolver, loader));
  let ctx = now(AsyncContext::full(&rt)).unwrap();
  with(&ctx, |ctx| {
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
  let (view, cx) = view(cx);
  let weak = view.downgrade();
  drop(view);
  cx.update(|window, _| window.remove_window());
  cx.run_until_parked();
  assert!(weak.upgrade().is_none());
}

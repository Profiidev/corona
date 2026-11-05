//! The element tree: style methods and values, events, child views, timers, theme, window,
//! retained states and the component extension point.

use std::time::Duration;

use gpui_kit::{
  self as gpui, AppContext, Div, Entity, EventEmitter, Modifiers, MouseButton, StyleRefinement,
  TestAppContext, VisualTestContext, div, point, px,
};
use rquickjs::{Class, Ctx, Function, JsLifetime, Result, Value, class::Trace};

use super::{modules, text};
use crate::{
  Node, Retained, ScriptRoot, ScriptView, Watches, app, constructor,
  element::{components::Family, style::NULLARY},
  new_el,
};

/// A test component: a div with a label, which only it takes
pub(crate) struct Probe {
  div: Div,
  label: String,
}

impl Node for Probe {
  fn kind(&self) -> &'static str {
    "Probe"
  }
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    Some(gpui::Styled::style(&mut self.div))
  }
  fn into_element(self: Box<Self>) -> gpui::AnyElement {
    use gpui::{IntoElement, ParentElement};
    self.div.child(self.label).into_any_element()
  }
  fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
    self
  }
}

mod probe {
  crate::el_methods! {
    /// Sets the probe's label.
    fn label(text: String) |c, n| crate::map(c, n, |mut p: super::Probe| {
      p.label = text;
      p
    });
  }
}

struct Emitter;

impl EventEmitter<u32> for Emitter {}

/// A retained test state emitting numbers
#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub(crate) struct TestState {
  #[qjs(skip_trace)]
  state: Retained<Emitter>,
}

#[rquickjs::methods]
impl TestState {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'_>) -> Result<Self> {
    let entity = app().new(|_| Emitter);
    Ok(Self {
      state: Retained::new(&ctx, entity)?,
    })
  }

  pub fn on<'js>(&mut self, ctx: Ctx<'js>, name: String, f: Function<'js>) -> Result<bool> {
    (self.state).on(&ctx, &["change"], name, f, |n: &u32, _| {
      Some(("change", *n))
    })
  }

  pub fn emit(&self, ctx: Ctx<'_>, n: u32) -> Result<()> {
    let entity = self.state.entity(&ctx)?.clone();
    entity.update(app(), |_, cx| cx.emit(n));
    Ok(())
  }

  pub fn release(&mut self) -> bool {
    self.state.release()
  }
}

fn exports<'js>(ctx: &Ctx<'js>) -> Result<Vec<(&'static str, Value<'js>)>> {
  let make = Function::new(ctx.clone(), |ctx: Ctx<'js>| {
    let probe = Probe {
      div: div(),
      label: String::new(),
    };
    new_el(&ctx, probe)
  })?;
  let state = Class::<TestState>::create_constructor(ctx)?.expect("constructor");
  Ok(vec![
    ("Probe", constructor(ctx, "Probe", make)?),
    ("TestState", state.into_value()),
  ])
}

pub(crate) const FAMILY: Family = Family {
  module: "corona/component",
  names: &["Probe", "TestState"],
  exports,
  methods: probe::install,
  d_ts: || "  export const Probe: { new (): Element; new: () => Element };\n".into(),
  el_d_ts: probe::d_ts,
};

fn open<'a>(
  cx: &'a mut TestAppContext,
  source: &'static str,
) -> (Entity<ScriptView>, &'a mut VisualTestContext) {
  super::view(cx, source)
}

fn eval(view: &Entity<ScriptView>, cx: &mut VisualTestContext, code: &str) -> String {
  view.read_with(cx, |view, _| view.eval(code))
}

const STYLES: &str = r##"
import { View, div, svg, image, ScrollHandle, Background, PathBuilder } from "corona/kit";
import { h_flex, v_flex } from "corona/base";
import { Probe } from "corona/component";

const attempt = (f) => {
  try {
    f();
    return "ok";
  } catch (e) {
    return `${e.name}: ${e.message}`;
  }
};

export default class Main extends View {
  render(_cx) {
    const results = [
      attempt(() => div().bgg()),
      attempt(() => div().w("10px").h("2rem").p("50%").m("auto").gap("md").rounded("lg")),
      attempt(() => div().text_size(12).line_height(1.5).font_weight(700).opacity(0.5)),
      attempt(() => div().p("auto")),
      attempt(() => div().bg("primay")),
      attempt(() => div().bg("#ff000")),
      attempt(() => div().bg("#ff0000").text_color("muted_foreground").border_color(0x112233)),
      attempt(() => div().w("12pt")),
      attempt(() => h_flex().items_center().font_bold().flex_grow().hover((s) => s.bg("accent"))),
      attempt(() => div().when(true, (el) => el.p_2()).map((el) => el.debug_red())),
      attempt(() => {
        const a = div();
        div().child(a);
        div().child(a);
      }),
      attempt(() => div().role("buton")),
      attempt(() => svg("x.svg").child(div())),
      attempt(() => image(new Uint8Array([1, 2]))),
      attempt(() => div().bg(Background.linear_gradient(90, "primary", "#00ff00").opacity(0.5))),
      attempt(() => PathBuilder.stroke(1).move_to(0, 0).line_to(4, 4).build()),
      attempt(() => PathBuilder.stroke(1).move_to(0, 0).line_to(Infinity, 0).dash_array([4, 4]).build()),
      attempt(() => PathBuilder.stroke(1).move_to(0, 0).line_to(1e9, 0).dash_array([0.5]).build()),
      attempt(() => PathBuilder.fill().move_to(0, 0).line_to(NaN, 0).build()),
      attempt(() => div().overflow_y_scroll().track_scroll(new ScrollHandle()).id("list")),
      attempt(() => new Probe().label("a").p_2()),
      attempt(() => Probe.new().label("b")),
      attempt(() => div().label("c")),
      attempt(() => div().on_mouse_down("middel", () => {})),
    ];
    return v_flex().children(results.map((r) => r + "\n"));
  }
}
"##;

#[gpui::test]
fn style_methods_and_values(cx: &mut TestAppContext) {
  let (view, cx) = open(cx, STYLES);
  let lines = [
    "TypeError: not a function",
    "ok",
    "ok",
    "TypeError: invalid length \"auto\", expected 4, \"4px\", \"1rem\", \"50%\" or a token",
    "TypeError: unknown color \"primay\", did you mean \"primary\"?",
    "TypeError: invalid color \"#ff000\", expected #rrggbb",
    "ok",
    "TypeError: invalid length \"12pt\", expected 4, \"4px\", \"1rem\", \"50%\", \"auto\" or a token",
    "ok",
    "ok",
    "TypeError: element was already used as a child",
    "TypeError: unknown role \"buton\", did you mean \"button\"?",
    "TypeError: svg(): relative image paths need a plugin directory",
    "TypeError: image(): not a PNG, JPEG, WebP, GIF, BMP, TIFF or ICO",
    "ok",
    "ok",
    "RangeError: PathBuilder: coordinates, radii and rotation must be finite",
    "RangeError: PathBuilder: path too long for its dash_array",
    "RangeError: PathBuilder: coordinates, radii and rotation must be finite",
    "ok",
    "ok",
    "ok",
    "TypeError: this method is not available on div",
    "TypeError: unknown mouse button \"middel\", did you mean \"middle\"?",
  ];
  let expected: String = lines.iter().map(|l| format!("{l}\n")).collect();
  assert_eq!(text(&view, cx), expected);
}

#[test]
fn hex_and_distance() {
  use crate::element::style::{hex, to_hex, unknown};
  assert_eq!(hex("#fff").map(to_hex).as_deref(), Some("#ffffff"));
  assert_eq!(hex("#11223380").map(to_hex).as_deref(), Some("#11223380"));
  assert_eq!(hex("112233"), None);
  assert_eq!(unknown("x", "zzzzzz", &["primary"]), "unknown x \"zzzzzz\"");
}

/// The no-argument style table in `element/style/generated.rs` is gpui's, regenerated with
/// `CORONA_REGEN_STYLES=1 cargo nextest run -p corona_js style_table`.
#[test]
fn style_table_matches_gpui() {
  let mut names: Vec<&str> = gpui::styled_reflection::methods::<StyleRefinement>()
    .into_iter()
    .chain(gpui::base::styled_ext_reflection_methods::<StyleRefinement>())
    .map(|m| m.name)
    // only in debug builds
    .filter(|name| !["debug", "debug_below"].contains(name))
    .collect();
  // gpui-base makes these with a macro, which reflection does not see
  names.extend([
    "font_thin",
    "font_extralight",
    "font_light",
    "font_normal",
    "font_medium",
    "font_semibold",
    "font_bold",
    "font_extrabold",
    "font_black",
  ]);
  let file: String = names.iter().map(|n| format!("  {n}\n")).collect();
  let file = format!(
    "// Generated by `style_table_matches_gpui` (CORONA_REGEN_STYLES=1): do not edit.\nnullary! {{\n{file}}}\n"
  );
  if std::env::var_os("CORONA_REGEN_STYLES").is_some() {
    std::fs::write(
      concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/element/style/generated.rs"
      ),
      &file,
    )
    .unwrap();
  }
  assert_eq!(
    file,
    include_str!("../element/style/generated.rs"),
    "gpui's style methods changed"
  );
  // a component method of a style's name takes no argument it needs, so it still types as one
  let el = crate::element::el_d_ts();
  for (name, _) in NULLARY {
    for line in el
      .lines()
      .filter(|l| l.starts_with(&format!("    {name}(")))
    {
      let args = &line[name.len() + 5..];
      // `this: Element<..>` is no argument
      let args = args.strip_prefix("this: ").map_or(args, |a| {
        a.split_once('>')
          .map_or("", |(_, r)| r.trim_start_matches(", "))
      });
      let first = args.split([',', ')']).next().unwrap_or_default();
      assert!(
        first.is_empty() || first.contains("?:"),
        "{name} takes an argument: {line}"
      );
    }
  }
}

const EVENTS: &str = r##"
import { View, div } from "corona/kit";

export default class Main extends View {
  init(_props, cx) {
    this.log = [];
    this.focus = cx.focus_handle();
    this.bound = cx.bind_keys([{ keystroke: "ctrl-s", action: "save" }]);
    try {
      cx.bind_keys([{ keystroke: "ctrl-x", action: "other::save" }]);
    } catch (e) {
      globalThis.spoof = e instanceof TypeError;
    }
  }

  render(_cx) {
    return div()
      .size_full()
      .track_focus(this.focus)
      .on_click((e, cx) => {
        this.log.push(`click:${e.click_count}:${e.modifiers.shift}`);
        this.focus.focus();
        cx.notify();
      })
      .on_mouse_down("right", (e, cx) => {
        this.log.push(`down:${e.button}:${e.position.x}`);
        cx.notify();
      })
      .on_key_down((e, cx) => {
        this.log.push(`key:${e.keystroke}:${e.key}:${e.modifiers.control}`);
        cx.notify();
      })
      .on_action("save", (e, cx) => {
        this.log.push(`action:${e.action}:${cx.phase()}`);
        cx.notify();
      })
      .child(`${this.bound}|${this.focus.is_focused()}|${this.log.join(",")}`);
  }
}
"##;

#[gpui::test]
fn events_reach_handlers_with_payloads(cx: &mut TestAppContext) {
  // rooted: a binding without a `context` fires inside the plugin's window only
  let (view, cx) = rooted(cx, EVENTS);
  assert_eq!(text(&view, cx), "1|false|");
  assert_eq!(eval(&view, cx, "String(globalThis.spoof)"), "true");

  cx.simulate_click(point(px(10.), px(10.)), Modifiers::shift());
  cx.run_until_parked();
  assert_eq!(text(&view, cx), "1|true|click:1:true");

  cx.simulate_mouse_down(
    point(px(12.), px(10.)),
    MouseButton::Right,
    Modifiers::none(),
  );
  cx.run_until_parked();
  assert_eq!(text(&view, cx), "1|true|click:1:true,down:right:12");

  // a bound chord dispatches its action instead of a key event
  cx.simulate_keystrokes("ctrl-a ctrl-s");
  cx.run_until_parked();
  assert_eq!(
    text(&view, cx),
    "1|true|click:1:true,down:right:12,key:ctrl-a:a:true,action:save:event"
  );
}

const CHILDREN: &str = r##"
import { View, div } from "corona/kit";

globalThis.log = [];

class Child extends View {
  init(props, _cx) {
    this.n = props.n;
  }

  update(props) {
    this.n = props.n;
  }

  render(_cx) {
    log.push(`child:${this.n}`);
    return `child:${this.n}`;
  }
}

export default class Main extends View {
  init(_props, cx) {
    this.child = cx.new(Child, { n: 1 });
    cx.timer.after(5, () => this.child.set_props({ n: 2 }));
  }

  render(cx) {
    log.push("parent");
    let refused = "";
    try {
      cx.new(Child);
    } catch (e) {
      refused = e.message;
    }
    return div().size_full().child(`parent|${refused}`).child(this.child);
  }
}
"##;

const REENTRANT: &str = r##"
import { View, div } from "corona/kit";

class Child extends View {
  init(props, _cx) {
    this.props = props;
  }

  update(props) {
    this.props = props;
    globalThis.updated = props.n;
  }

  render(_cx) {
    return div().size_full().on_click(() => this.props.tap()).child(`child:${this.props.n}`);
  }
}

export default class Main extends View {
  init(_props, cx) {
    const tap = () => this.child.set_props({ n: 2, tap });
    this.child = cx.new(Child, { n: 1, tap });
  }

  render(_cx) {
    return div().size_full().child(this.child);
  }
}
"##;

/// `view` mounted cached in a [`ScriptRoot`]
fn rooted<'a>(
  cx: &'a mut TestAppContext,
  source: &'static str,
) -> (Entity<ScriptView>, &'a mut VisualTestContext) {
  let (root, cx) = cx.add_window_view(|window, cx| {
    let watches = Watches::default();
    let modules = modules(&watches, cx);
    let view = cx.new(|cx| ScriptView::new(source, modules, watches, window, cx).unwrap());
    ScriptRoot::new(view)
  });
  cx.run_until_parked();
  let view = root.read_with(cx, |root, _| root.view().clone());
  (view, cx)
}

#[gpui::test]
fn child_views_update_alone(cx: &mut TestAppContext) {
  let (view, cx) = rooted(cx, CHILDREN);
  assert_eq!(
    text(&view, cx),
    "parent|cx.new() in render; create child views in init or a handler"
  );
  assert_eq!(eval(&view, cx, "log.join()"), "parent,child:1");

  // gpui renders the views around a notified one again, but not `update` on them
  cx.executor().advance_clock(Duration::from_millis(5));
  cx.run_until_parked();
  assert_eq!(
    eval(&view, cx, "log.join()"),
    "parent,child:1,parent,child:2"
  );
}

#[gpui::test]
fn child_handler_may_set_its_own_props(cx: &mut TestAppContext) {
  let (view, cx) = rooted(cx, REENTRANT);
  cx.simulate_click(point(px(10.), px(10.)), Modifiers::none());
  cx.run_until_parked();
  assert_eq!(eval(&view, cx, "String(globalThis.updated)"), "2");
}

const TIMERS: &str = r##"
import { View, div } from "corona/kit";

export default class Main extends View {
  init(_props, cx) {
    this.ticks = 0;
    this.once = false;
    cx.timer.after(10, (cx) => {
      this.once = true;
      cx.notify();
    });
    const every = cx.timer.every(4, (cx) => {
      this.ticks += 1;
      if (this.ticks === 2) every.cancel();
      cx.notify();
    });
    cx.spawn(async (cx) => {
      await cx.sleep(20);
      this.slept = true;
      cx.notify();
    });
  }

  render(_cx) {
    return div().child(`once:${this.once}|ticks:${this.ticks}|slept:${!!this.slept}`);
  }
}
"##;

#[gpui::test]
fn timers_and_tasks(cx: &mut TestAppContext) {
  let (view, cx) = open(cx, TIMERS);
  assert_eq!(text(&view, cx), "once:false|ticks:0|slept:false");
  for (ms, expected) in [
    (4, "once:false|ticks:1|slept:false"),
    (4, "once:false|ticks:2|slept:false"),
    (6, "once:true|ticks:2|slept:false"),
    (10, "once:true|ticks:2|slept:true"),
  ] {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
    assert_eq!(text(&view, cx), expected);
  }
}

const THEME: &str = r##"
import { View, div } from "corona/kit";
import { set_theme } from "corona/base";

export default class Main extends View {
  render(cx) {
    const t = cx.theme();
    return div()
      .size_full()
      .bg("primary")
      .p("md")
      .rounded("full")
      .child(`${t.appearance}|${t.primary}|${t.colors.ring}|${t.spacing.md}|${t.radius.full}|${t.typography.md.size}`)
      .on_click((_e, cx) => {
        set_theme({ appearance: "dark", tokens: { colors: { primary: "#112233" }, spacing: { md: 20 } } });
        cx.notify();
      });
  }
}
"##;

#[gpui::test]
fn theme_tokens(cx: &mut TestAppContext) {
  let (view, cx) = open(cx, THEME);
  assert_eq!(text(&view, cx), "light|#171717|#a3a3a3|12|9999|16");

  cx.simulate_click(point(px(10.), px(10.)), Modifiers::none());
  cx.run_until_parked();
  assert_eq!(text(&view, cx), "dark|#112233|#a3a3a3|20|9999|16");
}

const COUNTS: &str = r##"
import { View, div } from "corona/kit";

export default class Main extends View {
  render(_cx) {
    return div().size_full().child("hi");
  }
}
"##;

/// A frame in which the view did not notify paints without calling `render`.
#[gpui::test]
fn clean_frames_do_not_enter_js(cx: &mut TestAppContext) {
  let (view, cx) = rooted(cx, COUNTS);
  let root = cx.update(|window, _| window.root::<ScriptRoot>().flatten().unwrap());
  let renders = |cx: &mut VisualTestContext| view.read_with(cx, |view, _| view.renders);
  assert_eq!(renders(cx), 1);

  root.update(cx, |root, cx| root.set_background(Some(gpui::red()), cx));
  cx.run_until_parked();
  assert_eq!(renders(cx), 1, "a clean view rendered again");

  view.update(cx, |_, cx| cx.notify());
  cx.run_until_parked();
  assert_eq!(renders(cx), 2);
}

const STATES: &str = r##"
import { View, div, list, uniform_list } from "corona/kit";
import { TestState } from "corona/component";

globalThis.log = [];

export default class Main extends View {
  init(_props, _cx) {
    this.state = new TestState();
    this.unknown = this.state.on("nope", () => {});
    this.state.on("change", (value, cx) => {
      this.got = value;
      cx.notify();
    });
  }

  render(_cx) {
    return div()
      .size_full()
      .child(`unknown:${this.unknown}|got:${this.got}`)
      .child(list("l", 2, (i) => `${i}`, (i) => {
        log.push(`item${i}`);
        return div().h(10).child(`item${i}`);
      }).h(40))
      .child(uniform_list("u", 3, (i) => `${i}`, (range) => {
        log.push(`range${range.start}-${range.end}`);
        return [...Array(range.end - range.start).keys()].map((i) => div().h(10).child(`row${i}`));
      }).h(40))
      .on_click((_e, _cx) => this.state.emit(7));
  }
}
"##;

#[gpui::test]
fn retained_states_and_lists(cx: &mut TestAppContext) {
  let (view, cx) = open(cx, STATES);
  assert_eq!(text(&view, cx), "unknown:false|got:undefined");
  let log = eval(&view, cx, "log.join()");
  assert!(
    log.contains("item0,item1") && log.ends_with("range0-3"),
    "{log}"
  );

  cx.simulate_click(point(px(1.), px(1.)), Modifiers::none());
  cx.run_until_parked();
  assert_eq!(text(&view, cx), "unknown:false|got:7");
}

const WINDOW: &str = r##"
import { View, div } from "corona/kit";

globalThis.log = [];

export default class Main extends View {
  init(_props, cx) {
    this.focus = cx.focus_handle();
  }

  render(_cx) {
    return div()
      .size_full()
      .track_focus(this.focus)
      .on_click((_e, _cx) => {
        this.focus.focus();
        window.dispatch_action("ping");
      })
      .on_action("ping", (e, _cx) => {
        log.push(`action:${e.action}`);
        window.open_dialog(() => {
          log.push("dialog");
          return div().child("in dialog");
        });
        log.push(`open:${window.has_active_dialog()}`);
        log.push(`toast:${window.push_toast({ title: "hi", id: "t1" })}`);
        log.push(`size:${window.viewport_size().width > 0}`);
      });
  }
}
"##;

#[gpui::test]
fn window_overlays_and_actions(cx: &mut TestAppContext) {
  cx.update(gpui::init);
  let (root, cx) = cx.add_window_view(|window, cx| {
    let watches = Watches::default();
    let modules = modules(&watches, cx);
    let view = cx.new(|cx| ScriptView::new(WINDOW, modules, watches, window, cx).unwrap());
    let content = cx.new(|_| ScriptRoot::new(view));
    gpui::base::Root::new(content, window, cx)
  });
  cx.run_until_parked();
  cx.simulate_click(point(px(10.), px(10.)), Modifiers::none());
  cx.run_until_parked();
  let content = root.read_with(cx, |root, _| root.view().clone());
  let content = content.downcast::<ScriptRoot>().ok().unwrap();
  let view = content.read_with(cx, |root, _| root.view().clone());
  let log = eval(&view, cx, "log.join()");
  assert!(
    log.starts_with("action:ping,open:true,toast:t1,size:true,dialog"),
    "{log}"
  );
}

/// Child views, pending timers, kept dialogs and `.on` handlers all hold JS: closing the
/// window must free them before the runtime, which aborts otherwise.
#[gpui::test]
fn closing_frees_children_timers_and_states(cx: &mut TestAppContext) {
  for source in [CHILDREN, TIMERS, STATES] {
    let (view, cx) = open(cx, source);
    let weak = view.downgrade();
    let rt = view.read_with(cx, |view, _| view.runtime());
    drop(view);
    cx.update(|window, _| window.remove_window());
    cx.run_until_parked();
    assert!(weak.upgrade().is_none());
    assert!(rt.upgrade().is_none(), "the runtime leaked");
  }
}

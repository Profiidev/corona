//! Dock areas: panels, persisted layouts, chrome handlers and dock commands.

use gpui_kit::{self as gpui, Modifiers, TestAppContext, point, px};

use super::{eval, rooted};

const DOCK: &str = r##"
import { View, div } from "gpui-kit";
import { h_flex, v_flex, DockArea, dock_area, dock_content } from "gpui-base";

globalThis.log = [];

class Inbox extends View {
  init(props, _cx) {
    this.note = props?.note ?? "fresh";
  }
  serialize() {
    return { note: this.note };
  }
  deserialize(data) {
    this.note = data.note;
    log.push(`restored:${data.note}`);
  }
  render(_cx) {
    return div().size_full().child(`inbox:${this.note}`);
  }
}

export default class Main extends View {
  init(_props, cx) {
    globalThis.qualified = DockArea.register_panel("inbox", Inbox);
    this.area = DockArea.new("workspace");
    this.area.on("layout_changed", (cx) => log.push(`changed:${typeof cx.notify}`));
    this.area.add_panel(cx.new(Inbox, { note: "a" }), { name: "inbox" });
    this.area.add_panel(cx.new(Inbox, { note: "b" }), { name: "inbox" });
    this.area.add_panel(cx.new(Inbox, { note: "side" }), { name: "inbox", placement: "left", size: 120 });
    try {
      this.area.add_panel(cx.new(Inbox), { name: "inbox", placement: "top" });
    } catch (e) {
      log.push(e.message);
    }
  }

  render(_cx) {
    const area = this.area;
    const summary = () => area.panels().map((p) => `${p.placement}:${p.index}:${p.active}`).join(" ");
    const button = (id, f) => div().id(id).w(50).h(20).on_click((_e, cx) => { f(); cx.notify(); });
    return v_flex().size_full().child(h_flex().h(20).children([
      button("panels", () => log.push(summary())),
      button("roundtrip", () => {
        const saved = area.dump();
        log.push(`saved:${JSON.stringify(saved).match(/"note":"\w+"/g).join("+")}`);
        area.load(saved);
        log.push(`loaded:${summary()}`);
      }),
      button("open", () => log.push(`left:${area.is_dock_open("left")}:${area.dock_size("left")}`)),
      button("zoom", () => log.push(`zoom:${area.is_zoomed()}:${area.panels().length}`)),
    ])).child(dock_area(area)
      .flex_1()
      .w_full()
      .tab_bar((group) => h_flex().h(20).child(div().id(`zoom-${group.node}`).w(50).h(20).toggle_zoom(group))
        .child(div().id(`close-${group.node}`).w(50).h(20).close_panel(group, group.tabs[group.active_index]?.id ?? -1))
        .children(group.tabs.map((t) =>
        div().id(`tab-${group.node}-${t.index}`).w(50).h(20).select_tab(group, t.index).drag_tab(group, t.index)
          .drop_tab(group, t.index).child(t.active ? "*" : "-"))))
      .dock((dock) => v_flex().size_full().child(div().id(`toggle-${dock.placement}`).h(20).toggle_dock(dock)
        .resize_dock(dock).child(dock.placement)).child(dock_content()))
      .empty_group((_group) => div().child("empty"))
      .drop_indicator((_drop) => null));
  }
}
"##;

fn click(cx: &mut gpui::VisualTestContext, x: f32, y: f32) {
  cx.simulate_click(point(px(x), px(y)), Modifiers::none());
  cx.run_until_parked();
}

#[gpui::test]
fn dock_area_keeps_panels_and_layouts(cx: &mut TestAppContext) {
  let (view, cx) = rooted(cx, DOCK);
  assert_eq!(eval(&view, cx, "qualified"), "shell:/inbox");
  click(cx, 25., 10.);
  // the center group's first tab, past its zoom and close buttons
  click(cx, 245., 30.);
  click(cx, 25., 10.);
  click(cx, 75., 10.);
  click(cx, 125., 10.);
  // the left dock's own toggle, drawn by its chrome
  click(cx, 60., 30.);
  click(cx, 125., 10.);
  // with the left dock closed the center starts at 0: close its active panel, then zoom it
  click(cx, 75., 30.);
  click(cx, 175., 10.);
  click(cx, 25., 30.);
  click(cx, 175., 10.);
  let log = eval(&view, cx, "log.join('|')");
  let lines: Vec<&str> = log
    .split('|')
    .filter(|l| !l.starts_with("changed:"))
    .collect();
  assert_eq!(
    lines,
    [
      "unknown dock placement \"top\"",
      "center:0:false center:1:true left:0:true",
      "center:0:true center:1:false left:0:true",
      "saved:\"note\":\"a\"+\"note\":\"b\"+\"note\":\"side\"",
      "restored:a",
      "restored:b",
      "restored:side",
      "loaded:center:0:true center:1:false left:0:true",
      "left:true:120",
      "left:false:120",
      "zoom:false:2",
      "zoom:true:2",
    ],
    "{log}"
  );
  assert!(log.contains("changed:function"), "{log}");
}

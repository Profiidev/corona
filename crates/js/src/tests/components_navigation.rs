//! Navigation: tabs, accordions, pagination, panes, scrolling, sidebars, menus, commands.

use gpui_kit::{self as gpui, Modifiers, TestAppContext, point, px};

use super::{eval, rooted, text};

/// Builds every navigation component of both modules with its methods, one result line each
const ALL: &str = r##"
import { View, div, ScrollHandle } from "gpui-kit";
import * as base from "gpui-base";
import * as c from "gpui-component";

const attempt = (f) => {
  try {
    globalThis.built.push(f());
    return "ok";
  } catch (e) {
    return `${e.name}: ${e.message}`;
  }
};

export default class Main extends View {
  init(_props, _cx) {
    this.command = c.CommandState();
    this.scroll = c.ScrollbarHandle();
    this.list = base.VirtualListScrollHandle.new();
    this.slider = c.SliderState(1);
  }

  render(_cx) {
    globalThis.built = [];
    const rows = (range) => [...Array(range.end - range.start).keys()].map((i) => div().child(`${i}`));
    const results = [
      attempt(() => div().child("|")),
      attempt(() => base.Tabs.new("tabs").child(base.Tab.new("t1").selected(true).disabled(false).set_position(1, 2).on_click(() => {}).child("One"))),
      attempt(() => base.Accordion.new("acc").child(base.AccordionItem.new().open(true).disabled(false)
        .header(base.AccordionHeader.new(base.AccordionTrigger.new("tr").on_change(() => {}).child("Q")).aria_level(2))
        .panel(base.AccordionPanel.new().keep_mounted(true).child("A")))),
      attempt(() => base.Pagination.new("pg").accessibility_label("Pages").children(base.pagination_items(5, 20).map((i) => div().child(i.page ? `${i.page}` : "…")))),
      attempt(() => base.Collapsible.new().open(true).content(div().child("body")).child("head")),
      attempt(() => base.h_resizable("h").h(100).on_resize(() => {}).child(base.resizable_panel().size(100).size_range(50, 200).visible(true).child("a")).child(div().child("b"))),
      attempt(() => base.v_resizable("v").child(base.resizable_panel())),
      attempt(() => base.Scrollbar.vertical(this.scroll).mode("always").scroll_size(10, 100).viewport_from_layout()),
      attempt(() => base.Scrollbar.new(this.list).scroll_axis("both")),
      attempt(() => base.v_virtual_list("vl", 3, 20, (i) => `${i}`, rows).h(60).track_scroll(this.list).with_item_to_measure_index(0)),
      attempt(() => base.h_virtual_list("hl", 2, [30, 40], (i) => `${i}`, rows).w(60)),
      attempt(() => div().id("s").h(40).overflow_y_scrollbar().child("x")),
      attempt(() => div().overflow_scrollbar()),
      attempt(() => div().overflow_x_scrollbar()),
      attempt(() => c.TabBar.new("tb").selected_index(0).variant("pill").menu(false).size("small").on_change(() => {}).child(c.Tab.new().label("A").aria_label("a").disabled(false).selected(true)).child(c.Tab.new().label("B"))),
      attempt(() => c.Tabs.new("tabs2").child(c.Tab.new().label("C"))),
      attempt(() => c.Accordion.new("ca").multiple(true).bordered(true).size("small").on_toggle(() => {}).child(c.AccordionItem.new().title(div().child("T")).open(true).disabled(false).child("body"))),
      attempt(() => c.Collapsible.new().open(false).motion_id("m").content(div().child("x"))),
      attempt(() => c.Pagination.new("cp").current_page(2).total_pages(9).visible_pages(5).compact().size("small").disabled(false).on_change(() => {})),
      attempt(() => c.Stepper.new("st").selected_index(1).vertical(true).text_center(true).disabled(false).size("small").on_change(() => {}).child(c.StepperItem.new().disabled(false).child("one"))),
      attempt(() => c.Breadcrumb.new(["Home", "Docs"])),
      attempt(() => c.GroupBox.new().title(div().child("Group")).variant("outline").child("x")),
      attempt(() => c.StatusBar.new().left_content(div().child("l")).right_content(div().child("r")).child("m")),
      attempt(() => c.Toolbar.new("tool").child("x")),
      attempt(() => c.Separator.new().label("or").color("border").dashed()),
      attempt(() => c.VerticalSeparator.new()),
      attempt(() => c.DashedSeparator.new()),
      attempt(() => c.VerticalDashedSeparator.new()),
      attempt(() => c.Resizable.new("rs").axis("vertical").cross_size(100).child(c.ResizablePanel.new().size(50))),
      attempt(() => c.Scroll.new(this.scroll).scroll_axis("both").h(40).child("x")),
      attempt(() => c.Scrollbar.new("sb", this.scroll).scroll_axis("vertical").mode("hover")),
      attempt(() => c.Sidebar.new("side").side("left").collapsible("icon").collapsed(false)
        .header(c.SidebarHeader.new().selected(false).child("H")).footer(c.SidebarFooter.new().child("F"))
        .child(c.SidebarMenu.new().child(c.SidebarMenuItem.new("Home").icon("home").selected(true).default_open(true).click_to_open(true).click_to_toggle(true).disabled(false).on_click(() => {}).child(c.SidebarMenuItem.new("Sub"))))),
      attempt(() => c.SidebarToggleButton.new().side("left").collapsed(true).on_click(() => {})),
      attempt(() => c.DropdownMenu.new("dm", "More").item("One", () => {})),
      attempt(() => c.DropdownButton.new("db", "Save").outline().disabled(false).selected(false).size("small").variant("primary").menu_anchor("bottom_right").on_click(() => {}).menu_item("As", () => {})),
      attempt(() => c.NativeMenuTrigger.new("nm", "Menu").disabled(false).child(c.NativeMenuItem.new("Open", "open").disabled(false).checked(true).action("open2")).child(c.NativeMenuSeparator.new())),
      attempt(() => c.Command.new(this.command).searchable(true).filterable(true).bordered(true).placeholder("Search").max_height(200).on_query(() => {}).on_select(() => {}).on_confirm(() => {}).on_cancel(() => {})
        .child(c.CommandItem.new("Run").keyword("go").checked(false).disabled(false).action("run")).child(c.CommandSeparator.new()).child(c.CommandGroup.new("G").child(c.CommandItem.new("x")))),
      attempt(() => c.MenuBar.new("bar").child(c.Menu.new("File").disabled(false).child(c.MenuItem.new("Quit", "quit").checked(false).disabled(false)).child(c.MenuSeparator.new()))),
      attempt(() => c.TabBar.new("x").child(div())),
      attempt(() => c.Tab.new().href("x")),
      attempt(() => c.Stepper.new("x").vertical().groups(2)),
      attempt(() => base.Slider.new(this.slider).vertical()),
    ];
    return div().size_full().children(built).child(div().children(results.map((r) => r + "\n")));
  }
}
"##;

#[gpui::test]
fn every_navigation_component_renders(cx: &mut TestAppContext) {
  let (view, cx) = rooted(cx, ALL);
  let text = text(&view, cx);
  let lines: Vec<&str> = (text.lines())
    .filter(|l| l.ends_with("ok") || l.contains("Error"))
    .collect();
  let (ok, rest) = lines.split_at(lines.len() - 4);
  for (i, line) in ok.iter().enumerate() {
    assert!(line.ends_with("ok"), "line {i}: {line}");
  }
  assert_eq!(
    rest,
    [
      "TypeError: TabBar takes Tab children, not div",
      "TypeError: this method is not available on Tab",
      "TypeError: this method is not available on Stepper",
      "TypeError: this method is not available on Slider",
    ]
  );
}

const EVENTS: &str = r##"
import { View, div } from "gpui-kit";
import { v_flex, Collapsible } from "gpui-base";
import { TabBar, Tab, SidebarToggleButton } from "gpui-component";

globalThis.log = [];

export default class Main extends View {
  init(_props, _cx) {
    this.tab = 0;
    this.open = false;
  }

  render(_cx) {
    return v_flex().w(400).children([
      Collapsible.new().open(this.open).content(div().id("body").h(30).w(400).on_click(() => log.push("body"))),
      div().id("toggle").h(30).w(400).on_click((_e, cx) => {
        log.push("toggle");
        this.open = !this.open;
        cx.notify();
      }),
      TabBar.new("tabs").h(30).selected_index(this.tab).on_change((i, cx) => {
        log.push(`tab:${i}`);
        this.tab = i;
        cx.notify();
      }).child(Tab.new().label("A").w(100)).child(Tab.new().label("B").w(100)),
      SidebarToggleButton.new().on_click((e) => log.push(`side:${e.click_count}`)),
    ]);
  }
}
"##;

#[gpui::test]
fn events_carry_their_payloads(cx: &mut TestAppContext) {
  let (view, cx) = rooted(cx, EVENTS);
  // closed, then open: the toggle moves down under the content
  for (x, y) in [(10., 15.), (10., 15.), (150., 75.), (10., 105.)] {
    cx.simulate_click(point(px(x), px(y)), Modifiers::none());
    cx.run_until_parked();
  }
  assert_eq!(eval(&view, cx, "log.join()"), "toggle,body,tab:1,side:1");
}

const ROWS: &str = r##"
import { View, div } from "gpui-kit";
import { v_flex, v_virtual_list, Scrollbar } from "gpui-base";
import { MenuBar, Menu, MenuItem, NativeMenuTrigger } from "gpui-component";

globalThis.log = [];

export default class Main extends View {
  init(_props, _cx) {
    this.bar = true;
    this.rows = ["a", "b", "c"];
  }

  render(_cx) {
    return v_flex().w(300).h(400).children([
      v_virtual_list("rows", 3, 20, (i) => `row-${this.rows[i]}`, (range) =>
        this.rows.slice(range.start, range.end).map((r) => div().w(300).h(20).child(r)))
        .h(60)
        .on_item_click((key, cx) => {
          log.push(`click:${key}`);
          this.bar = false;
          cx.notify();
        })
        .on_item_secondary_click((key, e) => log.push(`menu:${key}:${e.button}`)),
      Scrollbar.vertical("rows"),
      div().id("area").h(40).overflow_y_scroll().child(div().h(200)),
      Scrollbar.new("area"),
      NativeMenuTrigger.new("nm", "Menu").on_effect_error(() => {}),
    ]).when(this.bar, (el) => el.child(MenuBar.new("bar").child(Menu.new("File").child(MenuItem.new("Quit", "quit")))));
  }
}
"##;

#[gpui::test]
fn virtual_rows_report_keys_and_menus_come_back(cx: &mut TestAppContext) {
  let menus = |cx: &mut gpui::VisualTestContext| {
    cx.update(|_, cx| {
      let menus = cx.get_menus().unwrap_or_default();
      menus.iter().map(|m| m.name.to_string()).collect::<Vec<_>>()
    })
  };
  cx.update(|cx| cx.set_menus([gpui::Menu::new("Host")]));
  let (view, cx) = rooted(cx, ROWS);
  assert_eq!(menus(cx), ["File"], "{}", eval(&view, cx, "log.join()"));
  cx.simulate_mouse_down(
    point(px(10.), px(30.)),
    gpui::MouseButton::Right,
    Modifiers::none(),
  );
  cx.run_until_parked();
  cx.simulate_click(point(px(10.), px(50.)), Modifiers::none());
  cx.run_until_parked();
  assert_eq!(
    eval(&view, cx, "log.join()"),
    "menu:row-b:right,click:row-c"
  );
  // the bar is gone, and with it its menus
  assert_eq!(menus(cx), ["Host"]);
}

//! Overlay components: anchored surfaces, window effects and alerts.

use std::time::Duration;

use gpui_kit::{
  self as gpui, Entity, Modifiers, Pixels, Point, TestAppContext, VisualTestContext,
  component::WindowExt, point, px,
};

use super::rooted;
use crate::ScriptView;

fn log(view: &Entity<ScriptView>, cx: &mut VisualTestContext) -> String {
  view.read_with(cx, |view, _| view.eval("log.join()"))
}

fn click(cx: &mut VisualTestContext, at: Point<Pixels>) {
  cx.simulate_click(at, Modifiers::none());
  cx.run_until_parked();
}

/// Where a surface below its trigger at y 100..140 shows its content
const INSIDE: (f32, f32) = (100., 200.);
const TRIGGER: (f32, f32) = (100., 120.);

fn at((x, y): (f32, f32)) -> Point<Pixels> {
  point(px(x), px(y))
}

#[gpui::test]
fn base_popover_is_controlled_through_open_and_on_open_change(cx: &mut TestAppContext) {
  const SOURCE: &str = r#"
import { View, div } from "corona/kit";
import { v_flex, Popover } from "corona/base";

globalThis.log = [];

export default class Main extends View {
  init() {
    this.open = false;
    this.hits = 0;
  }

  render(_cx) {
    return v_flex().w(400).h(400).child(div().w(400).h(100)).child(
      Popover.new("menu")
        .anchor("top_left")
        .open(this.open)
        .on_open_change((open, cx) => {
          log.push(`open:${open}`);
          this.open = open;
          cx.notify();
        })
        .trigger(div().w(300).h(40).child("Open"))
        .content(div().id("body").w(200).h(160).on_click((_e, cx) => {
          log.push(`hit:${++this.hits}`);
          cx.notify();
        })),
    );
  }
}
"#;
  let (view, cx) = rooted(cx, SOURCE);
  click(cx, at(INSIDE));
  assert_eq!(log(&view, cx), "", "a closed popover draws no content");

  click(cx, at(TRIGGER));
  click(cx, at(INSIDE));
  assert_eq!(log(&view, cx), "open:true,hit:1");

  click(cx, at((300., 380.)));
  click(cx, at(INSIDE));
  assert_eq!(log(&view, cx), "open:true,hit:1,open:false");
}

#[gpui::test]
fn component_popover_opens_from_its_button(cx: &mut TestAppContext) {
  const SOURCE: &str = r#"
import { View, div } from "corona/kit";
import { v_flex } from "corona/base";
import { Popover } from "corona/component";

globalThis.log = [];

export default class Main extends View {
  render(_cx) {
    return v_flex().w(400).h(400).child(div().w(400).h(100)).child(
      new Popover("menu", "Open")
        .card_anchor("top_left")
        .appearance(false)
        .default_open(false)
        .on_open_change((open, _cx) => log.push(`open:${open}`))
        .content(div().id("body").w(200).h(160).on_click(() => log.push("hit"))),
    );
  }
}
"#;
  let (view, cx) = rooted(cx, SOURCE);
  click(cx, at((20., 110.)));
  click(cx, at(INSIDE));
  assert_eq!(log(&view, cx), "open:true,hit");
}

/// Rests the pointer on the trigger past the delay, then clicks the content
fn hover(cx: &mut TestAppContext, source: &'static str) {
  let (view, cx) = rooted(cx, source);
  cx.simulate_mouse_move(at(TRIGGER), None, Modifiers::none());
  cx.executor().advance_clock(Duration::from_millis(60));
  cx.run_until_parked();
  click(cx, at(INSIDE));
  assert_eq!(log(&view, cx), "hit");
}

#[gpui::test]
fn base_hover_card_opens_after_its_delay(cx: &mut TestAppContext) {
  hover(
    cx,
    r#"
import { View, div } from "corona/kit";
import { v_flex, HoverCard } from "corona/base";

globalThis.log = [];

export default class Main extends View {
  render(_cx) {
    return v_flex().w(400).h(400).child(div().w(400).h(100)).child(
      HoverCard.new("card")
        .anchor("top_left")
        .open_delay(50)
        .close_delay(50)
        .trigger(div().w(300).h(40).child("Hover"))
        .content(div().id("body").w(200).h(160).on_click(() => log.push("hit"))),
    );
  }
}
"#,
  );
}

#[gpui::test]
fn component_hover_card_opens_after_its_delay(cx: &mut TestAppContext) {
  hover(
    cx,
    r#"
import { View, div } from "corona/kit";
import { v_flex } from "corona/base";
import { HoverCard } from "corona/component";

globalThis.log = [];

export default class Main extends View {
  render(_cx) {
    return v_flex().w(400).h(400).child(div().w(400).h(100)).child(
      HoverCard.new("card")
        .card_anchor("top_left")
        .open_delay(50)
        .close_delay(50)
        .appearance(false)
        .trigger_element(div().w(300).h(40).child("Hover"))
        .content(div().id("body").w(200).h(160).on_click(() => log.push("hit"))),
    );
  }
}
"#,
  );
}

#[gpui::test]
fn popup_shows_its_content_while_set(cx: &mut TestAppContext) {
  const SOURCE: &str = r#"
import { View, div } from "corona/kit";
import { v_flex, Popup } from "corona/base";

globalThis.log = [];

export default class Main extends View {
  init() {
    this.open = false;
  }

  render(_cx) {
    const trigger = div().w(300).h(40).child("Open");
    return v_flex().w(400).h(400).child(div().w(400).h(100)).child(
      Popup.new("options", trigger)
        .anchor("top_left")
        .w(300)
        .on_click((_e, cx) => {
          log.push("trigger");
          this.open = !this.open;
          cx.notify();
        })
        .when(this.open, (el) =>
          el.content(div().id("body").w(200).h(160).on_click(() => log.push("hit")))),
    );
  }
}
"#;
  let (view, cx) = rooted(cx, SOURCE);
  click(cx, at(INSIDE));
  click(cx, at(TRIGGER));
  click(cx, at(INSIDE));
  assert_eq!(log(&view, cx), "trigger,hit");
}

const EFFECTS: &str = r#"
import { View, div } from "corona/kit";
import { v_flex } from "corona/base";
import { Dialog, AlertDialog, Sheet, Notification } from "corona/component";

globalThis.log = [];

export default class Main extends View {
  render(_cx) {
    return v_flex()
      .child(
        new Dialog("dialog", "Dialog")
          .w(200)
          .h(40)
          .title("Title")
          .on_ok(() => log.push("ok"))
          .on_cancel(() => log.push("cancel"))
          .on_close((cx) => log.push(`close:${typeof cx.notify}`))
          .content(() => {
            log.push("content");
            return div().child("in dialog");
          }),
      )
      .child(
        AlertDialog.new("alert", "Alert")
          .w(200)
          .h(40)
          .title("Sure?")
          .description("It cannot be undone.")
          .show_cancel(true),
      )
      .child(
        Sheet.new("sheet", "Sheet", (message, _cx) => log.push(`error:${message}`))
          .w(200)
          .h(40)
          .placement("left")
          .content(() => {
            throw new Error("boom");
          }),
      )
      .child(
        Notification.new("note", "Note")
          .w(200)
          .h(40)
          .title("Saved")
          .message("All good.")
          .type("success")
          .autohide(false)
          .on_close(() => log.push("note closed")),
      );
  }
}
"#;

#[gpui::test]
fn window_effects_open_from_their_buttons(cx: &mut TestAppContext) {
  let (view, cx) = rooted(cx, EFFECTS);
  assert_eq!(log(&view, cx), "", "nothing opens while rendering");

  click(cx, at((20., 20.)));
  assert_eq!(log(&view, cx), "content");
  assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
  cx.simulate_keystrokes("escape");
  cx.run_until_parked();
  // the content renders again on every redraw of the dialog, its close animation included
  let logged = log(&view, cx);
  assert!(
    logged.starts_with("content,") && logged.contains(",cancel,close:function"),
    "{logged}"
  );
  assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));

  click(cx, at((20., 60.)));
  assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
  cx.update(|window, cx| window.close_all_dialogs(cx));
  cx.run_until_parked();

  // a content fn that throws is reported once, however often the sheet renders
  click(cx, at((20., 100.)));
  assert!(cx.update(|window, cx| window.has_active_sheet(cx)));
  cx.update(|window, _| window.refresh());
  cx.run_until_parked();
  let logged = log(&view, cx);
  assert_eq!(logged.matches("error:").count(), 1, "{logged}");
  assert!(logged.contains("boom"), "{logged}");
  cx.update(|window, cx| window.close_sheet(cx));
  cx.run_until_parked();

  click(cx, at((20., 140.)));
  cx.update(|window, cx| window.clear_notifications(cx));
  // it closes once its exit transition ran
  cx.executor().advance_clock(Duration::from_secs(1));
  cx.run_until_parked();
  assert!(
    log(&view, cx).ends_with("note closed"),
    "{}",
    log(&view, cx)
  );
}

#[gpui::test]
fn alerts_and_wrong_kinds(cx: &mut TestAppContext) {
  const SOURCE: &str = r#"
import { View, div } from "corona/kit";
import { v_flex, Popover as BasePopover, Popup } from "corona/base";
import { Alert, ErrorAlert, HoverCard, Popover, Dialog } from "corona/component";

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
      attempt(() => Alert.new("a", "Saved").title("Done").banner().visible(true).p_2()),
      attempt(() => new ErrorAlert("e", "Failed")),
      attempt(() => div().open(true)),
      attempt(() => BasePopover.new("p").open_delay(5)),
      attempt(() => BasePopover.new("p").anchor("topLeft")),
      attempt(() => HoverCard.new("h").open_delay(-1)),
      attempt(() => Popover.new("p", "l").title("x")),
      attempt(() => Dialog.new("d", "D").description("x")),
      attempt(() => Dialog.new("d", "D").type("info")),
      attempt(() => Popup.new("u", div()).on_click(() => {}).id("u")),
      attempt(() => div().on_click(() => {}).type("info")),
    ];
    return v_flex().child(Alert.new("shown", "Shown")).children(results.map((r) => r + "\n"));
  }
}
"#;
  let (view, cx) = rooted(cx, SOURCE);
  let lines = [
    "ok",
    "ok",
    "TypeError: this method is not available on div",
    "TypeError: this method is not available on Popover",
    "TypeError: unknown anchor \"topLeft\", did you mean \"top_left\"?",
    "RangeError: invalid duration -1ms",
    "TypeError: this method is not available on Popover",
    "TypeError: this method is not available on Dialog",
    "TypeError: this method is not available on Dialog",
    "ok",
    "TypeError: this method is not available on div",
  ];
  let expected: String = lines.iter().map(|l| format!("{l}\n")).collect();
  assert_eq!(super::text(&view, cx), expected);
}

/// A method several families define reaches the one of the element's kind, whatever the
/// argument types of the others
#[gpui::test]
fn same_names_reach_their_family(cx: &mut TestAppContext) {
  const SOURCE: &str = r#"
import { View, div } from "corona/kit";
import * as base from "corona/base";
import * as c from "corona/component";

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
      attempt(() => base.Popover.new("p").open(true)),
      attempt(() => base.Collapsible.new().open(true)),
      attempt(() => base.Select.new("s").open(true)),
      attempt(() => c.Dialog.new("d", "D").title("text")),
      attempt(() => c.GroupBox.new().title(div())),
      attempt(() => base.Popup.new("u", div()).content(div())),
      attempt(() => base.Collapsible.new().content(div())),
      attempt(() => c.Alert.new("a", "m").visible(false)),
      attempt(() => base.resizable_panel().visible(false)),
      attempt(() => div().visible()),
      attempt(() => c.Alert.new("a", "m").size("small")),
      attempt(() => div().size(10)),
      attempt(() => c.Tooltip.new("t", "Label", "Help").primary()),
      attempt(() => c.Separator.new().label("or")),
      attempt(() => c.Button.new("b").label("Go")),
      attempt(() => c.Notification.new("n", "N").on_click(() => {})),
      attempt(() => c.Button.new("b").on_click(() => {})),
      attempt(() => c.SidebarToggleButton.new().on_click(() => {})),
      attempt(() => div().on_click(() => {})),
    ];
    return div().children(results.map((r) => r + "\n"));
  }
}
"#;
  let (view, cx) = rooted(cx, SOURCE);
  let text = super::text(&view, cx);
  assert!(text.lines().all(|l| l == "ok"), "{text}");
}

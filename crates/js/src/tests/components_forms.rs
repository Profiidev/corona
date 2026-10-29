//! Form controls: every component renders, its methods apply, events reach handlers.

use gpui_kit::{self as gpui, Modifiers, TestAppContext, point, px};

use super::{eval, rooted, text};

/// Builds every form component of both modules with its methods, one result line each
const ALL: &str = r##"
import { View, div } from "gpui-kit";
import * as base from "gpui-base";
import * as c from "gpui-component";

const attempt = (f) => {
  try {
    const el = f();
    globalThis.built.push(el);
    return "ok";
  } catch (e) {
    return `${e.name}: ${e.message}`;
  }
};

export default class Main extends View {
  init(_props, cx) {
    this.input = c.InputState("name", "Ada");
    this.area = c.TextareaState("text");
    this.slider = c.SliderState(20);
    this.otp = c.OtpState(4);
    this.color = c.ColorPickerState();
    this.date = c.DatePickerState();
    this.time = c.TimeFieldState();
    this.base_input = base.InputState.new({ placeholder: "p", value: "v" });
    this.base_slider = base.SliderState.new({ min: 0, max: 10, value: 5 });
    this.base_otp = base.OtpState.new(3, { masked: true });
    this.focus = cx.focus_handle();
  }

  render(_cx) {
    globalThis.built = [];
    const rows = () => [{ id: "a", label: "A" }, { id: "b", label: "B", disabled: true }];
    const results = [
      attempt(() => div().child("|")),
      attempt(() => c.Button.new("b").label("Go").primary().size("small").outline().tooltip("t").loading(false).selected(true).compact().disabled(false).on_click(() => {}).child("x")),
      attempt(() => c.Button.new("b2").secondary().danger().success().warning().ghost().link()),
      attempt(() => c.Link.new("l").href("https://example.com").disabled(false).on_click(() => {}).child("link")),
      attempt(() => c.Checkbox.new("cb").label("Check").checked(true).size("large").tooltip("t").disabled(false).on_change(() => {})),
      attempt(() => c.Switch.new("sw").label("S").checked(false).size("small").on_change(() => {})),
      attempt(() => c.Toggle.new("tg").label("T").checked(true).outline().size("small").on_change(() => {})),
      attempt(() => c.Radio.new("r").label("R").checked(true).tab_stop(true).accessibility_label("r").size("small").on_change(() => {})),
      attempt(() => c.RadioGroup.new("rg").selected_index(1).disabled(false).on_change(() => {}).child(c.Radio.new("r1").label("One")).child("Two")),
      attempt(() => c.HorizontalRadioGroup.new("hrg").children(["A", "B"])),
      attempt(() => c.Label.new("Name").secondary("(opt)").masked(false).highlights("Na")),
      attempt(() => c.Kbd.new("ctrl-s").outline().appearance(true)),
      attempt(() => c.Clipboard.new("clip").value("copied").tooltip("Copy")),
      attempt(() => c.Rating.new("rate").value(3).max(5).color("primary").size("small").disabled(false).on_change(() => {})),
      attempt(() => c.Input.new(this.input).aria_label("name").disabled(false).on_change(() => {})),
      attempt(() => c.NumberInput.new(this.input).placeholder("0").disabled(false)),
      attempt(() => c.Textarea.new(this.area).appearance(true).bordered(true).readonly(false).aria_label("t").on_change(() => {})),
      attempt(() => c.OtpInput.new(this.otp).groups(2).disabled(false)),
      attempt(() => c.Slider.new(this.slider).vertical().reverse().disabled(false)),
      attempt(() => c.ColorPicker.new(this.color).label("Color").accessibility_label("c")),
      attempt(() => c.DatePicker.new(this.date).placeholder("Pick").disabled(false)),
      attempt(() => c.TimeField.new(this.time).disabled(false)),
      attempt(() => c.Select.new("sel", rows, (row) => div().child(row.label), () => {}).placeholder("Pick").menu_width(200).disabled(false)),
      attempt(() => c.Combobox.new("cmb", rows, () => {}, () => {}).placeholder("Pick").search_placeholder("Search").menu_width(200)),
      attempt(() => c.Form.new().columns(2).label_width(100).size("small").child(c.Field.new().label("F").description("d").required(true).visible(true).label_indent(true).align("center").col_span(2).child("x"))),
      attempt(() => c.VForm.new().children([c.Field.new()])),
      attempt(() => c.HForm.new()),
      attempt(() => c.InputGroup.new("ig").input(c.InputGroupInput.new(this.input)).addon(c.InputGroupAddon.new("a").align("inline-end").child(c.InputGroupButton.new("igb").label("x").icon("icons/x.svg").variant("ghost").size("xsmall").outline().loading(false).tooltip("t").aria_label("x").on_click(() => {}))).disabled(false).readonly(false).invalid(false).focus_ring(true).aria_label("g").size("small")),
      attempt(() => c.InputGroup.new("ig2").input(c.InputGroupTextarea.new(this.area)).addon(c.InputGroupAddon.new("b").child(c.InputGroupText.new().child("@")))),
      attempt(() => base.Button.new("bb").selected(true).disabled(false).accessibility_label("b").on_click(() => {}).child("base")),
      attempt(() => base.Link.new("bl").href("https://example.com").child("x")),
      attempt(() => base.Checkbox.new("bc").checked(true).on_change(() => {})),
      attempt(() => base.Switch.new("bs").checked(true).on_change(() => {})),
      attempt(() => base.Radio.new("br").checked(true).set_position(1, 2).on_change(() => {})),
      attempt(() => base.Toggle.new("bt").pressed(true).on_change(() => {})),
      attempt(() => base.RadioGroup.new("brg").axis("horizontal").child(base.Radio.new("x"))),
      attempt(() => base.ToggleGroup.new("btg").axis("vertical")),
      attempt(() => base.Select.new("bsel").open(false).content_focus_handle(this.focus).on_open_change(() => {}).on_confirm(() => {}).on_dismiss(() => {})),
      attempt(() => base.Combobox.new("bcmb").open(false).on_open_change(() => {})),
      attempt(() => base.DatePicker.new("bdp", this.focus).open(false).on_open_change(() => {})),
      attempt(() => base.Input.new(this.base_input)),
      attempt(() => base.NumberInput.new(this.base_input).decrement_button(div().child("-")).increment_button(div().child("+")).controls_right().on_step(() => {})),
      attempt(() => base.Textarea.new(this.area)),
      attempt(() => base.OtpInput.new(this.base_otp)),
      attempt(() => base.OtpInput.new(this.base_otp).cell_style((s) => s.size(20))),
      attempt(() => base.OtpInput.new(this.base_otp).cell_active_style((s) => s.bg("primary"))),
      attempt(() => base.OtpInput.new(this.base_otp).caret_style((s) => s.w(2))),
      attempt(() => base.Slider.new(this.base_slider).axis("horizontal").child(base.SliderTrack.new(this.base_slider).child(base.SliderIndicator.new(this.base_slider).range_style((s) => s.bg("primary")).child(base.SliderThumb.new(this.base_slider).start(false).size(10))))),
      attempt(() => div().checked(true)),
      attempt(() => c.Button.new("x").groups(2)),
      attempt(() => c.Rating.new("x").value("three")),
      attempt(() => div().size(10).on_click(() => {})),
      attempt(() => c.Button.new("x").size("huge")),
    ];
    return div().size_full().children(built).child(div().children(results.map((r) => r + "\n")));
  }
}
"##;

#[gpui::test]
fn every_form_component_renders(cx: &mut TestAppContext) {
  let (view, cx) = rooted(cx, ALL);
  let text = text(&view, cx);
  let lines: Vec<&str> = text
    .lines()
    .filter(|l| l.starts_with("ok") || l.contains("Error"))
    .collect();
  let (ok, rest) = lines.split_at(lines.len() - 5);
  for (i, line) in ok.iter().enumerate() {
    assert_eq!(*line, "ok", "line {i}");
  }
  assert_eq!(
    rest,
    [
      "TypeError: this method is not available on div",
      "TypeError: this method is not available on Button",
      "TypeError: Error converting from js 'string' into type 'f64'",
      "ok",
      "TypeError: unknown size \"huge\"",
    ]
  );
}

const EVENTS: &str = r##"
import { View } from "gpui-kit";
import { v_flex } from "gpui-base";
import * as base from "gpui-base";
import { Button, Checkbox, Switch, Input, InputState } from "gpui-component";

globalThis.log = [];

export default class Main extends View {
  init(_props, _cx) {
    this.checked = false;
    this.pressed = false;
    this.state = InputState("type");
  }

  render(_cx) {
    return v_flex().w(300).children([
      Button.new("b").label("Go").h(30).w(300).on_click((e, cx) => {
        log.push(`click:${e.click_count}`);
        cx.notify();
      }),
      Checkbox.new("c").label("Check").h(30).checked(this.checked).on_change((v, cx) => {
        log.push(`check:${v}`);
        this.checked = v;
        cx.notify();
      }),
      Switch.new("s").label("Switch").h(30).on_change((v) => log.push(`switch:${v}`)),
      base.Toggle.new("t").w(300).h(30).pressed(this.pressed).on_change((v, cx) => {
        log.push(`pressed:${v}`);
        this.pressed = v;
        cx.notify();
      }),
      Input.new(this.state).h(30).on_change((text) => log.push(`text:${text}`)),
    ]);
  }
}
"##;

#[gpui::test]
fn events_carry_their_payloads(cx: &mut TestAppContext) {
  let (view, cx) = rooted(cx, EVENTS);
  for y in [15., 45., 75., 105., 105.] {
    cx.simulate_click(point(px(10.), px(y)), Modifiers::none());
    cx.run_until_parked();
  }
  cx.simulate_click(point(px(100.), px(135.)), Modifiers::none());
  cx.simulate_input("hi");
  cx.run_until_parked();
  assert_eq!(
    eval(&view, cx, "log.join()"),
    "click:1,check:true,switch:true,pressed:true,pressed:false,text:h,text:hi"
  );
}

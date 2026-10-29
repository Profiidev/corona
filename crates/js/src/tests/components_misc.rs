//! Chat, editor, diff, text, settings, questionnaires, `TextView` and the FPS HUD.

use gpui_kit::{self as gpui, Modifiers, TestAppContext, point, px};

use super::{eval, rooted, text};

/// Builds every misc component with its methods, one result line each
const ALL: &str = r##"
import { View, div } from "gpui-kit";
import { v_flex, TextView } from "gpui-base";
import * as c from "gpui-component";
import { fps_monitor, show_fps_monitor, hide_fps_monitor, fps_monitor_visible } from "gpui-fps";

const attempt = (f) => {
  try {
    globalThis.built.push(f());
    return "ok";
  } catch (e) {
    return `${e.name}: ${e.message}`;
  }
};

const PATCH = `diff --git a/a.txt b/a.txt
--- a/a.txt
+++ b/a.txt
@@ -1 +1 @@
-old
+new
`;

globalThis.log = [];

export default class Main extends View {
  init(_props, _cx) {
    this.scroller = c.MessageScrollerState(3);
    this.editor = c.EditorState("fn main() {}", "rust");
    this.diff = c.DiffState(PATCH, "split");
    this.other = c.InputState("Other");
    log.push(`visible:${fps_monitor_visible()}`);
    show_fps_monitor({ anchor: "bottom_left", frame_budget: 16 });
    log.push(`visible:${fps_monitor_visible()}`);
    log.push(`hidden:${hide_fps_monitor()}:${fps_monitor_visible()}`);
  }

  render(_cx) {
    globalThis.built = [];
    const results = [
      attempt(() => div().child("|")),
      attempt(() => c.Attachment.new("att").status("uploading").axis("vertical").size("small").child("file.txt")),
      attempt(() => c.Bubble.new().alignment("end").variant("tinted").child("hi")),
      attempt(() => c.Marker.new("mk").variant("separator").loading(true).loading_style("shimmer").child("typing")),
      attempt(() => c.Message.new().alignment("start").child(c.Bubble.new().child("yo"))),
      attempt(() => c.ShimmerText.new("Thinking").id("sh").duration_ms(500).spread(0.3).reverse(false).once(true)),
      attempt(() => c.MessageScroller.new("ms", this.scroller, (i) => { log.push(`item:${i}`); return div().h(20).child(`m${i}`); })
        .scrollbar(true).jump_button(true).jump_button_label("Latest").h(100)),
      attempt(() => c.Editor.new(this.editor).disabled(false).appearance(true).bordered(true).readonly(false).aria_label("code").h(100)),
      attempt(() => c.Diff.new(this.diff).line_number(true).syntax_highlight(true).header_visible(true).change_background(true)
        .soft_wrap(false).hunk_separator("simple").change_indicator("bars").h(100)),
      attempt(() => c.Text.new("plain").text_color("primary")),
      attempt(() => c.Settings.new("settings").size("small").sidebar_width(200).sidebar_size_range(100, 300).default_selected_page(0)
        .child(c.SettingPage.new("General").description("d").default_open(true).resettable(false)
          .child(c.SettingGroup.new().title("Look").description("g")
            .child(c.SettingItem.new("Dark").description("i").layout("horizontal").keywords(["theme"]).disabled(false)
              .content(() => div().child("field")))))),
      attempt(() => c.Questionnaire.new("q").shortcuts("numbers")
        .child(c.QuestionnaireItem.new("color", "Color?").description("pick").required(true).multiple(false).disabled(false)
          .child(c.QuestionnaireChoice.new("red", "Red").description("warm").disabled(false).default_selected(true))
          .child(c.QuestionnaireChoice.new("blue", "Blue")))
        .child(c.QuestionnaireItem.new("why", "Why?").child(c.QuestionnaireInput.new(this.other, "Because").disabled(false)))),
      attempt(() => TextView.markdown("md", "# Hi\n[link](https://example.com)").selectable(true).scrollable(false).on_link_click(() => {})),
      attempt(() => TextView.html("html", "<p>hi</p>")),
      attempt(() => v_flex().relative().size_full().child(fps_monitor().anchor("top_left").frame_budget(16))),
      attempt(() => c.Bubble.new().variant("plain")),
      attempt(() => c.Marker.new("x").variant("plain")),
      attempt(() => div().variant("plain")),
    ];
    return div().size_full().children(built).child(div().children(results.map((r) => r + "\n")));
  }
}
"##;

#[gpui::test]
fn every_misc_component_renders(cx: &mut TestAppContext) {
  let (view, cx) = rooted(cx, ALL);
  let text = text(&view, cx);
  let lines: Vec<&str> = (text.lines())
    .filter(|l| l.ends_with("ok") || l.contains("Error"))
    .collect();
  let (ok, rest) = lines.split_at(lines.len() - 3);
  for (i, line) in ok.iter().enumerate() {
    assert!(line.ends_with("ok"), "line {i}: {line}");
  }
  assert_eq!(
    rest,
    [
      "TypeError: unknown variant \"plain\"",
      "ok",
      "TypeError: this method is not available on div",
    ]
  );
  let log = eval(&view, cx, "log.join()");
  assert!(
    log.starts_with("visible:false,visible:true,hidden:true:false") && log.contains("item:0"),
    "{log}"
  );
}

const EDITOR: &str = r##"
import { View, div } from "gpui-kit";
import { Editor, EditorState } from "gpui-component";

globalThis.log = [];

export default class Main extends View {
  init(_props, _cx) {
    this.state = EditorState("");
    this.state.on("change", (text) => log.push(`change:${text}`));
  }

  render(_cx) {
    return div().size_full().child(Editor.new(this.state).size_full());
  }
}
"##;

#[gpui::test]
fn editor_state_reports_edits(cx: &mut TestAppContext) {
  let (view, cx) = rooted(cx, EDITOR);
  cx.simulate_click(point(px(20.), px(20.)), Modifiers::none());
  cx.simulate_input("a");
  cx.run_until_parked();
  assert_eq!(eval(&view, cx, "log.join()"), "change:a");
}

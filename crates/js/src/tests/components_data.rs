//! Data display: lists, tables, trees, charts, calendars, carousels, progress, avatars, tags.

use std::time::Duration;

use gpui_kit::{self as gpui, TestAppContext};

use super::{eval, rooted, text};

/// Builds every data component of both modules with its methods, one result line each
const ALL: &str = r##"
import { View, div } from "corona/kit";
import * as base from "corona/base";
import * as c from "corona/component";

const attempt = (f) => {
  try {
    globalThis.built.push(f());
    return "ok";
  } catch (e) {
    return `${e.name}: ${e.message}`;
  }
};

globalThis.log = [];

export default class Main extends View {
  init(_props, _cx) {
    this.calendar = c.CalendarState();
    this.base_calendar = base.CalendarState.new();
    this.carousel = c.CarouselState(3, 0, "horizontal", true);
    this.table = c.DataTableState(["name", "size"]);
  }

  render(_cx) {
    globalThis.built = [];
    const rows = [{ id: "a", label: "A", value: 1 }, { id: "b", label: "B", value: 3 }];
    const results = [
      attempt(() => div().child("|")),
      attempt(() => base.Progress.new("p").value(40).indeterminate(false).accessibility_label("Load")
        .child(base.ProgressTrack.new().w(100).h(4).child(base.ProgressIndicator.new().w(40).h(4)))),
      attempt(() => base.Avatar.new().w(40).h(40)
        .fallback(base.AvatarFallback.new().child("AL"))),
      attempt(() => base.Table.new("t").row_count(2).column_count(1).accessibility_label("T")
        .child(base.TableHeader.new("h").child(base.TableRow.new("r0", 1).child(base.TableHead.new("hd", 1).child("Name"))))
        .child(base.TableBody.new("b").child(base.TableRow.new("r1", 2).child(base.TableCell.new("c", 1).child("Ada"))))
        .child(base.TableCaption.new("cap").child("People"))),
      attempt(() => c.Badge.new().count(5).max(99).color("primary").size("small").child("Inbox")),
      attempt(() => c.Badge.new().dot().child("x")),
      attempt(() => c.Tag.new().variant("success").outline().rounded_full().size("small").child("new")),
      attempt(() => c.Skeleton.new().secondary().w(100).h(10)),
      attempt(() => c.Spinner.new().size("large").icon("loader_circle").color("primary").ease("linear")),
      attempt(() => c.Empty.new().child(c.EmptyHeader.new().media(c.EmptyMedia.new().variant("icon").child("?"))
        .title(c.EmptyTitle.new().child("Nothing")).description(c.EmptyDescription.new().child("Here")))
        .child(c.EmptyContent.new().child("Add"))),
      attempt(() => c.Icon.new("icons/check.svg").size("small").color("primary").rotate(1.5)),
      attempt(() => c.DescriptionList.new().vertical().bordered(true).columns(2).size("small")
        .child(c.DescriptionItem.new("Name").value("Ada").span(2)).child(c.DescriptionItem.new("Age").value(36))),
      attempt(() => c.Progress.new("cp").value(50).loading(false).color("primary").size("small").accessibility_label("x")),
      attempt(() => c.Avatar.new().name("Ada Lovelace").size("large")),
      attempt(() => c.BarChart.new(() => rows).grid(true).label_axis(true).value_axis(true).tick_margin(1).h(100)),
      attempt(() => c.LineChart.new(() => rows).grid().x_axis(true).tick_margin(1).dot().natural().linear().step_after()),
      attempt(() => c.AreaChart.new(() => rows).grid(false).x_axis(false).natural()),
      attempt(() => c.PieChart.new(() => rows).inner_radius(10).pad_angle(0.1).labels(true)),
      attempt(() => c.RadarChart.new(() => rows).grid(true).grid_levels(3).dot()),
      attempt(() => c.Calendar.new(this.calendar).number_of_months(1).size("small")),
      attempt(() => c.Carousel.new("car", this.carousel).accessibility_label("Slides")
        .child(c.CarouselContent.new(this.carousel).child(c.CarouselItem.new("i0", 0, this.carousel).accessibility_label("one").child("one")))
        .child(c.CarouselPrevious.new(this.carousel).size("small")).child(c.CarouselNext.new(this.carousel))
        .child(c.CarouselPagination.new().child(c.CarouselPaginationItem.new("d0", 0, this.carousel).size("small")))),
      attempt(() => c.List.new("list", () => rows, (row) => { log.push(`row:${row.id}`); return div().h(20).child(row.label); }).h(100)),
      attempt(() => c.DataTable.new(this.table, () => rows, (row, column) => { log.push(`cell:${row.id}:${column}`); return div().child(`${row[column]}`); })
        .stripe(true).bordered(true).scrollbar_visible(true, false).row_selectable(true).column_selectable(false)
        .cell_selectable(false).row_header(false).sortable(false).column_resizable(true).column_movable(false).h(100)),
      attempt(() => c.Tree.new("tree").h(100).child(c.TreeItem.new("root", "Root").expanded(true).disabled(false).child(c.TreeItem.new("leaf", "Leaf")))),
      attempt(() => c.Table.new().size("small").accessibility_label("T")
        .child(c.TableHeader.new().child(c.TableRow.new().child(c.TableHead.new().col_span(1).text_center().child("Name"))))
        .child(c.TableBody.new().child(c.TableRow.new().child(c.TableCell.new().text_right().child("Ada"))))
        .child(c.TableFooter.new()).child(c.TableCaption.new().child("People"))),
      attempt(() => c.Table.new().child(div())),
      attempt(() => div().count(3)),
      attempt(() => div().text_center()),
      attempt(() => c.Tag.new().size(10)),
    ];
    return div().size_full().children(built).child(div().children(results.map((r) => r + "\n")));
  }
}
"##;

#[gpui::test]
fn every_data_component_renders(cx: &mut TestAppContext) {
  let (view, cx) = rooted(cx, ALL);
  cx.executor().advance_clock(Duration::from_millis(100));
  cx.run_until_parked();
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
      "TypeError: a Table takes TableHeader, TableBody, TableFooter and TableCaption children",
      "TypeError: this method is not available on div",
      "ok",
      "ok",
    ]
  );
  let log = eval(&view, cx, "log.join()");
  assert!(log.contains("row:a") && log.contains("row:b"), "{log}");
  assert!(
    log.contains("cell:a:name") && log.contains("cell:b:size"),
    "{log}"
  );
}

const STATES: &str = r##"
import { View, div } from "corona/kit";
import { CalendarState } from "corona/base";
import { CarouselState } from "corona/component";

globalThis.log = [];

export default class Main extends View {
  init(_props, _cx) {
    this.calendar = CalendarState.new();
    this.calendar.set_value("2026-02-14");
    const grid = this.calendar.month_days()[0];
    log.push(`value:${this.calendar.value()}`);
    log.push(`weeks:${grid.length >= 4}:${grid[0].length}`);
    this.calendar.set_value(["2026-02-01", null]);
    log.push(`range:${JSON.stringify(this.calendar.value())}`);
    this.carousel = CarouselState(3);
    this.carousel.on("change", (i) => log.push(`slide:${i}`));
    log.push(`next:${this.carousel.select_next()}`);
    log.push(`index:${this.carousel.selected_index()}:${this.carousel.item_count()}`);
    try {
      this.calendar.set_value("14.02.2026");
    } catch (e) {
      log.push(e.message);
    }
  }

  render(_cx) {
    return div();
  }
}
"##;

#[gpui::test]
fn retained_states_report_and_emit(cx: &mut TestAppContext) {
  let (view, cx) = rooted(cx, STATES);
  assert_eq!(
    eval(&view, cx, "log.join('|')"),
    // the event arrives once the call into JS returned
    "value:2026-02-14|weeks:true:7|range:[\"2026-02-01\",null]|next:true|index:1:3\
     |invalid date \"14.02.2026\", expected YYYY-MM-DD|slide:1"
  );
}

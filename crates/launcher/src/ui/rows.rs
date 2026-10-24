use std::{ops::Range, rc::Rc};

use corona_components::components::window_icon::WindowIcon;
use gpui_kit::{
  AnyElement, App, Context, FontWeight, HighlightStyle, InteractiveElement, IntoElement, ObjectFit,
  ParentElement, SharedString, StatefulInteractiveElement, Styled, StyledImage, StyledText, Window,
  assets::IconName,
  component::{
    ActiveTheme, Icon as KitIcon, Sizable, input::Input, scroll::ScrollableElement, text::TextView,
  },
  div, img, list,
  prelude::FluentBuilder,
  px, relative,
};
use rust_i18n::t;

use super::{
  launcher::{Body, Item, Launcher, Page},
  overlays, style,
};
use crate::{
  command::{Accessory, Detail, DetailBody, Evaluation, Icon, Section},
  registry::Row,
  view::Layout,
};

pub(super) fn frame(
  this: &Launcher,
  window: &mut Window,
  cx: &mut Context<Launcher>,
) -> AnyElement {
  let theme = cx.theme();
  let page = this.current();
  div()
    .relative()
    .size_full()
    .flex()
    .flex_col()
    .rounded(px(style::RADIUS))
    .border_1()
    .border_color(theme.colors.border)
    .bg(style::window_fill(cx))
    .overflow_hidden()
    .text_color(theme.foreground)
    .font_family(theme.font_family.clone())
    .child(search_bar(this, page, cx))
    .child(div().flex_1().min_h_0().child(body(this, page, window, cx)))
    .child(footer(this, page, cx))
    .children(overlays::panel(this, cx))
    .children(overlays::filter_menu(this, cx))
    .children(overlays::confirm(this, cx))
    .children(overlays::toast(this, cx))
    .into_any_element()
}

fn search_bar(this: &Launcher, page: &Page, cx: &mut Context<Launcher>) -> impl IntoElement {
  let theme = cx.theme();
  let muted = theme.colors.muted_foreground;
  let nested = this.pages().len() > 1;
  let loading = this.busy > 0
    || match &page.body {
      Body::List(view) => view.source.loading(cx),
      Body::Root => page.scope.as_ref().is_some_and(|s| s.source.loading(cx)),
      _ => false,
    };
  let (chip_fill, chip_border) = style::chip(cx);
  div()
    .relative()
    .h(px(style::SEARCH_HEIGHT))
    .flex_none()
    .px(px(16.))
    .flex()
    .items_center()
    .gap_2()
    .border_b_1()
    .border_color(theme.colors.border)
    .when(nested, |el| {
      el.child(
        div()
          .id("launcher-back")
          .size(px(26.))
          .flex()
          .items_center()
          .justify_center()
          .rounded(px(7.))
          .bg(style::key_cap(cx))
          .cursor_pointer()
          .on_click(cx.listener(|this, _, window, cx| this.go_back(window, cx)))
          .child(KitIcon::new(IconName::ArrowLeft).with_size(px(14.))),
      )
    })
    .when_some(page.title.clone().filter(|_| nested), |el, title| {
      el.child(
        div()
          .text_size(px(13.))
          .text_color(muted)
          .whitespace_nowrap()
          .child(title),
      )
    })
    .when_some(page.scope.as_ref(), |el, scope| {
      el.child(
        div()
          .h(px(28.))
          .px(px(10.))
          .flex()
          .items_center()
          .rounded(px(8.))
          .bg(chip_fill)
          .border_1()
          .border_color(chip_border)
          .text_size(px(13.))
          .child(scope.label.clone()),
      )
    })
    .child(
      div()
        .flex_1()
        .min_w_0()
        .text_size(px(18.))
        .when(!page.searches(), |el| el.invisible())
        .child(Input::new(&this.input).appearance(false)),
    )
    .when_some(
      match &page.body {
        Body::List(view) => view.filter.clone(),
        _ => None,
      },
      |el, filter| {
        let label = filter
          .options
          .get(filter.selected.get())
          .map(|(_, l)| l.clone())
          .unwrap_or_default();
        el.child(
          div()
            .id("launcher-filter")
            .h(px(28.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap_1()
            .rounded(px(8.))
            .border_1()
            .border_color(theme.colors.border)
            .text_size(px(12.5))
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| this.toggle_filter(cx)))
            .child(format!("{}: {label}", filter.label))
            .child(KitIcon::new(IconName::ChevronDown).with_size(px(12.))),
        )
      },
    )
    .when_some(page.args.as_ref(), |el, args| {
      el.children(args.inputs.iter().enumerate().map(|(i, input)| {
        let error = args.error.as_ref().filter(|(e, _)| *e == i);
        let focused = args.focused == i;
        div()
          .h(px(30.))
          .w(px(130.))
          .px(px(8.))
          .flex()
          .items_center()
          .rounded(px(8.))
          .border_1()
          .border_color(match (error.is_some(), focused) {
            (true, _) => theme.colors.danger,
            (false, true) => style::focus_ring(cx),
            (false, false) => theme.colors.border,
          })
          .text_size(px(13.))
          .child(Input::new(input).appearance(false).small())
      }))
    })
    .when(loading, |el| {
      el.child(
        div()
          .absolute()
          .left_0()
          .bottom_0()
          .h(px(2.))
          .w(relative(0.3))
          .bg(theme.colors.primary),
      )
    })
}

fn body(
  this: &Launcher,
  page: &Page,
  window: &mut Window,
  cx: &mut Context<Launcher>,
) -> AnyElement {
  match &page.body {
    Body::Root | Body::List(_) => results(this, page, window, cx),
    Body::Form(form) => form.clone().into_any_element(),
    Body::Detail(detail) => detail_view(detail.markdown.clone(), &detail.metadata, cx),
    Body::Custom { element, .. } => element.clone().into_any_element(),
  }
}

fn results(this: &Launcher, page: &Page, _: &mut Window, cx: &mut Context<Launcher>) -> AnyElement {
  let theme = cx.theme();
  if page.rows.is_empty() {
    let empty = match &page.body {
      Body::List(view) => view.empty.clone(),
      _ => None,
    }
    .unwrap_or_else(|| t!("app.launcher.no_results").into());
    return div()
      .size_full()
      .flex()
      .items_center()
      .justify_center()
      .text_size(px(14.))
      .text_color(theme.colors.muted_foreground)
      .child(empty)
      .into_any_element();
  }

  let weak = cx.entity().downgrade();
  let rows: Rc<Vec<RowView>> = Rc::new(
    page
      .rows
      .iter()
      .enumerate()
      .map(|(i, row)| row_view(this, row, i == page.selected, cx))
      .collect(),
  );
  let list_element = match page.layout() {
    Layout::List => {
      let items: Rc<Vec<ItemView>> = Rc::new(
        page
          .items
          .iter()
          .map(|item| match item {
            Item::Header(section) => ItemView::Header(section_label(*section)),
            Item::Row(i) => ItemView::Row(*i),
          })
          .collect(),
      );
      list(page.list.clone(), move |ix, _, cx| match &items[ix] {
        ItemView::Header(label) => header(label.clone(), cx),
        ItemView::Row(i) => render_row(&rows[*i], *i, weak.clone(), cx),
      })
      .size_full()
      .into_any_element()
    }
    Layout::Grid {
      columns,
      cell,
      titles,
    } => {
      let columns = columns.max(1);
      list(page.list.clone(), move |line, _, cx| {
        let start = line * columns;
        let end = (start + columns).min(rows.len());
        div()
          .w_full()
          .flex()
          .gap(px(6.))
          .px(px(4.))
          .pb(px(6.))
          .children((start..end).map(|i| render_cell(&rows[i], i, cell, titles, weak.clone(), cx)))
          // a short last line keeps the columns of the others
          .children((end..start + columns).map(|_| div().flex_1().min_w_0()))
          .into_any_element()
      })
      .size_full()
      .into_any_element()
    }
  };

  let preview = this.shows_preview(cx).then(|| {
    page
      .selected_row()
      .and_then(|row| row.command.detail(cx))
      .map(|detail| preview_pane(detail, cx))
      .unwrap_or_else(|| div().flex_1().into_any_element())
  });
  div()
    .size_full()
    .flex()
    .child(
      div()
        .h_full()
        .p(px(8.))
        .map(|el| match preview.is_some() {
          true => el.w(px(style::LIST_WIDTH)).flex_none(),
          false => el.flex_1(),
        })
        .child(list_element),
    )
    .children(preview.map(|pane| {
      div()
        .flex_1()
        .h_full()
        .min_w_0()
        .border_l_1()
        .border_color(theme.colors.border)
        .child(pane)
    }))
    .into_any_element()
}

enum ItemView {
  Header(SharedString),
  Row(usize),
}

/// What a row draws, computed while the launcher can be read
struct RowView {
  id: SharedString,
  icon: Icon,
  title: SharedString,
  highlights: Vec<u32>,
  subtitle: Option<SharedString>,
  hint: Option<(SharedString, bool)>,
  accessories: Vec<Accessory>,
  evaluation: Option<Evaluation>,
  selected: bool,
}

fn row_view(this: &Launcher, row: &Row, selected: bool, cx: &App) -> RowView {
  let mut title = row.meta.title.clone();
  let mut highlights = row.found.indices.clone();
  let mut hint = None;
  if selected && row.found.args.is_some() {
    match this.inline_args(row) {
      Ok(args) => {
        if let Some(text) = row.command.describe(&args, cx) {
          title = text;
          highlights.clear();
        }
      }
      Err(e) => hint = Some((SharedString::from(e.to_string()), true)),
    }
  }
  RowView {
    id: row.id(),
    icon: row.meta.icon.clone(),
    title,
    highlights,
    subtitle: row.meta.subtitle.clone(),
    hint,
    accessories: row.meta.accessories.clone(),
    evaluation: row.meta.evaluation.clone(),
    selected,
  }
}

fn section_label(section: Section) -> SharedString {
  match section {
    Section::Running => t!("app.launcher.section.running"),
    Section::Favorites => t!("app.launcher.section.favorites"),
    Section::Recent => t!("app.launcher.section.recent"),
    Section::Suggestions => t!("app.launcher.section.suggestions"),
    Section::Applications => t!("app.launcher.section.applications"),
    Section::Commands => t!("app.launcher.section.commands"),
    Section::Results => t!("app.launcher.section.results"),
    Section::Fallback => t!("app.launcher.section.fallback"),
  }
  .into()
}

fn header(label: SharedString, cx: &App) -> AnyElement {
  div()
    .h(px(style::HEADER_HEIGHT))
    .px(px(10.))
    .pt(px(10.))
    .pb(px(6.))
    .text_size(px(12.))
    .font_weight(FontWeight::MEDIUM)
    .text_color(cx.theme().colors.muted_foreground)
    .child(label)
    .into_any_element()
}

/// Byte ranges of the chars at `indices`
fn char_ranges(text: &str, indices: &[u32]) -> Vec<Range<usize>> {
  text
    .char_indices()
    .enumerate()
    .filter(|(i, _)| indices.contains(&(*i as u32)))
    .map(|(_, (byte, c))| byte..byte + c.len_utf8())
    .collect()
}

fn render_row(
  row: &RowView,
  index: usize,
  weak: gpui_kit::WeakEntity<Launcher>,
  cx: &App,
) -> AnyElement {
  let theme = cx.theme();
  let highlight = HighlightStyle {
    font_weight: Some(FontWeight::BOLD),
    color: Some(theme.colors.primary),
    ..Default::default()
  };
  if let Some(evaluation) = &row.evaluation {
    return evaluation_tile(evaluation, row.selected, index, weak, cx);
  }
  let ranges = char_ranges(&row.title, &row.highlights)
    .into_iter()
    .map(|r| (r, highlight))
    .collect::<Vec<_>>();
  let (title_color, sub) = match &row.hint {
    Some((_, true)) => (theme.colors.danger, row.hint.as_ref().map(|h| h.0.clone())),
    _ => (theme.foreground, row.subtitle.clone()),
  };
  div()
    .id(("launcher-row", index))
    .w_full()
    .h(px(style::ROW_HEIGHT))
    .px(px(10.))
    .flex()
    .items_center()
    .gap(px(12.))
    .rounded(px(style::ROW_RADIUS))
    .when(row.selected, |el| el.bg(style::selected(cx)))
    .when(!row.selected, |el| el.hover(|s| s.bg(style::hover(cx))))
    .cursor_pointer()
    .on_click(move |_, window, cx| {
      let _ = weak.update(cx, |this, cx| this.click(index, window, cx));
    })
    .child(icon(&row.icon, style::TILE, &row.id, cx))
    .child(
      div()
        .flex_1()
        .min_w_0()
        .flex()
        .items_baseline()
        .gap_2()
        .overflow_hidden()
        .child(
          div()
            .flex_none()
            .max_w(relative(0.7))
            .text_size(px(14.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(title_color)
            .whitespace_nowrap()
            .text_ellipsis()
            .child(StyledText::new(row.title.clone()).with_highlights(ranges)),
        )
        .when_some(sub, |el, sub| {
          el.child(
            div()
              .min_w_0()
              .text_size(px(13.))
              .text_color(theme.colors.muted_foreground)
              .whitespace_nowrap()
              .text_ellipsis()
              .child(sub),
          )
        }),
    )
    .children(row.accessories.iter().map(|a| accessory(a, cx)))
    .into_any_element()
}

/// The calculator's tile: input left, result right, a note under them
fn evaluation_tile(
  evaluation: &Evaluation,
  selected: bool,
  index: usize,
  weak: gpui_kit::WeakEntity<Launcher>,
  cx: &App,
) -> AnyElement {
  let theme = cx.theme();
  let half = |text: SharedString, muted: bool| {
    div()
      .flex_1()
      .min_w_0()
      .h_full()
      .flex()
      .items_center()
      .justify_center()
      .px_3()
      .text_size(px(if muted { 22. } else { 26. }))
      .font_weight(FontWeight::MEDIUM)
      .when(muted, |el| el.text_color(theme.colors.muted_foreground))
      .whitespace_nowrap()
      .text_ellipsis()
      .child(text)
  };
  div()
    .id(("launcher-evaluation", index))
    .w_full()
    .mb_1()
    .h(px(style::EVALUATION_HEIGHT))
    .flex()
    .flex_col()
    .rounded(px(style::ROW_RADIUS))
    .border_1()
    .border_color(theme.colors.border)
    .when(selected, |el| el.bg(style::selected(cx)))
    .cursor_pointer()
    .on_click(move |_, window, cx| {
      let _ = weak.update(cx, |this, cx| this.click(index, window, cx));
    })
    .child(
      div()
        .flex_1()
        .flex()
        .items_center()
        .child(half(evaluation.input.clone(), true))
        .child(KitIcon::new(IconName::ArrowRight).with_size(px(18.)))
        .child(half(evaluation.output.clone(), false)),
    )
    .when_some(evaluation.note.clone(), |el, note| {
      el.child(
        div()
          .pb_2()
          .text_center()
          .text_size(px(12.))
          .text_color(theme.colors.muted_foreground)
          .child(note),
      )
    })
    .into_any_element()
}

fn render_cell(
  row: &RowView,
  index: usize,
  cell: f32,
  titles: bool,
  weak: gpui_kit::WeakEntity<Launcher>,
  cx: &App,
) -> AnyElement {
  let theme = cx.theme();
  div()
    .id(("launcher-cell", index))
    .flex_1()
    .min_w_0()
    .flex()
    .flex_col()
    .items_center()
    .gap_1()
    .cursor_pointer()
    .on_click(move |_, window, cx| {
      let _ = weak.update(cx, |this, cx| this.click(index, window, cx));
    })
    .child(
      div()
        .w_full()
        .h(px(cell))
        .overflow_hidden()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(10.))
        .when(row.selected, |el| {
          el.bg(style::selected(cx))
            .border_2()
            .border_color(style::focus_ring(cx))
        })
        .when(!row.selected, |el| el.hover(|s| s.bg(style::hover(cx))))
        .child(match &row.icon {
          // pictures fill the tile; cover paints past the image's bounds, a box clips it
          Icon::Image(path) => div()
            .size_full()
            .p(px(4.))
            .child(
              div()
                .size_full()
                .rounded(px(8.))
                .overflow_hidden()
                .child(img(path.clone()).size_full().object_fit(ObjectFit::Cover)),
            )
            .into_any_element(),
          other => icon(other, cell * 0.6, &row.id, cx),
        }),
    )
    .when(titles, |el| {
      el.child(
        div()
          .w_full()
          .text_size(px(11.5))
          .text_color(theme.colors.muted_foreground)
          .text_center()
          .whitespace_nowrap()
          .text_ellipsis()
          .child(row.title.clone()),
      )
    })
    .into_any_element()
}

pub(super) fn icon(icon: &Icon, size: f32, id: &SharedString, cx: &App) -> AnyElement {
  let theme = cx.theme();
  let tile = |content: AnyElement, fill: Option<gpui_kit::Background>| {
    div()
      .size(px(size))
      .flex_none()
      .flex()
      .items_center()
      .justify_center()
      .rounded(px(size / 4.))
      .map(|el| match fill {
        Some(fill) => el.bg(fill),
        None => el.bg(style::key_cap(cx)),
      })
      .child(content)
      .into_any_element()
  };
  let glyph = px((size * 0.57).round());
  match icon {
    Icon::None => div().size(px(size)).flex_none().into_any_element(),
    Icon::Named(name) => tile(
      KitIcon::new(*name).with_size(glyph).into_any_element(),
      None,
    ),
    Icon::Tile(name, hue) => tile(
      KitIcon::new(*name)
        .with_size(glyph)
        .text_color(gpui_kit::white())
        .into_any_element(),
      Some(style::tile(*hue)),
    ),
    Icon::App(names) => WindowIcon::new(
      names.first().map(|n| n.to_string()).unwrap_or_default(),
      gpui_kit::ElementId::Name(id.clone()),
    )
    .names(names.iter().map(|n| n.to_string()))
    .size(size as u16)
    .into_any_element(),
    Icon::Glyph(text) => div()
      .size(px(size))
      .flex_none()
      .flex()
      .items_center()
      .justify_center()
      .text_size(px(size * 0.75))
      .child(text.clone())
      .into_any_element(),
    Icon::Sample(text, family) => div()
      .size(px(size))
      .flex_none()
      .flex()
      .items_center()
      .justify_center()
      .font_family(family.clone())
      .text_size(px(size * 0.5))
      .child(text.clone())
      .into_any_element(),
    Icon::Swatch(color) => div()
      .size(px(size))
      .flex_none()
      .rounded(px(size / 4.))
      .border_1()
      .border_color(theme.colors.border)
      .bg(*color)
      .into_any_element(),
    Icon::Image(path) => div()
      .size(px(size))
      .flex_none()
      .rounded(px(size / 4.))
      .overflow_hidden()
      .child(img(path.clone()).size_full().object_fit(ObjectFit::Cover))
      .into_any_element(),
  }
}

fn accessory(accessory: &Accessory, cx: &App) -> AnyElement {
  let theme = cx.theme();
  match accessory {
    Accessory::Tag(text, tone) => {
      let (fill, color) = style::tone(*tone, cx);
      div()
        .h(px(20.))
        .px(px(8.))
        .flex()
        .items_center()
        .rounded_full()
        .bg(fill)
        .text_color(color)
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .child(text.clone())
        .into_any_element()
    }
    Accessory::Text(text) => div()
      .text_size(px(12.5))
      .text_color(theme.colors.muted_foreground)
      .whitespace_nowrap()
      .child(text.clone())
      .into_any_element(),
    Accessory::Icon(name) => KitIcon::new(*name)
      .with_size(px(14.))
      .text_color(theme.colors.muted_foreground)
      .into_any_element(),
    Accessory::Progress(value) => div()
      .w(px(60.))
      .h(px(4.))
      .rounded_full()
      .bg(style::key_cap(cx))
      .child(
        div()
          .h_full()
          .w(relative(value.clamp(0., 1.)))
          .rounded_full()
          .bg(theme.colors.primary),
      )
      .into_any_element(),
  }
}

fn preview_pane(detail: Detail, cx: &App) -> AnyElement {
  let theme = cx.theme();
  let body = match detail.body {
    DetailBody::None => None,
    DetailBody::Text(text) => Some(div().text_size(px(13.)).child(text).into_any_element()),
    DetailBody::Code(text) => Some(
      div()
        .p_3()
        .rounded(px(8.))
        .bg(style::hover(cx))
        .font_family(theme.mono_font_family.clone())
        .text_size(px(12.5))
        .child(text)
        .into_any_element(),
    ),
    DetailBody::Markdown(text) => {
      Some(TextView::markdown("launcher-preview-md", text).into_any_element())
    }
    DetailBody::Image(path) => Some(
      img(path)
        .w_full()
        .max_h(px(260.))
        .object_fit(ObjectFit::Contain)
        .into_any_element(),
    ),
  };
  div()
    .id("launcher-preview")
    .size_full()
    .overflow_y_scrollbar()
    .p(px(16.))
    .flex()
    .flex_col()
    .gap_3()
    .children(body)
    .when(!detail.tags.is_empty(), |el| {
      el.child(
        div().flex().flex_wrap().gap_1().children(
          detail
            .tags
            .iter()
            .map(|(t, tone)| accessory(&Accessory::Tag(t.clone(), *tone), cx)),
        ),
      )
    })
    .when(!detail.metadata.is_empty(), |el| {
      el.child(metadata(&detail.metadata, cx))
    })
    .into_any_element()
}

fn metadata(rows: &[(SharedString, SharedString)], cx: &App) -> AnyElement {
  let theme = cx.theme();
  div()
    .flex()
    .flex_col()
    .gap_1()
    .pt_2()
    .border_t_1()
    .border_color(theme.colors.border)
    .children(rows.iter().map(|(k, v)| {
      div()
        .flex()
        .justify_between()
        .gap_2()
        .text_size(px(12.5))
        .child(
          div()
            .text_color(theme.colors.muted_foreground)
            .child(k.clone()),
        )
        .child(div().text_right().child(v.clone()))
    }))
    .into_any_element()
}

fn detail_view(
  markdown: SharedString,
  meta: &[(SharedString, SharedString)],
  cx: &App,
) -> AnyElement {
  let theme = cx.theme();
  div()
    .size_full()
    .flex()
    .child(
      div()
        .id("launcher-detail")
        .flex_1()
        .h_full()
        .overflow_y_scrollbar()
        .p(px(20.))
        .child(TextView::markdown("launcher-detail-md", markdown)),
    )
    .when(!meta.is_empty(), |el| {
      el.child(
        div()
          .w(px(style::SIDEBAR_WIDTH))
          .h_full()
          .p(px(16.))
          .border_l_1()
          .border_color(theme.colors.border)
          .child(metadata(meta, cx)),
      )
    })
    .into_any_element()
}

fn footer(this: &Launcher, page: &Page, cx: &mut Context<Launcher>) -> impl IntoElement {
  let theme = cx.theme();
  let muted = theme.colors.muted_foreground;
  let status: SharedString = if this.busy > 0 {
    t!("app.launcher.working").into()
  } else {
    match &page.body {
      Body::Root | Body::List(_) => page
        .selected_row()
        .and_then(|r| r.meta.subtitle.clone())
        .unwrap_or_else(|| t!("app.launcher.results", count = page.rows.len()).into()),
      Body::Form(_) => t!("app.launcher.form_status").into(),
      _ => page.title.clone().unwrap_or_default(),
    }
  };
  let primary: Option<SharedString> = match &page.body {
    Body::Root | Body::List(_) => page
      .selected_row()
      .filter(|r| this.inline_args(r).is_ok())
      .and_then(|r| r.command.actions(cx).into_iter().next())
      .map(|a| a.title),
    Body::Detail(detail) => detail.actions.first().map(|a| a.title.clone()),
    Body::Form(form) => Some(form.read(cx).submit_label().clone()),
    Body::Custom { .. } => None,
  };
  let is_form = matches!(page.body, Body::Form(_));
  div()
    .h(px(style::FOOTER_HEIGHT))
    .flex_none()
    .px(px(14.))
    .flex()
    .items_center()
    .gap_2()
    .bg(style::footer_fill())
    .border_t_1()
    .border_color(theme.colors.border)
    .text_size(px(12.5))
    .child(
      div()
        .flex_1()
        .min_w_0()
        .text_color(muted)
        .whitespace_nowrap()
        .text_ellipsis()
        .child(status),
    )
    .when_some(primary, |el, primary| {
      el.child(div().child(primary))
        .child(style::kbd(if is_form { "ctrl-enter" } else { "enter" }, cx))
        .child(div().w(px(1.)).h(px(16.)).mx_1().bg(theme.colors.border))
    })
    .child(
      div()
        .text_color(muted)
        .child(t!("app.launcher.actions").to_string()),
    )
    .child(style::kbd("ctrl-o", cx))
}

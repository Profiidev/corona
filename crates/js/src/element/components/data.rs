//! Data display: lists, tables, trees, charts, calendars, carousels, progress, avatars,
//! badges, tags and empty states.
//!
//! [`BASE`] holds the unstyled parts of `corona/base` (`Progress`, `Avatar`, `Table` and their
//! parts), [`FAMILY`] the styled widgets of `corona/component`; `CalendarState` is in both.
//! Widgets that are not `Styled` sit in a styled `div` ([`W`]). Parts another element takes
//! whole (`AvatarImage`, `EmptyTitle`, `TreeItem`, ..) are passed to its slot method, not to
//! `child`. `List` and `DataTable` render their rows lazily through view slots, like `list()`.

use std::any::{Any, TypeId};

use gpui_kit::{
  AnyElement, App, AppContext, Div, Entity, ImageSource, IntoElement, ParentElement, Refineable,
  RenderOnce, SharedString, StyleRefinement, Styled, WeakEntity, Window,
  base::{
    self, AvatarFallback, AvatarImage, CalendarEvent, CalendarState, Date, ProgressIndicator,
    ProgressTrack, TableBody, TableCaption, TableCell, TableHead, TableHeader, TableRow,
  },
  component::{
    Icon, IconName, IndexPath, Sizable, Size,
    avatar::Avatar,
    badge::Badge,
    calendar::Calendar,
    carousel::{
      Carousel, CarouselContent, CarouselEvent, CarouselItem, CarouselNext, CarouselPagination,
      CarouselPaginationItem, CarouselPrevious, CarouselState,
    },
    chart::{AreaChart, BarChart, LineChart, PieChart, RadarChart},
    description_list::{DescriptionItem, DescriptionList},
    empty::{
      Empty, EmptyContent, EmptyDescription, EmptyHeader, EmptyMedia, EmptyMediaVariant, EmptyTitle,
    },
    list::{List, ListDelegate, ListItem, ListState},
    progress::Progress,
    skeleton::Skeleton,
    spinner::Spinner,
    table::{self, Column, DataTable, TableDelegate, TableEvent, TableState},
    tag::{Tag, TagVariant},
    tree::{Tree, TreeItem, TreeState},
  },
  div, img, px, radians,
};
use rquickjs::{
  Array, Class, Ctx, Exception, FromJs, Function, IntoJs, JsLifetime, Object, Result, Value,
  class::Trace,
};

use super::{Family, SizeArg, constructor};
use crate::{
  Child, Color, El, Named, Node, Opt, Retained, ScriptView, TsType, app, frame_or_throw, map,
  new_el,
  runtime::runtime,
  view::{kit::resolve_image_path, window_or_throw},
};

/// A widget that is not `Styled`, or takes typed children, in a styled, interactive `div`
pub(crate) struct W<T> {
  kind: &'static str,
  div: Div,
  inner: T,
  add: Option<fn(&mut T, AnyElement)>,
}

impl<T> W<T> {
  fn new(kind: &'static str, inner: T) -> Self {
    Self {
      kind,
      div: div(),
      inner,
      add: None,
    }
  }

  fn map(self, f: impl FnOnce(T) -> T) -> Self {
    Self {
      inner: f(self.inner),
      ..self
    }
  }
}

impl<T: Sizable> Sizable for W<T> {
  fn with_size(self, size: impl Into<Size>) -> Self {
    self.map(|t| t.with_size(size))
  }
}

impl<T: IntoElement + 'static> Node for W<T> {
  fn kind(&self) -> &'static str {
    self.kind
  }
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    Some(Styled::style(&mut self.div))
  }
  fn interactivity(&mut self) -> Option<&mut gpui_kit::Interactivity> {
    Some(gpui_kit::InteractiveElement::interactivity(&mut self.div))
  }
  fn add_child(&mut self, child: AnyElement) -> bool {
    self.add.map(|add| add(&mut self.inner, child)).is_some()
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    self.div.child(self.inner).into_any_element()
  }
  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }
}

fn add<T: ParentElement>(t: &mut T, child: AnyElement) {
  t.extend([child]);
}

/// Data another element takes whole, drawing nothing by itself: `DescriptionItem`, `TreeItem`
pub(crate) struct Data<T>(&'static str, T);

impl<T: 'static> Node for Data<T> {
  fn kind(&self) -> &'static str {
    self.0
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    div().into_any_element()
  }
  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }
}

crate::node!(base::Progress, "Progress", style, interactive, parent);
crate::node!(gpui_kit::Empty, "Empty");
crate::node!(ProgressTrack, "ProgressTrack", style, parent);
crate::node!(ProgressIndicator, "ProgressIndicator", style, parent);
crate::node!(base::Avatar, "Avatar", style, interactive);
crate::node!(AvatarImage, "AvatarImage", style, interactive);
crate::node!(AvatarFallback, "AvatarFallback", style, parent);
crate::node!(base::Table, "Table", style, interactive, parent);
crate::node!(TableHeader, "TableHeader", style, interactive, parent);
crate::node!(TableBody, "TableBody", style, interactive, parent);
crate::node!(TableRow, "TableRow", style, interactive, parent);
crate::node!(TableHead, "TableHead", style, interactive, parent);
crate::node!(TableCell, "TableCell", style, interactive, parent);
crate::node!(TableCaption, "TableCaption", style, interactive, parent);

crate::node!(Tag, "Tag", style, parent);
crate::node!(Skeleton, "Skeleton", style);
crate::node!(Empty, "Empty", style, parent);
crate::node!(EmptyHeader, "EmptyHeader", style);
crate::node!(EmptyMedia, "EmptyMedia", style, parent);
crate::node!(EmptyTitle, "EmptyTitle", style, parent);
crate::node!(EmptyDescription, "EmptyDescription", style, parent);
crate::node!(EmptyContent, "EmptyContent", style, parent);
crate::node!(Icon, "Icon", style);
crate::node!(Progress, "Progress", style);
crate::node!(Avatar, "Avatar", style, interactive);
crate::node!(Calendar, "Calendar", style);
crate::node!(Carousel, "Carousel", style, parent);
crate::node!(CarouselContent, "CarouselContent", style, parent);
crate::node!(CarouselItem, "CarouselItem", style, parent);
crate::node!(CarouselPrevious, "CarouselPrevious", style, parent);
crate::node!(CarouselNext, "CarouselNext", style, parent);
crate::node!(CarouselPagination, "CarouselPagination", style, parent);
crate::node!(
  CarouselPaginationItem,
  "CarouselPaginationItem",
  style,
  parent
);

crate::node!(table::Table, "Table", style);
crate::node!(table::TableHeader, "TableHeader", style);
crate::node!(table::TableBody, "TableBody", style);
crate::node!(table::TableFooter, "TableFooter", style);
crate::node!(table::TableRow, "TableRow", style);
crate::node!(table::TableHead, "TableHead", style, parent);
crate::node!(table::TableCell, "TableCell", style, parent);
crate::node!(table::TableCaption, "TableCaption", style, parent);

/// `child` of the parents taking typed children, typed as the generic one
#[allow(dead_code)]
mod children {
  use super::*;

  crate::el_methods! {
    fn child(child: Child<'js>) for ["Table", "TableHeader", "TableBody", "TableFooter", "TableRow", "Tree", "TreeItem", "DescriptionList", "Empty"] |c, n| typed(c, n, child);
    fn children(children: Vec<Child<'js>>) for ["Table", "TableHeader", "TableBody", "TableFooter", "TableRow", "Tree", "TreeItem", "DescriptionList", "Empty"] |c, n| {
      children.into_iter().try_for_each(|child| typed(c, n, child))
    };
  }

  /// Takes a child as a `T` when it is one
  fn take<T: Node>(c: &Ctx<'_>, child: &Child<'_>) -> Result<Option<T>> {
    let Child::El(el) = child else {
      return Ok(None);
    };
    let frame = frame_or_throw(c)?;
    let node = frame.take(c, &el.borrow())?;
    match node.into_any().downcast::<T>() {
      Ok(t) => Ok(Some(*t)),
      Err(node) => {
        drop(node);
        Ok(None)
      }
    }
  }

  /// Whether the child element is a `T`, leaving it in the frame
  fn holds<T: Node>(c: &Ctx<'_>, child: &Child<'_>) -> Result<bool> {
    let Child::El(el) = child else {
      return Ok(false);
    };
    let mut is_t = false;
    crate::with_node(el, |_, n| {
      is_t = is::<T>(&**n);
      Ok(())
    })?;
    let _ = c;
    Ok(is_t)
  }

  fn typed(c: &Ctx<'_>, n: &mut Box<dyn Node>, child: Child<'_>) -> Result<()> {
    macro_rules! adopt {
      ($parent:ty: $($child:ty => $add:expr),+ ; $what:literal) => {
        if is::<$parent>(&**n) {
          $(if holds::<$child>(c, &child)? {
            let child: $child = take(c, &child)?.expect("checked");
            return map(c, n, |p: $parent| $add(p, child));
          })+
          return Err(Exception::throw_type(c, $what));
        }
      };
    }
    adopt!(table::Table:
      table::TableHeader => |p: table::Table, c| p.child(c),
      table::TableBody => |p: table::Table, c| p.child(c),
      table::TableFooter => |p: table::Table, c| p.child(c),
      table::TableCaption => |p: table::Table, c| p.child(c);
      "a Table takes TableHeader, TableBody, TableFooter and TableCaption children");
    adopt!(table::TableHeader: table::TableRow => |p: table::TableHeader, c| p.child(c);
      "a TableHeader takes TableRow children");
    adopt!(table::TableBody: table::TableRow => |p: table::TableBody, c| p.child(c);
      "a TableBody takes TableRow children");
    adopt!(table::TableFooter: table::TableRow => |p: table::TableFooter, c| p.child(c);
      "a TableFooter takes TableRow children");
    adopt!(table::TableRow:
      table::TableHead => |p: table::TableRow, c| p.child(c),
      table::TableCell => |p: table::TableRow, c| p.child(c);
      "a TableRow takes TableHead and TableCell children");
    adopt!(TreeEl: Data<TreeItem> => |t: TreeEl, i: Data<TreeItem>| {
      let mut t = t;
      t.items.push(i.1);
      t
    }; "a Tree takes TreeItem children");
    adopt!(Data<TreeItem>: Data<TreeItem> => |t: Data<TreeItem>, i: Data<TreeItem>| {
      Data(t.0, t.1.child(i.1))
    }; "a TreeItem takes TreeItem children");
    adopt!(W<DescriptionList>: Data<DescriptionItem> => |d: W<DescriptionList>, i: Data<DescriptionItem>| {
      d.map(|d| d.child(i.1))
    }; "a DescriptionList takes DescriptionItem children");
    if is::<Empty>(&**n) {
      if holds::<EmptyHeader>(c, &child)? {
        let header: EmptyHeader = take(c, &child)?.expect("checked");
        return map(c, n, |e: Empty| e.header(header));
      }
      if holds::<EmptyContent>(c, &child)? {
        let content: EmptyContent = take(c, &child)?.expect("checked");
        return map(c, n, |e: Empty| e.content(content));
      }
    }
    Err(crate::miss(c, n.kind()))
  }
}

/// Whether the node is a `T`
fn is<T: 'static>(n: &dyn Node) -> bool {
  (n as &dyn Any).type_id() == TypeId::of::<T>()
}

/// Applies the body of the type the node is, or throws: one method for several widgets.
macro_rules! any {
  ($c:expr, $n:expr, $($ty:ty => |$x:ident| $body:expr),+ $(,)?) => {{
    let (c, n) = ($c, $n);
    $(if is::<$ty>(&**n) { map(c, n, |$x: $ty| $body) } else)+ {
      Err(crate::miss(c, n.kind()))
    }
  }};
}

/// A string argument out of a fixed set, typed as their union
macro_rules! choice {
  ($name:ident($ty:ty) $what:literal { $($js:literal => $val:expr),+ $(,)? }) => {
    pub(crate) struct $name($ty);

    impl<'js> FromJs<'js> for $name {
      fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        let name = String::from_js(ctx, value)?;
        Ok(Self(match name.as_str() {
          $($js => $val,)+
          _ => {
            let known = &[$($js),+];
            return Err(Exception::throw_type(ctx, &crate::element::style::unknown($what, &name, known)));
          }
        }))
      }
    }

    impl TsType for $name {
      fn ts() -> String {
        [$(concat!("\"", $js, "\"")),+].join(" | ")
      }
    }
  };
}

choice!(Sz(Size) "size" {
  "xsmall" => Size::XSmall,
  "small" => Size::Small,
  "medium" => Size::Medium,
  "large" => Size::Large,
});

/// A `Tag` or `EmptyMedia` variant
pub(crate) enum Variant {
  Tag(TagVariant),
  Media(EmptyMediaVariant),
}

choice!(Var(Variant) "variant" {
  "primary" => Variant::Tag(TagVariant::Primary),
  "secondary" => Variant::Tag(TagVariant::Secondary),
  "danger" => Variant::Tag(TagVariant::Danger),
  "success" => Variant::Tag(TagVariant::Success),
  "warning" => Variant::Tag(TagVariant::Warning),
  "info" => Variant::Tag(TagVariant::Info),
  "default" => Variant::Media(EmptyMediaVariant::Default),
  "icon" => Variant::Media(EmptyMediaVariant::Icon),
});

choice!(SpinnerIcon(IconName) "spinner icon" {
  "loader" => IconName::Loader,
  "loader_circle" => IconName::LoaderCircle,
});

choice!(Ease(fn(f32) -> f32) "easing" {
  "linear" => gpui_kit::linear,
  "ease_in_out" => gpui_kit::ease_in_out,
  "ease_out_quint" => |t| gpui_kit::ease_out_quint()(t),
});

choice!(Axis(gpui_kit::Axis) "axis" {
  "horizontal" => gpui_kit::Axis::Horizontal,
  "vertical" => gpui_kit::Axis::Vertical,
});

/// An element another takes whole, typed as `Element`
pub(crate) struct Part<'js>(Class<'js, El>);

impl<'js> FromJs<'js> for Part<'js> {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    Class::from_js(ctx, value).map(Self)
  }
}

impl TsType for Part<'_> {
  fn ts() -> String {
    "Element".into()
  }
}

impl Part<'_> {
  /// Moves the element out of the render as a `T`, or throws.
  fn take<T: Node>(self, ctx: &Ctx<'_>, expected: &str) -> Result<T> {
    let node = frame_or_throw(ctx)?.take(ctx, &self.0.borrow())?;
    let kind = node.kind();
    match node.into_any().downcast::<T>() {
      Ok(t) => Ok(*t),
      Err(_) => Err(Exception::throw_type(
        ctx,
        &format!("expected {expected}, got {kind}"),
      )),
    }
  }
}

pub(crate) struct Parts<'js>(Vec<Part<'js>>);

impl<'js> FromJs<'js> for Parts<'js> {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    Vec::from_js(ctx, value).map(Self)
  }
}

impl TsType for Parts<'_> {
  fn ts() -> String {
    "Element[]".into()
  }
}

impl Parts<'_> {
  fn take<T: Node>(self, ctx: &Ctx<'_>, expected: &str) -> Result<Vec<T>> {
    self.0.into_iter().map(|p| p.take(ctx, expected)).collect()
  }
}

/// A number or a string, for `value`
pub(crate) enum NumStr {
  Num(f32),
  Str(String),
}

impl<'js> FromJs<'js> for NumStr {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    match value.as_number() {
      Some(n) => Ok(Self::Num(n as f32)),
      None => String::from_js(ctx, value).map(Self::Str),
    }
  }
}

impl TsType for NumStr {
  fn ts() -> String {
    "number | string".into()
  }
}

/// Rows: an array, or a function returning one
pub(crate) struct Rows<'js>(Array<'js>);

impl<'js> FromJs<'js> for Rows<'js> {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    let value = match value.as_function() {
      Some(f) => f.call(())?,
      None => value,
    };
    Array::from_js(ctx, value)
      .map(Self)
      .map_err(|_| Exception::throw_type(ctx, "expected an array of rows"))
  }
}

/// An image file: an absolute path under a read grant, or a path in the plugin's directory
fn source(ctx: &Ctx<'_>, path: String) -> Result<ImageSource> {
  Ok(resolve_image_path(ctx, "image", path)?.into())
}

/// One chart datum, `{ label, value }`
pub(crate) struct Row {
  label: SharedString,
  value: f64,
}

fn chart_rows<'js>(ctx: &Ctx<'js>, rows: Rows<'js>) -> Result<Vec<Row>> {
  (rows.0.iter::<Value>())
    .map(|row| {
      let row = Object::from_js(ctx, row?)?;
      let label: Option<String> = row.get("label")?;
      let value: Option<f64> = row.get("value")?;
      match (label, value) {
        (Some(label), Some(value)) => Ok(Row {
          label: label.into(),
          value,
        }),
        _ => Err(Exception::throw_type(
          ctx,
          "a chart row is { label: string, value: number }",
        )),
      }
    })
    .collect()
}

type Bar = W<BarChart<Row, SharedString, f64>>;
type Line = W<LineChart<Row, SharedString, f64>>;
type Area = W<AreaChart<Row, SharedString, f64>>;
type Pie = W<PieChart<Row>>;
type Radar = W<RadarChart<Row, f64>>;

/// A chart filling a `div` the script sizes
fn chart<'js, T: IntoElement + 'static>(
  ctx: &Ctx<'js>,
  kind: &'static str,
  chart: T,
) -> Result<Class<'js, El>> {
  let mut w = W::new(kind, chart);
  w.div = w.div.size_full();
  new_el(ctx, w)
}

/// Where a lazy row renders: the view, and its slot holding the row function
type Slot = Option<(WeakEntity<ScriptView>, u64)>;

/// Keeps `render(index, cx)` for this render of the view.
fn slot<'js>(ctx: &Ctx<'js>, render: Function<'js>) -> Result<Slot> {
  let view = frame_or_throw(ctx)?.view.clone();
  let key = runtime(ctx)?.slot(view.entity_id(), render, true);
  Ok(Some((view, key)))
}

/// Renders row `index` of `slot`.
fn render_row(slot: &Slot, index: usize, window: &mut Window, cx: &mut App) -> AnyElement {
  let Some((view, key)) = slot.as_ref().and_then(|(v, k)| Some((v.upgrade()?, *k))) else {
    return div().into_any_element();
  };
  view
    .update(cx, |view, cx| {
      let args = |a: &mut rquickjs::function::Args<'_>| a.push_arg(index);
      view.render_content(key, Some(index), window, cx, args, |ctx, frame, value| {
        frame.child(ctx, Child::from_js(ctx, value)?)
      })
    })
    .unwrap_or_else(|e| format!("render failed: {e}").into_any_element())
}

/// The rows of a `List`
pub(crate) struct Rows2 {
  slot: Slot,
  count: usize,
  selected: Option<IndexPath>,
}

impl ListDelegate for Rows2 {
  type Item = ListItem;

  fn items_count(&self, section: usize, _: &App) -> usize {
    if section == 0 { self.count } else { 0 }
  }

  fn render_item(
    &mut self,
    ix: IndexPath,
    window: &mut Window,
    cx: &mut gpui_kit::Context<ListState<Self>>,
  ) -> Option<ListItem> {
    let row = render_row(&self.slot, ix.row, window, cx);
    Some(
      ListItem::new(ix.row)
        .selected(self.selected == Some(ix))
        .child(row),
    )
  }

  fn set_selected_index(
    &mut self,
    ix: Option<IndexPath>,
    _: &mut Window,
    _: &mut gpui_kit::Context<ListState<Self>>,
  ) {
    self.selected = ix;
  }
}

/// `List(id, rows, render_row)`: its state is kept by `id`, across renders
#[derive(IntoElement)]
pub(crate) struct ListEl {
  id: SharedString,
  slot: Slot,
  count: usize,
  style: StyleRefinement,
}

impl Node for ListEl {
  fn kind(&self) -> &'static str {
    "List"
  }
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    Some(&mut self.style)
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    (*self).into_any_element()
  }
  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }
}

impl RenderOnce for ListEl {
  fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let rows = Rows2 {
      slot: None,
      count: 0,
      selected: None,
    };
    let state = window.use_keyed_state(self.id, cx, |window, cx| ListState::new(rows, window, cx));
    state.update(cx, |state, cx| {
      let rows = state.delegate_mut();
      (rows.slot, rows.count) = (self.slot, self.count);
      cx.notify();
    });
    let mut list = List::new(&state);
    Styled::style(&mut list).refine(&self.style);
    list
  }
}

/// The rows of a `DataTable`
pub(crate) struct Cells {
  slot: Slot,
  rows: usize,
  columns: Vec<Column>,
}

impl TableDelegate for Cells {
  fn columns_count(&self, _: &App) -> usize {
    self.columns.len()
  }

  fn rows_count(&self, _: &App) -> usize {
    self.rows
  }

  fn column(&self, col_ix: usize, _: &App) -> Column {
    self.columns[col_ix].clone()
  }

  fn render_td(
    &mut self,
    row_ix: usize,
    col_ix: usize,
    window: &mut Window,
    cx: &mut gpui_kit::Context<TableState<Self>>,
  ) -> impl IntoElement {
    render_row(&self.slot, row_ix * self.columns.len() + col_ix, window, cx)
  }
}

/// `DataTable(state, rows, render_cell)`
pub(crate) struct DataTableEl {
  w: W<DataTable<Cells>>,
  state: Entity<TableState<Cells>>,
}

impl DataTableEl {
  fn state(self, f: impl FnOnce(&mut TableState<Cells>)) -> Self {
    self.state.update(app(), |s, cx| {
      f(s);
      cx.notify();
    });
    self
  }
}

impl Node for DataTableEl {
  fn kind(&self) -> &'static str {
    "DataTable"
  }
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    self.w.style()
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    Box::new(self.w).into_element()
  }
  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }
}

/// `Tree(id)`: its state is kept by `id`, and keeps what is expanded while the items stay
#[derive(IntoElement)]
pub(crate) struct TreeEl {
  id: SharedString,
  items: Vec<TreeItem>,
  style: StyleRefinement,
}

/// The tree state of a `Tree`, and the items it was made from
struct Kept {
  state: Entity<TreeState>,
  print: String,
}

/// Ids and labels, nested: what tells new items from the kept ones
fn print(items: &[TreeItem]) -> String {
  let item = |i: &TreeItem| format!("{}:{}[{}]", i.id, i.label, print(&i.children));
  items.iter().map(item).collect::<Vec<_>>().join(",")
}

impl Node for TreeEl {
  fn kind(&self) -> &'static str {
    "Tree"
  }
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    Some(&mut self.style)
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    (*self).into_any_element()
  }
  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }
}

impl RenderOnce for TreeEl {
  fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    let print = print(&self.items);
    let items = self.items.clone();
    let kept = window.use_keyed_state(self.id, cx, |_, cx| Kept {
      state: cx.new(|cx| TreeState::new(cx).items(items)),
      print: print.clone(),
    });
    let state = kept.update(cx, |kept, cx| {
      if kept.print != print {
        kept.print = print;
        kept.state.update(cx, |s, cx| s.set_items(self.items, cx));
      }
      kept.state.clone()
    });
    let mut tree = Tree::new(&state, |ix, entry, selected, _, _| {
      ListItem::new(ix)
        .selected(selected)
        .pl(px(12. * entry.depth() as f32))
        .child(entry.item().label.clone())
    });
    Styled::style(&mut tree).refine(&self.style);
    tree
  }
}

/// A calendar date: `"YYYY-MM-DD"`, a `[start, end]` range of them, or `null`
pub(crate) struct JsDate(Date);

impl<'js> IntoJs<'js> for JsDate {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    let day = |d: Option<String>| match d {
      Some(d) => d.into_js(ctx),
      None => Ok(Value::new_null(ctx.clone())),
    };
    fn text<D: ToString>(d: Option<D>) -> Option<String> {
      d.map(|d| d.to_string())
    }
    match self.0 {
      Date::Single(d) => day(text(d)),
      Date::Range(a, b) => vec![day(text(a))?, day(text(b))?].into_js(ctx),
    }
  }
}

impl<'js> FromJs<'js> for JsDate {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    let day = |v: Value<'js>| {
      if v.is_null() || v.is_undefined() {
        return Ok(None);
      }
      let text = String::from_js(ctx, v)?;
      text.parse().map(Some).map_err(|_| {
        Exception::throw_type(
          ctx,
          &format!("invalid date \"{text}\", expected YYYY-MM-DD"),
        )
      })
    };
    Ok(JsDate(match value.as_array() {
      Some(a) => Date::Range(day(a.get(0)?)?, day(a.get(1)?)?),
      None => Date::Single(day(value)?),
    }))
  }
}

/// `CalendarState`: a month grid and the selected date, for `Calendar` or drawn by the script
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "CalendarState")]
pub(crate) struct JsCalendarState {
  #[qjs(skip_trace)]
  state: Retained<CalendarState>,
}

impl Named for JsCalendarState {
  const TS: &'static str = "CalendarState";
}

impl JsCalendarState {
  fn read<R>(&self, ctx: &Ctx<'_>, f: impl FnOnce(&CalendarState) -> R) -> Result<R> {
    Ok(f(self.state.entity(ctx)?.read(app())))
  }

  fn update(&self, ctx: &Ctx<'_>, f: impl FnOnce(&mut CalendarState)) -> Result<()> {
    self.state.entity(ctx)?.update(app(), |s, cx| {
      f(s);
      cx.notify();
    });
    Ok(())
  }
}

#[rquickjs::methods]
impl JsCalendarState {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'_>) -> Result<Self> {
    let window = window_or_throw(&ctx)?;
    let entity = app().new(|cx| CalendarState::new(window, cx));
    Ok(Self {
      state: Retained::new(&ctx, entity)?,
    })
  }

  #[qjs(static, rename = "new")]
  pub fn create(ctx: Ctx<'_>) -> Result<Self> {
    Self::new(ctx)
  }

  pub fn month_days(&self, ctx: Ctx<'_>) -> Result<Vec<Vec<Vec<String>>>> {
    self.read(&ctx, |s| {
      let days = |w: &Vec<_>| w.iter().map(ToString::to_string).collect();
      let month = |m: &Vec<Vec<_>>| m.iter().map(days).collect();
      s.month_days().iter().map(month).collect()
    })
  }

  pub fn year(&self, ctx: Ctx<'_>) -> Result<i32> {
    self.read(&ctx, |s| s.current_year())
  }

  pub fn month(&self, ctx: Ctx<'_>) -> Result<u32> {
    self.read(&ctx, |s| s.current_month().into())
  }

  pub fn today(&self, ctx: Ctx<'_>) -> Result<String> {
    self.read(&ctx, |s| s.today().to_string())
  }

  pub fn value(&self, ctx: Ctx<'_>) -> Result<JsDate> {
    self.read(&ctx, |s| JsDate(s.date()))
  }

  pub fn set_value(&self, ctx: Ctx<'_>, next: JsDate) -> Result<()> {
    self.update(&ctx, |s| {
      s.apply_date(next.0);
    })
  }

  pub fn next_month(&self, ctx: Ctx<'_>) -> Result<()> {
    self.update(&ctx, CalendarState::next_month)
  }

  pub fn prev_month(&self, ctx: Ctx<'_>) -> Result<()> {
    self.update(&ctx, CalendarState::prev_month)
  }

  pub fn on<'js>(&mut self, ctx: Ctx<'js>, name: String, f: Function<'js>) -> Result<bool> {
    (self.state).on(&ctx, &["change"], name, f, |e: &CalendarEvent, _| {
      let CalendarEvent::Selected(date) = e;
      Some(("change", JsDate(*date)))
    })
  }

  pub fn release(&mut self) -> bool {
    self.state.release()
  }
}

/// `CarouselState`: which slide a `Carousel` shows
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "CarouselState")]
pub(crate) struct JsCarouselState {
  #[qjs(skip_trace)]
  state: Retained<CarouselState>,
}

impl Named for JsCarouselState {
  const TS: &'static str = "CarouselState";
}

impl JsCarouselState {
  fn update<R>(
    &self,
    ctx: &Ctx<'_>,
    f: impl FnOnce(&mut CarouselState, &mut gpui_kit::Context<CarouselState>) -> R,
  ) -> Result<R> {
    Ok(self.state.entity(ctx)?.update(app(), f))
  }
}

#[rquickjs::methods]
impl JsCarouselState {
  #[qjs(constructor)]
  pub fn new(
    ctx: Ctx<'_>,
    item_count: u32,
    selected_index: Opt<u32>,
    axis: Opt<Axis>,
    looping: Opt<bool>,
  ) -> Result<Self> {
    let mut state = CarouselState::new(crate::element::items(&ctx, item_count)?);
    if let Some(i) = selected_index.0 {
      state = state.with_selected_index(i as usize);
    }
    if let Some(axis) = axis.0 {
      state = state.with_axis(axis.0);
    }
    if let Some(looping) = looping.0 {
      state = state.with_looping(looping);
    }
    let entity = app().new(|_| state);
    Ok(Self {
      state: Retained::new(&ctx, entity)?,
    })
  }

  #[qjs(static, rename = "new")]
  pub fn create(
    ctx: Ctx<'_>,
    item_count: u32,
    selected_index: Opt<u32>,
    axis: Opt<Axis>,
    looping: Opt<bool>,
  ) -> Result<Self> {
    Self::new(ctx, item_count, selected_index, axis, looping)
  }

  pub fn item_count(&self, ctx: Ctx<'_>) -> Result<u32> {
    self.update(&ctx, |s, _| s.item_count() as u32)
  }

  pub fn selected_index(&self, ctx: Ctx<'_>) -> Result<Option<u32>> {
    self.update(&ctx, |s, _| s.selected_index().map(|i| i as u32))
  }

  /// Selects a slide without a `change` event.
  pub fn set_selected_index(&self, ctx: Ctx<'_>, index: u32) -> Result<()> {
    self.update(&ctx, |s, cx| s.set_selected_index(index as usize, cx))
  }

  pub fn set_item_count(&self, ctx: Ctx<'_>, count: u32) -> Result<()> {
    let count = crate::element::items(&ctx, count)?;
    self.update(&ctx, |s, cx| s.set_item_count(count, cx))
  }

  /// Moves to the next slide, with a `change` event; false at the end.
  pub fn select_next(&self, ctx: Ctx<'_>) -> Result<bool> {
    self.update(&ctx, CarouselState::select_next)
  }

  pub fn select_previous(&self, ctx: Ctx<'_>) -> Result<bool> {
    self.update(&ctx, CarouselState::select_previous)
  }

  pub fn on<'js>(&mut self, ctx: Ctx<'js>, name: String, f: Function<'js>) -> Result<bool> {
    (self.state).on(&ctx, &["change"], name, f, |e: &CarouselEvent, _| {
      let CarouselEvent::Change(index) = e;
      Some(("change", *index as u32))
    })
  }

  pub fn release(&mut self) -> bool {
    self.state.release()
  }
}

/// `DataTableState`: the columns, selection and scroll of a `DataTable`
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "DataTableState")]
pub(crate) struct JsDataTableState {
  #[qjs(skip_trace)]
  state: Retained<TableState<Cells>>,
}

impl Named for JsDataTableState {
  const TS: &'static str = "DataTableState";
}

#[rquickjs::methods]
impl JsDataTableState {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'_>, columns: Vec<String>) -> Result<Self> {
    let window = window_or_throw(&ctx)?;
    let cells = Cells {
      slot: None,
      rows: 0,
      columns: columns
        .iter()
        .map(|c| Column::new(c.clone(), c.clone()))
        .collect(),
    };
    let entity = app().new(|cx| TableState::new(cells, window, cx));
    Ok(Self {
      state: Retained::new(&ctx, entity)?,
    })
  }

  #[qjs(static, rename = "new")]
  pub fn create(ctx: Ctx<'_>, columns: Vec<String>) -> Result<Self> {
    Self::new(ctx, columns)
  }

  pub fn on<'js>(&mut self, ctx: Ctx<'js>, name: String, f: Function<'js>) -> Result<bool> {
    let names = &["select_row", "double_click_row", "select_column"];
    (self.state).on(&ctx, names, name, f, |e: &TableEvent, _| match e {
      TableEvent::SelectRow(i) => Some(("select_row", *i as u32)),
      TableEvent::DoubleClickedRow(i) => Some(("double_click_row", *i as u32)),
      TableEvent::SelectColumn(i) => Some(("select_column", *i as u32)),
      _ => None,
    })
  }

  pub fn release(&mut self) -> bool {
    self.state.release()
  }
}

/// The entity of a state argument
fn entity<T: 'static>(ctx: &Ctx<'_>, state: &Retained<T>) -> Result<Entity<T>> {
  state.entity(ctx).cloned()
}

fn list<'js>(
  ctx: Ctx<'js>,
  id: String,
  rows: Rows<'js>,
  render: Function<'js>,
) -> Result<Class<'js, El>> {
  let wrap: Function = ctx.eval("(rows, f) => (i, cx) => f(rows[i], cx)")?;
  let count = rows.0.len();
  let render: Function = wrap.call((rows.0, render))?;
  let el = ListEl {
    id: id.into(),
    slot: slot(&ctx, render)?,
    count,
    style: StyleRefinement::default(),
  };
  new_el(&ctx, el)
}

fn data_table<'js>(
  ctx: Ctx<'js>,
  state: Class<'js, JsDataTableState>,
  rows: Rows<'js>,
  render: Function<'js>,
) -> Result<Class<'js, El>> {
  let state = entity(&ctx, &state.borrow().state)?;
  let keys: Vec<String> = (state.read(app()).delegate().columns)
    .iter()
    .map(|c| c.key.to_string())
    .collect();
  let wrap: Function = ctx.eval(
    "(rows, keys, f) => (i, cx) => f(rows[Math.floor(i / keys.length)], keys[i % keys.length], cx)",
  )?;
  let count = rows.0.len();
  let slot = slot(&ctx, wrap.call((rows.0, keys, render))?)?;
  state.update(app(), |s, cx| {
    let cells = s.delegate_mut();
    (cells.slot, cells.rows) = (slot, count);
    cx.notify();
  });
  let mut w = W::new("DataTable", DataTable::new(&state));
  w.div = w.div.size_full();
  new_el(&ctx, DataTableEl { w, state })
}

fn exports<'js>(ctx: &Ctx<'js>) -> Result<Vec<(&'static str, Value<'js>)>> {
  macro_rules! c {
    ($name:literal, $f:expr) => {
      (
        $name,
        constructor(ctx, $name, Function::new(ctx.clone(), $f)?)?,
      )
    };
  }
  macro_rules! state {
    ($ty:ty, |$ctx:ident $(, $arg:ident: $t:ty)*| $make:expr) => {
      constructor(ctx, stringify!($ty), Function::new(ctx.clone(), |$ctx: Ctx<'js> $(, $arg: $t)*| {
        Class::instance($ctx.clone(), $make?)
      })?)?
    };
  }
  macro_rules! carousel {
    ($ty:ident) => {
      |ctx: Ctx<'js>, s: Class<'js, JsCarouselState>| {
        new_el(&ctx, $ty::new(&entity(&ctx, &s.borrow().state)?))
      }
    };
    ($ty:ident, indexed) => {
      |ctx: Ctx<'js>, id: crate::Id, index: u32, s: Class<'js, JsCarouselState>| {
        let state = entity(&ctx, &s.borrow().state)?;
        new_el(&ctx, $ty::new(id.into_inner(), index as usize, &state))
      }
    };
  }
  Ok(vec![
    c!("Badge", |ctx: Ctx<'js>| {
      let mut w = W::new("Badge", Badge::new());
      w.add = Some(add::<Badge>);
      new_el(&ctx, w)
    }),
    c!("Tag", |ctx: Ctx<'js>| new_el(&ctx, Tag::new())),
    c!("Skeleton", |ctx: Ctx<'js>| new_el(&ctx, Skeleton::new())),
    c!("Spinner", |ctx: Ctx<'js>| new_el(
      &ctx,
      W::new("Spinner", Spinner::new())
    )),
    c!("Empty", |ctx: Ctx<'js>| new_el(&ctx, Empty::new())),
    c!("EmptyHeader", |ctx: Ctx<'js>| new_el(
      &ctx,
      EmptyHeader::new()
    )),
    c!("EmptyMedia", |ctx: Ctx<'js>| new_el(
      &ctx,
      EmptyMedia::new()
    )),
    c!("EmptyTitle", |ctx: Ctx<'js>| new_el(
      &ctx,
      EmptyTitle::new()
    )),
    c!("EmptyDescription", |ctx: Ctx<'js>| new_el(
      &ctx,
      EmptyDescription::new()
    )),
    c!("EmptyContent", |ctx: Ctx<'js>| new_el(
      &ctx,
      EmptyContent::new()
    )),
    c!("Icon", |ctx: Ctx<'js>, path: String| new_el(
      &ctx,
      Icon::empty().path(path)
    )),
    c!("Image", |ctx: Ctx<'js>, path: String| {
      let src = source(&ctx, path)?;
      new_el(&ctx, img(src))
    }),
    c!("DescriptionList", |ctx: Ctx<'js>| new_el(
      &ctx,
      W::new("DescriptionList", DescriptionList::new())
    )),
    c!("DescriptionItem", |ctx: Ctx<'js>, label: String| new_el(
      &ctx,
      Data("DescriptionItem", DescriptionItem::new(label))
    )),
    c!("Progress", |ctx: Ctx<'js>, id: crate::Id| new_el(
      &ctx,
      Progress::new(id.into_inner())
    )),
    c!("Avatar", |ctx: Ctx<'js>| new_el(&ctx, Avatar::new())),
    c!("BarChart", |ctx: Ctx<'js>, rows: Rows<'js>| {
      let rows = BarChart::new(chart_rows(&ctx, rows)?)
        .band(|r: &Row| r.label.clone())
        .value(|r: &Row| r.value)
        .label(|r: &Row| SharedString::from(r.value.to_string()));
      chart(&ctx, "BarChart", rows)
    }),
    c!("LineChart", |ctx: Ctx<'js>, rows: Rows<'js>| {
      let rows = LineChart::new(chart_rows(&ctx, rows)?)
        .x(|r: &Row| r.label.clone())
        .y(|r: &Row| r.value);
      chart(&ctx, "LineChart", rows)
    }),
    c!("AreaChart", |ctx: Ctx<'js>, rows: Rows<'js>| {
      let rows = AreaChart::new(chart_rows(&ctx, rows)?)
        .x(|r: &Row| r.label.clone())
        .y(|r: &Row| r.value);
      chart(&ctx, "AreaChart", rows)
    }),
    c!("PieChart", |ctx: Ctx<'js>, rows: Rows<'js>| {
      let rows = PieChart::new(chart_rows(&ctx, rows)?).value(|r: &Row| r.value as f32);
      chart(&ctx, "PieChart", rows)
    }),
    c!("RadarChart", |ctx: Ctx<'js>, rows: Rows<'js>| {
      let rows = RadarChart::new(chart_rows(&ctx, rows)?)
        .value(|r: &Row| r.value)
        .label(|r: &Row| r.label.clone());
      chart(&ctx, "RadarChart", rows)
    }),
    c!(
      "Calendar",
      |ctx: Ctx<'js>, s: Class<'js, JsCalendarState>| new_el(
        &ctx,
        Calendar::new(&entity(&ctx, &s.borrow().state)?)
      )
    ),
    (
      "CalendarState",
      state!(JsCalendarState, |ctx| JsCalendarState::new(ctx.clone())),
    ),
    (
      "Carousel",
      constructor(
        ctx,
        "Carousel",
        Function::new(
          ctx.clone(),
          |ctx: Ctx<'js>, id: crate::Id, s: Class<'js, JsCarouselState>| {
            let state = entity(&ctx, &s.borrow().state)?;
            new_el(&ctx, Carousel::new(id.into_inner(), &state))
          },
        )?,
      )?,
    ),
    c!("CarouselContent", carousel!(CarouselContent)),
    c!("CarouselItem", carousel!(CarouselItem, indexed)),
    c!("CarouselPrevious", carousel!(CarouselPrevious)),
    c!("CarouselNext", carousel!(CarouselNext)),
    c!("CarouselPagination", |ctx: Ctx<'js>| new_el(
      &ctx,
      CarouselPagination::new()
    )),
    c!(
      "CarouselPaginationItem",
      carousel!(CarouselPaginationItem, indexed)
    ),
    (
      "CarouselState",
      state!(
        JsCarouselState,
        |ctx, count: u32, index: Opt<u32>, axis: Opt<Axis>, looping: Opt<bool>| {
          JsCarouselState::new(ctx.clone(), count, index, axis, looping)
        }
      ),
    ),
    c!("List", list),
    c!("DataTable", data_table),
    (
      "DataTableState",
      state!(JsDataTableState, |ctx, columns: Vec<String>| {
        JsDataTableState::new(ctx.clone(), columns)
      }),
    ),
    c!("Table", |ctx: Ctx<'js>| new_el(&ctx, table::Table::new())),
    c!("TableHeader", |ctx: Ctx<'js>| new_el(
      &ctx,
      table::TableHeader::new()
    )),
    c!("TableBody", |ctx: Ctx<'js>| new_el(
      &ctx,
      table::TableBody::new()
    )),
    c!("TableFooter", |ctx: Ctx<'js>| new_el(
      &ctx,
      table::TableFooter::new()
    )),
    c!("TableRow", |ctx: Ctx<'js>| new_el(
      &ctx,
      table::TableRow::new()
    )),
    c!("TableHead", |ctx: Ctx<'js>| new_el(
      &ctx,
      table::TableHead::new()
    )),
    c!("TableCell", |ctx: Ctx<'js>| new_el(
      &ctx,
      table::TableCell::new()
    )),
    c!("TableCaption", |ctx: Ctx<'js>| new_el(
      &ctx,
      table::TableCaption::new()
    )),
    c!("Tree", |ctx: Ctx<'js>, id: String| {
      let tree = TreeEl {
        id: id.into(),
        items: Vec::new(),
        style: StyleRefinement::default(),
      };
      new_el(&ctx, tree)
    }),
    c!("TreeItem", |ctx: Ctx<'js>, id: String, label: String| {
      new_el(&ctx, Data("TreeItem", TreeItem::new(id, label)))
    }),
  ])
}

fn base_exports<'js>(ctx: &Ctx<'js>) -> Result<Vec<(&'static str, Value<'js>)>> {
  macro_rules! c {
    ($name:literal, $f:expr) => {
      (
        $name,
        constructor(ctx, $name, Function::new(ctx.clone(), $f)?)?,
      )
    };
  }
  macro_rules! part {
    ($ty:ty) => {
      |ctx: Ctx<'js>, id: crate::Id| new_el(&ctx, <$ty>::new(id.into_inner()))
    };
    ($ty:ty, indexed) => {
      |ctx: Ctx<'js>, id: crate::Id, index: u32| {
        new_el(&ctx, <$ty>::new(id.into_inner(), index as usize))
      }
    };
  }
  let calendar = Class::<JsCalendarState>::create_constructor(ctx)?.expect("constructor");
  Ok(vec![
    c!("Empty", |ctx: Ctx<'js>| new_el(&ctx, gpui_kit::Empty)),
    c!("Progress", part!(base::Progress)),
    c!("ProgressTrack", |ctx: Ctx<'js>| new_el(
      &ctx,
      ProgressTrack::new()
    )),
    c!("ProgressIndicator", |ctx: Ctx<'js>| new_el(
      &ctx,
      ProgressIndicator::new()
    )),
    c!("Avatar", |ctx: Ctx<'js>| new_el(&ctx, base::Avatar::new())),
    c!("AvatarImage", |ctx: Ctx<'js>, path: String| {
      let src = source(&ctx, path)?;
      new_el(&ctx, AvatarImage::new(src))
    }),
    c!("AvatarFallback", |ctx: Ctx<'js>| new_el(
      &ctx,
      AvatarFallback::new()
    )),
    c!("Table", part!(base::Table)),
    c!("TableHeader", part!(TableHeader)),
    c!("TableBody", part!(TableBody)),
    c!("TableRow", part!(TableRow, indexed)),
    c!("TableHead", part!(TableHead, indexed)),
    c!("TableCell", part!(TableCell, indexed)),
    c!("TableCaption", part!(TableCaption)),
    ("CalendarState", calendar.into_value()),
  ])
}

mod methods {
  use gpui_kit::Styled;

  use super::*;

  crate::el_methods! {
    /// The value of a `Progress`: a percentage (`corona/base` announces it, `corona/component`
    /// draws it, `0..=100`).
    fn value(value: f32) for ["Progress", "Progress@base"] |c, n| any!(c, n,
      base::Progress => |p| p.value(value),
      Progress => |p| p.value(value),
    );
    /// The text of a `DescriptionItem`.
    fn value(value: NumStr) for ["DescriptionItem"] |c, n| {
      let text = match value {
        NumStr::Num(v) => v.to_string(),
        NumStr::Str(v) => v,
      };
      any!(c, n, Data<DescriptionItem> => |d| Data(d.0, d.1.value(text)))
    };
    /// Withdraws a `corona/base` `Progress` value from the accessibility tree: "no idea how far".
    fn indeterminate(value: bool) for ["Progress@base"] |c, n| any!(c, n, base::Progress => |p| p.indeterminate(value));
    /// Fills an `Avatar`'s image slot with an `AvatarImage`; it renders instead of the fallback.
    fn image(image: Part<'js> as "Element<\"AvatarImage@base\">") for ["Avatar@base"] |c, n| {
      let image = image.take::<AvatarImage>(c, "an AvatarImage")?;
      any!(c, n, base::Avatar => |a| a.image(image))
    };
    /// Fills an `Avatar`'s fallback slot with an `AvatarFallback`, shown without an image.
    fn fallback(fallback: Part<'js> as "Element<\"AvatarFallback@base\">") for ["Avatar@base"] |c, n| {
      let fallback = fallback.take::<AvatarFallback>(c, "an AvatarFallback")?;
      any!(c, n, base::Avatar => |a| a.fallback(fallback))
    };
    /// A `Table`'s total number of rows, also those not rendered, as announced.
    fn row_count(count: u32) for ["Table@base"] |c, n| {
      let count = crate::element::items(c, count)?;
      any!(c, n, base::Table => |t| t.row_count(count))
    };
    /// A `Table`'s total number of columns, also those not rendered.
    fn column_count(count: u32) for ["Table@base"] |c, n| {
      any!(c, n, base::Table => |t| t.column_count(count as usize))
    };
    /// The semantic size of a styled widget.
    fn size(size: SizeArg<'js>) for ["Avatar", "Avatar@base", "Badge", "Calendar", "CarouselNext", "CarouselPaginationItem", "CarouselPrevious", "DescriptionList", "Icon", "Progress", "Progress@base", "Spinner", "Table", "Table@base", "Tag"] |c, n| {
      let size = Sz(size.get(c, &**n)?);
      any!(c, n,
        W<Badge> => |x| x.with_size(size.0),
        W<Spinner> => |x| x.with_size(size.0),
        W<DescriptionList> => |x| x.with_size(size.0),
        Progress => |x| x.with_size(size.0),
        Avatar => |x| x.with_size(size.0),
        Icon => |x| x.with_size(size.0),
        Tag => |x| x.with_size(size.0),
        Calendar => |x| x.with_size(size.0),
        CarouselPrevious => |x| x.with_size(size.0),
        CarouselNext => |x| x.with_size(size.0),
        CarouselPaginationItem => |x| x.with_size(size.0),
        table::Table => |x| x.with_size(size.0),
      )
    };
    /// What a screen reader announces.
    fn accessibility_label(label: String) for ["Carousel", "CarouselItem", "CarouselNext", "CarouselPagination", "CarouselPaginationItem", "CarouselPrevious", "Progress", "Progress@base", "Table", "Table@base"] |c, n| any!(c, n,
      base::Progress => |p| p.accessibility_label(label),
      Progress => |p| p.accessibility_label(label),
      table::Table => |t| t.accessibility_label(label),
      Carousel => |t| t.accessibility_label(label),
      CarouselItem => |t| t.accessibility_label(label),
      CarouselPrevious => |t| t.accessibility_label(label),
      CarouselNext => |t| t.accessibility_label(label),
      CarouselPagination => |t| t.accessibility_label(label),
      CarouselPaginationItem => |t| t.accessibility_label(label),
    );
    /// How many columns a table cell spans.
    fn col_span(span: u32) for ["TableCell", "TableHead"] |c, n| any!(c, n,
      table::TableHead => |t| t.col_span(span as usize),
      table::TableCell => |t| t.col_span(span as usize),
    );
    /// Centers a table cell's text.
    fn text_center() for ["TableCell", "TableCell@base", "TableHead", "TableHead@base"] |c, n| any!(c, n,
      table::TableHead => |t| t.text_center(),
      table::TableCell => |t| t.text_center(),
    );
    /// Right-aligns a table cell's text.
    fn text_right() for ["TableCell", "TableCell@base", "TableHead", "TableHead@base"] |c, n| any!(c, n,
      table::TableHead => |t| t.text_right(),
      table::TableCell => |t| t.text_right(),
    );
    /// The color of a `Badge`, `Spinner`, `Icon` or `Progress`.
    fn color(color: Color) for ["Badge", "Icon", "Progress", "Spinner"] |c, n| any!(c, n,
      W<Badge> => |x| x.map(|b| b.color(color.0)),
      W<Spinner> => |x| x.map(|s| s.color(color.0)),
      Icon => |x| x.text_color(color.0),
      Progress => |x| x.color(color.0),
    );
    /// A `Badge` as a dot; a `LineChart` or `RadarChart` with dots on its points.
    fn dot() for ["Badge", "LineChart", "RadarChart"] |c, n| any!(c, n,
      W<Badge> => |x| x.map(Badge::dot),
      Line => |x| x.map(|l| l.dot()),
      Radar => |x| x.map(|r| r.dot()),
    );
    /// The number a `Badge` shows.
    fn count(count: u32) for ["Badge"] |c, n| any!(c, n, W<Badge> => |x| x.map(|b| b.count(count as usize)));
    /// The largest number a `Badge` shows, `max+` above.
    fn max(max: u32) for ["Badge"] |c, n| any!(c, n, W<Badge> => |x| x.map(|b| b.max(max as usize)));
    /// The look of a `Tag`.
    fn variant(variant: Var as "\"primary\" | \"secondary\" | \"danger\" | \"success\" | \"warning\" | \"info\"") for ["Tag"] |c, n| match variant.0 {
      Variant::Tag(v) => any!(c, n, Tag => |t| t.with_variant(v)),
      Variant::Media(_) => Err(crate::miss(c, n.kind())),
    };
    /// The look of an `EmptyMedia`.
    fn variant(variant: Var as "\"default\" | \"icon\"") for ["EmptyMedia"] |c, n| match variant.0 {
      Variant::Media(v) => any!(c, n, EmptyMedia => |m| m.with_variant(v)),
      Variant::Tag(_) => Err(crate::miss(c, n.kind())),
    };
    /// Draws a `Tag` outlined.
    fn outline() for ["Tag"] |c, n| any!(c, n, Tag => |t| t.outline());
    /// The quieter color of a `Skeleton`.
    fn secondary() for ["Skeleton"] |c, n| any!(c, n, Skeleton => |s| s.secondary());
    /// The icon a `Spinner` turns.
    fn icon(icon: SpinnerIcon) for ["Spinner"] |c, n| any!(c, n, W<Spinner> => |x| x.map(|s| s.icon(icon.0)));
    /// How a `Spinner` turns.
    fn ease(ease: Ease) for ["Spinner"] |c, n| any!(c, n, W<Spinner> => |x| x.map(|s| s.ease(ease.0)));
    /// Fills an `Empty`'s header with an `EmptyHeader`.
    fn header(header: Part<'js> as "Element<\"EmptyHeader\">") for ["Empty"] |c, n| {
      let header = header.take::<EmptyHeader>(c, "an EmptyHeader")?;
      any!(c, n, Empty => |e| e.header(header))
    };
    /// Fills an `Empty`'s content with an `EmptyContent`.
    fn content(content: Part<'js> as "Element<\"EmptyContent\">") for ["Empty"] |c, n| {
      let content = content.take::<EmptyContent>(c, "an EmptyContent")?;
      any!(c, n, Empty => |e| e.content(content))
    };
    /// Fills an `EmptyHeader`'s media with an `EmptyMedia`.
    fn media(media: Part<'js> as "Element<\"EmptyMedia\">") for ["EmptyHeader"] |c, n| {
      let media = media.take::<EmptyMedia>(c, "an EmptyMedia")?;
      any!(c, n, EmptyHeader => |h| h.media(media))
    };
    /// Fills an `EmptyHeader`'s title with an `EmptyTitle`.
    fn title(title: Part<'js> as "Element<\"EmptyTitle\">") for ["EmptyHeader"] |c, n| {
      let title = title.take::<EmptyTitle>(c, "an EmptyTitle")?;
      any!(c, n, EmptyHeader => |h| h.title(title))
    };
    /// Fills an `EmptyHeader`'s description with an `EmptyDescription`.
    fn description(description: Part<'js> as "Element<\"EmptyDescription\">") for ["EmptyHeader"] |c, n| {
      let description = description.take::<EmptyDescription>(c, "an EmptyDescription")?;
      any!(c, n, EmptyHeader => |h| h.description(description))
    };
    /// Rotates an `Icon`.
    fn rotate(radians: f32) for ["Icon"] |c, n| any!(c, n, Icon => |i| i.rotate(super::radians(radians)));
    /// Animates a `corona/component` `Progress` as busy.
    fn loading(value: bool) for ["Progress"] |c, n| any!(c, n, Progress => |p| p.loading(value));
    /// The name an `Avatar` shows the initials of without an image.
    fn name(name: String) for ["Avatar"] |c, n| any!(c, n, Avatar => |a| a.name(name));
    /// The image of an `Avatar`: an absolute path under a read grant, or a plugin file.
    fn src(path: String) for ["Avatar", "Avatar@base"] |c, n| {
      let src = source(c, path)?;
      any!(c, n, Avatar => |a| a.src(src))
    };
    /// Lays a `DescriptionList` out in a column.
    fn vertical() for ["DescriptionList"] |c, n| any!(c, n, W<DescriptionList> => |x| x.map(|d| d.layout(gpui_kit::Axis::Vertical)));
    /// Draws a `DescriptionList` or `DataTable` with borders.
    fn bordered(value: bool) for ["DataTable", "DescriptionList"] |c, n| any!(c, n,
      W<DescriptionList> => |x| x.map(|d| d.bordered(value)),
      DataTableEl => |t| DataTableEl { w: t.w.map(|d| d.bordered(value)), ..t },
    );
    /// How many columns a `DescriptionList` has.
    fn columns(columns: u32) for ["DescriptionList"] |c, n| {
      any!(c, n, W<DescriptionList> => |x| x.map(|d| d.columns(columns as usize)))
    };
    /// Adds `DescriptionItem`s to a `DescriptionList`.
    fn items(items: Parts<'js> as "Element<\"DescriptionItem\">[]") for ["DescriptionList"] |c, n| {
      let items = items.take::<Data<DescriptionItem>>(c, "a DescriptionItem")?;
      any!(c, n, W<DescriptionList> => |x| x.map(|d| d.children(items.into_iter().map(|i| i.1))))
    };
    /// Adds `TreeItem`s to a `Tree` or `TreeItem`.
    fn items(items: Parts<'js> as "Element<\"TreeItem\">[]") for ["Tree", "TreeItem"] |c, n| {
      let items = items.take::<Data<TreeItem>>(c, "a TreeItem")?;
      let items = items.into_iter().map(|i| i.1);
      any!(c, n,
        TreeEl => |t| TreeEl { items: t.items.into_iter().chain(items).collect(), ..t },
        Data<TreeItem> => |t| Data(t.0, t.1.children(items)),
      )
    };
    /// Adds a separator row to a `DescriptionList`.
    fn separator() for ["DescriptionList"] |c, n| any!(c, n, W<DescriptionList> => |x| x.map(DescriptionList::separator));
    /// How many columns a `DescriptionItem` spans.
    fn span(span: u32) for ["DescriptionItem"] |c, n| {
      any!(c, n, Data<DescriptionItem> => |d| Data(d.0, d.1.span(span as usize)))
    };
    /// Whether a `TreeItem` starts expanded.
    fn expanded(value: bool) for ["TreeItem"] |c, n| any!(c, n, Data<TreeItem> => |t| Data(t.0, t.1.expanded(value)));
    /// Whether a `TreeItem` is disabled.
    fn disabled(value: bool) for ["TreeItem"] |c, n| any!(c, n, Data<TreeItem> => |t| Data(t.0, t.1.disabled(value)));
    /// Draws a chart's grid lines.
    fn grid(value: Opt<bool>) for ["AreaChart", "BarChart", "LineChart", "RadarChart"] |c, n| {
      let value = value.0.unwrap_or(true);
      any!(c, n,
      Bar => |x| x.map(|ch| ch.grid(value)),
      Line => |x| x.map(|ch| ch.grid(value)),
      Area => |x| x.map(|ch| ch.grid(value)),
      Radar => |x| x.map(|ch| ch.grid(value)),
      )
    };
    /// Draws a `BarChart`'s label axis.
    fn label_axis(value: bool) for ["BarChart"] |c, n| any!(c, n, Bar => |x| x.map(|ch| ch.label_axis(value)));
    /// Draws a `BarChart`'s value axis.
    fn value_axis(value: bool) for ["BarChart"] |c, n| any!(c, n, Bar => |x| x.map(|ch| ch.value_axis(value)));
    /// Draws a `LineChart`'s or `AreaChart`'s x axis.
    fn x_axis(value: bool) for ["AreaChart", "LineChart"] |c, n| any!(c, n,
      Line => |x| x.map(|ch| ch.x_axis(value)),
      Area => |x| x.map(|ch| ch.x_axis(value)),
    );
    /// Labels every `margin`th tick of a chart's axis.
    fn tick_margin(margin: u32) for ["AreaChart", "BarChart", "LineChart"] |c, n| any!(c, n,
      Bar => |x| x.map(|ch| ch.tick_margin(margin.max(1) as usize)),
      Line => |x| x.map(|ch| ch.tick_margin(margin.max(1) as usize)),
      Area => |x| x.map(|ch| ch.tick_margin(margin.max(1) as usize)),
    );
    /// Draws a smooth curve.
    fn natural() for ["AreaChart", "LineChart"] |c, n| any!(c, n,
      Line => |x| x.map(|ch| ch.natural()),
      Area => |x| x.map(|ch| ch.natural()),
    );
    /// Draws straight segments.
    fn linear() for ["AreaChart", "LineChart"] |c, n| any!(c, n,
      Line => |x| x.map(|ch| ch.linear()),
      Area => |x| x.map(|ch| ch.linear()),
    );
    /// Draws steps.
    fn step_after() for ["AreaChart", "LineChart"] |c, n| any!(c, n,
      Line => |x| x.map(|ch| ch.step_after()),
      Area => |x| x.map(|ch| ch.step_after()),
    );
    /// A `PieChart`'s hole, in pixels: a donut.
    fn inner_radius(radius: f32) for ["PieChart"] |c, n| any!(c, n, Pie => |x| x.map(|ch| ch.inner_radius(radius)));
    /// The gap between a `PieChart`'s slices, in radians.
    fn pad_angle(angle: f32) for ["PieChart"] |c, n| any!(c, n, Pie => |x| x.map(|ch| ch.pad_angle(angle)));
    /// Labels a `PieChart`'s slices.
    fn labels(value: bool) for ["PieChart"] |c, n| any!(c, n, Pie => |x| match value {
      true => x.map(|ch| ch.label(|r: &Row| r.label.clone())),
      false => x,
    });
    /// How many rings a `RadarChart`'s grid has.
    fn grid_levels(levels: u32) for ["RadarChart"] |c, n| {
      any!(c, n, Radar => |x| x.map(|ch| ch.grid_levels(levels.max(1) as usize)))
    };
    /// How many months a `Calendar` shows side by side.
    fn number_of_months(count: u32) for ["Calendar"] |c, n| {
      any!(c, n, Calendar => |x| x.number_of_months(count.max(1) as usize))
    };
    /// Stripes a `DataTable`'s rows.
    fn stripe(value: bool) for ["DataTable"] |c, n| {
      any!(c, n, DataTableEl => |t| DataTableEl { w: t.w.map(|d| d.stripe(value)), ..t })
    };
    /// When a `DataTable` shows its scrollbars.
    fn scrollbar_visible(vertical: bool, horizontal: bool) for ["DataTable"] |c, n| any!(c, n,
      DataTableEl => |t| DataTableEl {
        w: t.w.map(|d| d.scrollbar_visible(vertical, horizontal)),
        ..t
      },
    );
    /// Whether a `DataTable`'s rows can be selected.
    fn row_selectable(value: bool) for ["DataTable"] |c, n| {
      any!(c, n, DataTableEl => |t| t.state(|s| s.row_selectable = value))
    };
    /// Whether a `DataTable`'s columns can be selected.
    fn column_selectable(value: bool) for ["DataTable"] |c, n| {
      any!(c, n, DataTableEl => |t| t.state(|s| s.col_selectable = value))
    };
    /// Whether a `DataTable`'s cells can be selected.
    fn cell_selectable(value: bool) for ["DataTable"] |c, n| {
      any!(c, n, DataTableEl => |t| t.state(|s| s.cell_selectable = value))
    };
    /// Whether a `DataTable` has a row header column.
    fn row_header(value: bool) for ["DataTable"] |c, n| {
      any!(c, n, DataTableEl => |t| t.state(|s| s.row_header = value))
    };
    /// Whether a `DataTable`'s columns sort.
    fn sortable(value: bool) for ["DataTable"] |c, n| any!(c, n, DataTableEl => |t| t.state(|s| s.sortable = value));
    /// Whether a `DataTable`'s columns resize.
    fn column_resizable(value: bool) for ["DataTable"] |c, n| {
      any!(c, n, DataTableEl => |t| t.state(|s| s.col_resizable = value))
    };
    /// Whether a `DataTable`'s columns move.
    fn column_movable(value: bool) for ["DataTable"] |c, n| {
      any!(c, n, DataTableEl => |t| t.state(|s| s.col_movable = value))
    };
  }
}

/// A `{ new (..): Element; new: (..) => Element }` declaration
///
/// `args` starting with `<T> ` makes the constructor generic in `T`.
fn ctor(doc: &str, name: &str, args: &str) -> String {
  let doc = crate::jsdoc(&[doc], "  ");
  let (generic, args) = args.strip_prefix("<T> ").map_or(("", args), |a| ("<T>", a));
  format!(
    "{doc}  export const {name}: {{ new {generic}({args}): Element; new: {generic}({args}) => Element }};\n"
  )
}

fn base_d_ts() -> String {
  let parts: &[(&str, &str, &str)] = &[
    (
      " Takes no space, not even a gap: return it to hide a widget or card.",
      "Empty",
      "",
    ),
    (
      " The progress root: announces `value`, draws nothing; put a `ProgressTrack` in it.",
      "Progress",
      "id: string | number",
    ),
    (" The groove, a plain element.", "ProgressTrack", ""),
    (
      " The filled part, a plain element: size it from the value.",
      "ProgressIndicator",
      "",
    ),
    (
      " Renders its `image` slot, or its `fallback` without one; draws nothing else.",
      "Avatar",
      "",
    ),
    (
      " The image slot of an `Avatar`: an asset or an absolute path.",
      "AvatarImage",
      "source: string",
    ),
    (" The fallback slot of an `Avatar`.", "AvatarFallback", ""),
    (
      " An unstyled semantic table: nest the parts below.",
      "Table",
      "id: string | number",
    ),
    (
      " The header row group.",
      "TableHeader",
      "id: string | number",
    ),
    (" The body row group.", "TableBody", "id: string | number"),
    (
      " One row, `row_index` one-based.",
      "TableRow",
      "id: string | number, row_index: number",
    ),
    (
      " One column header, `column_index` one-based.",
      "TableHead",
      "id: string | number, column_index: number",
    ),
    (
      " One data cell, `column_index` one-based.",
      "TableCell",
      "id: string | number, column_index: number",
    ),
    (" A caption slot.", "TableCaption", "id: string | number"),
  ];
  let mut out: String = parts.iter().map(|(d, n, a)| ctor(d, n, a)).collect();
  out.push_str(CALENDAR_D_TS);
  out
}

const CALENDAR_D_TS: &str = r#"  /** A selected date: a day, a `[start, end]` range, or nothing. Days are `"YYYY-MM-DD"`. */
  export type CalendarDate = string | [string | null, string | null] | null;
  /** A calendar month and its selected date. Retained: create it in `init`. */
  export const CalendarState: { (): CalendarState; new: () => CalendarState };
  export interface CalendarState {
    /** Months of weeks of seven days, the neighbouring months' days included. */
    month_days(): string[][][];
    year(): number;
    /** 1–12. */
    month(): number;
    today(): string;
    value(): CalendarDate;
    /** Selects a day, a range, or nothing, without a `change` event. */
    set_value(next: CalendarDate): void;
    next_month(): void;
    prev_month(): void;
    /** `"change"`: a date was picked in a `Calendar`. */
    on(event: "change", handler: (date: CalendarDate, cx: Context) => void): boolean;
    release(): boolean;
  }
"#;

fn component_d_ts() -> String {
  let ctors: &[(&str, &str, &str)] = &[
    (" A count or dot over its children.", "Badge", ""),
    (" A small label.", "Tag", ""),
    (" A loading placeholder; size it.", "Skeleton", ""),
    (" A turning loading icon.", "Spinner", ""),
    (
      " An empty state: `header`, `content` and children.",
      "Empty",
      "",
    ),
    (
      " The header of an `Empty`: `media`, `title`, `description`.",
      "EmptyHeader",
      "",
    ),
    (" The media of an `EmptyHeader`.", "EmptyMedia", ""),
    (" The title of an `EmptyHeader`.", "EmptyTitle", ""),
    (
      " The description of an `EmptyHeader`.",
      "EmptyDescription",
      "",
    ),
    (" The actions of an `Empty`.", "EmptyContent", ""),
    (" An icon from the app's assets.", "Icon", "path: string"),
    (
      " An image from the app's assets, or a file by absolute path.",
      "Image",
      "path: string",
    ),
    (
      " Labelled values: add `DescriptionItem`s with `items`.",
      "DescriptionList",
      "",
    ),
    (
      " One labelled value of a `DescriptionList`.",
      "DescriptionItem",
      "label: string",
    ),
    (
      " A progress bar, `value` 0–100.",
      "Progress",
      "id: string | number",
    ),
    (" A round picture, or the initials of `name`.", "Avatar", ""),
    (
      " Bars of `{ label, value }` rows, or a function returning them; fills its size.",
      "BarChart",
      "rows: ChartRows",
    ),
    (" A line through the rows.", "LineChart", "rows: ChartRows"),
    (
      " A filled line through the rows.",
      "AreaChart",
      "rows: ChartRows",
    ),
    (" Slices of the rows.", "PieChart", "rows: ChartRows"),
    (" A radar of the rows.", "RadarChart", "rows: ChartRows"),
    (
      " A month grid of a `CalendarState`, picking dates.",
      "Calendar",
      "state: CalendarState",
    ),
    (
      " Slides: `CarouselContent` with `CarouselItem`s, and the controls.",
      "Carousel",
      "id: string | number, state: CarouselState",
    ),
    (
      " The clipped viewport of a `Carousel`.",
      "CarouselContent",
      "state: CarouselState",
    ),
    (
      " Slide `index`.",
      "CarouselItem",
      "id: string | number, index: number, state: CarouselState",
    ),
    (
      " The previous-slide button.",
      "CarouselPrevious",
      "state: CarouselState",
    ),
    (
      " The next-slide button.",
      "CarouselNext",
      "state: CarouselState",
    ),
    (
      " The row of `CarouselPaginationItem`s.",
      "CarouselPagination",
      "",
    ),
    (
      " The dot of slide `index`.",
      "CarouselPaginationItem",
      "id: string | number, index: number, state: CarouselState",
    ),
    (
      " A list of rows rendered lazily by `render_row`; its state is kept by `id`.",
      "List",
      "<T> id: string, rows: readonly T[] | (() => readonly T[]), render_row: (row: T, cx: Context) => Element",
    ),
    (
      " A table of rows, its visible cells rendered lazily by `render_cell`.",
      "DataTable",
      "<T> state: DataTableState, rows: readonly T[] | (() => readonly T[]), render_cell: (row: T, column: string, cx: Context) => Element",
    ),
    (
      " A tree of `TreeItem` children; what is expanded is kept by `id`.",
      "Tree",
      "id: string",
    ),
    (
      " One node of a `Tree`; nest more as children.",
      "TreeItem",
      "id: string, label: string",
    ),
    (
      " A styled table of `TableHeader`, `TableBody`, `TableFooter` and `TableCaption`.",
      "Table",
      "",
    ),
    (" The header rows of a `Table`.", "TableHeader", ""),
    (" The body rows of a `Table`.", "TableBody", ""),
    (" The footer rows of a `Table`.", "TableFooter", ""),
    (" A row of `TableHead`s or `TableCell`s.", "TableRow", ""),
    (" A column header cell.", "TableHead", ""),
    (" A data cell.", "TableCell", ""),
    (" A caption under a `Table`.", "TableCaption", ""),
  ];
  let mut out = String::from(COMPONENT_D_TS);
  out.extend(ctors.iter().map(|(d, n, a)| ctor(d, n, a)));
  out
}

const COMPONENT_D_TS: &str = r#"  import { CalendarState, CalendarDate } from "corona/base";
  export { CalendarState, CalendarDate };
  export interface ChartRow {
    label: string;
    value: number;
  }
  export type ChartRows = readonly ChartRow[] | (() => readonly ChartRow[]);
  /** Which slide a `Carousel` shows. Retained: create it in `init`. */
  export const CarouselState: {
    (item_count: number, selected_index?: number, axis?: "horizontal" | "vertical", looping?: boolean): CarouselState;
    new: (item_count: number, selected_index?: number, axis?: "horizontal" | "vertical", looping?: boolean) => CarouselState;
  };
  export interface CarouselState {
    item_count(): number;
    selected_index(): number | undefined;
    /** Selects a slide without a `change` event. */
    set_selected_index(index: number): void;
    set_item_count(count: number): void;
    /** Moves on, with a `change` event; false at the end. */
    select_next(): boolean;
    select_previous(): boolean;
    /** `"change"`: another slide was selected. */
    on(event: "change", handler: (index: number, cx: Context) => void): boolean;
    release(): boolean;
  }
  /** The columns, selection and scroll of a `DataTable`. Retained: create it in `init`. */
  export const DataTableState: { (columns: string[]): DataTableState; new: (columns: string[]) => DataTableState };
  export interface DataTableState {
    on(event: "select_row" | "double_click_row" | "select_column", handler: (index: number, cx: Context) => void): boolean;
    release(): boolean;
  }
"#;

/// The `corona/base` parts
pub(crate) const BASE: Family = Family {
  module: "corona/base",
  names: &[
    "Empty",
    "Progress",
    "ProgressTrack",
    "ProgressIndicator",
    "Avatar",
    "AvatarImage",
    "AvatarFallback",
    "Table",
    "TableHeader",
    "TableBody",
    "TableRow",
    "TableHead",
    "TableCell",
    "TableCaption",
    "CalendarState",
  ],
  exports: base_exports,
  methods: |_| Ok(()),
  d_ts: base_d_ts,
  el_d_ts: String::new,
};

/// The `corona/component` widgets, and the element methods of both
pub(crate) const FAMILY: Family = Family {
  module: "corona/component",
  names: &[
    "Badge",
    "Tag",
    "Skeleton",
    "Spinner",
    "Empty",
    "EmptyHeader",
    "EmptyMedia",
    "EmptyTitle",
    "EmptyDescription",
    "EmptyContent",
    "Icon",
    "Image",
    "DescriptionList",
    "DescriptionItem",
    "Progress",
    "Avatar",
    "BarChart",
    "LineChart",
    "AreaChart",
    "PieChart",
    "RadarChart",
    "Calendar",
    "CalendarState",
    "Carousel",
    "CarouselContent",
    "CarouselItem",
    "CarouselPrevious",
    "CarouselNext",
    "CarouselPagination",
    "CarouselPaginationItem",
    "CarouselState",
    "List",
    "DataTable",
    "DataTableState",
    "Tree",
    "TreeItem",
    "Table",
    "TableHeader",
    "TableBody",
    "TableFooter",
    "TableRow",
    "TableHead",
    "TableCell",
    "TableCaption",
  ],
  exports,
  methods: |proto| {
    methods::install(proto)?;
    children::install(proto)
  },
  d_ts: component_d_ts,
  el_d_ts: methods::d_ts,
};

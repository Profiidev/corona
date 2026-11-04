//! Chat (`Message`, `Bubble`, `Attachment`, `Marker`, `ShimmerText`, `MessageScroller`),
//! `Editor`, `Diff`, `Text`, settings, questionnaires, `TextView` (`corona/base`) and the FPS
//! HUD (`corona/fps`).
//!
//! Settings and questionnaires take typed children: a `SettingItem` becomes a
//! [`Carrier`] element its `SettingGroup` takes the native value back out of.
//!

use std::{
  any::Any,
  collections::HashMap,
  hash::{DefaultHasher, Hash, Hasher},
  marker::PhantomData,
  sync::Arc,
  time::Duration,
};

use gpui_fps::FpsOverlay;
use gpui_kit::{
  Anchor, AnyElement, App, AppContext, Axis, Bounds, Element, ElementId, Global, GlobalElementId,
  Image, ImageFormat, ImageSource, InspectorElementId, IntoElement, LayoutId, ParentElement,
  Pixels, Refineable, Style, StyleRefinement, Styled, WeakEntity, Window, WindowId,
  base::TextView,
  component::{
    Sizable, Size,
    attachment::{Attachment, AttachmentContent, AttachmentStatus},
    bubble::{Bubble, BubbleVariant},
    diff::{self, Diff, DiffChangeIndicator, DiffFile, DiffHunkSeparator, DiffMode},
    input::{Editor, EditorState as NativeEditor, InputEvent},
    marker::{Marker, MarkerLoadingStyle, MarkerVariant},
    message::{Message, MessageAlignment, MessageContent},
    message_scroller::{self as scroller, MessageScroller},
    questionnaire as q,
    setting::{SelectIndex, SettingField, SettingGroup, SettingItem, SettingPage, Settings},
    shimmer::ShimmerText,
    text::Text,
  },
  div, px,
};
use rquickjs::{
  Class, Ctx, Exception, FromJs, Function, IntoJs, JsLifetime, Object, Result, Value, class::Trace,
  function::Args,
};

use super::{Family, constructor};
use crate::{
  Child, Named, Node, On, Opt, Payload, Retained, ScriptView, TsType, app, element::style::unknown,
  frame, map, new_el, runtime::runtime, view::window_or_throw,
};

/// Every kind this file makes; their methods throw on any other element
const KINDS: &[&str] = &[
  "Attachment",
  "Bubble",
  "Marker",
  "Message",
  "ShimmerText",
  "MessageScroller",
  "Editor",
  "Diff",
  "Settings",
  "SettingPage",
  "SettingGroup",
  "SettingItem",
  "Questionnaire",
  "QuestionnaireItem",
  "QuestionnaireChoice",
  "QuestionnaireInput",
  "TextView",
  "fps_monitor",
];

fn not_here(ctx: &Ctx<'_>, n: &dyn Node) -> rquickjs::Error {
  crate::miss(ctx, n.kind())
}

// ---------------------------------------------------------------------------------------------
// Argument types

/// A closed set of string literals and what each means
trait Choices: 'static {
  const WHAT: &'static str;
  type Out: Copy + 'static;
  const ALL: &'static [(&'static str, Self::Out)];
}

/// The literals of a choice argument, or of two
trait Names {
  fn names() -> Vec<&'static str>;
}

impl<C: Choices> Names for C {
  fn names() -> Vec<&'static str> {
    C::ALL.iter().map(|(n, _)| *n).collect()
  }
}

impl<A: Choices, B: Choices> Names for (A, B) {
  fn names() -> Vec<&'static str> {
    let mut names = A::names();
    names.extend(B::names().into_iter().filter(|n| !A::names().contains(n)));
    names
  }
}

/// A string literal argument, parsed in the body: an element of another family then still
/// reaches its own method of the same name.
struct Lit<'js, C>(Value<'js>, PhantomData<C>);

impl<'js, C> FromJs<'js> for Lit<'js, C> {
  fn from_js(_: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    Ok(Self(value, PhantomData))
  }
}

impl<C: Names> TsType for Lit<'_, C> {
  fn ts() -> String {
    let names: Vec<String> = C::names().iter().map(|n| format!("\"{n}\"")).collect();
    names.join(" | ")
  }
}

impl<'js, C> Lit<'js, C> {
  fn get<D: Choices>(&self, ctx: &Ctx<'js>) -> Result<D::Out> {
    let name = String::from_js(ctx, self.0.clone())?;
    let found = D::ALL.iter().find(|(n, _)| *n == name);
    found
      .map(|(_, v)| *v)
      .ok_or_else(|| Exception::throw_type(ctx, &unknown(D::WHAT, &name, &D::names())))
  }
}

macro_rules! choices {
  ($($name:ident($what:literal): $out:ty { $($s:literal => $v:expr),* $(,)? })*) => {$(
    struct $name;
    impl Choices for $name {
      const WHAT: &'static str = $what;
      type Out = $out;
      const ALL: &'static [(&'static str, $out)] = &[$(($s, $v)),*];
    }
  )*};
}

choices! {
  Sizes("size"): Size {
    "xsmall" => Size::XSmall, "small" => Size::Small, "medium" => Size::Medium, "large" => Size::Large,
  }
  Axes("axis"): Axis { "horizontal" => Axis::Horizontal, "vertical" => Axis::Vertical }
  Statuses("status"): AttachmentStatus {
    "pending" => AttachmentStatus::Pending,
    "uploading" => AttachmentStatus::Uploading,
    "processing" => AttachmentStatus::Processing,
    "failed" => AttachmentStatus::Failed,
    "complete" => AttachmentStatus::Complete,
  }
  Alignments("alignment"): MessageAlignment {
    "start" => MessageAlignment::Start, "end" => MessageAlignment::End,
  }
  BubbleVariants("variant"): BubbleVariant {
    "filled" => BubbleVariant::Filled,
    "secondary" => BubbleVariant::Secondary,
    "muted" => BubbleVariant::Muted,
    "tinted" => BubbleVariant::Tinted,
    "outline" => BubbleVariant::Outline,
    "ghost" => BubbleVariant::Ghost,
    "destructive" => BubbleVariant::Destructive,
  }
  MarkerVariants("variant"): MarkerVariant {
    "plain" => MarkerVariant::Plain,
    "separator" => MarkerVariant::Separator,
    "border" => MarkerVariant::Border,
  }
  LoadingStyles("loading style"): MarkerLoadingStyle {
    "spinner" => MarkerLoadingStyle::Spinner, "shimmer" => MarkerLoadingStyle::Shimmer,
  }
  HunkSeparators("hunk separator"): DiffHunkSeparator {
    "metadata" => DiffHunkSeparator::Metadata,
    "line_info" => DiffHunkSeparator::LineInfo,
    "simple" => DiffHunkSeparator::Simple,
  }
  ChangeIndicators("change indicator"): DiffChangeIndicator {
    "signs" => DiffChangeIndicator::Signs,
    "bars" => DiffChangeIndicator::Bars,
    "none" => DiffChangeIndicator::None,
  }
  ShortcutModes("shortcut mode"): q::QuestionnaireShortcutMode {
    "letters" => q::QuestionnaireShortcutMode::Letters,
    "numbers" => q::QuestionnaireShortcutMode::Numbers,
  }
  DiffModes("diff mode"): DiffMode { "unified" => DiffMode::Unified, "split" => DiffMode::Split }
  Languages("language"): &'static str { "rust" => "rust", "json" => "json" }
  Anchors("anchor"): Anchor {
    "top_left" => Anchor::TopLeft,
    "top_right" => Anchor::TopRight,
    "bottom_left" => Anchor::BottomLeft,
    "bottom_right" => Anchor::BottomRight,
    "top_center" => Anchor::TopCenter,
    "bottom_center" => Anchor::BottomCenter,
    "left_center" => Anchor::LeftCenter,
    "right_center" => Anchor::RightCenter,
  }
}

/// The URL a `TextView` link click reports
struct Url(String);

impl Payload for Url {
  const TS: &'static str = "string";
}

impl<'js> IntoJs<'js> for Url {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    self.0.into_js(ctx)
  }
}

/// A finite, non-negative number of milliseconds
pub(super) fn millis(ctx: &Ctx<'_>, ms: f64) -> Result<Duration> {
  Duration::try_from_secs_f64(ms / 1000.)
    .map_err(|_| Exception::throw_range(ctx, &format!("invalid duration {ms}ms")))
}

// ---------------------------------------------------------------------------------------------
// Typed children

/// An element that draws nothing and carries `T` to the parent that takes it
struct Carrier<T: 'static>(Option<T>);

impl<T: 'static> IntoElement for Carrier<T> {
  type Element = Self;
  fn into_element(self) -> Self {
    self
  }
}

impl<T: 'static> Element for Carrier<T> {
  type RequestLayoutState = ();
  type PrepaintState = ();

  fn id(&self) -> Option<ElementId> {
    None
  }

  fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
    None
  }

  fn request_layout(
    &mut self,
    _: Option<&GlobalElementId>,
    _: Option<&InspectorElementId>,
    window: &mut Window,
    cx: &mut App,
  ) -> (LayoutId, ()) {
    let style = Style {
      display: gpui_kit::Display::None,
      ..Default::default()
    };
    (window.request_layout(style, None, cx), ())
  }

  fn prepaint(
    &mut self,
    _: Option<&GlobalElementId>,
    _: Option<&InspectorElementId>,
    _: Bounds<Pixels>,
    _: &mut (),
    _: &mut Window,
    _: &mut App,
  ) {
  }

  fn paint(
    &mut self,
    _: Option<&GlobalElementId>,
    _: Option<&InspectorElementId>,
    _: Bounds<Pixels>,
    _: &mut (),
    _: &mut (),
    _: &mut Window,
    _: &mut App,
  ) {
  }
}

/// The `T` a child carries, if it is one
fn carried<T: 'static>(mut child: AnyElement) -> Option<T> {
  child.downcast_mut::<Carrier<T>>()?.0.take()
}

/// No child type: a node taking none
enum Leaf {}

/// Implements [`Node`] for a node whose children are the `T`s of [`Carrier`]s:
/// `typed!(Node, "Kind", Child, |node, child| add, |node| into_element [, |node| &mut style])`.
macro_rules! typed {
  ($ty:ty, $kind:literal, $child:ty, |$s:ident, $c:ident| $add:expr, |$o:ident| $out:expr
    $(, |$st:ident| $style:expr)?) => {
    impl Node for $ty {
      fn kind(&self) -> &'static str {
        $kind
      }
      $(fn style(&mut self) -> Option<&mut StyleRefinement> {
        let $st = self;
        Some($style)
      })?
      #[allow(unreachable_code, unused_variables)]
      fn add_child(&mut self, child: AnyElement) -> bool {
        let Some($c) = carried::<$child>(child) else {
          return false;
        };
        let $s = self;
        $add;
        true
      }
      fn into_element(self: Box<Self>) -> AnyElement {
        let $o = *self;
        $out
      }
      fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
      }
    }
  };
}

/// What a lazy `content(() => Element)` renders, outside the render that set it
#[derive(Clone)]
struct Lazy {
  view: WeakEntity<ScriptView>,
  key: u64,
}

impl Lazy {
  fn new(ctx: &Ctx<'_>, f: Function<'_>) -> Result<Self> {
    let view = frame().view.clone();
    let key = runtime(ctx)?.slot(view.entity_id(), f, true);
    Ok(Self { view, key })
  }

  /// Calls the fn, with `index` for a row, and renders what it returns; `null` is nothing.
  fn render(&self, index: Option<usize>, window: &mut Window, cx: &mut App) -> AnyElement {
    let Some(view) = self.view.upgrade() else {
      return div().into_any_element();
    };
    let args = |a: &mut Args<'_>| match index {
      Some(i) => a.push_arg(i),
      None => Ok(()),
    };
    let rendered = view.update(cx, |view, cx| {
      view.render_content(
        self.key,
        index,
        window,
        cx,
        args,
        |ctx, frame, value| match value.is_null() || value.is_undefined() {
          true => Ok(div().into_any_element()),
          false => frame.child(ctx, Child::from_js(ctx, value)?),
        },
      )
    });
    rendered.unwrap_or_else(|e| format!("render failed: {e}").into_any_element())
  }
}

// ---------------------------------------------------------------------------------------------
// Chat

crate::node!(Bubble, "Bubble", style, parent);
crate::node!(Marker, "Marker", style, parent);
crate::node!(ShimmerText, "ShimmerText", style);
crate::node!(MessageScroller, "MessageScroller", style);
crate::node!(Editor, "Editor", style);
crate::node!(Diff, "Diff", style);
crate::node!(TextView, "TextView", style);
crate::node!(FpsOverlay, "fps_monitor");

/// A widget whose children go into its content slot
struct Slot<W> {
  widget: W,
  children: Vec<AnyElement>,
}

impl<W> Slot<W> {
  fn new(widget: W) -> Self {
    Self {
      widget,
      children: Vec::new(),
    }
  }
}

macro_rules! slot {
  ($ty:ty, $kind:literal, |$w:ident, $c:ident| $fill:expr) => {
    impl Node for Slot<$ty> {
      fn kind(&self) -> &'static str {
        $kind
      }
      fn style(&mut self) -> Option<&mut StyleRefinement> {
        Some(Styled::style(&mut self.widget))
      }
      fn add_child(&mut self, child: AnyElement) -> bool {
        self.children.push(child);
        true
      }
      fn into_element(self: Box<Self>) -> AnyElement {
        let ($w, $c) = (self.widget, self.children);
        match $c.is_empty() {
          true => $w.into_any_element(),
          false => $fill.into_any_element(),
        }
      }
      fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
      }
    }
  };
}

slot!(Attachment, "Attachment", |w, c| w
  .content(AttachmentContent::new().children(c)));
slot!(Message, "Message", |w, c| w
  .content(MessageContent::new().children(c)));

/// Retained virtual-list and tail-following state for a message transcript
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "MessageScrollerState")]
pub(crate) struct ScrollerState {
  #[qjs(skip_trace)]
  state: Retained<scroller::MessageScrollerState>,
}

impl Named for ScrollerState {
  const TS: &'static str = "MessageScrollerState";
}

impl ScrollerState {
  fn update<R>(
    &self,
    ctx: &Ctx<'_>,
    f: impl FnOnce(
      &mut scroller::MessageScrollerState,
      &mut gpui_kit::Context<'_, scroller::MessageScrollerState>,
    ) -> R,
  ) -> Result<R> {
    Ok(self.state.entity(ctx)?.clone().update(app(), f))
  }
}

#[rquickjs::methods]
impl ScrollerState {
  pub fn item_count(&self, ctx: Ctx<'_>) -> Result<u32> {
    Ok(self.state.entity(&ctx)?.read(app()).item_count() as u32)
  }

  /// Replaces every row with `count` new ones.
  pub fn reset(&self, ctx: Ctx<'_>, count: u32) -> Result<()> {
    let count = crate::element::items(&ctx, count)?;
    self.update(&ctx, |s, cx| s.reset(count, cx))
  }

  /// Adds `count` rows at the end, following them when at the tail.
  pub fn append(&self, ctx: Ctx<'_>, count: u32) -> Result<bool> {
    let count = crate::element::items(&ctx, count)?;
    self.update(&ctx, |s, cx| s.append(count, cx))
  }

  pub fn scroll_to_end(&self, ctx: Ctx<'_>) -> Result<()> {
    self.update(&ctx, |s, cx| s.scroll_to_end(cx))
  }

  pub fn is_following_tail(&self, ctx: Ctx<'_>) -> Result<bool> {
    Ok(self.state.entity(&ctx)?.read(app()).is_following_tail())
  }

  pub fn release(&mut self) -> bool {
    self.state.release()
  }
}

/// `MessageScroller(id, state, render_item)`
fn message_scroller<'js>(
  ctx: Ctx<'js>,
  id: String,
  state: Class<'js, ScrollerState>,
  render_item: Function<'js>,
) -> Result<Class<'js, crate::El>> {
  let state = state.borrow().state.entity(&ctx)?.clone();
  let lazy = Lazy::new(&ctx, render_item)?;
  let el = MessageScroller::new(id, state, move |i, window, cx| {
    lazy.render(Some(i), window, cx)
  });
  new_el(&ctx, el)
}

// ---------------------------------------------------------------------------------------------
// Editor and diff

/// Retained source-editor state
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "EditorState")]
pub(crate) struct EditorState {
  #[qjs(skip_trace)]
  state: Retained<NativeEditor>,
}

impl Named for EditorState {
  const TS: &'static str = "EditorState";
}

#[rquickjs::methods]
impl EditorState {
  /// The current text.
  pub fn value(&self, ctx: Ctx<'_>) -> Result<String> {
    Ok(self.state.entity(&ctx)?.read(app()).value().to_string())
  }

  /// Replaces the text, without a `change` event.
  pub fn set_value(&self, ctx: Ctx<'_>, text: String) -> Result<()> {
    let window = window_or_throw(&ctx)?;
    let entity = self.state.entity(&ctx)?.clone();
    entity.update(app(), |s, cx| s.set_value(text, window, cx));
    Ok(())
  }

  /// `"change"` and `"enter"` pass the text, `"focus"` and `"blur"` nothing.
  pub fn on<'js>(&mut self, ctx: Ctx<'js>, name: String, f: Function<'js>) -> Result<bool> {
    let entity = self.state.entity(&ctx)?.clone();
    let names = ["change", "enter", "focus", "blur"];
    self
      .state
      .on(&ctx, &names, name, f, move |e: &InputEvent, cx| {
        let value = || Some(entity.read(cx).value().to_string());
        Some(match e {
          InputEvent::Change => ("change", value()),
          InputEvent::PressEnter { .. } => ("enter", value()),
          InputEvent::Focus => ("focus", None),
          InputEvent::Blur => ("blur", None),
        })
      })
  }

  pub fn release(&mut self) -> bool {
    self.state.release()
  }
}

/// `EditorState(initial_value, language?)`; needs the window, so not in `render`'s callers
fn editor_state<'js>(
  ctx: Ctx<'js>,
  text: String,
  language: Opt<Lit<'js, Languages>>,
) -> Result<Class<'js, EditorState>> {
  let language = language.0.map(|l| l.get::<Languages>(&ctx)).transpose()?;
  let window = window_or_throw(&ctx)?;
  let entity = app().new(|cx| {
    let state = NativeEditor::new(window, cx).default_value(text);
    match language {
      Some(language) => state.language(language),
      None => state,
    }
  });
  let state = Retained::new(&ctx, entity)?;
  Class::instance(ctx, EditorState { state })
}

/// Retained readonly patch state
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "DiffState")]
pub(crate) struct DiffState {
  #[qjs(skip_trace)]
  state: Retained<diff::DiffState>,
}

impl Named for DiffState {
  const TS: &'static str = "DiffState";
}

#[rquickjs::methods]
impl DiffState {
  /// Shows another patch.
  pub fn set_patch(&self, ctx: Ctx<'_>, patch: String) -> Result<()> {
    let files = parse_patch(&ctx, &patch)?;
    let entity = self.state.entity(&ctx)?.clone();
    entity.update(app(), |s, cx| s.set_files(files, cx));
    Ok(())
  }

  pub fn release(&mut self) -> bool {
    self.state.release()
  }
}

fn parse_patch(ctx: &Ctx<'_>, patch: &str) -> Result<Vec<DiffFile>> {
  DiffFile::parse(patch).map_err(|e| Exception::throw_type(ctx, &format!("invalid patch: {e}")))
}

/// `DiffState(patch, mode?)`
fn diff_state<'js>(
  ctx: Ctx<'js>,
  patch: String,
  mode: Opt<Lit<'js, DiffModes>>,
) -> Result<Class<'js, DiffState>> {
  let mode = mode.0.map(|m| m.get::<DiffModes>(&ctx)).transpose()?;
  let files = parse_patch(&ctx, &patch)?;
  let entity = app().new(|cx| diff::DiffState::new(files, cx).with_mode(mode.unwrap_or_default()));
  let state = Retained::new(&ctx, entity)?;
  Class::instance(ctx, DiffState { state })
}

// ---------------------------------------------------------------------------------------------
// Settings

struct SettingsNode(Settings);

typed!(
  SettingsNode,
  "Settings",
  SettingPage,
  |s, page| s.0 = std::mem::replace(&mut s.0, Settings::new("")).page(page),
  |o| o.0.into_any_element()
);

struct PageNode(SettingPage);

typed!(
  PageNode,
  "SettingPage",
  SettingGroup,
  |s, group| s.0 = std::mem::replace(&mut s.0, SettingPage::new("")).group(group),
  |o| Carrier(Some(o.0)).into_any_element()
);

struct GroupNode(SettingGroup);

typed!(
  GroupNode,
  "SettingGroup",
  SettingItem,
  |s, item| s.0 = std::mem::replace(&mut s.0, SettingGroup::new()).item(item),
  |o| Carrier(Some(o.0)).into_any_element(),
  |s| Styled::style(&mut s.0)
);

/// A setting item until it knows its field
struct ItemNode {
  title: String,
  field: Option<Lazy>,
  description: Option<String>,
  layout: Option<Axis>,
  keywords: Vec<String>,
  disabled: bool,
}

typed!(ItemNode, "SettingItem", Leaf, |_s, c| match c {}, |o| {
  let field = o.field.map(|lazy| {
    SettingField::render(move |_, window: &mut Window, cx: &mut App| lazy.render(None, window, cx))
  });
  let mut item = SettingItem::new(
    o.title,
    field.unwrap_or_else(|| SettingField::element(Empty)),
  )
  .keywords(o.keywords)
  .disabled(o.disabled);
  if let Some(description) = o.description {
    item = item.description(description);
  }
  if let Some(layout) = o.layout {
    item = item.layout(layout);
  }
  Carrier(Some(item)).into_any_element()
});

/// The field of an item without `content`
#[derive(Clone, Copy)]
struct Empty;

impl gpui_kit::component::setting::SettingFieldElement for Empty {
  type Element = gpui_kit::Empty;
  fn render_field(
    &self,
    _: &gpui_kit::component::setting::RenderOptions,
    _: &mut Window,
    _: &mut App,
  ) -> Self::Element {
    gpui_kit::Empty
  }
}

// ---------------------------------------------------------------------------------------------
// Questionnaire

#[derive(Hash)]
struct ChoiceData {
  value: String,
  label: String,
  description: Option<String>,
  disabled: bool,
  default_selected: bool,
}

#[derive(Hash)]
struct ItemData {
  name: String,
  label: String,
  description: Option<String>,
  required: bool,
  multiple: bool,
  disabled: bool,
  choices: Vec<ChoiceData>,
  input: Option<InputData>,
}

/// A free-form answer of a question, typed into an `InputState`
struct InputData {
  state: gpui_kit::Entity<gpui_kit::component::input::InputState>,
  label: String,
  disabled: bool,
}

impl Hash for InputData {
  fn hash<H: Hasher>(&self, h: &mut H) {
    (self.state.entity_id(), &self.label, self.disabled).hash(h);
  }
}

/// What a `QuestionnaireItem` takes
enum ItemPart {
  Choice(ChoiceData),
  Input(InputData),
}

typed!(
  ChoiceData,
  "QuestionnaireChoice",
  Leaf,
  |_s, c| match c {},
  |o| Carrier(Some(ItemPart::Choice(o))).into_any_element()
);

typed!(
  InputData,
  "QuestionnaireInput",
  Leaf,
  |_s, c| match c {},
  |o| Carrier(Some(ItemPart::Input(o))).into_any_element()
);

typed!(
  ItemData,
  "QuestionnaireItem",
  ItemPart,
  |s, c| match c {
    ItemPart::Choice(c) => s.choices.push(c),
    ItemPart::Input(i) => s.input = Some(i),
  },
  |o| Carrier(Some(o)).into_any_element()
);

struct QuestionnaireNode {
  id: String,
  shortcuts: Option<q::QuestionnaireShortcutMode>,
  items: Vec<ItemData>,
  style: StyleRefinement,
}

typed!(
  QuestionnaireNode,
  "Questionnaire",
  ItemData,
  |s, item| s.items.push(item),
  |o| questionnaire(o),
  |s| &mut s.style
);

/// The flow, its state kept by id and rebuilt when the questions change
struct Kept {
  state: gpui_kit::Entity<q::QuestionnaireState>,
  hash: u64,
  error: Option<String>,
}

fn build(
  items: &[ItemData],
  mode: Option<q::QuestionnaireShortcutMode>,
  hash: u64,
  cx: &mut App,
) -> Kept {
  let defs = items.iter().map(|i| {
    let choices = i.choices.iter().map(|c| {
      let choice = q::QuestionnaireChoiceDefinition::new(c.value.clone(), c.label.clone())
        .with_disabled(c.disabled)
        .with_default_selected(c.default_selected);
      match &c.description {
        Some(d) => choice.with_description(d.clone()),
        None => choice,
      }
    });
    let item = q::QuestionnaireItemDefinition::new(i.name.clone(), i.label.clone())
      .with_required(i.required)
      .with_multiple(i.multiple)
      .with_disabled(i.disabled)
      .with_choices(choices);
    let item = match &i.input {
      Some(input) => item.with_input(
        q::QuestionnaireInputDefinition::new(input.state.clone(), input.label.clone())
          .with_disabled(input.disabled),
      ),
      None => item,
    };
    match &i.description {
      Some(d) => item.with_description(d.clone()),
      None => item,
    }
  });
  let defs: Vec<_> = defs.collect();
  let mut error = None;
  let state = cx.new(|cx| match q::QuestionnaireState::new(defs, cx) {
    Ok(state) => match mode {
      Some(mode) => state.with_shortcuts(mode),
      None => state,
    },
    Err(e) => {
      error = Some(format!("invalid questionnaire: {e}"));
      q::QuestionnaireState::new(Vec::new(), cx).expect("an empty questionnaire is valid")
    }
  });
  Kept { state, hash, error }
}

fn questionnaire(o: QuestionnaireNode) -> AnyElement {
  let mut hasher = DefaultHasher::new();
  (&o.items, o.shortcuts.map(|m| m as u8)).hash(&mut hasher);
  let hash = hasher.finish();
  let (window, cx) = (crate::window(), app());
  let kept = window.use_keyed_state(format!("questionnaire:{}", o.id), cx, |_, cx| {
    build(&o.items, o.shortcuts, hash, cx)
  });
  kept.update(cx, |kept, cx| {
    if kept.hash != hash {
      *kept = build(&o.items, o.shortcuts, hash, cx);
    }
  });
  let kept = kept.read(cx);
  if let Some(error) = &kept.error {
    return error.clone().into_any_element();
  }
  let state = &kept.state;
  let mut flow = q::Questionnaire::new(state).child(q::QuestionnaireProgress::new(state));
  for item in &o.items {
    let name = &item.name;
    let choices = item
      .choices
      .iter()
      .map(|c| q::QuestionnaireChoice::new(state, name, &c.value));
    flow = flow.child(
      q::QuestionnaireItem::new(state, name)
        .child(q::QuestionnaireTitle::new(state, name))
        .child(q::QuestionnaireDescription::new(state, name))
        .child(
          (q::QuestionnaireChoices::new(state, name).children(choices))
            .child(q::QuestionnaireInput::new(state, name.clone())),
        )
        .child(q::QuestionnaireError::new(state, name)),
    );
  }
  let mut flow = flow.child(
    q::QuestionnaireActions::new(state)
      .child(q::QuestionnairePrevious::new(state))
      .child(q::QuestionnaireSkip::new(state))
      .child(q::QuestionnaireNext::new(state))
      .child(q::QuestionnaireSubmit::new(state)),
  );
  Styled::style(&mut flow).refine(&o.style);
  flow.into_any_element()
}

// ---------------------------------------------------------------------------------------------
// FPS HUD

/// The root-owned HUD of each window that shows one
#[derive(Default)]
struct Huds(HashMap<WindowId, (Anchor, Option<Duration>)>);

impl Global for Huds {}

/// The HUD [`ScriptRoot`](crate::ScriptRoot) draws over its window, if shown
pub(crate) fn fps_hud(window: &mut Window, cx: &mut App) -> Option<FpsOverlay> {
  let id = window.window_handle().window_id();
  let (anchor, budget) = *cx.try_global::<Huds>()?.0.get(&id)?;
  let overlay = gpui_fps::fps_monitor(window, cx).anchor(anchor);
  Some(match budget {
    Some(budget) => overlay.frame_budget(budget),
    None => overlay,
  })
}

/// `show_fps_monitor({ anchor?, frame_budget? })`
fn show_fps_monitor<'js>(ctx: Ctx<'js>, options: Opt<Object<'js>>) -> Result<()> {
  let (mut anchor, mut budget) = (Anchor::TopRight, None);
  if let Some(options) = options.0 {
    if let Some(a) = options.get::<_, Option<Value>>("anchor")? {
      anchor = Lit::<Anchors>(a, PhantomData).get::<Anchors>(&ctx)?;
    }
    if let Some(ms) = options.get::<_, Option<f64>>("frame_budget")? {
      budget = Some(millis(&ctx, ms)?);
    }
  }
  let window = window_or_throw(&ctx)?;
  let id = window.window_handle().window_id();
  app()
    .default_global::<Huds>()
    .0
    .insert(id, (anchor, budget));
  window.refresh();
  Ok(())
}

fn hide_fps_monitor(ctx: Ctx<'_>) -> Result<bool> {
  let window = window_or_throw(&ctx)?;
  let id = window.window_handle().window_id();
  let shown = app().default_global::<Huds>().0.remove(&id).is_some();
  window.refresh();
  Ok(shown)
}

fn fps_monitor_visible(ctx: Ctx<'_>) -> Result<bool> {
  let id = window_or_throw(&ctx)?.window_handle().window_id();
  Ok(
    app()
      .try_global::<Huds>()
      .is_some_and(|h| h.0.contains_key(&id)),
  )
}

// ---------------------------------------------------------------------------------------------
// Methods

/// A method's arms: on an element of `$kind`, runs the `let`s (parsing arguments, so another
/// family's elements see theirs unparsed), then `map`s with `$body`; on any other, throws.
macro_rules! on_kind {
  ($c:ident, $n:ident, {
    $($kind:literal => $(let $v:ident = $e:expr;)* |$x:ident: $ty:ty| $body:expr),* $(,)?
  }) => {
    match $n.kind() {
      $($kind => {
        $(let $v = $e;)*
        map($c, $n, |$x: $ty| $body)
      })*
      _ => Err(not_here($c, &**$n)),
    }
  };
}

mod methods {
  use super::*;

  crate::el_methods! {
    kinds: KINDS;
    /// The semantic size of an `Attachment` or `Settings`.
    fn size(size: Lit<'js, Sizes>) |c, n| on_kind!(c, n, {
      "Attachment" => let s = size.get::<Sizes>(c)?;
        |a: Slot<Attachment>| Slot { widget: a.widget.with_size(s), ..a },
      "Settings" => let s = size.get::<Sizes>(c)?; |a: SettingsNode| SettingsNode(a.0.with_size(s)),
    });
    /// The attachment's lifecycle status.
    fn status(status: Lit<'js, Statuses>) |c, n| on_kind!(c, n, {
      "Attachment" => let s = status.get::<Statuses>(c)?;
        |a: Slot<Attachment>| Slot { widget: a.widget.status(s), ..a },
    });
    /// The attachment's layout axis.
    fn axis(axis: Lit<'js, Axes>) |c, n| on_kind!(c, n, {
      "Attachment" => let x = axis.get::<Axes>(c)?;
        |a: Slot<Attachment>| Slot { widget: a.widget.axis(x), ..a },
    });
    /// The sender edge a `Message` or `Bubble` sits on.
    fn alignment(alignment: Lit<'js, Alignments>) |c, n| on_kind!(c, n, {
      "Bubble" => let a = alignment.get::<Alignments>(c)?; |b: Bubble| b.alignment(a),
      "Message" => let a = alignment.get::<Alignments>(c)?;
        |m: Slot<Message>| Slot { widget: m.widget.alignment(a), ..m },
    });
    /// The treatment of a `Bubble` or `Marker`.
    fn variant(variant: Lit<'js, (BubbleVariants, MarkerVariants)>) |c, n| on_kind!(c, n, {
      "Bubble" => let v = variant.get::<BubbleVariants>(c)?; |b: Bubble| b.with_variant(v),
      "Marker" => let v = variant.get::<MarkerVariants>(c)?; |m: Marker| m.with_variant(v),
    });
    /// Whether the marker shows its loading treatment.
    fn loading(loading: bool) |c, n| on_kind!(c, n, { "Marker" => |m: Marker| m.loading(loading) });
    /// The marker's loading treatment.
    fn loading_style(loading_style: Lit<'js, LoadingStyles>) |c, n| on_kind!(c, n, {
      "Marker" => let s = loading_style.get::<LoadingStyles>(c)?;
        |m: Marker| m.with_loading_style(s),
    });
    /// A stable identity for a `ShimmerText`'s animation.
    fn id(id: crate::Id) |c, n| on_kind!(c, n, {
      "ShimmerText" => |s: ShimmerText| s.id(id.into_inner()),
    });
    /// How long one shimmer sweep takes.
    fn duration_ms(duration_ms: f64) |c, n| on_kind!(c, n, {
      "ShimmerText" => let d = millis(c, duration_ms)?; |s: ShimmerText| s.duration(d),
    });
    /// The highlight's half-width, relative to the text.
    fn spread(spread: f32) |c, n| on_kind!(c, n, {
      "ShimmerText" => |s: ShimmerText| s.spread(spread),
    });
    /// Sweeps right to left.
    fn reverse(reverse: bool) |c, n| on_kind!(c, n, {
      "ShimmerText" => |s: ShimmerText| s.reverse(reverse),
    });
    /// Sweeps once instead of looping.
    fn once(once: bool) |c, n| on_kind!(c, n, { "ShimmerText" => |s: ShimmerText| s.once(once) });
    /// Shows the transcript's scrollbar.
    fn scrollbar(scrollbar: bool) |c, n| on_kind!(c, n, {
      "MessageScroller" => |s: MessageScroller| s.scrollbar(scrollbar),
    });
    /// Shows the jump-to-latest button.
    fn jump_button(jump_button: bool) |c, n| on_kind!(c, n, {
      "MessageScroller" => |s: MessageScroller| s.jump_button(jump_button),
    });
    /// The jump-to-latest button's label.
    fn jump_button_label(label: String) |c, n| on_kind!(c, n, {
      "MessageScroller" => |s: MessageScroller| s.with_jump_button_label(label),
    });
    /// Disables an `Editor`, a `SettingItem`'s field, a questionnaire choice or question.
    fn disabled(disabled: bool) |c, n| on_kind!(c, n, {
      "Editor" => |e: Editor| e.disabled(disabled),
      "SettingItem" => |i: ItemNode| ItemNode { disabled, ..i },
      "QuestionnaireChoice" => |d: ChoiceData| ChoiceData { disabled, ..d },
      "QuestionnaireItem" => |d: ItemData| ItemData { disabled, ..d },
      "QuestionnaireInput" => |d: InputData| InputData { disabled, ..d },
    });
    /// Draws the editor's own background and border.
    fn appearance(appearance: bool) |c, n| on_kind!(c, n, {
      "Editor" => |e: Editor| e.appearance(appearance),
    });
    fn bordered(bordered: bool) |c, n| on_kind!(c, n, {
      "Editor" => |e: Editor| e.bordered(bordered),
    });
    fn readonly(readonly: bool) |c, n| on_kind!(c, n, {
      "Editor" => |e: Editor| e.readonly(readonly),
    });
    /// What a screen reader announces.
    fn aria_label(label: String) |c, n| on_kind!(c, n, {
      "Editor" => |e: Editor| e.aria_label(label),
    });
    /// Shows line numbers.
    fn line_number(value: bool) |c, n| on_kind!(c, n, { "Diff" => |d: Diff| d.line_number(value) });
    fn syntax_highlight(value: bool) |c, n| on_kind!(c, n, {
      "Diff" => |d: Diff| d.syntax_highlight(value),
    });
    /// Shows a header above each file.
    fn header_visible(value: bool) |c, n| on_kind!(c, n, {
      "Diff" => |d: Diff| d.header_visible(value),
    });
    /// Tints changed lines.
    fn change_background(value: bool) |c, n| on_kind!(c, n, {
      "Diff" => |d: Diff| d.change_background(value),
    });
    /// Wraps long lines.
    fn soft_wrap(value: bool) |c, n| on_kind!(c, n, { "Diff" => |d: Diff| d.soft_wrap(value) });
    /// How the start of each hunk is marked.
    fn hunk_separator(separator: Lit<'js, HunkSeparators>) |c, n| on_kind!(c, n, {
      "Diff" => let s = separator.get::<HunkSeparators>(c)?; |d: Diff| d.hunk_separator(s),
    });
    /// How changed lines are marked beside the code.
    fn change_indicator(indicator: Lit<'js, ChangeIndicators>) |c, n| on_kind!(c, n, {
      "Diff" => let i = indicator.get::<ChangeIndicators>(c)?; |d: Diff| d.change_indicator(i),
    });
    /// The text under a setting's or question's title.
    fn description(text: String) |c, n| on_kind!(c, n, {
      "SettingItem" => |i: ItemNode| ItemNode { description: Some(text), ..i },
      "SettingGroup" => |g: GroupNode| GroupNode(g.0.description(text)),
      "SettingPage" => |p: PageNode| PageNode(p.0.description(text)),
      "QuestionnaireChoice" => |d: ChoiceData| ChoiceData { description: Some(text), ..d },
      "QuestionnaireItem" => |d: ItemData| ItemData { description: Some(text), ..d },
    });
    /// The axis a setting's label and field are laid out along.
    fn layout(axis: Lit<'js, Axes>) |c, n| on_kind!(c, n, {
      "SettingItem" => let layout = Some(axis.get::<Axes>(c)?); |i: ItemNode| ItemNode { layout, ..i },
    });
    /// Search keywords matching the setting.
    fn keywords(keywords: Vec<String>) |c, n| on_kind!(c, n, {
      "SettingItem" => |i: ItemNode| ItemNode { keywords, ..i },
    });
    /// The setting group's title.
    fn title(text: String) |c, n| on_kind!(c, n, {
      "SettingGroup" => |g: GroupNode| GroupNode(g.0.title(text)),
    });
    /// Renders a `SettingItem`'s field, or what follows a `SettingPage`'s title, lazily.
    fn content(render: Function<'js>) |c, n| on_kind!(c, n, {
      "SettingItem" => let field = Some(Lazy::new(c, render)?); |i: ItemNode| ItemNode { field, ..i },
      "SettingPage" => let lazy = Lazy::new(c, render)?;
        |p: PageNode| PageNode(p.0.title_suffix(move |w, cx| lazy.render(None, w, cx))),
    });
    /// Opens the page's groups.
    fn default_open(default_open: bool) |c, n| on_kind!(c, n, {
      "SettingPage" => |p: PageNode| PageNode(p.0.default_open(default_open)),
    });
    /// Shows the reset control.
    fn resettable(resettable: bool) |c, n| on_kind!(c, n, {
      "SettingPage" => |p: PageNode| PageNode(p.0.resettable(resettable)),
    });
    /// The sidebar's width in pixels.
    fn sidebar_width(pixels: f32) |c, n| on_kind!(c, n, {
      "Settings" => |s: SettingsNode| SettingsNode(s.0.sidebar_width(px(pixels))),
    });
    /// How far the sidebar can be resized, in pixels.
    fn sidebar_size_range(minimum: f32, maximum: f32) |c, n| on_kind!(c, n, {
      "Settings" => |s: SettingsNode| SettingsNode(s.0.sidebar_size_range(px(minimum)..px(maximum))),
    });
    /// The page shown first.
    fn default_selected_page(index: u32) |c, n| on_kind!(c, n, {
      "Settings" => let i = SelectIndex { page_ix: index as usize, group_ix: None };
        |s: SettingsNode| SettingsNode(s.0.default_selected_index(i)),
    });
    /// Selects the choice at first.
    fn default_selected(value: bool) |c, n| on_kind!(c, n, {
      "QuestionnaireChoice" => |d: ChoiceData| ChoiceData { default_selected: value, ..d },
    });
    /// Requires an answer; hides Skip.
    fn required(value: bool) |c, n| on_kind!(c, n, {
      "QuestionnaireItem" => |d: ItemData| ItemData { required: value, ..d },
    });
    /// Accepts several choices.
    fn multiple(value: bool) |c, n| on_kind!(c, n, {
      "QuestionnaireItem" => |d: ItemData| ItemData { multiple: value, ..d },
    });
    /// Gives each choice of the active question a letter or number shortcut.
    fn shortcuts(mode: Lit<'js, ShortcutModes>) |c, n| on_kind!(c, n, {
      "Questionnaire" => let shortcuts = Some(mode.get::<ShortcutModes>(c)?);
        |q: QuestionnaireNode| QuestionnaireNode { shortcuts, ..q },
    });
    /// `handler(url, cx)` on a link click, instead of opening it.
    fn on_link_click(handler: On<'js, Url>) |c, n| on_kind!(c, n, {
      "TextView" => let l = frame().listener(handler.0, |url: &gpui_kit::SharedString| Url(url.to_string()));
        |t: TextView| t.on_link_click(move |url, _, w, cx| l(url, w, cx)),
    });
    /// Whether the text can be selected; on when left out.
    fn selectable(value: Opt<bool>) |c, n| on_kind!(c, n, {
      "TextView" => |t: TextView| t.selectable(value.0.unwrap_or(true)),
    });
    /// Whether the view scrolls its content; on when left out.
    fn scrollable(value: Opt<bool>) |c, n| on_kind!(c, n, {
      "TextView" => |t: TextView| t.scrollable(value.0.unwrap_or(true)),
    });
    /// Where in its `relative()` parent the HUD sits.
    fn anchor(anchor: Lit<'js, Anchors>) |c, n| on_kind!(c, n, {
      "fps_monitor" => let a = anchor.get::<Anchors>(c)?; |f: FpsOverlay| f.anchor(a),
    });
    /// The frame budget in milliseconds the HUD grades against.
    fn frame_budget(ms: f64) |c, n| on_kind!(c, n, {
      "fps_monitor" => let b = millis(c, ms)?; |f: FpsOverlay| f.frame_budget(b),
    });
  }
}

// ---------------------------------------------------------------------------------------------
// Exports

fn func<'js, P, F: rquickjs::function::IntoJsFunc<'js, P> + 'js>(
  ctx: &Ctx<'js>,
  name: &str,
  f: F,
) -> Result<Value<'js>> {
  constructor(ctx, name, Function::new(ctx.clone(), f)?)
}

fn exports<'js>(ctx: &Ctx<'js>) -> Result<Vec<(&'static str, Value<'js>)>> {
  Ok(vec![
    (
      "Attachment",
      func(ctx, "Attachment", |ctx: Ctx<'js>, id: String| {
        new_el(&ctx, Slot::new(Attachment::new().id(id)))
      })?,
    ),
    (
      "Bubble",
      func(ctx, "Bubble", |ctx: Ctx<'js>| new_el(&ctx, Bubble::new()))?,
    ),
    (
      "Marker",
      func(ctx, "Marker", |ctx: Ctx<'js>, id: String| {
        new_el(&ctx, Marker::new().id(id))
      })?,
    ),
    (
      "Message",
      func(ctx, "Message", |ctx: Ctx<'js>| {
        new_el(&ctx, Slot::new(Message::new()))
      })?,
    ),
    (
      "ShimmerText",
      func(ctx, "ShimmerText", |ctx: Ctx<'js>, text: String| {
        new_el(&ctx, ShimmerText::new(text))
      })?,
    ),
    (
      "MessageScrollerState",
      func(ctx, "MessageScrollerState", |ctx: Ctx<'js>, count: u32| {
        let count = crate::element::items(&ctx, count)?;
        let entity = app().new(|cx| scroller::MessageScrollerState::new(count, cx));
        let state = Retained::new(&ctx, entity)?;
        Class::instance(ctx, ScrollerState { state })
      })?,
    ),
    (
      "MessageScroller",
      func(ctx, "MessageScroller", message_scroller)?,
    ),
    ("EditorState", func(ctx, "EditorState", editor_state)?),
    (
      "Editor",
      func(
        ctx,
        "Editor",
        |ctx: Ctx<'js>, state: Class<'js, EditorState>| {
          let state = state.borrow().state.entity(&ctx)?.clone();
          new_el(&ctx, Editor::new(&state))
        },
      )?,
    ),
    ("DiffState", func(ctx, "DiffState", diff_state)?),
    (
      "Diff",
      func(
        ctx,
        "Diff",
        |ctx: Ctx<'js>, state: Class<'js, DiffState>| {
          let state = state.borrow().state.entity(&ctx)?.clone();
          new_el(&ctx, Diff::new(&state))
        },
      )?,
    ),
    (
      "Text",
      func(ctx, "Text", |ctx: Ctx<'js>, value: String| {
        new_el(&ctx, div().child(Text::from(value)))
      })?,
    ),
    (
      "Settings",
      func(ctx, "Settings", |ctx: Ctx<'js>, id: String| {
        new_el(&ctx, SettingsNode(Settings::new(id)))
      })?,
    ),
    (
      "SettingPage",
      func(ctx, "SettingPage", |ctx: Ctx<'js>, title: String| {
        new_el(&ctx, PageNode(SettingPage::new(title)))
      })?,
    ),
    (
      "SettingGroup",
      func(ctx, "SettingGroup", |ctx: Ctx<'js>| {
        new_el(&ctx, GroupNode(SettingGroup::new()))
      })?,
    ),
    (
      "SettingItem",
      func(ctx, "SettingItem", |ctx: Ctx<'js>, title: String| {
        let item = ItemNode {
          title,
          field: None,
          description: None,
          layout: None,
          keywords: Vec::new(),
          disabled: false,
        };
        new_el(&ctx, item)
      })?,
    ),
    (
      "Questionnaire",
      func(ctx, "Questionnaire", |ctx: Ctx<'js>, id: String| {
        let node = QuestionnaireNode {
          id,
          shortcuts: None,
          items: Vec::new(),
          style: StyleRefinement::default(),
        };
        new_el(&ctx, node)
      })?,
    ),
    (
      "QuestionnaireItem",
      func(
        ctx,
        "QuestionnaireItem",
        |ctx: Ctx<'js>, name: String, label: String| {
          let item = ItemData {
            name,
            label,
            description: None,
            required: false,
            multiple: false,
            disabled: false,
            choices: Vec::new(),
            input: None,
          };
          new_el(&ctx, item)
        },
      )?,
    ),
    (
      "QuestionnaireInput",
      func(
        ctx,
        "QuestionnaireInput",
        |ctx: Ctx<'js>, state: Class<'js, super::forms::JsInputState>, label: String| {
          let state = super::forms::JsInputState::get(&state)?;
          new_el(
            &ctx,
            InputData {
              state,
              label,
              disabled: false,
            },
          )
        },
      )?,
    ),
    (
      "QuestionnaireChoice",
      func(
        ctx,
        "QuestionnaireChoice",
        |ctx: Ctx<'js>, value: String, label: String| {
          let choice = ChoiceData {
            value,
            label,
            description: None,
            disabled: false,
            default_selected: false,
          };
          new_el(&ctx, choice)
        },
      )?,
    ),
  ])
}

const D_TS: &str = r#"  export type ComponentSize = "xsmall" | "small" | "medium" | "large";
  /** A file or image attachment; its children fill its content. */
  export const Attachment: { new (id: string): Element; new: (id: string) => Element };
  /** A message bubble; its children are its content. */
  export const Bubble: { new (): Element; new: () => Element };
  /** A compact conversation status marker. */
  export const Marker: { new (id: string): Element; new: (id: string) => Element };
  /** A message row; its children fill its content. */
  export const Message: { new (): Element; new: () => Element };
  /** Animated loading text. */
  export const ShimmerText: { new (text: string): Element; new: (text: string) => Element };
  /** Retained virtual-list and tail-following state for a message transcript. */
  export interface MessageScrollerState {
    item_count(): number;
    /** Replaces every row with `count` new ones. */
    reset(count: number): void;
    /** Adds `count` rows at the end, following them when at the tail. */
    append(count: number): boolean;
    scroll_to_end(): void;
    is_following_tail(): boolean;
    release(): boolean;
  }
  export const MessageScrollerState: {
    (item_count: number): MessageScrollerState;
    new (item_count: number): MessageScrollerState;
    new: (item_count: number) => MessageScrollerState;
  };
  /** A virtualized message transcript; `render_item(index)` renders a row. */
  export const MessageScroller: {
    new (id: string, state: MessageScrollerState, render_item: (index: number) => Element | null): Element;
    new: (id: string, state: MessageScrollerState, render_item: (index: number) => Element | null) => Element;
  };
  /** Retained source-editor state. Made in `init` or a handler: it needs the window. */
  export interface EditorState {
    value(): string;
    /** Replaces the text, without a `change` event. */
    set_value(text: string): void;
    /** `"change"` and `"enter"` pass the text, `"focus"` and `"blur"` nothing; false for another name. */
    on(event: "change" | "enter" | "focus" | "blur", handler: (value: string | undefined, cx: Context) => void): boolean;
    release(): boolean;
  }
  export const EditorState: {
    (initial_value: string, language?: "rust" | "json"): EditorState;
    new (initial_value: string, language?: "rust" | "json"): EditorState;
    new: (initial_value: string, language?: "rust" | "json") => EditorState;
  };
  /** A source editor; takes no children. */
  export const Editor: { new (state: EditorState): Element; new: (state: EditorState) => Element };
  /** Retained readonly patch state, from unified or Git diff text. */
  export interface DiffState {
    /** Shows another patch. */
    set_patch(patch: string): void;
    release(): boolean;
  }
  export const DiffState: {
    (patch: string, mode?: "unified" | "split"): DiffState;
    new (patch: string, mode?: "unified" | "split"): DiffState;
    new: (patch: string, mode?: "unified" | "split") => DiffState;
  };
  /** A readonly patch viewer; takes no children. */
  export const Diff: { new (state: DiffState): Element; new: (state: DiffState) => Element };
  /** Plain text in a styleable wrapper. */
  export const Text: { new (value: string): Element; new: (value: string) => Element };
  /** A settings surface; takes `SettingPage` children. */
  export const Settings: { new (id: string): Element; new: (id: string) => Element };
  /** A settings page; takes `SettingGroup` children. */
  export const SettingPage: { new (title: string): Element; new: (title: string) => Element };
  /** A settings group; takes `SettingItem` children. */
  export const SettingGroup: { new (): Element; new: () => Element };
  /** One setting; `content(() => field)` renders its field. */
  export const SettingItem: { new (title: string): Element; new: (title: string) => Element };
  /** A questionnaire keyed by `id`; takes `QuestionnaireItem` children, rebuilt when they change. */
  export const Questionnaire: { new (id: string): Element; new: (id: string) => Element };
  /** One question; takes `QuestionnaireChoice` children. */
  export const QuestionnaireItem: { new (name: string, label: string): Element; new: (name: string, label: string) => Element };
  /** A free-form answer of a question, typed into `state`. */
  export const QuestionnaireInput: { new (state: InputState, label: string): Element; new: (state: InputState, label: string) => Element };
  /** One answer of a question. */
  export const QuestionnaireChoice: { new (value: string, label: string): Element; new: (value: string, label: string) => Element };
"#;

pub(crate) const FAMILY: Family = Family {
  module: "corona/component",
  names: &[
    "Attachment",
    "Bubble",
    "Marker",
    "Message",
    "ShimmerText",
    "MessageScrollerState",
    "MessageScroller",
    "EditorState",
    "Editor",
    "DiffState",
    "Diff",
    "Text",
    "Settings",
    "SettingPage",
    "SettingGroup",
    "SettingItem",
    "Questionnaire",
    "QuestionnaireItem",
    "QuestionnaireInput",
    "QuestionnaireChoice",
  ],
  exports,
  methods: methods::install,
  d_ts: || D_TS.into(),
  el_d_ts: methods::d_ts,
};

/// `view` that loads no image but `data:` ones, and opens only http(s) links, unless a script
/// handles the click. A `TextView`'s defaults fetch URLs and open any scheme.
fn sandboxed(view: TextView) -> TextView {
  let images = std::sync::Mutex::new(std::collections::HashMap::<String, Arc<Image>>::new());
  view
    .image_source(move |uri| {
      let mut images = images.lock().unwrap_or_else(|e| e.into_inner());
      let image = images
        .entry(uri.to_string())
        .or_insert_with(|| data_image(uri));
      ImageSource::Image(image.clone())
    })
    .on_link_click(|url, _, _, cx| {
      if crate::view::web_url(url) {
        cx.open_url(url);
      }
    })
}

/// The image a base64 `data:` URL carries; an undecodable one for anything else
fn data_image(uri: &str) -> Arc<Image> {
  use base64::Engine as _;
  let image = uri
    .strip_prefix("data:")
    .and_then(|r| r.split_once(','))
    .and_then(|(head, body)| {
      let (mime, "base64") = head.rsplit_once(';')? else {
        return None;
      };
      let format = ImageFormat::from_mime_type(mime)?;
      let bytes = base64::engine::general_purpose::STANDARD
        .decode(body)
        .ok()?;
      Some(Image::from_bytes(format, bytes))
    })
    .unwrap_or_else(|| Image::from_bytes(ImageFormat::Png, Vec::new()));
  Arc::new(image)
}

fn base_exports<'js>(ctx: &Ctx<'js>) -> Result<Vec<(&'static str, Value<'js>)>> {
  let text_view = Object::new(ctx.clone())?;
  let html =
    |ctx: Ctx<'js>, id: String, html: String| new_el(&ctx, sandboxed(TextView::html(id, html)));
  let md =
    |ctx: Ctx<'js>, id: String, md: String| new_el(&ctx, sandboxed(TextView::markdown(id, md)));
  text_view.set("html", Function::new(ctx.clone(), html)?)?;
  text_view.set("markdown", Function::new(ctx.clone(), md)?)?;
  Ok(vec![("TextView", text_view.into_value())])
}

/// `TextView` of `corona/base`
pub(crate) const BASE: Family = Family {
  module: "corona/base",
  names: &["TextView"],
  exports: base_exports,
  methods: |_| Ok(()),
  d_ts: || {
    r#"  /** Rich HTML or Markdown text. */
  export const TextView: {
    html(id: string, html: string): Element;
    markdown(id: string, markdown: string): Element;
  };
"#
    .into()
  },
  el_d_ts: String::new,
};

fn fps_exports<'js>(ctx: &Ctx<'js>) -> Result<Vec<(&'static str, Value<'js>)>> {
  let monitor = |ctx: Ctx<'js>| {
    let overlay = gpui_fps::fps_monitor(window_or_throw(&ctx)?, app());
    new_el(&ctx, overlay)
  };
  let f = |f: Function<'js>| f.into_value();
  Ok(vec![
    ("fps_monitor", f(Function::new(ctx.clone(), monitor)?)),
    (
      "show_fps_monitor",
      f(Function::new(ctx.clone(), show_fps_monitor)?),
    ),
    (
      "hide_fps_monitor",
      f(Function::new(ctx.clone(), hide_fps_monitor)?),
    ),
    (
      "fps_monitor_visible",
      f(Function::new(ctx.clone(), fps_monitor_visible)?),
    ),
  ])
}

/// The FPS HUD, `corona/fps`
pub(crate) const FPS: Family = Family {
  module: "corona/fps",
  names: &[
    "fps_monitor",
    "show_fps_monitor",
    "hide_fps_monitor",
    "fps_monitor_visible",
  ],
  exports: fps_exports,
  methods: |_| Ok(()),
  d_ts: || {
    r#"  import { Element } from "corona/kit";
  export type Anchor = "top_left" | "top_right" | "bottom_left" | "bottom_right" | "top_center" | "bottom_center" | "left_center" | "right_center";
  /** The performance HUD as an element; its parent must be `relative()`. */
  export function fps_monitor(): Element;
  export interface FpsMonitorOptions {
    /** Default `top_right`. */
    anchor?: Anchor;
    /** Frame budget in milliseconds. */
    frame_budget?: number;
  }
  /** Draws the HUD over the window until `hide_fps_monitor()`; again, it moves it. */
  export function show_fps_monitor(options?: FpsMonitorOptions): void;
  /** Takes the HUD down; true if it was up. */
  export function hide_fps_monitor(): boolean;
  export function fps_monitor_visible(): boolean;
"#
    .into()
  },
  el_d_ts: String::new,
};

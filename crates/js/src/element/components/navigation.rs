//! Navigation: tabs, accordions, pagination, collapsibles, resizable panes, scrollbars,
//! virtual lists, steppers, sidebars, breadcrumbs, toolbars, menus and the command palette.
//!
//! [`BASE`] binds `gpui_kit::base` widgets for `corona/base`, [`FAMILY`] `gpui_kit::component`
//! ones for `corona/component`; `Tab`, `Accordion`, `Collapsible`, `Pagination` and `Scrollbar`
//! are in both. Containers that take one kind of child (`TabBar`, `Stepper`, `Sidebar`,
//! `Menu`, ..) adopt it through their own `child`.

use std::{any::Any, cell::RefCell, marker::PhantomData, mem, rc::Rc};

use gpui_kit::{
  AnyElement, App, AppContext, Axis, ClickEvent, Div, IntoElement, Overflow, ParentElement,
  ScrollStrategy, StatefulInteractiveElement, StyleRefinement, Styled, Window,
  base::{self as b, PaginationItem, ScrollbarAxis, ScrollbarMode},
  component::{
    self as c, Disableable, IconName, IndexPath, Selectable, Side, Sizable,
    button::{Button, ButtonVariants},
    command::{Command, CommandGroup, CommandItem, CommandState},
    group_box::{GroupBoxVariant, GroupBoxVariants},
    menu::{DropdownMenu, PopupMenuItem},
    native_menu::NativeMenu,
    scroll::{Scrollable, ScrollableElement},
    sidebar::{
      Sidebar, SidebarCollapsible, SidebarFooter, SidebarHeader, SidebarMenu, SidebarMenuItem,
      SidebarToggleButton,
    },
    tab::TabVariant,
  },
  div, px, size,
};
use rquickjs::{
  Class, Ctx, Exception, FromJs, Function, IntoJs, JsLifetime, Object, Result, Value,
  class::Trace,
  function::{Args, IntoJsFunc},
};

use super::{Family, SizeArg, constructor};
use crate::{
  Child, Color, El, Id, Named, Node, On, Opt, Payload, Retained, ScriptView, ScrollHandle, TsType,
  ViewCx, app,
  element::{I, events::Pt},
  frame, frame_or_throw, new_el,
  runtime::runtime,
};

pub(crate) const BASE: Family = Family {
  module: "corona/base",
  names: &[
    "Tabs",
    "Tab",
    "Accordion",
    "AccordionItem",
    "AccordionHeader",
    "AccordionPanel",
    "AccordionTrigger",
    "Pagination",
    "pagination_items",
    "Collapsible",
    "h_resizable",
    "v_resizable",
    "resizable_panel",
    "Scrollbar",
    "v_virtual_list",
    "h_virtual_list",
    "VirtualListScrollHandle",
  ],
  exports: base_exports,
  // `FAMILY` installs the methods of both
  methods: |_| Ok(()),
  d_ts: || BASE_D_TS.into(),
  el_d_ts: String::new,
};

pub(crate) const FAMILY: Family = Family {
  module: "corona/component",
  names: &[
    "TabBar",
    "Tabs",
    "Tab",
    "Accordion",
    "AccordionItem",
    "Collapsible",
    "Pagination",
    "Stepper",
    "StepperItem",
    "Breadcrumb",
    "GroupBox",
    "StatusBar",
    "Toolbar",
    "Separator",
    "VerticalSeparator",
    "DashedSeparator",
    "VerticalDashedSeparator",
    "Resizable",
    "ResizablePanel",
    "Scroll",
    "Scrollbar",
    "ScrollbarHandle",
    "Sidebar",
    "SidebarHeader",
    "SidebarFooter",
    "SidebarMenu",
    "SidebarMenuItem",
    "SidebarToggleButton",
    "DropdownMenu",
    "NativeMenuTrigger",
    "NativeMenuItem",
    "NativeMenuSeparator",
    "CommandState",
    "Command",
    "CommandItem",
    "CommandGroup",
    "CommandSeparator",
    "Menu",
    "MenuItem",
    "MenuSeparator",
    "MenuBar",
    "DropdownButton",
  ],
  exports: component_exports,
  methods: install,
  d_ts: || COMPONENT_D_TS.into(),
  el_d_ts,
};

// ---------------------------------------------------------------- nodes

crate::node!(b::Tabs, "Tabs", style, interactive, parent);
crate::node!(b::Tab, "Tab", style, interactive, parent);
crate::node!(b::Accordion, "Accordion", style, interactive, parent);
crate::node!(b::AccordionItem, "AccordionItem", style, parent);
crate::node!(b::AccordionHeader, "AccordionHeader", style, parent);
crate::node!(b::AccordionPanel, "AccordionPanel", style, parent);
crate::node!(
  b::AccordionTrigger,
  "AccordionTrigger",
  style,
  interactive,
  parent
);
crate::node!(b::Pagination, "Pagination", style, interactive, parent);
crate::node!(b::Collapsible, "Collapsible", style, parent);
crate::node!(b::ResizablePanel, "ResizablePanel", style, parent);
crate::node!(b::Scrollbar, "Scrollbar");
/// A virtual list, with the row handlers `on_item_click` set after it was made
struct VList {
  list: b::VirtualList,
  /// The scroll registry name a `Scrollbar` finds it by
  name: Option<String>,
  hooks: Rc<RefCell<RowHooks>>,
}

/// What a virtual list's rows report to, as view slots
#[derive(Default)]
struct RowHooks {
  click: Option<u64>,
  secondary: Option<u64>,
}

impl Node for VList {
  fn kind(&self) -> &'static str {
    "VirtualList"
  }
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    Some(Styled::style(&mut self.list))
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    self.list.into_any_element()
  }
  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }
}

/// A row key and the press, what `on_item_secondary_click` gets as `(key, event)`
struct KeyPress(String, crate::element::events::MouseButtonEvent);

impl<'js> IntoJs<'js> for KeyPress {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    vec![self.0.into_js(ctx)?, self.1.into_js(ctx)?].into_js(ctx)
  }
}

/// The scroll handle a `Scrollbar` names `name` by: a scroll area's or a virtual list's
fn named_scroll(ctx: &Ctx<'_>, name: &str) -> Result<AnyScroll> {
  let view = frame_or_throw(ctx)?.view.entity_id();
  Ok(runtime(ctx)?.keyed(view, format!("scroll:{name}"), || {
    AnyScroll::Plain(gpui_kit::ScrollHandle::new())
  }))
}

/// Points scroll registry name `name` at `handle`
fn name_scroll(ctx: &Ctx<'_>, name: &str, handle: AnyScroll) -> Result<()> {
  let view = frame_or_throw(ctx)?.view.entity_id();
  runtime(ctx)?.set_keyed(view, format!("scroll:{name}"), handle);
  Ok(())
}

/// The id name of an element, if it has one
fn name_of(n: &mut Box<dyn Node>) -> Option<String> {
  match n.interactivity()?.element_id.as_ref()? {
    gpui_kit::ElementId::Name(name) => Some(name.to_string()),
    _ => None,
  }
}

/// A `div` named by `id(..)` that scrolls: the handle its name stands for in the registry
fn link_scroll(c: &Ctx<'_>, n: &mut Box<dyn Node>) -> Result<()> {
  let Some(name) = name_of(n) else {
    return Ok(());
  };
  let overflow = n.style().map(|s| s.overflow.clone()).unwrap_or_default();
  let scrolls = [overflow.x, overflow.y].contains(&Some(Overflow::Scroll));
  if !scrolls {
    return Ok(());
  }
  if let AnyScroll::Plain(handle) = named_scroll(c, &name)? {
    crate::element::interact(c, n, |i| {
      gpui_kit::StatefulInteractiveElement::track_scroll(i, &handle)
    })?;
  }
  Ok(())
}
crate::node!(Scrollable<Div>, "Scrollable", style, interactive, parent);
crate::node!(c::tab::TabBar, "TabBar", style);
crate::node!(c::tab::Tab, "Tab", style, interactive, parent);
crate::node!(c::accordion::Accordion, "Accordion", style);
crate::node!(c::accordion::AccordionItem, "AccordionItem", style, parent);
crate::node!(c::collapsible::Collapsible, "Collapsible", style, parent);
crate::node!(c::pagination::Pagination, "Pagination", style);
crate::node!(c::stepper::Stepper, "Stepper", style);
crate::node!(c::stepper::StepperItem, "StepperItem", style, parent);
crate::node!(c::breadcrumb::Breadcrumb, "Breadcrumb", style);
crate::node!(c::group_box::GroupBox, "GroupBox", style, parent);
crate::node!(c::status_bar::StatusBar, "StatusBar", style, parent);
crate::node!(c::toolbar::Toolbar, "Toolbar", style, parent);
crate::node!(c::separator::Separator, "Separator", style);
crate::node!(Sidebar<SidebarMenu>, "Sidebar", style);
crate::node!(SidebarHeader, "SidebarHeader", style, interactive, parent);
crate::node!(SidebarFooter, "SidebarFooter", style, interactive, parent);
crate::node!(SidebarToggleButton, "SidebarToggleButton");
crate::node!(Command, "Command", style);

/// [`Node`] for parts that only mean something inside their container, which renders them
macro_rules! part {
  ($($ty:ty => $kind:literal),* $(,)?) => {$(
    impl Node for $ty {
      fn kind(&self) -> &'static str {
        $kind
      }
      fn into_element(self: Box<Self>) -> AnyElement {
        div().into_any_element()
      }
      fn into_any(self: Box<Self>) -> Box<dyn Any> {
        self
      }
    }
  )*};
}

part! {
  AppMenu => "Menu",
  MenuEntry => "MenuItem",
  MenuSeparator => "MenuSeparator",
  SidebarMenu => "SidebarMenu",
  SideItem => "SidebarMenuItem",
  CommandItem => "CommandItem",
  CommandGroup => "CommandGroup",
  CommandSeparator => "CommandSeparator",
  NativeItem => "NativeMenuItem",
  NativeSeparator => "NativeMenuSeparator",
}

/// A `SidebarMenuItem` and its submenu, which `SidebarMenuItem::children` sets all at once
struct SideItem {
  item: SidebarMenuItem,
  children: Vec<SideItem>,
}

impl SideItem {
  fn build(self) -> SidebarMenuItem {
    match self.children.is_empty() {
      true => self.item,
      false => (self.item).children(self.children.into_iter().map(SideItem::build)),
    }
  }
}

struct CommandSeparator;

struct NativeItem {
  label: String,
  action: String,
  disabled: bool,
  checked: bool,
}

struct NativeSeparator;

/// A row or column of resizable panels; styles land on the frame it fills
struct Group {
  frame: Div,
  group: b::ResizablePanelGroup,
}

impl Node for Group {
  fn kind(&self) -> &'static str {
    "Resizable"
  }
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    Some(Styled::style(&mut self.frame))
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    self.frame.child(self.group).into_any_element()
  }
  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }
}

/// A scroll area tracking a `ScrollbarHandle`
struct Scroll(Div);

impl Node for Scroll {
  fn kind(&self) -> &'static str {
    "Scroll"
  }
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    Some(Styled::style(&mut self.0))
  }
  fn interactivity(&mut self) -> Option<&mut gpui_kit::Interactivity> {
    Some(gpui_kit::InteractiveElement::interactivity(&mut self.0))
  }
  fn add_child(&mut self, child: AnyElement) -> bool {
    self.0.extend([child]);
    true
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    self.0.into_any_element()
  }
  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }
}

type Listener = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A button opening a menu of items, each calling its handler
struct Dropdown {
  button: Button,
  items: Vec<(String, Listener)>,
}

impl Node for Dropdown {
  fn kind(&self) -> &'static str {
    "DropdownMenu"
  }
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    Some(Styled::style(&mut self.button))
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    let items = self.items;
    let menu = self.button.dropdown_menu(move |menu, _, _| {
      items.iter().fold(menu, |menu, (label, l)| {
        let l = l.clone();
        menu.item(PopupMenuItem::new(label.clone()).on_click(move |e, w, cx| l(e, w, cx)))
      })
    });
    menu.into_any_element()
  }
  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }
}

/// A button popping up a native menu whose items dispatch actions
struct NativeTrigger {
  button: Button,
  items: Vec<Option<NativeItem>>,
}

impl Node for NativeTrigger {
  fn kind(&self) -> &'static str {
    "NativeMenuTrigger"
  }
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    Some(Styled::style(&mut self.button))
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    let items = Rc::new(self.items);
    let button = self.button.on_click(move |e, window, cx| {
      let action = |a: &str| {
        Box::new(crate::element::events::ScriptAction(
          a.to_string().into(),
          0,
        )) as _
      };
      let menu = items
        .iter()
        .fold(NativeMenu::new(), |menu, item| match item {
          None => menu.separator(),
          Some(i) if i.disabled => {
            menu.menu_with_disabled(i.label.clone(), true, action(&i.action))
          }
          Some(i) if i.checked => menu.menu_with_check(i.label.clone(), true, action(&i.action)),
          Some(i) => menu.menu(i.label.clone(), action(&i.action)),
        });
      menu.show(e.position(), window, cx);
    });
    button.into_any_element()
  }
  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }
}

/// One entry of a `Menu`: an item dispatching `action`, or a separator
#[derive(Clone, Hash)]
struct MenuEntry {
  label: String,
  action: String,
  disabled: bool,
  checked: bool,
}

struct MenuSeparator;

/// An application menu of a `MenuBar`
#[derive(Clone, Hash)]
struct AppMenu {
  label: String,
  disabled: bool,
  entries: Vec<Option<MenuEntry>>,
}

impl AppMenu {
  fn build(&self) -> gpui_kit::Menu {
    let items = self.entries.iter().map(|e| match e {
      None => gpui_kit::MenuItem::separator(),
      Some(i) => {
        let action = crate::element::events::ScriptAction(i.action.clone().into(), 0);
        (gpui_kit::MenuItem::action(i.label.clone(), action))
          .disabled(i.disabled)
          .checked(i.checked)
      }
    });
    gpui_kit::Menu::new(self.label.clone())
      .disabled(self.disabled)
      .items(items)
  }
}

/// The application menus, installed when they change, and the in-window bar showing them
#[derive(IntoElement)]
struct MenuBar {
  id: String,
  menus: Vec<AppMenu>,
}

impl gpui_kit::RenderOnce for MenuBar {
  fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
    use std::hash::{Hash, Hasher};
    let bar = window.use_keyed_state(format!("menu-bar:{}", self.id), cx, |_, cx| {
      c::menu::AppMenuBar::new(cx)
    });
    let bar = bar.read(cx).clone();
    let mut hasher = std::hash::DefaultHasher::new();
    self.menus.hash(&mut hasher);
    let revision = hasher.finish();
    // made when the bar first draws, released when it no longer does: it puts back the menus
    // that were there before
    let guard = window.use_keyed_state(format!("menu-bar-guard:{}", self.id), cx, |_, cx| {
      let previous = cx.get_menus().unwrap_or_default();
      cx.on_release(|guard: &mut MenuGuard, cx| {
        let previous = std::mem::take(&mut guard.previous);
        cx.set_menus(previous.iter().map(restore_menu));
        b::GlobalState::global_mut(cx).set_app_menus(previous);
      })
      .detach();
      MenuGuard {
        previous,
        revision: 0,
      }
    });
    if guard.read(cx).revision != revision {
      guard.update(cx, |g, _| g.revision = revision);
      let owned = self.menus.iter().map(|m| m.build().owned()).collect();
      cx.set_menus(self.menus.iter().map(AppMenu::build));
      b::GlobalState::global_mut(cx).set_app_menus(owned);
      bar.update(cx, |bar, cx| bar.reload(cx));
    }
    div().child(bar)
  }
}

/// The application menus a `MenuBar` replaced, and the revision of its own
struct MenuGuard {
  previous: Vec<gpui_kit::OwnedMenu>,
  revision: u64,
}

/// A menu `get_menus` answered, as `set_menus` takes it
fn restore_menu(menu: &gpui_kit::OwnedMenu) -> gpui_kit::Menu {
  use gpui_kit::{MenuItem, OwnedMenuItem};
  let items = menu.items.iter().map(|item| match item {
    OwnedMenuItem::Separator => MenuItem::Separator,
    OwnedMenuItem::Submenu(menu) => MenuItem::Submenu(restore_menu(menu)),
    OwnedMenuItem::SystemMenu(m) => MenuItem::SystemMenu(gpui_kit::OsMenu {
      name: m.name.clone(),
      menu_type: m.menu_type,
    }),
    OwnedMenuItem::Action {
      name,
      action,
      os_action,
      checked,
      disabled,
    } => MenuItem::Action {
      name: name.clone().into(),
      action: action.boxed_clone(),
      os_action: *os_action,
      checked: *checked,
      disabled: *disabled,
    },
  });
  gpui_kit::Menu {
    name: menu.name.clone(),
    items: items.collect(),
    disabled: menu.disabled,
  }
}

/// A button with a menu of `menu_item`s
struct DropButton {
  button: c::button::DropdownButton,
  action: Button,
  anchor: gpui_kit::Anchor,
  items: Vec<(String, Listener)>,
}

impl Node for DropButton {
  fn kind(&self) -> &'static str {
    "DropdownButton"
  }
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    Some(Styled::style(&mut self.button))
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    let mut button = self.button.button(self.action);
    let items = self.items;
    if !items.is_empty() {
      button = button.dropdown_menu_with_anchor(self.anchor, move |menu, _, _| {
        items.iter().fold(menu, |menu, (label, l)| {
          let l = l.clone();
          menu.item(PopupMenuItem::new(label.clone()).on_click(move |e, w, cx| l(e, w, cx)))
        })
      });
    }
    button.into_any_element()
  }
  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }
}

impl Node for MenuBar {
  fn kind(&self) -> &'static str {
    "MenuBar"
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    self.into_any_element()
  }
  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }
}

// ---------------------------------------------------------------- argument and payload types

/// Whether `n` is a `T`
fn is<T: Node>(n: &dyn Node) -> bool {
  (n as &dyn Any).is::<T>()
}

/// A miss: the method of the same name defined before this one gets the element
fn not_on(ctx: &Ctx<'_>, n: &dyn Node) -> rquickjs::Error {
  crate::miss(ctx, n.kind())
}

/// Applies the branch for the concrete type of `n`; other kinds are a miss.
macro_rules! on {
  ($c:expr, $n:expr, $($t:ty => $f:expr),+ $(,)?) => {{
    let (c, n): (&Ctx<'_>, &mut Box<dyn Node>) = ($c, $n);
    $(if is::<$t>(&**n) {
      return crate::map::<$t>(c, n, $f);
    })+
    Err(not_on(c, &**n))
  }};
}

/// The element of a child, which must be a `T`
fn part<T: Node>(ctx: &Ctx<'_>, child: Child<'_>, want: &str) -> Result<T> {
  let Child::El(el) = child else {
    return Err(Exception::throw_type(ctx, &format!("expected {want}")));
  };
  let node = frame_or_throw(ctx)?.take(ctx, &el.borrow())?;
  match is::<T>(&*node) {
    true => Ok(*node.into_any().downcast::<T>().expect("checked")),
    false => Err(Exception::throw_type(
      ctx,
      &format!("expected {want}, not {}", node.kind()),
    )),
  }
}

/// The gpui element of a child
fn element(ctx: &Ctx<'_>, child: Child<'_>) -> Result<AnyElement> {
  frame_or_throw(ctx)?.child(ctx, child)
}

/// An argument converted only once the element turned out to take it, so that a method of
/// the same name for other kinds still gets it as it came
pub struct Later<'js, T>(Value<'js>, PhantomData<T>);

impl<'js, T> FromJs<'js> for Later<'js, T> {
  fn from_js(_: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    Ok(Self(value, PhantomData))
  }
}

impl<T: TsType> TsType for Later<'_, T> {
  fn ts() -> String {
    T::ts()
  }
}

impl<'js, T: FromJs<'js>> Later<'js, T> {
  fn get(self) -> Result<T> {
    T::from_js(&self.0.ctx().clone(), self.0)
  }
}

fn unknown(ctx: &Ctx<'_>, what: &str, name: &str, names: &[&str]) -> rquickjs::Error {
  Exception::throw_type(ctx, &crate::element::style::unknown(what, name, names))
}

/// The value `names` pairs with `name`, or a JS error naming the choices
fn pick<T: Clone>(ctx: &Ctx<'_>, what: &str, name: &str, names: &[(&str, T)]) -> Result<T> {
  match names.iter().find(|(n, _)| *n == name) {
    Some((_, v)) => Ok(v.clone()),
    None => {
      let all: Vec<&str> = names.iter().map(|(n, _)| *n).collect();
      Err(unknown(ctx, what, name, &all))
    }
  }
}

const SIDES: &[(&str, Side)] = &[("left", Side::Left), ("right", Side::Right)];
const AXES: &[(&str, Axis)] = &[
  ("horizontal", Axis::Horizontal),
  ("vertical", Axis::Vertical),
];
const SCROLL_AXES: &[(&str, ScrollbarAxis)] = &[
  ("vertical", ScrollbarAxis::Vertical),
  ("horizontal", ScrollbarAxis::Horizontal),
  ("both", ScrollbarAxis::Both),
];

/// A handler `(cx: Context) => void`
pub struct OnCx<'js>(Function<'js>);

impl<'js> FromJs<'js> for OnCx<'js> {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    Ok(Self(Function::from_js(ctx, value)?))
  }
}

impl TsType for OnCx<'_> {
  fn ts() -> String {
    "(cx: Context) => void".into()
  }
}

/// A payload that is the handler's `cx` itself, so `(cx) => ..` gets it first
struct CxFirst(ViewCx);

impl<'js> IntoJs<'js> for CxFirst {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    self.0.into_js(ctx)
  }
}

impl OnCx<'_> {
  fn listener(self) -> impl Fn(&(), &mut Window, &mut App) + 'static {
    let view = frame().view.clone();
    frame().listener(self.0, move |_: &()| {
      CxFirst(ViewCx::for_view(view.clone()))
    })
  }
}

/// Payload newtypes, so no other family's `impl Payload for u32` can clash
macro_rules! payloads {
  ($($name:ident($ty:ty) => $ts:literal),* $(,)?) => {$(
    pub struct $name($ty);
    impl<'js> IntoJs<'js> for $name {
      fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
        self.0.into_js(ctx)
      }
    }
    impl Payload for $name {
      const TS: &'static str = $ts;
    }
  )*};
}

payloads! {
  Num(u32) => "number",
  Nums(Vec<u32>) => "number[]",
  Sizes(Vec<f32>) => "number[]",
  Text(String) => "string",
  Change(bool) => "boolean",
  Click(ClickPayload) => "ClickEvent",
  Path(PathPayload) => "{ section: number; row: number }",
}

#[derive(rquickjs::IntoJs)]
pub struct ClickPayload {
  click_count: u32,
  position: Pt,
  modifiers: crate::element::events::Mods,
}

#[derive(rquickjs::IntoJs)]
pub struct PathPayload {
  section: u32,
  row: u32,
}

fn click(e: &ClickEvent) -> Click {
  Click(ClickPayload {
    click_count: e.click_count() as u32,
    position: e.position().into(),
    modifiers: e.modifiers().into(),
  })
}

fn path(p: &IndexPath) -> Path {
  Path(PathPayload {
    section: p.section as u32,
    row: p.row as u32,
  })
}

// ---------------------------------------------------------------- retained states

/// A virtual list's scroll position, for `track_scroll` and `Scrollbar`
#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct VirtualListScrollHandle {
  #[qjs(skip_trace)]
  handle: b::VirtualListScrollHandle,
}

impl Named for VirtualListScrollHandle {
  const TS: &'static str = "import(\"corona/base\").VirtualListScrollHandle";
}

#[rquickjs::methods]
impl VirtualListScrollHandle {
  #[qjs(constructor)]
  pub fn new() -> Self {
    Self {
      handle: b::VirtualListScrollHandle::new(),
    }
  }

  #[qjs(static, rename = "new")]
  pub fn create() -> Self {
    Self::new()
  }

  /// Puts item `index` on screen: `"top"` (default) or `"center"`.
  pub fn scroll_to_item(&self, ctx: Ctx<'_>, index: u32, strategy: Opt<String>) -> Result<()> {
    let strategy = match strategy.0.as_deref() {
      None | Some("top") => ScrollStrategy::Top,
      Some("center") => ScrollStrategy::Center,
      Some(s) => return Err(unknown(&ctx, "scroll strategy", s, &["top", "center"])),
    };
    self.handle.scroll_to_item(index as usize, strategy);
    Ok(())
  }

  pub fn scroll_to_bottom(&self) {
    self.handle.scroll_to_bottom();
  }
}

/// A command palette's query, selection and scroll, kept across renders
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "CommandState")]
pub struct CommandStateHandle {
  #[qjs(skip_trace)]
  state: Retained<CommandState>,
}

impl Named for CommandStateHandle {
  const TS: &'static str = "CommandState";
}

#[rquickjs::methods]
impl CommandStateHandle {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'_>) -> Result<Self> {
    let mut window = crate::runtime::scope::try_window().ok_or_else(|| {
      Exception::throw_type(
        &ctx,
        "CommandState needs a window: make it in init or a handler",
      )
    })?;
    let entity = app().new(|cx| CommandState::new(&mut window, cx));
    Ok(Self {
      state: Retained::new(&ctx, entity)?,
    })
  }

  #[qjs(static, rename = "new")]
  pub fn create(ctx: Ctx<'_>) -> Result<Self> {
    Self::new(ctx)
  }

  /// The search text.
  pub fn query(&self, ctx: Ctx<'_>) -> Result<String> {
    let cx = app();
    Ok(self.state.entity(&ctx)?.read(&cx).query(&cx).to_string())
  }

  pub fn release(&mut self) -> bool {
    self.state.release()
  }
}

/// A `ScrollHandle` or `VirtualListScrollHandle`, what a scrollbar follows
#[derive(Clone)]
enum AnyScroll {
  Plain(gpui_kit::ScrollHandle),
  List(b::VirtualListScrollHandle),
}

impl<'js> FromJs<'js> for AnyScroll {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    if value.is_string() || value.is_number() {
      let name = rquickjs::Coerced::<String>::from_js(ctx, value)?.0;
      return named_scroll(ctx, &name);
    }
    if let Ok(h) = Class::<ScrollHandle>::from_js(ctx, value.clone()) {
      return Ok(Self::Plain(h.borrow().handle.clone()));
    }
    let h = Class::<VirtualListScrollHandle>::from_js(ctx, value).map_err(|_| {
      Exception::throw_type(ctx, "expected a ScrollHandle or VirtualListScrollHandle")
    })?;
    Ok(Self::List(h.borrow().handle.clone()))
  }
}

impl AnyScroll {
  fn scrollbar(self, axis: ScrollbarAxis) -> b::Scrollbar {
    match self {
      Self::Plain(h) => b::Scrollbar::new(&h).axis(axis),
      Self::List(h) => b::Scrollbar::new(&h).axis(axis),
    }
  }
}

// ---------------------------------------------------------------- element methods

mod methods {
  use super::*;

  crate::el_methods! {
    /// Whether a `Tab`, sidebar row, header or footer shows as selected.
    fn selected(value: bool) for ["DropdownButton", "SidebarFooter", "SidebarHeader", "SidebarMenuItem", "Tab", "Tab@base"] |c, n| on!(c, n,
      b::Tab => |t| t.selected(value),
      c::tab::Tab => |t| t.selected(value),
      SidebarHeader => |t| t.selected(value),
      SidebarFooter => |t| t.selected(value),
      SideItem => |t| SideItem { item: t.item.active(value), ..t },
      DropButton => |t| DropButton { button: t.button.selected(value), ..t },
    );
    /// Disables a tab, accordion part, step, pagination, menu or command item.
    fn disabled(value: bool) for ["Accordion", "AccordionItem", "AccordionItem@base", "CommandItem", "DropdownButton", "Menu", "MenuItem", "NativeMenuItem", "NativeMenuTrigger", "Pagination", "SidebarMenuItem", "Stepper", "StepperItem", "Tab", "Tab@base", "Toolbar"] |c, n| on!(c, n,
      b::Tab => |t| t.disabled(value),
      b::AccordionItem => |t| t.disabled(value),
      c::tab::Tab => |t| t.disabled(value),
      c::accordion::Accordion => |t| t.disabled(value),
      c::accordion::AccordionItem => |t| t.disabled(value),
      c::pagination::Pagination => |t| t.disabled(value),
      c::stepper::Stepper => |t| t.disabled(value),
      c::stepper::StepperItem => |t| t.disabled(value),
      c::toolbar::Toolbar => |t| t.disabled(value),
      SideItem => |t| SideItem { item: t.item.disable(value), ..t },
      CommandItem => |t| t.disabled(value),
      NativeItem => |t| NativeItem { disabled: value, ..t },
      MenuEntry => |t| MenuEntry { disabled: value, ..t },
      AppMenu => |t| AppMenu { disabled: value, ..t },
      DropButton => |t| DropButton { button: t.button.disabled(value), ..t },
      NativeTrigger => |t| NativeTrigger { button: t.button.disabled(value), ..t },
    );
    /// A `Tab`'s one-based place among `size` tabs, announced as "tab 2 of 5".
    fn set_position(position: u32, size: u32) for ["Tab@base"] |c, n| on!(c, n,
      b::Tab => |t| t.set_position(position as usize, size as usize),
    );
    /// Whether an accordion item or a collapsible is open.
    fn open(value: bool) for ["AccordionItem", "AccordionItem@base", "Collapsible", "Collapsible@base"] |c, n| on!(c, n,
      b::AccordionItem => |t| t.open(value),
      b::Collapsible => |t| t.open(value),
      c::accordion::AccordionItem => |t| t.open(value),
      c::collapsible::Collapsible => |t| t.open(value),
    );
    /// An `AccordionItem`'s `AccordionHeader`.
    fn header(el: Child<'js> as "Element<\"AccordionHeader@base\">") for ["AccordionItem@base"] |c, n| {
      let header = part::<b::AccordionHeader>(c, el, "an AccordionHeader")?;
      on!(c, n, b::AccordionItem => |t| t.header(header))
    };
    /// A `Sidebar`'s header.
    fn header(el: Child<'js>) for ["Sidebar"] |c, n| {
      if !is::<Sidebar<SidebarMenu>>(&**n) {
        return Err(not_on(c, &**n));
      }
      let header = element(c, el)?;
      crate::map(c, n, |t: Sidebar<SidebarMenu>| t.header(header))
    };
    /// A `Sidebar`'s footer.
    fn footer(el: Child<'js>) for ["Sidebar"] |c, n| {
      if !is::<Sidebar<SidebarMenu>>(&**n) {
        return Err(not_on(c, &**n));
      }
      let footer = element(c, el)?;
      crate::map(c, n, |t: Sidebar<SidebarMenu>| t.footer(footer))
    };
    /// An `AccordionItem`'s `AccordionPanel`.
    fn panel(element: Child<'js> as "Element<\"AccordionPanel@base\">") for ["AccordionItem@base"] |c, n| {
      if !is::<b::AccordionItem>(&**n) {
        return Err(not_on(c, &**n));
      }
      let panel = part::<b::AccordionPanel>(c, element, "an AccordionPanel")?;
      crate::map(c, n, |t: b::AccordionItem| t.panel(panel))
    };
    /// What revealing a collapsible shows.
    fn content(el: Child<'js>) for ["Collapsible", "Collapsible@base"] |c, n| {
      if !is::<b::Collapsible>(&**n) && !is::<c::collapsible::Collapsible>(&**n) {
        return Err(not_on(c, &**n));
      }
      let content = element(c, el)?;
      on!(c, n,
        b::Collapsible => |t| t.content(content),
        c::collapsible::Collapsible => |t| t.content(content),
      )
    };
    /// A stable identity for a collapsible's reveal motion.
    fn motion_id(id: Id) for ["Collapsible"] |c, n| on!(c, n,
      c::collapsible::Collapsible => |t| t.motion_id(id.into_inner()),
    );
    /// An `AccordionHeader`'s announced heading level, 3 by default.
    fn aria_level(level: u32) for ["AccordionHeader@base"] |c, n| on!(c, n,
      b::AccordionHeader => |t| t.level(level as usize),
    );
    /// Keeps a shut `AccordionPanel` in the tree, with its scroll position and focus.
    fn keep_mounted(value: Opt<bool>) for ["AccordionPanel@base"] |c, n| on!(c, n,
      b::AccordionPanel => |t| t.keep_mounted(value.0.unwrap_or(true)),
    );
    /// `handler(open, cx)` with the open state an `AccordionTrigger` asks for.
    fn on_change(handler: On<'js, Change>) for ["AccordionTrigger@base"] |c, n| {
      let l = frame().listener(handler.0, |open: &bool| *open);
      on!(c, n, b::AccordionTrigger => |t| t.on_change(move |open, _, w, cx| l(&open, w, cx)))
    };
    /// `handler(index, cx)` with the index a `TabBar` or `Stepper`, or the page a `Pagination`,
    /// asks for.
    fn on_change(handler: On<'js, Num>) for ["Pagination", "Stepper", "TabBar"] |c, n| {
      let l = frame().listener(handler.0, |i: &usize| Num(*i as u32));
      on!(c, n,
        c::tab::TabBar => |t| t.on_click(l),
        c::stepper::Stepper => |t| t.on_click(l),
        c::pagination::Pagination => |t| t.on_click(l),
      )
    };
    /// A resizable panel's initial size in pixels.
    fn size(pixels: f32) for ["ResizablePanel", "ResizablePanel@base"] |c, n| {
      on!(c, n, b::ResizablePanel => |t| t.size(px(pixels)))
    };
    /// A component's semantic size.
    fn size(size: SizeArg<'js>) for ["Accordion", "Accordion@base", "DropdownButton", "Pagination", "Pagination@base", "Stepper", "TabBar"] |c, n| {
      let s = size.get(c, &**n)?;
      on!(c, n,
        c::tab::TabBar => |t| t.with_size(s),
        c::accordion::Accordion => |t| t.with_size(s),
        c::pagination::Pagination => |t| t.with_size(s),
        c::stepper::Stepper => |t| t.with_size(s),
        DropButton => |t| DropButton { button: t.button.with_size(s), ..t },
      )
    };
    /// Whether a `resizable_panel()` is drawn; hidden, it keeps its place. Default `true`.
    fn visible(value: Opt<bool>) for ["ResizablePanel", "ResizablePanel@base"] |c, n| on!(c, n,
      b::ResizablePanel => |t| t.visible(value.0.unwrap_or(true)),
    );
    /// How far a resizable panel may be dragged, in pixels.
    fn size_range(min: f32, max: Opt<f32>) for ["ResizablePanel", "ResizablePanel@base"] |c, n| on!(c, n,
      b::ResizablePanel => |t| t.size_range(px(min)..px(max.0.unwrap_or(f32::MAX))),
    );
    /// `handler(sizes, cx)` after a drag resized the panels of a resizable group.
    fn on_resize(handler: On<'js, Sizes>) for ["Resizable", "Resizable@base"] |c, n| {
      let l = frame().listener(handler.0, |s: &Vec<f32>| Sizes(s.clone()));
      on!(c, n, Group => |g| Group {
        group: g.group.on_resize(move |state, w, cx| {
          let sizes = state.read(cx).sizes().iter().map(|p| f32::from(*p)).collect();
          l(&sizes, w, cx)
        }),
        ..g
      })
    };
    /// A `Resizable`'s direction: `"horizontal"` or `"vertical"`.
    fn axis(value: String as "\"horizontal\" | \"vertical\"") for ["Resizable", "Resizable@base"] |c, n| {
      let axis = pick(c, "axis", &value, AXES)?;
      on!(c, n, Group => |g| Group { group: g.group.axis(axis), ..g })
    };
    /// A `Resizable`'s size across its axis, in pixels.
    fn cross_size(pixels: f32) for ["Resizable", "Resizable@base"] |c, n| on!(c, n,
      Group => |g| Group { group: g.group.size(px(pixels)), ..g },
    );
    /// When a `Scrollbar` shows: `"scrolling"`, `"hover"` or `"always"`.
    fn mode(value: String as "\"scrolling\" | \"hover\" | \"always\"") for ["Scrollbar"] |c, n| {
      let modes = [("scrolling", ScrollbarMode::Scrolling), ("hover", ScrollbarMode::Hover), ("always", ScrollbarMode::Always)];
      let mode = pick(c, "scrollbar mode", &value, &modes)?;
      on!(c, n, b::Scrollbar => |s| s.mode(mode))
    };
    /// The content size a `Scrollbar` measures its thumb against, in pixels.
    fn scroll_size(width: f32, height: f32) for ["Scrollbar"] |c, n| on!(c, n,
      b::Scrollbar => |s| s.scroll_size(size(px(width), px(height))),
    );
    /// Makes a `Scrollbar` take its viewport from its own bounds.
    fn viewport_from_layout(enabled: Opt<bool>) for ["Scrollbar"] |c, n| on!(c, n,
      b::Scrollbar => |s| match enabled.0 {
        Some(false) => s,
        _ => s.viewport_from_layout(),
      },
    );
    /// Which axes a `Scrollbar` or `Scroll` covers: `"vertical"`, `"horizontal"` or `"both"`.
    fn scroll_axis(axis: String as "\"vertical\" | \"horizontal\" | \"both\"") for ["Scroll", "Scrollbar"] |c, n| {
      let axis = pick(c, "scroll axis", &axis, SCROLL_AXES)?;
      on!(c, n,
        b::Scrollbar => |s| s.axis(axis),
        Scroll => |s| Scroll(scroll_axis(s.0, axis)),
      )
    };
    /// Lets a `VirtualListScrollHandle` drive a virtual list.
    fn track_scroll(handle: super::Later<'js, Class<'js, VirtualListScrollHandle>>) for ["VirtualList@base"] |c, n| {
      if !is::<VList>(&**n) {
        return Err(not_on(c, &**n));
      }
      let handle = handle.get()?.borrow().handle.clone();
      let name = (&**n as &dyn Any).downcast_ref::<VList>().and_then(|l| l.name.clone());
      if let Some(name) = name {
        name_scroll(c, &name, AnyScroll::List(handle.clone()))?;
      }
      on!(c, n, VList => |l| VList { list: l.list.track_scroll(&handle), ..l })
    };
    /// Which item a virtual list measures for its cross-axis size.
    fn with_item_to_measure_index(index: u32) for ["VirtualList@base"] |c, n| on!(c, n,
      VList => |l| VList { list: l.list.with_item_to_measure_index(index as usize), ..l },
    );
    /// A `Tab`'s text, or a `Separator`'s.
    fn label(label: String) for ["Separator", "Tab"] |c, n| on!(c, n,
      c::tab::Tab => |t| t.label(label),
      c::separator::Separator => |t| t.label(label),
    );
    /// What a `Tab` announces.
    fn aria_label(label: String) for ["Tab", "Tab@base"] |c, n| on!(c, n,
      c::tab::Tab => |t| t.aria_label(label),
    );
    /// `handler(key, cx)` when a row of a virtual list is clicked, `key` what `get_key` gave.
    fn on_item_click(handler: On<'js, Text>) for ["VirtualList@base"] |c, n| {
      if !is::<VList>(&**n) {
        return Err(not_on(c, &**n));
      }
      let view = frame_or_throw(c)?.view.entity_id();
      let key = runtime(c)?.slot(view, handler.0, true);
      on!(c, n, VList => |l| {
        l.hooks.borrow_mut().click = Some(key);
        l
      })
    };
    /// `handler(key, event, cx)` on a secondary press over a row of a virtual list.
    fn on_item_secondary_click(handler: Function<'js> as "(key: string, event: ClickEvent, cx: Context) => void") for ["VirtualList@base"] |c, n| {
      if !is::<VList>(&**n) {
        return Err(not_on(c, &**n));
      }
      let spread: Function = c.eval("(f) => (p, cx) => f(p[0], p[1], cx)")?;
      let handler: Function = spread.call((handler,))?;
      let view = frame_or_throw(c)?.view.entity_id();
      let key = runtime(c)?.slot(view, handler, true);
      on!(c, n, VList => |l| {
        l.hooks.borrow_mut().secondary = Some(key);
        l
      })
    };
    /// The zero-based selected tab or current step.
    fn selected_index(index: u32) for ["Stepper", "TabBar"] |c, n| on!(c, n,
      c::tab::TabBar => |t| t.selected_index(index as usize),
      c::stepper::Stepper => |t| t.selected_index(index as usize),
    );
    /// A `DropdownButton`'s look.
    fn variant(variant: String as "\"primary\" | \"secondary\" | \"danger\" | \"ghost\"") for ["DropdownButton"] |c, n| {
      let kinds = ["primary", "secondary", "danger", "ghost"];
      let v = pick(c, "variant", &variant, &kinds.map(|k| (k, k)))?;
      on!(c, n, DropButton => |t| DropButton {
        button: match v {
          "primary" => t.button.primary(),
          "secondary" => t.button.secondary(),
          "danger" => t.button.danger(),
          _ => t.button.ghost(),
        },
        ..t
      })
    };
    /// A `GroupBox`'s look.
    fn variant(variant: String as "\"normal\" | \"fill\" | \"outline\"") for ["GroupBox"] |c, n| {
      let groups = [("normal", GroupBoxVariant::Normal), ("fill", GroupBoxVariant::Fill), ("outline", GroupBoxVariant::Outline)];
      let v = pick(c, "variant", &variant, &groups)?;
      on!(c, n, c::group_box::GroupBox => |g| g.with_variant(v))
    };
    /// A `TabBar`'s look.
    fn variant(variant: String as "\"tab\" | \"outline\" | \"pill\" | \"segmented\" | \"underline\"") for ["TabBar"] |c, n| {
      let tabs = [
        ("tab", TabVariant::Tab),
        ("outline", TabVariant::Outline),
        ("pill", TabVariant::Pill),
        ("segmented", TabVariant::Segmented),
        ("underline", TabVariant::Underline),
      ];
      let v = pick(c, "variant", &variant, &tabs)?;
      on!(c, n, c::tab::TabBar => |t| t.with_variant(v))
    };
    /// Whether a `TabBar` lists tabs that do not fit in a menu.
    fn menu(value: bool) for ["TabBar"] |c, n| on!(c, n, c::tab::TabBar => |t| t.menu(value));
    /// Whether an `Accordion` keeps several items open.
    fn multiple(value: bool) for ["Accordion"] |c, n| on!(c, n,
      c::accordion::Accordion => |t| t.multiple(value),
    );
    /// Whether an `Accordion` or `Command` draws its border.
    fn bordered(value: bool) for ["Accordion", "Command"] |c, n| on!(c, n,
      c::accordion::Accordion => |t| t.bordered(value),
      Command => |t| t.bordered(value),
    );
    /// `handler(open_indices, cx)` after an `Accordion` item was toggled.
    fn on_toggle(handler: On<'js, Nums>) for ["Accordion"] |c, n| {
      let l = frame().listener(handler.0, |ix: &Vec<u32>| Nums(ix.clone()));
      on!(c, n, c::accordion::Accordion => |t| t.on_toggle_click(move |ix, w, cx| {
        l(&ix.iter().map(|i| *i as u32).collect(), w, cx)
      }))
    };
    /// An `AccordionItem`'s title row or a `GroupBox`'s title.
    fn title(title: Child<'js>) for ["AccordionItem", "GroupBox"] |c, n| {
      let is_item = is::<c::accordion::AccordionItem>(&**n);
      if !is_item && !is::<c::group_box::GroupBox>(&**n) {
        return Err(not_on(c, &**n));
      }
      let title = element(c, title)?;
      on!(c, n,
        c::accordion::AccordionItem => |t| t.title(title),
        c::group_box::GroupBox => |t| t.title(title),
      )
    };
    /// The current one-based page.
    fn current_page(page: u32) for ["Pagination"] |c, n| on!(c, n,
      c::pagination::Pagination => |t| t.current_page(page as usize),
    );
    /// How many pages there are.
    fn total_pages(pages: u32) for ["Pagination"] |c, n| on!(c, n,
      c::pagination::Pagination => |t| t.total_pages(pages as usize),
    );
    /// How many page buttons show at most.
    fn visible_pages(pages: u32) for ["Pagination"] |c, n| on!(c, n,
      c::pagination::Pagination => |t| t.visible_pages(pages as usize),
    );
    /// Shows only the previous and next buttons.
    fn compact() for ["Pagination"] |c, n| on!(c, n, c::pagination::Pagination => |t| t.compact());
    /// Lays a `Stepper` out top to bottom.
    fn vertical(value: Opt<bool>) for ["Stepper"] |c, n| on!(c, n,
      c::stepper::Stepper => |t| t.layout(match value.0 {
        Some(false) => Axis::Horizontal,
        _ => Axis::Vertical,
      }),
    );
    /// Centers each step's text in a horizontal `Stepper`.
    fn text_center(value: Opt<bool>) for ["Stepper"] |c, n| on!(c, n,
      c::stepper::Stepper => |t| t.text_center(value.0.unwrap_or(true)),
    );
    /// A `Separator`'s line color.
    fn color(color: Color) for ["Separator"] |c, n| on!(c, n,
      c::separator::Separator => |t| t.color(color.0),
    );
    /// Draws a `Separator` dashed.
    fn dashed() for ["Separator"] |c, n| on!(c, n, c::separator::Separator => |t| t.dashed());
    /// Adds to a `StatusBar`'s leading side.
    fn left_content(el: Child<'js>) for ["StatusBar"] |c, n| {
      if !is::<c::status_bar::StatusBar>(&**n) {
        return Err(not_on(c, &**n));
      }
      let e = element(c, el)?;
      crate::map(c, n, |t: c::status_bar::StatusBar| t.left(e))
    };
    /// Adds to a `StatusBar`'s trailing side.
    fn right_content(el: Child<'js>) for ["StatusBar"] |c, n| {
      if !is::<c::status_bar::StatusBar>(&**n) {
        return Err(not_on(c, &**n));
      }
      let e = element(c, el)?;
      crate::map(c, n, |t: c::status_bar::StatusBar| t.right(e))
    };
    /// The side a `Sidebar` or `SidebarToggleButton` sits on: `"left"` or `"right"`.
    fn side(side: String as "\"left\" | \"right\"") for ["Sidebar", "SidebarToggleButton"] |c, n| {
      let side = pick(c, "side", &side, SIDES)?;
      on!(c, n,
        Sidebar<SidebarMenu> => |t| t.side(side),
        SidebarToggleButton => |t| t.side(side),
      )
    };
    /// How a `Sidebar` collapses: `"icon"`, `"offcanvas"` or `"none"`.
    fn collapsible(mode: String as "\"icon\" | \"offcanvas\" | \"none\"") for ["Sidebar"] |c, n| {
      let modes = [("icon", SidebarCollapsible::Icon), ("offcanvas", SidebarCollapsible::Offcanvas), ("none", SidebarCollapsible::None)];
      let mode = pick(c, "collapsible mode", &mode, &modes)?;
      on!(c, n, Sidebar<SidebarMenu> => |t| t.collapsible(mode))
    };
    /// Whether a `Sidebar` is collapsed, and which icon its toggle shows.
    fn collapsed(value: bool) for ["Sidebar", "SidebarToggleButton"] |c, n| on!(c, n,
      Sidebar<SidebarMenu> => |t| t.collapsed(value),
      SidebarToggleButton => |t| t.collapsed(value),
    );
    /// `handler(event, cx)` when a sidebar row or toggle is pressed.
    fn on_click(handler: On<'js, Click>) for ["DropdownButton", "SidebarMenuItem", "SidebarToggleButton"] |c, n| {
      if !is::<SideItem>(&**n) && !is::<SidebarToggleButton>(&**n) && !is::<DropButton>(&**n) {
        return Err(not_on(c, &**n));
      }
      let l = frame().listener(handler.0, click);
      on!(c, n,
        SideItem => |t| SideItem { item: t.item.on_click(l), ..t },
        SidebarToggleButton => |t| t.on_click(l),
        DropButton => |t| DropButton { action: t.action.on_click(l), ..t },
      )
    };
    /// Whether a sidebar row's submenu starts open.
    fn default_open(value: bool) for ["SidebarMenuItem"] |c, n| on!(c, n,
      SideItem => |t| SideItem { item: t.item.default_open(value), ..t },
    );
    /// Lets a press on a sidebar row open its submenu.
    fn click_to_open(value: bool) for ["SidebarMenuItem"] |c, n| on!(c, n,
      SideItem => |t| SideItem { item: t.item.click_to_open(value), ..t },
    );
    /// Lets a press on a sidebar row toggle its submenu.
    fn click_to_toggle(value: bool) for ["SidebarMenuItem"] |c, n| on!(c, n,
      SideItem => |t| SideItem { item: t.item.click_to_toggle(value), ..t },
    );
    /// A sidebar row's icon: `"home"`, `"components"`, `"settings"`, `"archive"` or `"account"`.
    fn icon(icon: String as "\"home\" | \"components\" | \"settings\" | \"archive\" | \"account\"") for ["SidebarMenuItem"] |c, n| {
      let icons = [
        ("home", IconName::SquareTerminal),
        ("components", IconName::LayoutDashboard),
        ("settings", IconName::Settings2),
        ("archive", IconName::BookOpen),
        ("account", IconName::User),
      ];
      let icon = pick(c, "icon", &icon, &icons)?;
      on!(c, n, SideItem => |t| SideItem { item: t.item.icon(icon), ..t })
    };
    /// Whether a `Command` shows its search field.
    fn searchable(value: bool) for ["Command"] |c, n| on!(c, n, Command => |t| t.searchable(value));
    /// Whether a `Command` filters its items by the query.
    fn filterable(value: bool) for ["Command"] |c, n| on!(c, n, Command => |t| t.filterable(value));
    /// A `Command`'s search placeholder.
    fn placeholder(text: String) for ["Command"] |c, n| on!(c, n, Command => |t| t.placeholder(text));
    /// How tall a `Command`'s results grow, in pixels.
    fn max_height(pixels: f32) for ["Command"] |c, n| on!(c, n, Command => |t| t.max_h(px(pixels)));
    /// `handler(query, cx)` after a `Command`'s query changed.
    fn on_query(handler: On<'js, Text>) for ["Command"] |c, n| {
      let l = frame().listener(handler.0, |q: &str| Text(q.to_string()));
      on!(c, n, Command => |t| t.on_query(l))
    };
    /// `handler({ section, row }, cx)` after a `Command`'s highlight moved.
    fn on_select(handler: On<'js, Path>) for ["Command"] |c, n| {
      let l = frame().listener(handler.0, path);
      on!(c, n, Command => |t| t.on_select(move |p, w, cx| l(&p, w, cx)))
    };
    /// `handler({ section, row }, cx)` after a `Command` item was confirmed.
    fn on_confirm(handler: On<'js, Path>) for ["Command"] |c, n| {
      let l = frame().listener(handler.0, path);
      on!(c, n, Command => |t| t.on_confirm(move |p, w, cx| l(&p, w, cx)))
    };
    /// `handler(cx)` when Escape is pressed on a `Command` with an empty query.
    fn on_cancel(handler: OnCx<'js>) for ["Command"] |c, n| {
      let l = handler.listener();
      on!(c, n, Command => |t| t.on_cancel(move |w, cx| l(&(), w, cx)))
    };
    /// Another word a `CommandItem` is found by.
    fn keyword(keyword: String) for ["CommandItem"] |c, n| on!(c, n, CommandItem => |t| t.keywords([keyword]));
    /// Whether a command or native menu item shows a check.
    fn checked(value: bool) for ["CommandItem", "MenuItem", "NativeMenuItem"] |c, n| on!(c, n,
      CommandItem => |t| t.checked(value),
      NativeItem => |t| NativeItem { checked: value, ..t },
      MenuEntry => |t| MenuEntry { checked: value, ..t },
    );
    /// The action a command or native menu item dispatches, for `on_action`.
    fn action(action: String) for ["CommandItem", "NativeMenuItem"] |c, n| {
      let action = crate::element::events::scoped(c, &action)?;
      on!(c, n,
        CommandItem => |t| t.action(Box::new(crate::element::events::ScriptAction(action.into(), 0))),
        NativeItem => |t| NativeItem { action, ..t },
      )
    };
    /// The outlined look of a `DropdownButton`.
    fn outline() for ["DropdownButton"] |c, n| on!(c, n, DropButton => |t| DropButton { button: t.button.outline(), ..t });
    /// Where a `DropdownButton`'s menu opens.
    fn menu_anchor(anchor: String as "\"top_right\" | \"bottom_right\" | \"bottom_left\" | \"top_left\"") for ["DropdownButton"] |c, n| {
      let anchors = [
        ("top_right", gpui_kit::Anchor::TopRight),
        ("bottom_right", gpui_kit::Anchor::BottomRight),
        ("bottom_left", gpui_kit::Anchor::BottomLeft),
        ("top_left", gpui_kit::Anchor::TopLeft),
      ];
      let anchor = pick(c, "anchor", &anchor, &anchors)?;
      on!(c, n, DropButton => |t| DropButton { anchor, ..t })
    };
    /// Adds a `DropdownButton` menu item calling `handler(cx)`.
    fn menu_item(label: String, handler: OnCx<'js>) for ["DropdownButton"] |c, n| {
      if !is::<DropButton>(&**n) {
        return Err(not_on(c, &**n));
      }
      let l = handler.listener();
      on!(c, n, DropButton => |mut d| {
        d.items.push((label, Rc::new(move |_: &ClickEvent, w: &mut Window, cx: &mut App| l(&(), w, cx))));
        d
      })
    };
    /// `handler(message, cx)` when the native menu cannot be shown. Showing one cannot fail
    /// on this platform layer, so it is kept and never called.
    fn on_effect_error(handler: On<'js, Text>) for ["NativeMenuTrigger"] |c, n| {
      let _ = handler;
      on!(c, n, NativeTrigger => |t| t)
    };
    /// Adds a `DropdownMenu` item calling `handler(cx)`.
    fn item(label: String, handler: OnCx<'js>) for ["DropdownMenu"] |c, n| {
      if !is::<Dropdown>(&**n) {
        return Err(not_on(c, &**n));
      }
      let l = handler.listener();
      on!(c, n, Dropdown => |mut d| {
        d.items.push((label, Rc::new(move |_: &ClickEvent, w: &mut Window, cx: &mut App| l(&(), w, cx))));
        d
      })
    };
  }
}

/// Applies a `ScrollableElement` method to a `div`
fn scrollable(
  ctx: &Ctx<'_>,
  n: &mut Box<dyn Node>,
  f: impl FnOnce(Div) -> Scrollable<Div>,
) -> Result<()> {
  if !is::<Div>(&**n) {
    return Err(not_on(ctx, &**n));
  }
  let mut d = *mem::replace(n, Box::new(StyleRefinement::default()))
    .into_any()
    .downcast::<Div>()
    .expect("checked");
  // the caller's location would name every scrollable alike
  let id = gpui_kit::InteractiveElement::interactivity(&mut d)
    .element_id
    .clone();
  let s = f(d);
  *n = Box::new(match id {
    Some(id) => s.id(id),
    None => s,
  });
  Ok(())
}

/// Scrolls `d` along `axis` only
fn scroll_axis(mut d: Div, axis: ScrollbarAxis) -> Div {
  let style = Styled::style(&mut d);
  let (x, y) = match axis {
    ScrollbarAxis::Vertical => (Overflow::Visible, Overflow::Scroll),
    ScrollbarAxis::Horizontal => (Overflow::Scroll, Overflow::Visible),
    ScrollbarAxis::Both => (Overflow::Scroll, Overflow::Scroll),
  };
  style.overflow.x = Some(x);
  style.overflow.y = Some(y);
  match axis {
    ScrollbarAxis::Horizontal => d.flex_row(),
    _ => d.flex_col(),
  }
}

// ---------------------------------------------------------------- typed children

/// A child that is no element of a typed container
struct Plain(AnyElement);

impl Node for Plain {
  fn kind(&self) -> &'static str {
    "element"
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    self.0
  }
  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }
}

/// Takes `T` out of `child`
fn into<T: Node>(child: Box<dyn Node>) -> T {
  *child.into_any().downcast::<T>().expect("checked")
}

/// The adoption rules: `Parent: Child => add, ..;` per container
macro_rules! containers {
  ($ctx:ident, $n:ident, $take:ident; $(
    $parent:ty: $($child:ty => |$p:ident, $c:ident| $add:expr),+;
  )*) => {$(
    if is::<$parent>(&**$n) {
      let child = $take()?;
      $(if is::<$child>(&*child) {
        let $c = into::<$child>(child);
        crate::map($ctx, $n, |$p: $parent| $add)?;
        return Ok(true);
      })+
      let takes = [$(stringify!($child)),+].map(|t| t.rsplit("::").next().unwrap_or(t));
      return Err(Exception::throw_type(
        $ctx,
        &format!("{} takes {} children, not {}", $n.kind(), takes.join(" or "), child.kind()),
      ));
    }
  )*};
}

fn add(
  ctx: &Ctx<'_>,
  n: &mut Box<dyn Node>,
  take: impl FnOnce() -> Result<Box<dyn Node>>,
) -> Result<bool> {
  if is::<Group>(&**n) {
    let child = take()?;
    let panel = match is::<b::ResizablePanel>(&*child) {
      true => into::<b::ResizablePanel>(child),
      false => b::ResizablePanel::from(child.into_element()),
    };
    crate::map(ctx, n, |g: Group| Group {
      group: g.group.child(panel),
      ..g
    })?;
    return Ok(true);
  }
  containers! { ctx, n, take;
    c::tab::TabBar: c::tab::Tab => |p, c| p.child(c);
    c::accordion::Accordion: c::accordion::AccordionItem => |p, c| p.item(move |_| c);
    c::stepper::Stepper: c::stepper::StepperItem => |p, c| p.item(c);
    Sidebar<SidebarMenu>: SidebarMenu => |p, c| p.child(c);
    SidebarMenu: SideItem => |p, c| p.child(c.build());
    SideItem: SideItem => |p, c| {
      let mut p = p;
      p.children.push(c);
      p
    };
    Command:
      CommandItem => |p, c| p.item(c),
      CommandGroup => |p, c| p.group(c),
      CommandSeparator => |p, _c| p.separator();
    CommandGroup: CommandItem => |p, c| p.item(c);
    AppMenu:
      MenuEntry => |p, c| {
        let mut p = p;
        p.entries.push(Some(c));
        p
      },
      MenuSeparator => |p, _c| {
        let mut p = p;
        p.entries.push(None);
        p
      };
    MenuBar: AppMenu => |p, c| {
      let mut p = p;
      p.menus.push(c);
      p
    };
    NativeTrigger:
      NativeItem => |p, c| {
        let mut p = p;
        p.items.push(Some(c));
        p
      },
      NativeSeparator => |p, _c| {
        let mut p = p;
        p.items.push(None);
        p
      };
  }
  Ok(false)
}

/// `overflow_*_scrollbar` of a `div`
mod scrollbars {
  use super::*;

  crate::el_methods! {
    /// A stable name for this element; a scrolling one is what a `Scrollbar` of that name
    /// drives.
    fn id(id: Id) for ["div"] |c, n| {
      crate::element::interact(c, n, |mut i| {
        i.0.element_id = Some(id.into_inner());
        i
      })?;
      link_scroll(c, n)
    };
    /// Scrolls both axes for overflowing children.
    fn overflow_scroll() for ["div"] |c, n| {
      crate::element::interact(c, n, gpui_kit::StatefulInteractiveElement::overflow_scroll)?;
      link_scroll(c, n)
    };
    /// Scrolls horizontally for overflowing children.
    fn overflow_x_scroll() for ["div"] |c, n| {
      crate::element::interact(c, n, gpui_kit::StatefulInteractiveElement::overflow_x_scroll)?;
      link_scroll(c, n)
    };
    /// Scrolls vertically for overflowing children.
    fn overflow_y_scroll() for ["div"] |c, n| {
      crate::element::interact(c, n, gpui_kit::StatefulInteractiveElement::overflow_y_scroll)?;
      link_scroll(c, n)
    };
    /// Lets a `ScrollHandle` drive this element; a `Scrollbar` of its name drives it too.
    fn track_scroll(handle: Class<'js, ScrollHandle>) for ["div"] |c, n| {
      let handle = handle.borrow().handle.clone();
      if let Some(name) = name_of(n) {
        name_scroll(c, &name, AnyScroll::Plain(handle.clone()))?;
      }
      crate::element::interact(c, n, |i| gpui_kit::StatefulInteractiveElement::track_scroll(i, &handle))
    };
    /// Scrolls both axes and paints base-layer scrollbars.
    fn overflow_scrollbar() for ["div"] |c, n| scrollable(c, n, |d| d.overflow_scrollbar());
    /// Scrolls horizontally and paints a base-layer scrollbar.
    fn overflow_x_scrollbar() for ["div"] |c, n| scrollable(c, n, |d| d.overflow_x_scrollbar());
    /// Scrolls vertically and paints a base-layer scrollbar.
    fn overflow_y_scrollbar() for ["div"] |c, n| scrollable(c, n, |d| d.overflow_y_scrollbar());
  }
}

/// `child` of the containers taking one kind of child, typed as the generic one
#[allow(dead_code)]
mod children {
  use super::*;

  crate::el_methods! {
    fn child(child: Child<'js>) for ["Resizable", "TabBar", "Accordion", "Stepper", "Sidebar", "SidebarMenu", "SidebarMenuItem", "Command", "CommandGroup", "NativeMenuTrigger", "Menu", "MenuBar"] |c, n| adopt(c, n, child);
    fn children(children: Vec<Child<'js>>) for ["Resizable", "TabBar", "Accordion", "Stepper", "Sidebar", "SidebarMenu", "SidebarMenuItem", "Command", "CommandGroup", "NativeMenuTrigger", "Menu", "MenuBar"] |c, n| {
      children.into_iter().try_for_each(|child| adopt(c, n, child))
    };
  }
}

/// Adds `child` to a container taking one kind of child; a miss for other elements.
fn adopt(c: &Ctx<'_>, n: &mut Box<dyn Node>, child: Child<'_>) -> Result<()> {
  let take = || -> Result<Box<dyn Node>> {
    match child {
      Child::El(el) => frame_or_throw(c)?.take(c, &el.borrow()),
      child => Ok(Box::new(Plain(element(c, child)?))),
    }
  };
  match add(c, n, take)? {
    true => Ok(()),
    false => Err(not_on(c, &**n)),
  }
}

fn install(proto: &Object<'_>) -> Result<()> {
  methods::install(proto)?;
  scrollbars::install(proto)?;
  children::install(proto)
}

fn el_d_ts() -> String {
  methods::d_ts() + &scrollbars::d_ts()
}

// ---------------------------------------------------------------- exports

fn func<'js, P>(ctx: &Ctx<'js>, f: impl IntoJsFunc<'js, P> + 'js) -> Result<Value<'js>> {
  Ok(Function::new(ctx.clone(), f)?.into_value())
}

fn ctor<'js, P>(
  ctx: &Ctx<'js>,
  name: &str,
  f: impl IntoJsFunc<'js, P> + 'js,
) -> Result<Value<'js>> {
  constructor(ctx, name, Function::new(ctx.clone(), f)?)
}

/// `pagination_items(current, total, visible?)`: pages and inclusive ellipsis ranges
fn pagination_items<'js>(
  ctx: Ctx<'js>,
  current: u32,
  total: u32,
  visible: Opt<u32>,
) -> Result<Vec<Object<'js>>> {
  let mut state = b::PaginationState::new(current as usize, total as usize);
  if let Some(v) = visible.0 {
    state = state.visible_pages(v as usize);
  }
  (state.items().into_iter())
    .map(|item| {
      let o = Object::new(ctx.clone())?;
      match item {
        PaginationItem::Page(p) => o.set("page", p as u32)?,
        PaginationItem::Ellipsis(r) => o.set("ellipsis", vec![r.start as u32, r.end as u32 - 1])?,
      }
      Ok(o)
    })
    .collect()
}

/// `v_virtual_list`/`h_virtual_list(id, count, sizes, get_key, render(range, cx))`.
/// ponytail: `get_key` is unused until `on_item_click` exists, as for `uniform_list`
fn virtual_list<'js>(
  ctx: &Ctx<'js>,
  axis: Axis,
  id: Id,
  count: u32,
  sizes: Value<'js>,
  get_key: Function<'js>,
  render: Function<'js>,
) -> Result<Class<'js, El>> {
  let count = crate::element::items(ctx, count)? as u32;
  let sizes: Vec<f32> = match sizes.as_number() {
    Some(s) => vec![s as f32; count as usize],
    None => Vec::from_js(ctx, sizes)?,
  };
  if sizes.len() != count as usize {
    return Err(Exception::throw_type(
      ctx,
      "item_sizes must have item_count entries",
    ));
  }
  let sizes = Rc::new(sizes.into_iter().map(|s| size(px(s), px(s))).collect());
  let view = (frame_or_throw(ctx)?.view.upgrade())
    .ok_or_else(|| Exception::throw_type(ctx, "the view is gone"))?;
  let rt = runtime(ctx)?;
  let key = rt.slot(view.entity_id(), render, true);
  let key_fn = rt.slot(view.entity_id(), get_key, true);
  let hooks = Rc::new(RefCell::new(RowHooks::default()));
  let row_hooks = hooks.clone();
  let rows = move |view: &mut ScriptView,
                   range: std::ops::Range<usize>,
                   window: &mut Window,
                   cx: &mut gpui_kit::Context<ScriptView>| {
    let start = range.start;
    let args = |a: &mut Args<'_>| {
      let r = Object::new(a.ctx().clone())?;
      r.set("start", range.start)?;
      r.set("end", range.end)?;
      a.push_arg(r)
    };
    let id = cx.entity_id();
    let (click, secondary) = {
      let hooks = row_hooks.borrow();
      (hooks.click, hooks.secondary)
    };
    view
      .render_content(key, None, window, cx, args, |ctx, value| {
        let items: Vec<Child> = Vec::from_js(ctx, value)?;
        let rt = runtime(ctx)?;
        let slot_fn = |key: u64| -> Result<Option<Function>> {
          let f = rt
            .slots
            .borrow()
            .get(&(id, key))
            .and_then(|s| s.content.clone());
          f.map(|f| f.restore(ctx)).transpose()
        };
        let get_key = slot_fn(key_fn)?;
        let (click, secondary) = (
          click.map(slot_fn).transpose()?.flatten(),
          secondary.map(slot_fn).transpose()?.flatten(),
        );
        let mut out = Vec::with_capacity(items.len());
        for (i, item) in items.into_iter().enumerate() {
          let el = crate::element::frame_child(ctx, item)?;
          if click.is_none() && secondary.is_none() {
            out.push(el);
            continue;
          }
          let row_key: String = match &get_key {
            Some(f) => f.call::<_, rquickjs::Coerced<String>>((start + i,))?.0,
            None => (start + i).to_string(),
          };
          let mut row = gpui_kit::InteractiveElement::id(
            div(),
            gpui_kit::ElementId::Name(row_key.clone().into()),
          )
          .child(el);
          if let Some(f) = &click {
            let key = row_key.clone();
            row =
              row.on_click(frame().listener(f.clone(), move |_: &ClickEvent| Text(key.clone())));
          }
          if let Some(f) = &secondary {
            let key = row_key.clone();
            let l = frame().listener(f.clone(), move |e: &gpui_kit::MouseDownEvent| {
              KeyPress(key.clone(), e.into())
            });
            row = gpui_kit::InteractiveElement::on_mouse_down(row, gpui_kit::MouseButton::Right, l);
          }
          out.push(row.into_any_element());
        }
        Ok(out)
      })
      .unwrap_or_else(|e| {
        tracing::warn!("virtual list render failed: {e}");
        Vec::new()
      })
  };
  let name = match id.into_inner() {
    gpui_kit::ElementId::Name(name) => Some(name.to_string()),
    other => {
      let list = b::virtual_list(view, other, axis, sizes, rows);
      return new_el(
        ctx,
        VList {
          list,
          name: None,
          hooks,
        },
      );
    }
  };
  let name_str = name.clone().unwrap_or_default();
  // a `Scrollbar` named after the list drives it
  let handle = b::VirtualListScrollHandle::new();
  name_scroll(ctx, &name_str, AnyScroll::List(handle.clone()))?;
  let list = b::virtual_list(view, name_str, axis, sizes, rows).track_scroll(&handle);
  new_el(ctx, VList { list, name, hooks })
}

fn resizable<'js>(ctx: &Ctx<'js>, group: b::ResizablePanelGroup) -> Result<Class<'js, El>> {
  new_el(
    ctx,
    Group {
      frame: div().size_full(),
      group,
    },
  )
}

fn base_exports<'js>(ctx: &Ctx<'js>) -> Result<Vec<(&'static str, Value<'js>)>> {
  let scrollbar = Object::new(ctx.clone())?;
  for (name, axis) in [
    ("new", ScrollbarAxis::Both),
    ("horizontal", ScrollbarAxis::Horizontal),
    ("vertical", ScrollbarAxis::Vertical),
  ] {
    let make = move |ctx: Ctx<'js>, handle: AnyScroll| new_el(&ctx, handle.scrollbar(axis));
    scrollbar.set(name, Function::new(ctx.clone(), make)?)?;
  }
  let handle = Class::<VirtualListScrollHandle>::create_constructor(ctx)?.expect("constructor");
  Ok(vec![
    (
      "Tabs",
      ctor(ctx, "Tabs", |ctx: Ctx<'js>, id: Id| {
        new_el(&ctx, b::Tabs::new(id.into_inner()))
      })?,
    ),
    (
      "Tab",
      ctor(ctx, "Tab", |ctx: Ctx<'js>, id: Id| {
        new_el(&ctx, b::Tab::new(id.into_inner()))
      })?,
    ),
    (
      "Accordion",
      ctor(ctx, "Accordion", |ctx: Ctx<'js>, id: Id| {
        new_el(&ctx, b::Accordion::new(id.into_inner()))
      })?,
    ),
    (
      "AccordionItem",
      ctor(ctx, "AccordionItem", |ctx: Ctx<'js>| {
        new_el(&ctx, b::AccordionItem::new())
      })?,
    ),
    (
      "AccordionHeader",
      ctor(
        ctx,
        "AccordionHeader",
        |ctx: Ctx<'js>, trigger: Child<'js>| {
          let trigger = part::<b::AccordionTrigger>(&ctx, trigger, "an AccordionTrigger")?;
          new_el(&ctx, b::AccordionHeader::new(trigger))
        },
      )?,
    ),
    (
      "AccordionPanel",
      ctor(ctx, "AccordionPanel", |ctx: Ctx<'js>| {
        new_el(&ctx, b::AccordionPanel::new())
      })?,
    ),
    (
      "AccordionTrigger",
      ctor(ctx, "AccordionTrigger", |ctx: Ctx<'js>, id: Id| {
        new_el(&ctx, b::AccordionTrigger::new(id.into_inner()))
      })?,
    ),
    (
      "Pagination",
      ctor(ctx, "Pagination", |ctx: Ctx<'js>, id: Id| {
        // its render reads no state: the page buttons are the script's
        new_el(
          &ctx,
          b::Pagination::new(id.into_inner(), b::PaginationState::new(1, 1)),
        )
      })?,
    ),
    ("pagination_items", func(ctx, pagination_items)?),
    (
      "Collapsible",
      ctor(ctx, "Collapsible", |ctx: Ctx<'js>| {
        new_el(&ctx, b::Collapsible::new())
      })?,
    ),
    (
      "h_resizable",
      func(ctx, |ctx: Ctx<'js>, id: Id| {
        resizable(&ctx, b::h_resizable(id.into_inner()))
      })?,
    ),
    (
      "v_resizable",
      func(ctx, |ctx: Ctx<'js>, id: Id| {
        resizable(&ctx, b::v_resizable(id.into_inner()))
      })?,
    ),
    (
      "resizable_panel",
      func(ctx, |ctx: Ctx<'js>| new_el(&ctx, b::resizable_panel()))?,
    ),
    ("Scrollbar", scrollbar.into_value()),
    (
      "v_virtual_list",
      func(
        ctx,
        |ctx: Ctx<'js>,
         id: Id,
         count: u32,
         sizes: Value<'js>,
         key: Function<'js>,
         render: Function<'js>| {
          virtual_list(&ctx, Axis::Vertical, id, count, sizes, key, render)
        },
      )?,
    ),
    (
      "h_virtual_list",
      func(
        ctx,
        |ctx: Ctx<'js>,
         id: Id,
         count: u32,
         sizes: Value<'js>,
         key: Function<'js>,
         render: Function<'js>| {
          virtual_list(&ctx, Axis::Horizontal, id, count, sizes, key, render)
        },
      )?,
    ),
    ("VirtualListScrollHandle", handle.into_value()),
  ])
}

fn separator<'js>(
  ctx: &Ctx<'js>,
  name: &str,
  make: fn() -> c::separator::Separator,
) -> Result<Value<'js>> {
  ctor(ctx, name, move |ctx: Ctx<'js>| new_el(&ctx, make()))
}

fn component_exports<'js>(ctx: &Ctx<'js>) -> Result<Vec<(&'static str, Value<'js>)>> {
  let tab_bar = |ctx: Ctx<'js>, id: Id| new_el(&ctx, c::tab::TabBar::new(id.into_inner()));

  Ok(vec![
    ("TabBar", ctor(ctx, "TabBar", tab_bar)?),
    ("Tabs", ctor(ctx, "Tabs", tab_bar)?),
    (
      "Tab",
      ctor(ctx, "Tab", |ctx: Ctx<'js>| new_el(&ctx, c::tab::Tab::new()))?,
    ),
    (
      "Accordion",
      ctor(ctx, "Accordion", |ctx: Ctx<'js>, id: Id| {
        new_el(&ctx, c::accordion::Accordion::new(id.into_inner()))
      })?,
    ),
    (
      "AccordionItem",
      ctor(ctx, "AccordionItem", |ctx: Ctx<'js>| {
        new_el(&ctx, c::accordion::AccordionItem::new())
      })?,
    ),
    (
      "Collapsible",
      ctor(ctx, "Collapsible", |ctx: Ctx<'js>| {
        new_el(&ctx, c::collapsible::Collapsible::new())
      })?,
    ),
    (
      "Pagination",
      ctor(ctx, "Pagination", |ctx: Ctx<'js>, id: Id| {
        new_el(&ctx, c::pagination::Pagination::new(id.into_inner()))
      })?,
    ),
    (
      "Stepper",
      ctor(ctx, "Stepper", |ctx: Ctx<'js>, id: Id| {
        new_el(&ctx, c::stepper::Stepper::new(id.into_inner()))
      })?,
    ),
    (
      "StepperItem",
      ctor(ctx, "StepperItem", |ctx: Ctx<'js>| {
        new_el(&ctx, c::stepper::StepperItem::new())
      })?,
    ),
    (
      "Breadcrumb",
      ctor(ctx, "Breadcrumb", |ctx: Ctx<'js>, labels: Vec<String>| {
        new_el(&ctx, c::breadcrumb::Breadcrumb::new().children(labels))
      })?,
    ),
    (
      "GroupBox",
      ctor(ctx, "GroupBox", |ctx: Ctx<'js>| {
        new_el(&ctx, c::group_box::GroupBox::new())
      })?,
    ),
    (
      "StatusBar",
      ctor(ctx, "StatusBar", |ctx: Ctx<'js>| {
        new_el(&ctx, c::status_bar::StatusBar::new())
      })?,
    ),
    (
      "Toolbar",
      ctor(ctx, "Toolbar", |ctx: Ctx<'js>, id: Id| {
        new_el(&ctx, c::toolbar::Toolbar::new(id.into_inner()))
      })?,
    ),
    (
      "Separator",
      separator(ctx, "Separator", c::separator::Separator::horizontal)?,
    ),
    (
      "VerticalSeparator",
      separator(ctx, "VerticalSeparator", c::separator::Separator::vertical)?,
    ),
    (
      "DashedSeparator",
      separator(
        ctx,
        "DashedSeparator",
        c::separator::Separator::horizontal_dashed,
      )?,
    ),
    (
      "VerticalDashedSeparator",
      separator(
        ctx,
        "VerticalDashedSeparator",
        c::separator::Separator::vertical_dashed,
      )?,
    ),
    (
      "Resizable",
      ctor(ctx, "Resizable", |ctx: Ctx<'js>, id: Id| {
        resizable(&ctx, c::resizable::h_resizable(id.into_inner()))
      })?,
    ),
    (
      "ResizablePanel",
      ctor(ctx, "ResizablePanel", |ctx: Ctx<'js>| {
        new_el(&ctx, b::resizable_panel())
      })?,
    ),
    (
      "Scroll",
      ctor(
        ctx,
        "Scroll",
        |ctx: Ctx<'js>, handle: Class<'js, ScrollHandle>| {
          let mut d = div().flex();
          let i = mem::take(gpui_kit::InteractiveElement::interactivity(&mut d));
          *gpui_kit::InteractiveElement::interactivity(&mut d) =
            I(i).track_scroll(&handle.borrow().handle).0;
          new_el(&ctx, Scroll(scroll_axis(d, ScrollbarAxis::Vertical)))
        },
      )?,
    ),
    (
      "Scrollbar",
      ctor(
        ctx,
        "Scrollbar",
        |ctx: Ctx<'js>, id: Id, handle: AnyScroll| {
          new_el(
            &ctx,
            handle.scrollbar(ScrollbarAxis::Both).id(id.into_inner()),
          )
        },
      )?,
    ),
    (
      "ScrollbarHandle",
      ctor(ctx, "ScrollbarHandle", ScrollHandle::new)?,
    ),
    (
      "Sidebar",
      ctor(ctx, "Sidebar", |ctx: Ctx<'js>, id: Id| {
        new_el(&ctx, Sidebar::<SidebarMenu>::new(id.into_inner()))
      })?,
    ),
    (
      "SidebarHeader",
      ctor(ctx, "SidebarHeader", |ctx: Ctx<'js>| {
        new_el(&ctx, SidebarHeader::new())
      })?,
    ),
    (
      "SidebarFooter",
      ctor(ctx, "SidebarFooter", |ctx: Ctx<'js>| {
        new_el(&ctx, SidebarFooter::new())
      })?,
    ),
    (
      "SidebarMenu",
      ctor(ctx, "SidebarMenu", |ctx: Ctx<'js>| {
        new_el(&ctx, SidebarMenu::new())
      })?,
    ),
    (
      "SidebarMenuItem",
      ctor(ctx, "SidebarMenuItem", |ctx: Ctx<'js>, label: String| {
        let item = SidebarMenuItem::new(label);
        new_el(
          &ctx,
          SideItem {
            item,
            children: Vec::new(),
          },
        )
      })?,
    ),
    (
      "SidebarToggleButton",
      ctor(ctx, "SidebarToggleButton", |ctx: Ctx<'js>| {
        new_el(&ctx, SidebarToggleButton::new())
      })?,
    ),
    (
      "DropdownMenu",
      ctor(
        ctx,
        "DropdownMenu",
        |ctx: Ctx<'js>, id: Id, label: String| {
          let button = Button::new(id.into_inner()).label(label);
          new_el(
            &ctx,
            Dropdown {
              button,
              items: Vec::new(),
            },
          )
        },
      )?,
    ),
    (
      "NativeMenuTrigger",
      ctor(
        ctx,
        "NativeMenuTrigger",
        |ctx: Ctx<'js>, id: Id, label: String| {
          let button = Button::new(id.into_inner()).label(label);
          new_el(
            &ctx,
            NativeTrigger {
              button,
              items: Vec::new(),
            },
          )
        },
      )?,
    ),
    (
      "NativeMenuItem",
      ctor(
        ctx,
        "NativeMenuItem",
        |ctx: Ctx<'js>, label: String, action: String| {
          let action = crate::element::events::scoped(&ctx, &action)?;
          new_el(
            &ctx,
            NativeItem {
              label,
              action,
              disabled: false,
              checked: false,
            },
          )
        },
      )?,
    ),
    (
      "NativeMenuSeparator",
      ctor(ctx, "NativeMenuSeparator", |ctx: Ctx<'js>| {
        new_el(&ctx, NativeSeparator)
      })?,
    ),
    (
      "CommandState",
      ctor(ctx, "CommandState", CommandStateHandle::new)?,
    ),
    (
      "Menu",
      ctor(ctx, "Menu", |ctx: Ctx<'js>, label: String| {
        new_el(
          &ctx,
          AppMenu {
            label,
            disabled: false,
            entries: Vec::new(),
          },
        )
      })?,
    ),
    (
      "MenuItem",
      ctor(
        ctx,
        "MenuItem",
        |ctx: Ctx<'js>, label: String, action: String| {
          let action = crate::element::events::scoped(&ctx, &action)?;
          new_el(
            &ctx,
            MenuEntry {
              label,
              action,
              disabled: false,
              checked: false,
            },
          )
        },
      )?,
    ),
    (
      "MenuSeparator",
      ctor(ctx, "MenuSeparator", |ctx: Ctx<'js>| {
        new_el(&ctx, MenuSeparator)
      })?,
    ),
    (
      "MenuBar",
      ctor(ctx, "MenuBar", |ctx: Ctx<'js>, id: String| {
        new_el(
          &ctx,
          MenuBar {
            id,
            menus: Vec::new(),
          },
        )
      })?,
    ),
    (
      "DropdownButton",
      ctor(
        ctx,
        "DropdownButton",
        |ctx: Ctx<'js>, id: Id, label: String| {
          let drop = DropButton {
            button: c::button::DropdownButton::new(id.into_inner()),
            action: Button::new("action").label(label),
            anchor: gpui_kit::Anchor::TopRight,
            items: Vec::new(),
          };
          new_el(&ctx, drop)
        },
      )?,
    ),
    (
      "Command",
      ctor(
        ctx,
        "Command",
        |ctx: Ctx<'js>, state: Class<'js, CommandStateHandle>| {
          let state = state.borrow().state.entity(&ctx)?.clone();
          new_el(&ctx, Command::new(&state))
        },
      )?,
    ),
    (
      "CommandItem",
      ctor(ctx, "CommandItem", |ctx: Ctx<'js>, label: String| {
        new_el(&ctx, CommandItem::new().label(label))
      })?,
    ),
    (
      "CommandGroup",
      ctor(ctx, "CommandGroup", |ctx: Ctx<'js>, label: String| {
        new_el(&ctx, CommandGroup::new().label(label))
      })?,
    ),
    (
      "CommandSeparator",
      ctor(ctx, "CommandSeparator", |ctx: Ctx<'js>| {
        new_el(&ctx, CommandSeparator)
      })?,
    ),
  ])
}

// ---------------------------------------------------------------- typings

const BASE_D_TS: &str = r#"  type Ctor<A extends any[]> = { new (...args: A): Element; new: (...args: A) => Element };
  /** A tab list; each `Tab` is told whether it is selected and reports clicks. */
  export const Tabs: Ctor<[id: string | number]>;
  /** One tab: `selected(..)` in, `on_click(..)` out. */
  export const Tab: Ctor<[id: string | number]>;
  /** An accordion: items, each with an `AccordionHeader` and an `AccordionPanel`. Draws nothing. */
  export const Accordion: Ctor<[id: string | number]>;
  /** One item: `open(..)`, `header(..)`, `panel(..)`. */
  export const AccordionItem: Ctor<[]>;
  /** The heading owning an item's trigger. */
  export const AccordionHeader: Ctor<[trigger: Element<"AccordionTrigger">]>;
  /** The region an item reveals; gone while shut unless `keep_mounted()`. */
  export const AccordionPanel: Ctor<[]>;
  /** The button: `on_change(open => ..)` asks for the other state. */
  export const AccordionTrigger: Ctor<[id: string | number]>;
  /** A pagination landmark; draw the pages from `pagination_items`. */
  export const Pagination: Ctor<[id: string | number]>;
  /** One entry of a page layout: a page, or a gap standing for the pages `[first, last]`. */
  export type PaginationEntry =
    | { page: number; ellipsis?: undefined }
    | { ellipsis: [first: number, last: number]; page?: undefined };
  /** Which pages to draw around `current_page`, and where the gaps fall. */
  export function pagination_items(current_page: number, total_pages: number, visible_pages?: number): PaginationEntry[];
  /** Renders its `content` only while `open`; children always. */
  export const Collapsible: Ctor<[]>;
  /** A row of `resizable_panel()`s with draggable dividers; sizes persist under `id`. */
  export function h_resizable(id: string | number): Element<"Resizable">;
  /** A column of `resizable_panel()`s. */
  export function v_resizable(id: string | number): Element<"Resizable">;
  /** One pane of a resizable group: `size(px)`, `size_range(min, max)`. */
  export function resizable_panel(): Element<"ResizablePanel">;
  /** A scroll area's or virtual list's name, or its handle. */
  type ScrollTarget = string | number | import("corona/kit").ScrollHandle | VirtualListScrollHandle;
  /** A scrollbar driving the area that tracks `handle`; it fills the element it is put in. */
  export const Scrollbar: {
    new: (handle: ScrollTarget) => Element<"Scrollbar">;
    horizontal: (handle: ScrollTarget) => Element<"Scrollbar">;
    vertical: (handle: ScrollTarget) => Element<"Scrollbar">;
  };
  /** A list of rows of the given heights that renders only the visible ones. */
  export function v_virtual_list(
    id: string | number,
    item_count: number,
    item_sizes: number | number[],
    get_key: (index: number) => string,
    render: (range: import("corona/kit").ItemRange, cx: import("corona/kit").Context) => Element[],
  ): Element<"VirtualList">;
  /** `v_virtual_list` along the other axis; `item_sizes` are widths. */
  export function h_virtual_list(
    id: string | number,
    item_count: number,
    item_sizes: number | number[],
    get_key: (index: number) => string,
    render: (range: import("corona/kit").ItemRange, cx: import("corona/kit").Context) => Element[],
  ): Element<"VirtualList">;
  /** A virtual list's scroll position; `track_scroll(handle)` on the list. */
  export class VirtualListScrollHandle {
    constructor();
    static new(): VirtualListScrollHandle;
    /** Puts item `index` on screen. */
    scroll_to_item(index: number, strategy?: "top" | "center"): void;
    scroll_to_bottom(): void;
  }
"#;

const COMPONENT_D_TS: &str = r#"  type Ctor<A extends any[]> = { new (...args: A): Element; new: (...args: A) => Element };
  /** A styled tab bar taking `Tab` children. */
  export const TabBar: Ctor<[id: string | number]>;
  /** @deprecated Use `TabBar`. */
  export const Tabs: Ctor<[id: string | number]>;
  /** One tab of a `TabBar`. */
  export const Tab: Ctor<[]>;
  /** An accordion taking `AccordionItem` children. */
  export const Accordion: Ctor<[id: string | number]>;
  /** One item: `title(..)`, `open(..)`, children as its content. */
  export const AccordionItem: Ctor<[]>;
  /** Renders its `content` only while `open`. */
  export const Collapsible: Ctor<[]>;
  /** Page buttons: `current_page`, `total_pages`, `on_change(page => ..)`. */
  export const Pagination: Ctor<[id: string | number]>;
  /** Steps taking `StepperItem` children. */
  export const Stepper: Ctor<[id: string | number]>;
  export const StepperItem: Ctor<[]>;
  /** A trail of labels. */
  export const Breadcrumb: Ctor<[labels: string[]]>;
  /** A titled container. */
  export const GroupBox: Ctor<[]>;
  /** A bar: children in the middle, `left_content`/`right_content` at the edges. */
  export const StatusBar: Ctor<[]>;
  export const Toolbar: Ctor<[id: string | number]>;
  export const Separator: Ctor<[]>;
  export const VerticalSeparator: Ctor<[]>;
  export const DashedSeparator: Ctor<[]>;
  export const VerticalDashedSeparator: Ctor<[]>;
  /** Panes with draggable dividers, horizontal unless `axis("vertical")`; takes `ResizablePanel`s. */
  export const Resizable: Ctor<[id: string | number]>;
  export const ResizablePanel: Ctor<[]>;
  /** A vertically scrolling area tracking `handle`; `scroll_axis(..)` changes the axes. */
  export const Scroll: Ctor<[handle: ScrollbarHandle]>;
  /** A scrollbar driving the area that tracks `handle`. */
  export const Scrollbar: Ctor<[id: string | number, handle: string | number | ScrollbarHandle | import("corona/base").VirtualListScrollHandle]>;
  export type ScrollbarHandle = import("corona/kit").ScrollHandle;
  export const ScrollbarHandle: { (): ScrollbarHandle; new: () => ScrollbarHandle };
  /** A collapsible sidebar taking `SidebarMenu` children, `header(..)` and `footer(..)`. */
  export const Sidebar: Ctor<[id: string | number]>;
  export const SidebarHeader: Ctor<[]>;
  export const SidebarFooter: Ctor<[]>;
  /** Rows: `SidebarMenuItem` children. */
  export const SidebarMenu: Ctor<[]>;
  /** A row; `SidebarMenuItem` children make its submenu. */
  export const SidebarMenuItem: Ctor<[label: string]>;
  export const SidebarToggleButton: Ctor<[]>;
  /** A button opening a menu of `item(label, cx => ..)`s. */
  export const DropdownMenu: Ctor<[id: string | number, label: string]>;
  /** A button popping up the OS menu of its `NativeMenuItem` children. */
  export const NativeMenuTrigger: Ctor<[id: string | number, label: string]>;
  /** An item dispatching `action`, for `on_action`. */
  export const NativeMenuItem: Ctor<[label: string, action: string]>;
  export const NativeMenuSeparator: Ctor<[]>;
  /** A command palette's query and selection; make it in `init`. */
  export interface CommandState {
    /** The search text. */
    query(): string;
    release(): boolean;
  }
  export const CommandState: { (): CommandState; new: () => CommandState };
  /** A searchable command palette taking `CommandItem`, `CommandGroup` and `CommandSeparator`. */
  export const Command: Ctor<[state: CommandState]>;
  export const CommandItem: Ctor<[label: string]>;
  /** A labelled section taking `CommandItem` children. */
  export const CommandGroup: Ctor<[label: string]>;
  export const CommandSeparator: Ctor<[]>;
  /** An application menu of `MenuItem`s and `MenuSeparator`s, for a `MenuBar`. */
  export const Menu: Ctor<[label: string]>;
  /** A menu item dispatching `action`, for `on_action`. */
  export const MenuItem: Ctor<[label: string, action: string]>;
  export const MenuSeparator: Ctor<[]>;
  /** Installs its `Menu` children as the application menus and draws them in the window. */
  export const MenuBar: Ctor<[id: string]>;
  /** A labelled button with a menu of `menu_item(label, cx => ..)`s. */
  export const DropdownButton: Ctor<[id: string | number, label: string]>;
"#;

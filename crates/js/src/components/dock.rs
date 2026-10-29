//! Dockable layouts: `DockArea` (a retained gpui-base dock area), `dock_area(area)` drawing it
//! with script chrome, `dock_content()` and the dock commands chrome elements carry.
//!
//! The layout is the user's, so it lives in the retained area, not in a description: drags,
//! resizes and closed tabs happen without the script rendering. A panel is a script view
//! (`cx.new(Class)`) in a [`ScriptPanel`]. `DockArea.register_panel(name, Class)` teaches the
//! panel registry to rebuild it from a saved layout: `load` constructs the views first, base
//! rebuilds the tree from them, then each gets its `deserialize(data)`. `dump` reads each
//! panel's `serialize()` first. Both run inside the script's call, so no JS runs from Rust.
//!
//! Base draws no chrome. The four handlers of `dock_area(..)` are view slots base calls from
//! layout, with the group or dock as JSON. Their elements act through dock commands
//! (`select_tab(group, i)`, ..), resolved at the click against the contexts the skin recorded
//! while it drew.

use std::{
  cell::RefCell,
  collections::{HashMap, HashSet, VecDeque},
  rc::{Rc, Weak},
  sync::Arc,
};

use gpui_kit::{
  AnyElement, AnyView, App, AppContext, Context, Div, DragMoveEvent, Empty, Entity, EventEmitter,
  FocusHandle, Focusable, InteractiveElement, IntoElement, ParentElement, Render, StyleRefinement,
  Styled, WeakEntity, Window,
  base::dock::{
    DockArea, DockAreaRenderer, DockAreaState, DockContext, DockEvent, DockPlacement, DragPanel,
    DropIndicator, PaneRef, Panel, PanelBuildContext, PanelEvent, PanelId, PanelInfo, PanelState,
    PanelView, TabGroupContext, TabGroupRenderer, register_panel,
  },
  div, px,
};
use rquickjs::{
  CatchResultExt, Class, Ctx, Exception, FromJs, Function, JsLifetime, Object, Result, Value,
  class::Trace,
  function::{Args, Constructor},
};
use serde_json::{Value as Json, json};

use super::{Family, ctor, declare};
use crate::{
  Child, El, Node, Opt, Retained, ScriptView, ViewCx, app,
  el::interact,
  frame_or_throw, new_el,
  plugin::plugin,
  runtime::{Runtime, runtime},
  scope::try_frame,
  view::{ViewHandle, current_view, new_child, window_or_throw},
};

// ---------------------------------------------------------------------------------------------
// Panels

/// `name` leaked once and the same pointer after: `Panel::panel_name` is `&'static str`.
/// ponytail: one leak per distinct panel name, bounded by the panels plugins register
fn intern(name: &str) -> &'static str {
  thread_local! {
    static NAMES: RefCell<HashSet<&'static str>> = RefCell::default();
  }
  NAMES.with(|names| {
    let mut names = names.borrow_mut();
    match names.get(name) {
      Some(name) => name,
      None => {
        let name: &'static str = Box::leak(name.into());
        names.insert(name);
        name
      }
    }
  })
}

/// `shell:<plugin>/<panel>`: plugins, and the host's own panels, never collide in a layout
fn qualified(ctx: &Ctx<'_>, panel: &str) -> Result<&'static str> {
  Ok(intern(&format!("shell:{}/{panel}", plugin(ctx)?.id)))
}

/// A dock panel whose body is a script view
pub struct ScriptPanel {
  name: &'static str,
  view: Entity<ScriptView>,
  focus: FocusHandle,
  closable: bool,
  zoomable: bool,
  visible: bool,
  /// What the view's `serialize()` answered when `dump()` asked
  payload: Option<Json>,
  /// The child view the registry built, released with the panel
  built: Option<(Weak<Runtime>, u32)>,
}

impl Panel for ScriptPanel {
  fn panel_name(&self) -> &'static str {
    self.name
  }
  fn visible(&self, _: &App) -> bool {
    self.visible
  }
  fn closable(&self, _: &App) -> bool {
    self.closable
  }
  fn zoomable(&self, _: &App) -> bool {
    self.zoomable
  }
  fn dump(&self, _: &App) -> PanelState {
    let mut state = PanelState::new(self.name);
    if let Some(payload) = self.payload.clone() {
      state.info = PanelInfo::panel(payload);
    }
    state
  }
  fn on_removed(&mut self, _: &mut Window, _: &mut Context<Self>) {
    if let Some((rt, id)) = self.built.take()
      && let Some(rt) = rt.upgrade()
    {
      let child = rt.children.borrow_mut().remove(&id);
      drop(child);
    }
  }
}

impl EventEmitter<PanelEvent> for ScriptPanel {}

impl Focusable for ScriptPanel {
  fn focus_handle(&self, _: &App) -> FocusHandle {
    self.focus.clone()
  }
}

impl Render for ScriptPanel {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    div()
      .track_focus(&self.focus)
      .size_full()
      .child(self.view.clone())
  }
}

/// Stands in for a saved panel nothing can rebuild now, and writes it back out unchanged
struct RetainedPanel {
  name: &'static str,
  state: PanelState,
  focus: FocusHandle,
}

impl Panel for RetainedPanel {
  fn panel_name(&self) -> &'static str {
    self.name
  }
  fn dump(&self, _: &App) -> PanelState {
    self.state.clone()
  }
}

impl EventEmitter<PanelEvent> for RetainedPanel {}

impl Focusable for RetainedPanel {
  fn focus_handle(&self, _: &App) -> FocusHandle {
    self.focus.clone()
  }
}

impl Render for RetainedPanel {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    Empty
  }
}

/// A view `load` made for a panel: its runtime, its child id and the view
type Prebuilt = (Weak<Runtime>, u32, Entity<ScriptView>);

thread_local! {
  /// The views `load` constructed for the registry to take, by panel name
  static PREBUILT: RefCell<HashMap<&'static str, VecDeque<Prebuilt>>> =
    RefCell::default();
  /// The panels the registry rebuilt during a `load`, and their saved data
  static RESTORED: RefCell<Vec<(Entity<ScriptView>, Json)>> = RefCell::default();
}

/// The registry's builder for `name`: a view `load` constructed, or a placeholder
fn build(name: &'static str, context: PanelBuildContext, cx: &mut App) -> Arc<dyn PanelView> {
  let built = PREBUILT.with(|p| p.borrow_mut().get_mut(name).and_then(VecDeque::pop_front));
  let Some((rt, id, view)) = built else {
    let state = context.state().clone();
    return Arc::new(cx.new(|cx| RetainedPanel {
      name,
      state,
      focus: cx.focus_handle(),
    }));
  };
  if let PanelInfo::Panel(data) = context.info()
    && !data.is_null()
  {
    RESTORED.with(|r| r.borrow_mut().push((view.clone(), data.clone())));
  }
  Arc::new(cx.new(|cx| ScriptPanel {
    name,
    view,
    focus: cx.focus_handle(),
    closable: true,
    zoomable: true,
    visible: true,
    payload: None,
    built: Some((rt, id)),
  }))
}

/// The panel names of a saved layout's leaves
fn leaves(state: &PanelState, out: &mut Vec<String>) {
  if state.children.is_empty() && matches!(state.info, PanelInfo::Panel(_)) {
    out.push(state.panel_name.clone());
  }
  state.children.iter().for_each(|c| leaves(c, out));
}

/// Calls `method` of a view's JS object, if it has one
fn call_view<'js>(
  ctx: &Ctx<'js>,
  view: &Entity<ScriptView>,
  method: &str,
  arg: Option<Value<'js>>,
) -> Result<Option<Value<'js>>> {
  let instance = view.read(app()).instance().clone().restore(ctx)?;
  let Ok(f) = instance.get::<_, Function>(method) else {
    return Ok(None);
  };
  let mut args = Args::new(ctx.clone(), 2);
  args.this(instance)?;
  if let Some(arg) = arg {
    args.push_arg(arg)?;
  }
  f.call_arg(args).map(Some)
}

// ---------------------------------------------------------------------------------------------
// The skin: chrome handlers and the contexts their commands resolve against

/// Which view slot draws each piece of chrome, written by the latest `dock_area(..)`
#[derive(Clone, Default)]
struct Hooks {
  view: Option<WeakEntity<ScriptView>>,
  tab_bar: Option<u64>,
  empty_group: Option<u64>,
  drop_indicator: Option<u64>,
  dock: Option<u64>,
}

#[derive(Default)]
pub(crate) struct Skin {
  area: std::cell::Cell<u64>,
  hooks: RefCell<Hooks>,
  groups: RefCell<HashMap<u64, TabGroupContext>>,
  docks: RefCell<HashMap<DockPlacement, DockContext>>,
}

thread_local! {
  /// Every live area's skin, by the area's entity id, for commands to find
  static SKINS: RefCell<HashMap<u64, Weak<Skin>>> = RefCell::default();
  /// The content of the dock whose chrome handler runs, for `dock_content()` to take
  static DOCK_CONTENT: RefCell<Option<AnyElement>> = const { RefCell::new(None) };
}

fn skin(area: u64) -> Option<Rc<Skin>> {
  SKINS.with(|s| s.borrow().get(&area).and_then(Weak::upgrade))
}

/// Slot indexes of the chrome that is not one per group
const DOCK_INDEX: usize = 1 << 30;
const DROP_INDEX: usize = DOCK_INDEX + 8;

impl Skin {
  /// Calls the chrome handler `key` picks with `payload`; `None` for none, or `null`
  fn draw(
    &self,
    key: impl Fn(&Hooks) -> Option<u64>,
    index: usize,
    payload: Json,
    window: &mut Window,
    cx: &mut App,
  ) -> Option<AnyElement> {
    let hooks = self.hooks.borrow().clone();
    let (key, view) = (key(&hooks)?, hooks.view.as_ref()?.upgrade()?);
    let json = payload.to_string();
    let drawn = view.update(cx, |view, cx| {
      let args = move |a: &mut Args<'_>| {
        let payload = a.ctx().json_parse(json)?;
        a.push_arg(payload)
      };
      view.render_content(
        key,
        Some(index),
        window,
        cx,
        args,
        |ctx, frame, value| match value.is_null() || value.is_undefined() {
          true => Ok(None),
          false => frame.child(ctx, Child::from_js(ctx, value)?).map(Some),
        },
      )
    });
    drawn.unwrap_or_else(|e| Some(format!("dock chrome failed: {e}").into_any_element()))
  }

  fn group(&self, group: &TabGroupContext, cx: &App) -> Json {
    let active = group.active_panel().map(|p| p.panel_id(cx));
    let tabs: Vec<Json> = (group.panels().iter().enumerate())
      .map(|(index, panel)| {
        let id = panel.panel_id(cx);
        json!({
          "index": index,
          "name": panel.panel_name(cx),
          "id": id.as_u64(),
          "active": Some(id) == active,
          "visible": panel.visible(cx),
          "closable": panel.closable(cx),
          "zoomable": panel.zoomable(cx),
        })
      })
      .collect();
    json!({
      "area": self.area.get(),
      "node": group.node().as_u64(),
      "active_index": group.active_ix(),
      "zoomed": group.is_zoomed(),
      "collapsed": group.is_collapsed(),
      "locked": group.is_locked(),
      "draggable": group.is_draggable(),
      "droppable": group.is_droppable(),
      "closable": group.is_closable(),
      "tabs": tabs,
    })
  }
}

fn rect(origin: gpui_kit::Point<gpui_kit::Pixels>, size: gpui_kit::Size<gpui_kit::Pixels>) -> Json {
  json!({
    "x": f32::from(origin.x),
    "y": f32::from(origin.y),
    "width": f32::from(size.width),
    "height": f32::from(size.height),
  })
}

fn placement_name(p: DockPlacement) -> &'static str {
  match p {
    DockPlacement::Center => "center",
    DockPlacement::Left => "left",
    DockPlacement::Right => "right",
    DockPlacement::Bottom => "bottom",
  }
}

fn placement(ctx: &Ctx<'_>, name: &str) -> Result<DockPlacement> {
  Ok(match name {
    "center" => DockPlacement::Center,
    "left" => DockPlacement::Left,
    "right" => DockPlacement::Right,
    "bottom" => DockPlacement::Bottom,
    _ => {
      let known = ["center", "left", "right", "bottom"];
      let message = crate::style::unknown("dock placement", name, &known);
      return Err(Exception::throw_type(ctx, &message));
    }
  })
}

/// One skin, as both renderers base asks
struct SkinRef(Rc<Skin>);

impl DockAreaRenderer for SkinRef {
  fn render_dock(
    &self,
    dock: &DockContext,
    content: AnyElement,
    window: &mut Window,
    cx: &mut App,
  ) -> AnyElement {
    let skin = &self.0;
    skin
      .docks
      .borrow_mut()
      .insert(dock.placement(), dock.clone());
    if skin.hooks.borrow().dock.is_none() {
      return content;
    }
    let payload = json!({
      "area": skin.area.get(),
      "placement": placement_name(dock.placement()),
      "size": f32::from(dock.size()),
      "open": dock.is_open(),
      "collapsible": dock.is_collapsible(),
    });
    let previous = DOCK_CONTENT.with(|c| c.borrow_mut().replace(content));
    let index = DOCK_INDEX + dock.placement() as usize;
    let drawn = skin.draw(|h| h.dock, index, payload, window, cx);
    let unplaced = DOCK_CONTENT.with(|c| std::mem::replace(&mut *c.borrow_mut(), previous));
    match (drawn, unplaced) {
      (Some(drawn), None) => drawn,
      (Some(drawn), Some(content)) => {
        tracing::warn!("a dock's chrome drew no dock_content(); its panels follow it");
        div()
          .size_full()
          .child(drawn)
          .child(content)
          .into_any_element()
      }
      (None, Some(content)) => content,
      (None, None) => Empty.into_any_element(),
    }
  }

  fn tab_group_renderer(&self) -> Rc<dyn TabGroupRenderer> {
    Rc::new(SkinRef(self.0.clone()))
  }
}

impl TabGroupRenderer for SkinRef {
  fn render_tab_bar(
    &self,
    group: &TabGroupContext,
    window: &mut Window,
    cx: &mut App,
  ) -> AnyElement {
    let skin = &self.0;
    let node = group.node().as_u64();
    skin.groups.borrow_mut().insert(node, group.clone());
    let payload = skin.group(group, cx);
    (skin.draw(
      |h| h.tab_bar,
      node as usize % DOCK_INDEX,
      payload,
      window,
      cx,
    ))
    .unwrap_or_else(|| Empty.into_any_element())
  }

  /// A folded group draws no panel behind its tabs
  fn render_active_panel(
    &self,
    panel: AnyView,
    group: &TabGroupContext,
    _: &mut Window,
    _: &mut App,
  ) -> AnyElement {
    match group.is_collapsed() {
      true => Empty.into_any_element(),
      false => panel.into_any_element(),
    }
  }

  fn render_empty(
    &self,
    group: &TabGroupContext,
    window: &mut Window,
    cx: &mut App,
  ) -> Option<AnyElement> {
    let skin = &self.0;
    let node = group.node().as_u64();
    skin.groups.borrow_mut().insert(node, group.clone());
    let payload = skin.group(group, cx);
    skin.draw(
      |h| h.empty_group,
      node as usize % DOCK_INDEX,
      payload,
      window,
      cx,
    )
  }

  fn render_drop_indicator(
    &self,
    drop: DropIndicator,
    window: &mut Window,
    cx: &mut App,
  ) -> Option<AnyElement> {
    let skin = &self.0;
    let b = drop.bounds();
    let side = drop.placement().map(|p| match p {
      gpui_kit::base::Placement::Left => "left",
      gpui_kit::base::Placement::Right => "right",
      gpui_kit::base::Placement::Top => "top",
      gpui_kit::base::Placement::Bottom => "bottom",
    });
    let payload = json!({
      "placement": side,
      "bounds": rect(b.origin, b.size),
      "from": rect(drop.from().origin(), drop.from().size()),
      "to": rect(drop.to().origin(), drop.to().size()),
    });
    skin.draw(|h| h.drop_indicator, DROP_INDEX, payload, window, cx)
  }
}

// ---------------------------------------------------------------------------------------------
// The retained area

/// A dockable layout the user rearranges; `DockArea.new(id)` in `init` or a handler
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "DockArea")]
pub struct JsDockArea {
  #[qjs(skip_trace)]
  state: Retained<DockArea>,
  #[qjs(skip_trace)]
  skin: Rc<Skin>,
}

impl crate::Named for JsDockArea {
  const TS: &'static str = "DockArea";
}

/// Throws in `render`: an edit there would describe one layout and draw another
fn not_in_render(ctx: &Ctx<'_>, what: &str) -> Result<()> {
  match try_frame() {
    Some(_) => Err(Exception::throw_type(
      ctx,
      &format!("{what} changes the layout; call it from init(), a handler or a task"),
    )),
    None => Ok(()),
  }
}

/// `DockArea.new(id, { version }?)`
fn dock_area_new<'js>(
  ctx: Ctx<'js>,
  id: String,
  options: Opt<Object<'js>>,
) -> Result<Class<'js, JsDockArea>> {
  not_in_render(&ctx, "DockArea.new(id)")?;
  let version = match &options.0 {
    Some(o) => o.get::<_, Option<u32>>("version")?.map(|v| v as usize),
    None => None,
  };
  let window = window_or_throw(&ctx)?;
  let skin = Rc::new(Skin::default());
  let renderer = Rc::new(SkinRef(skin.clone()));
  let area = app().new(|cx| DockArea::new(id, version, window, cx).with_renderer(renderer));
  let key = area.entity_id().as_u64();
  skin.area.set(key);
  SKINS.with(|s| {
    let mut skins = s.borrow_mut();
    skins.retain(|_, s| s.strong_count() > 0);
    skins.insert(key, Rc::downgrade(&skin));
  });
  let state = Retained::new(&ctx, area)?;
  Class::instance(ctx, JsDockArea { state, skin })
}

/// `DockArea.register_panel(name, Class)`: the qualified name it registered under
fn register<'js>(ctx: Ctx<'js>, name: String, class: Function<'js>) -> Result<String> {
  let name = qualified(&ctx, &name)?;
  let rt = runtime(&ctx)?;
  let class = rquickjs::Persistent::save(&ctx, class);
  rt.panels.borrow_mut().insert(name.into(), class);
  register_panel(app(), name, move |context, _, cx| build(name, context, cx));
  Ok(name.into())
}

impl JsDockArea {
  fn area(&self, ctx: &Ctx<'_>) -> Result<Entity<DockArea>> {
    self.state.entity(ctx).cloned()
  }

  fn edit(
    &self,
    ctx: &Ctx<'_>,
    what: &str,
    f: impl FnOnce(&mut DockArea, &mut Window, &mut Context<DockArea>),
  ) -> Result<()> {
    not_in_render(ctx, what)?;
    let window = window_or_throw(ctx)?;
    self.area(ctx)?.update(app(), |a, cx| f(a, window, cx));
    Ok(())
  }

  fn read<R>(&self, ctx: &Ctx<'_>, f: impl FnOnce(&DockArea) -> R) -> Result<R> {
    Ok(f(self.area(ctx)?.read(app())))
  }

  /// The panels of every region, as `walk` meets them: `(placement, node, index, active, id)`
  fn each_panel(area: &DockArea, mut f: impl FnMut(DockPlacement, u64, usize, bool, PanelId)) {
    let regions = [
      DockPlacement::Center,
      DockPlacement::Left,
      DockPlacement::Right,
      DockPlacement::Bottom,
    ];
    for at in regions {
      let Some(tree) = area.layout(at) else {
        continue;
      };
      tree.root().walk(&mut |node| {
        if let PaneRef::Tabs { panels, active_ix } = node.kind() {
          for (i, id) in panels.iter().enumerate() {
            f(at, node.id().as_u64(), i, i == active_ix, *id);
          }
        }
      });
    }
  }
}

/// What `add_panel` is told about a panel
pub struct PanelOptions {
  name: String,
  placement: String,
  size: Option<f32>,
  closable: bool,
  zoomable: bool,
  visible: bool,
}

impl<'js> FromJs<'js> for PanelOptions {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    let o = Object::from_js(ctx, value)?;
    let name: String = o.get("name")?;
    if name.is_empty() {
      return Err(Exception::throw_type(
        ctx,
        "add_panel(view, options) needs a name",
      ));
    }
    Ok(Self {
      name,
      placement: o
        .get::<_, Option<String>>("placement")?
        .unwrap_or("center".into()),
      size: o.get("size")?,
      closable: o.get::<_, Option<bool>>("closable")?.unwrap_or(true),
      zoomable: o.get::<_, Option<bool>>("zoomable")?.unwrap_or(true),
      visible: o.get::<_, Option<bool>>("visible")?.unwrap_or(true),
    })
  }
}

#[rquickjs::methods]
impl JsDockArea {
  /// Docks `view`, a view from `cx.new(Class)`.
  pub fn add_panel<'js>(
    &self,
    ctx: Ctx<'js>,
    view: Class<'js, ViewHandle>,
    options: PanelOptions,
  ) -> Result<()> {
    let at = placement(&ctx, &options.placement)?;
    let view = view.borrow().entity(&ctx)?;
    let name = qualified(&ctx, &options.name)?;
    let panel = app().new(|cx| ScriptPanel {
      name,
      view,
      focus: cx.focus_handle(),
      closable: options.closable,
      zoomable: options.zoomable,
      visible: options.visible,
      payload: None,
      built: None,
    });
    let size = options.size.map(px);
    self.edit(&ctx, "add_panel(view, options)", |a, w, cx| {
      a.add_panel(panel, at, size, w, cx)
    })
  }

  /// Removes the panel with this id; false if there is none.
  pub fn remove_panel(&self, ctx: Ctx<'_>, id: f64) -> Result<bool> {
    not_in_render(&ctx, "remove_panel(id)")?;
    let area = self.area(&ctx)?;
    let Some(view) = area
      .read(app())
      .panel(PanelId::from_u64(id as u64))
      .cloned()
    else {
      return Ok(false);
    };
    let window = window_or_throw(&ctx)?;
    let any = view.as_any();
    if let Some(p) = any.downcast_ref::<Entity<ScriptPanel>>() {
      let p = p.clone();
      area.update(app(), |a, cx| a.remove_panel(p, window, cx));
    } else if let Some(p) = any.downcast_ref::<Entity<RetainedPanel>>() {
      let p = p.clone();
      area.update(app(), |a, cx| a.remove_panel(p, window, cx));
    } else {
      return Ok(false);
    }
    Ok(true)
  }

  /// Every panel, in tree order.
  pub fn panels<'js>(&self, ctx: Ctx<'js>) -> Result<Value<'js>> {
    let mut out = Vec::new();
    self.read(&ctx, |area| {
      let cx: &App = app();
      Self::each_panel(area, |at, node, index, active, id| {
        if let Some(p) = area.panel(id) {
          out.push(json!({
            "id": id.as_u64(),
            "name": p.panel_name(cx),
            "placement": placement_name(at),
            "node": node,
            "index": index,
            "active": active,
            "visible": p.visible(cx),
            "closable": p.closable(cx),
            "zoomable": p.zoomable(cx),
          }));
        }
      })
    })?;
    ctx.json_parse(Json::Array(out).to_string())
  }

  /// The whole layout as plain data, each panel's `serialize()` included.
  pub fn dump<'js>(&self, ctx: Ctx<'js>) -> Result<Value<'js>> {
    let area = self.area(&ctx)?;
    let mut panels = Vec::new();
    Self::each_panel(area.read(app()), |_, _, _, _, id| panels.push(id));
    for id in panels {
      let panel = area.read(app()).panel(id).cloned();
      let Some(panel) =
        panel.and_then(|p| p.as_any().downcast_ref::<Entity<ScriptPanel>>().cloned())
      else {
        continue;
      };
      let view = panel.read(app()).view.clone();
      let payload = match call_view(&ctx, &view, "serialize", None)? {
        Some(v) if !v.is_undefined() => {
          let json = ctx.json_stringify(v)?.map(|s| s.to_string()).transpose()?;
          json.and_then(|j| serde_json::from_str(&j).ok())
        }
        _ => None,
      };
      panel.update(app(), |p, _| p.payload = payload);
    }
    let state = area.read(app()).dump(app());
    let json = serde_json::to_string(&state)
      .map_err(|e| Exception::throw_type(&ctx, &format!("dump(): {e}")))?;
    ctx.json_parse(json)
  }

  /// Restores a layout `dump()` wrote, rebuilding registered panels and handing each its
  /// saved data through `deserialize(data)`.
  pub fn load<'js>(&self, ctx: Ctx<'js>, state: Value<'js>) -> Result<()> {
    not_in_render(&ctx, "load(state)")?;
    let json = ctx
      .json_stringify(state)?
      .map(|s| s.to_string())
      .transpose()?;
    let state: DockAreaState = serde_json::from_str(&json.unwrap_or_default())
      .map_err(|e| Exception::throw_type(&ctx, &format!("not a layout dump() wrote: {e}")))?;
    let mut names = Vec::new();
    for panel in [Some(&state.center)]
      .into_iter()
      .chain(
        [&state.left_dock, &state.right_dock, &state.bottom_dock]
          .map(|d| d.as_ref().map(|d| d.panel())),
      )
      .flatten()
    {
      leaves(panel, &mut names);
    }
    let rt = runtime(&ctx)?;
    for name in names {
      let class = rt.panels.borrow().get(&name).cloned();
      let Some(class) = class else {
        continue;
      };
      let class = Constructor::from_js(&ctx, class.restore(&ctx)?.into_value())?;
      let (handle, view) = new_child(&ctx, class, Value::new_undefined(ctx.clone()))?;
      let entry = (Rc::downgrade(&rt), handle.id(), view);
      PREBUILT.with(|p| {
        p.borrow_mut()
          .entry(intern(&name))
          .or_default()
          .push_back(entry)
      });
    }
    let window = window_or_throw(&ctx)?;
    let loaded = self
      .area(&ctx)?
      .update(app(), |a, cx| a.load(state, window, cx));
    // views base did not take, and the data of those it did
    let unused: Vec<u32> = PREBUILT.with(|p| {
      p.borrow_mut()
        .drain()
        .flat_map(|(_, v)| v)
        .map(|(_, id, _)| id)
        .collect()
    });
    let restored = RESTORED.with(|r| std::mem::take(&mut *r.borrow_mut()));
    for id in unused {
      let child = rt.children.borrow_mut().remove(&id);
      drop(child);
    }
    loaded.map_err(|e| Exception::throw_type(&ctx, &format!("load(state): {e}")))?;
    for (view, data) in restored {
      let data = ctx.json_parse(data.to_string())?;
      if let Err(e) = call_view(&ctx, &view, "deserialize", Some(data)).catch(&ctx) {
        tracing::warn!("a dock panel's deserialize(data): {e}");
      }
    }
    Ok(())
  }

  pub fn has_dock(&self, ctx: Ctx<'_>, at: String) -> Result<bool> {
    let at = placement(&ctx, &at)?;
    self.read(&ctx, |a| a.has_dock(at))
  }

  pub fn is_dock_open(&self, ctx: Ctx<'_>, at: String) -> Result<bool> {
    let at = placement(&ctx, &at)?;
    self.read(&ctx, |a| a.is_dock_open(at))
  }

  pub fn toggle_dock(&self, ctx: Ctx<'_>, at: String) -> Result<()> {
    let at = placement(&ctx, &at)?;
    self.edit(&ctx, "toggle_dock(placement)", |a, w, cx| {
      a.toggle_dock(at, w, cx)
    })
  }

  pub fn remove_dock(&self, ctx: Ctx<'_>, at: String) -> Result<()> {
    let at = placement(&ctx, &at)?;
    self.edit(&ctx, "remove_dock(placement)", |a, w, cx| {
      a.remove_dock(at, w, cx)
    })
  }

  pub fn dock_size(&self, ctx: Ctx<'_>, at: String) -> Result<Option<f32>> {
    let at = placement(&ctx, &at)?;
    self.read(&ctx, |a| a.dock_size(at).map(f32::from))
  }

  pub fn set_dock_size(&self, ctx: Ctx<'_>, at: String, size: f32) -> Result<()> {
    let at = placement(&ctx, &at)?;
    self.edit(&ctx, "set_dock_size(placement, size)", |a, w, cx| {
      a.set_dock_size(at, px(size.max(0.)), w, cx)
    })
  }

  pub fn set_dock_collapsible(&self, ctx: Ctx<'_>, at: String, value: bool) -> Result<()> {
    let at = placement(&ctx, &at)?;
    self.edit(
      &ctx,
      "set_dock_collapsible(placement, collapsible)",
      |a, w, cx| a.set_dock_collapsible(at, value, w, cx),
    )
  }

  pub fn is_locked(&self, ctx: Ctx<'_>) -> Result<bool> {
    self.read(&ctx, DockArea::is_locked)
  }

  pub fn set_locked(&self, ctx: Ctx<'_>, locked: bool) -> Result<()> {
    self.edit(&ctx, "set_locked(locked)", |a, w, cx| {
      a.set_locked(locked, w, cx)
    })
  }

  pub fn is_zoomed(&self, ctx: Ctx<'_>) -> Result<bool> {
    self.read(&ctx, DockArea::is_zoomed)
  }

  pub fn zoom_out(&self, ctx: Ctx<'_>) -> Result<()> {
    self.edit(&ctx, "zoom_out()", |a, w, cx| a.set_zoomed_out(w, cx))
  }

  /// `on("layout_changed", cx => ..)` after every edit.
  pub fn on<'js>(&mut self, ctx: Ctx<'js>, name: String, f: Function<'js>) -> Result<bool> {
    if name != "layout_changed" {
      return Err(Exception::throw_type(
        &ctx,
        &format!("unknown dock event \"{name}\"; the only one is \"layout_changed\""),
      ));
    }
    let view = current_view();
    (self.state).on(
      &ctx,
      &["layout_changed"],
      name,
      f,
      move |e: &DockEvent, _| {
        let cx = ViewCx::for_view(view.clone()?);
        matches!(e, DockEvent::LayoutChanged).then_some(("layout_changed", cx))
      },
    )
  }

  pub fn release(&mut self) -> bool {
    self.state.release()
  }
}

// ---------------------------------------------------------------------------------------------
// Elements

/// `dock_area(area)`: the area, sized to fill by default, and its chrome handlers
struct AreaEl {
  frame: Div,
  area: Entity<DockArea>,
  skin: Rc<Skin>,
  hooks: Hooks,
}

impl Node for AreaEl {
  fn kind(&self) -> &'static str {
    "dock_area"
  }
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    Some(Styled::style(&mut self.frame))
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    // the frame about to be drawn records its own contexts
    self.skin.groups.borrow_mut().clear();
    self.skin.docks.borrow_mut().clear();
    *self.skin.hooks.borrow_mut() = self.hooks;
    self.frame.child(self.area).into_any_element()
  }
  fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
    self
  }
}

/// `dock_content()`: the dock's own panels, inside the chrome of its handler
struct ContentEl {
  frame: Div,
  content: Option<AnyElement>,
}

impl Node for ContentEl {
  fn kind(&self) -> &'static str {
    "dock_content"
  }
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    Some(Styled::style(&mut self.frame))
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    match self.content {
      Some(content) => self.frame.child(content).into_any_element(),
      None => Empty.into_any_element(),
    }
  }
  fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
    self
  }
}

fn dock_area_el<'js>(ctx: Ctx<'js>, area: Class<'js, JsDockArea>) -> Result<Class<'js, El>> {
  let (entity, skin) = {
    let area = area.borrow();
    (area.area(&ctx)?, area.skin.clone())
  };
  let view = frame_or_throw(&ctx)?.view.clone();
  let el = AreaEl {
    frame: div().size_full(),
    area: entity,
    skin,
    hooks: Hooks {
      view: Some(view),
      ..Default::default()
    },
  };
  new_el(&ctx, el)
}

fn dock_content(ctx: Ctx<'_>) -> Result<Class<'_, El>> {
  let content = DOCK_CONTENT.with(|c| c.borrow_mut().take());
  if content.is_none() {
    tracing::warn!("dock_content() outside a dock's chrome handler, or twice inside one");
  }
  new_el(
    &ctx,
    ContentEl {
      frame: div(),
      content,
    },
  )
}

/// A chrome handler argument: `(state, cx) => Element | null`
pub struct Chrome<'js>(Function<'js>);

impl<'js> FromJs<'js> for Chrome<'js> {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    Function::from_js(ctx, value).map(Chrome)
  }
}

macro_rules! chrome_ts {
  ($($ty:ident => $ts:literal),*) => {$(
    pub struct $ty<'js>(Chrome<'js>);
    impl<'js> FromJs<'js> for $ty<'js> {
      fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        Chrome::from_js(ctx, value).map($ty)
      }
    }
    impl crate::TsType for $ty<'_> {
      fn ts() -> String {
        $ts.into()
      }
    }
  )*};
}

chrome_ts! {
  GroupChrome => "(group: import(\"gpui-base\").DockGroup, cx: Context) => Element",
  EmptyChrome => "(group: import(\"gpui-base\").DockGroup, cx: Context) => Element | null",
  DropChrome => "(drop: import(\"gpui-base\").DockDrop, cx: Context) => Element | null",
  DockChrome => "(dock: import(\"gpui-base\").DockRegion, cx: Context) => Element | null"
}

/// Keeps a chrome handler as a view slot for this render
fn hook(ctx: &Ctx<'_>, f: Function<'_>) -> Result<u64> {
  let view = frame_or_throw(ctx)?.view.entity_id();
  Ok(runtime(ctx)?.slot(view, f, true))
}

/// A group or a dock a chrome handler was handed, as a command names it
pub struct Target {
  area: u64,
  node: u64,
  placement: Option<String>,
}

impl<'js> FromJs<'js> for Target {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    let o = Object::from_js(ctx, value)
      .map_err(|_| Exception::throw_type(ctx, "expected the group or dock a chrome handler got"))?;
    Ok(Self {
      area: o.get::<_, Option<f64>>("area")?.unwrap_or_default() as u64,
      node: o.get::<_, Option<f64>>("node")?.unwrap_or_default() as u64,
      placement: o.get("placement")?,
    })
  }
}

impl crate::TsType for Target {
  fn ts() -> String {
    "import(\"gpui-base\").DockGroup".into()
  }
}

/// A dock, as `toggle_dock`/`resize_dock` take it
pub struct DockTarget(Target);

impl<'js> FromJs<'js> for DockTarget {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    Target::from_js(ctx, value).map(DockTarget)
  }
}

impl crate::TsType for DockTarget {
  fn ts() -> String {
    "import(\"gpui-base\").DockRegion".into()
  }
}

/// Drags a dock's edge: GPUI's drag value, painting nothing
#[derive(Clone, Copy)]
struct ResizingDock(DockPlacement);

impl Render for ResizingDock {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    Empty
  }
}

fn group_of(t: &Target) -> Option<TabGroupContext> {
  skin(t.area)?.groups.borrow().get(&t.node).cloned()
}

fn dock_of(ctx: &Ctx<'_>, t: &DockTarget) -> Result<(u64, DockPlacement)> {
  let at = t.0.placement.as_deref().unwrap_or_default();
  Ok((t.0.area, placement(ctx, at)?))
}

fn find_dock(area: u64, at: DockPlacement) -> Option<DockContext> {
  skin(area)?.docks.borrow().get(&at).cloned()
}

mod chrome {
  use super::*;

  crate::el_methods! {
    kinds: &["dock_area"];
    /// The tab bar above a group's displayed panel.
    fn tab_bar(handler: GroupChrome<'js>) |c, n| {
      let key = hook(c, handler.0.0)?;
      crate::map(c, n, |mut a: AreaEl| {
        a.hooks.tab_bar = Some(key);
        a
      })
    };
    /// What a group with no displayed panel shows.
    fn empty_group(handler: EmptyChrome<'js>) |c, n| {
      let key = hook(c, handler.0.0)?;
      crate::map(c, n, |mut a: AreaEl| {
        a.hooks.empty_group = Some(key);
        a
      })
    };
    /// The hint where a dragged panel would land.
    fn drop_indicator(handler: DropChrome<'js>) |c, n| {
      let key = hook(c, handler.0.0)?;
      crate::map(c, n, |mut a: AreaEl| {
        a.hooks.drop_indicator = Some(key);
        a
      })
    };
    /// One dock's chrome around its content; put `dock_content()` where its panels go.
    fn dock(handler: DockChrome<'js>) |c, n| {
      let key = hook(c, handler.0.0)?;
      crate::map(c, n, |mut a: AreaEl| {
        a.hooks.dock = Some(key);
        a
      })
    };
  }
}

/// The dock commands: what an element of a dock's chrome does when used
mod commands {
  use super::*;

  crate::el_methods! {
    /// Displays tab `index` of `group` on a click.
    fn select_tab(group: Target, index: u32) |c, n| interact(c, n, move |mut i| {
      i.0.on_click(move |_, w, cx| {
        if let Some(g) = group_of(&group) {
          g.select_tab(index as usize, w, cx);
        }
      });
      i
    });
    /// Closes `panel` (an id) of `group` on a click, if the group allows it.
    fn close_panel(group: Target, panel: f64) |c, n| interact(c, n, move |mut i| {
      i.0.on_click(move |_, w, cx| {
        if let Some(g) = group_of(&group) {
          g.close(PanelId::from_u64(panel as u64), w, cx);
        }
      });
      i
    });
    /// Zooms `group` in, or back out, on a click.
    fn toggle_zoom(group: Target) |c, n| interact(c, n, move |mut i| {
      i.0.on_click(move |_, w, cx| {
        if let Some(g) = group_of(&group) {
          g.toggle_zoom(w, cx);
        }
      });
      i
    });
    /// Makes this element the drag source of tab `index` of `group`.
    fn drag_tab(group: Target, index: u32) |c, n| {
      let drag = group_of(&group).and_then(|g| g.drag_panel(index as usize, app()));
      let Some(drag) = drag else {
        tracing::warn!("drag_tab(group, {index}) names no panel; the layout changed");
        return Ok(());
      };
      interact(c, n, move |mut i| {
        i.0.on_drag(drag, |drag: &DragPanel, _, _, cx| {
          cx.stop_propagation();
          cx.new(|_| drag.clone())
        });
        i
      })
    };
    /// Accepts a dragged panel into `group`, at slot `index` or appended.
    fn drop_tab(group: Target, index: Opt<u32>) |c, n| interact(c, n, move |mut i| {
      i.0.on_drop(move |drag: &DragPanel, w, cx| {
        if let Some(g) = group_of(&group) {
          g.drop_panel(drag.clone(), index.0.map(|i| i as usize), true, w, cx);
        }
      });
      i
    });
    /// Opens or closes `dock` on a click.
    fn toggle_dock(dock: DockTarget) |c, n| {
      let (area, at) = dock_of(c, &dock)?;
      interact(c, n, move |mut i| {
        i.0.on_click(move |_, w, cx| {
          if let Some(d) = find_dock(area, at) {
            d.toggle(w, cx);
          }
        });
        i
      })
    };
    /// Drags `dock`'s edge.
    fn resize_dock(dock: DockTarget) |c, n| {
      let (area, at) = dock_of(c, &dock)?;
      interact(c, n, move |mut i| {
        i.0.on_drag(ResizingDock(at), |drag: &ResizingDock, _, _, cx| {
          cx.stop_propagation();
          cx.new(|_| *drag)
        });
        i.0.on_drag_move(move |e: &DragMoveEvent<ResizingDock>, w, cx| {
          if e.drag(cx).0 == at
            && let Some(d) = find_dock(area, at)
          {
            d.resize_to(e.event.position, w, cx);
          }
        });
        i
      })
    };
  }
}

fn install(proto: &Object<'_>) -> Result<()> {
  chrome::install(proto)?;
  commands::install(proto)
}

fn el_d_ts() -> String {
  chrome::d_ts() + &commands::d_ts()
}

fn exports<'js>(ctx: &Ctx<'js>) -> Result<Vec<(&'static str, Value<'js>)>> {
  let area = Object::new(ctx.clone())?;
  area.set("new", Function::new(ctx.clone(), dock_area_new)?)?;
  area.set("register_panel", Function::new(ctx.clone(), register)?)?;
  Ok(vec![
    ("DockArea", area.into_value()),
    (
      "dock_area",
      Function::new(ctx.clone(), dock_area_el)?.into_value(),
    ),
    ctor(ctx, "dock_content", dock_content)?,
  ])
}

const D_TS: &str = r#"  /** Where a region sits relative to the center of a dock area. */
  export type DockPlacement = "center" | "left" | "right" | "bottom";
  /** One panel, as `panels()` reports it. */
  export interface DockPanel {
    readonly id: number;
    /** Namespaced: `shell:<plugin>/<name>`. */
    readonly name: string;
    readonly placement: DockPlacement;
    readonly node: number;
    readonly index: number;
    readonly active: boolean;
    readonly visible: boolean;
    readonly closable: boolean;
    readonly zoomable: boolean;
  }
  /** One tab of a group, as a chrome handler gets it. */
  export interface DockTab {
    readonly index: number;
    readonly name: string;
    readonly id: number;
    readonly active: boolean;
    readonly visible: boolean;
    readonly closable: boolean;
    readonly zoomable: boolean;
  }
  /** A tab group, as `tab_bar` and `empty_group` get it. */
  export interface DockGroup {
    readonly area: number;
    readonly node: number;
    readonly active_index: number;
    readonly zoomed: boolean;
    readonly collapsed: boolean;
    readonly locked: boolean;
    readonly draggable: boolean;
    readonly droppable: boolean;
    readonly closable: boolean;
    readonly tabs: readonly DockTab[];
  }
  /** One dock, as the `dock` handler gets it. */
  export interface DockRegion {
    readonly area: number;
    readonly placement: DockPlacement;
    readonly size: number;
    readonly open: boolean;
    readonly collapsible: boolean;
  }
  export interface DockRect { readonly x: number; readonly y: number; readonly width: number; readonly height: number }
  /** Where a dragged panel would land; `placement` null merges into the group's tabs. */
  export interface DockDrop {
    readonly placement: "left" | "right" | "top" | "bottom" | null;
    readonly bounds: DockRect;
    readonly from: DockRect;
    readonly to: DockRect;
  }
  export interface DockPanelOptions {
    /** What a saved layout files it under, and `register_panel` rebuilds it by. */
    name: string;
    placement?: DockPlacement;
    size?: number;
    closable?: boolean;
    zoomable?: boolean;
    visible?: boolean;
  }
  /** A dockable layout the user rearranges. Make it in `init` or a handler. */
  export interface DockArea {
    /** Docks `view`, a view from `cx.new(Class)`. */
    add_panel(view: import("gpui-kit").Entity, options: DockPanelOptions): void;
    remove_panel(id: number): boolean;
    panels(): DockPanel[];
    /** The layout as plain data, with each panel's `serialize()`. */
    dump(): any;
    /** Restores a `dump()`, rebuilding registered panels and calling their `deserialize(data)`. */
    load(state: any): void;
    has_dock(placement: DockPlacement): boolean;
    is_dock_open(placement: DockPlacement): boolean;
    toggle_dock(placement: DockPlacement): void;
    remove_dock(placement: DockPlacement): void;
    dock_size(placement: DockPlacement): number | null;
    set_dock_size(placement: DockPlacement, size: number): void;
    set_dock_collapsible(placement: DockPlacement, collapsible: boolean): void;
    is_locked(): boolean;
    set_locked(locked: boolean): void;
    is_zoomed(): boolean;
    zoom_out(): void;
    on(event: "layout_changed", handler: (cx: Context) => void): boolean;
    release(): boolean;
  }
  export const DockArea: {
    new: (id: string, options?: { version?: number }) => DockArea;
    /** Rebuilds `name`'s panel from `Class` when a saved layout names it; answers the qualified name. */
    register_panel: (name: string, Class: import("gpui-kit").ViewClass) => string;
  };
  /** Draws a dock area; its chrome comes from `tab_bar`, `empty_group`, `drop_indicator`, `dock`. */
  export function dock_area(area: DockArea): Element;
"#;

pub(crate) const FAMILY: Family = Family {
  module: "gpui-base",
  names: &["DockArea", "dock_area", "dock_content"],
  exports,
  methods: install,
  d_ts: || {
    D_TS.to_string()
      + &declare(
        "Where a dock's panels go inside its chrome handler.",
        "dock_content",
        "",
      )
  },
  el_d_ts,
};

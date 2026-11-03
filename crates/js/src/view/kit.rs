//! The built-in modules: `gpui-kit` (views, elements, `window`), `gpui-base` (layout helpers,
//! theme) and `gpui-component`, plus the component families of `components/`.

use std::{path::PathBuf, sync::Arc};

use gpui_kit::{Image, ImageFormat, IntoElement, base::StyledExt, div, img, svg, uniform_list};
use rquickjs::{
  Class, Ctx, Exception, FromJs, Function, Object, Result, TypedArray, Value,
  function::Args,
  loader::ModuleLoader,
  module::{Declarations, Exports, ModuleDef},
};

use crate::{
  Access, Child, ScrollHandle,
  element::{
    background::{backgrounds, path_builders},
    components::FAMILIES,
    el_d_ts, events, frame_or_throw, new_el,
    style::{COLOR_TOKENS, RADIUS_TOKENS, SPACING_TOKENS, nullary_d_ts},
  },
  runtime::{plugin::plugin, runtime},
  view::{theme, window::ScriptWindow},
};

/// The built-in module names
pub(crate) const NAMES: &[&str] = &[
  GpuiKit::NAME,
  GpuiBase::NAME,
  GpuiComponent::NAME,
  GpuiFps::NAME,
];

/// Adds the built-in modules to `loader`.
pub(crate) fn loader(loader: ModuleLoader) -> ModuleLoader {
  loader
    .with_module(GpuiKit::NAME, GpuiKit)
    .with_module(GpuiBase::NAME, GpuiBase)
    .with_module(GpuiComponent::NAME, GpuiComponent)
    .with_module(GpuiFps::NAME, GpuiFps)
}

/// Sets a context up for views: element methods and the `window` global.
pub(crate) fn install(ctx: &Ctx<'_>) -> Result<()> {
  crate::element::install(ctx)?;
  ctx.globals().set(
    "window",
    Class::instance(ctx.clone(), ScriptWindow::default())?,
  )
}

/// Declares and evaluates one module: its own exports and its families'
fn declare(decl: &Declarations<'_>, module: &str, own: &[&'static str]) -> Result<()> {
  let family = FAMILIES.iter().filter(|f| f.module == module);
  for name in own.iter().chain(family.flat_map(|f| f.names)) {
    decl.declare(*name)?;
  }
  Ok(())
}

fn evaluate<'js>(
  ctx: &Ctx<'js>,
  exports: &Exports<'js>,
  module: &str,
  own: Vec<(&'static str, Value<'js>)>,
) -> Result<()> {
  for (name, value) in own {
    exports.export(name, value)?;
  }
  for family in FAMILIES.iter().filter(|f| f.module == module) {
    for (name, value) in (family.exports)(ctx)? {
      exports.export(name, value)?;
    }
  }
  Ok(())
}

fn func<'js, P, F: rquickjs::function::IntoJsFunc<'js, P> + 'js>(
  ctx: &Ctx<'js>,
  f: F,
) -> Result<Value<'js>> {
  Ok(Function::new(ctx.clone(), f)?.into_value())
}

/// `svg(path)`: an absolute path, or one of the app's assets
fn svg_el<'js>(ctx: Ctx<'js>, path: String) -> Result<Class<'js, crate::El>> {
  let el = match path.starts_with('/') {
    true => svg().external_path(path),
    false => svg().path(path),
  };
  new_el(&ctx, el)
}

/// The format of image bytes, by their magic number
fn image_format(bytes: &[u8]) -> Option<ImageFormat> {
  Some(match bytes {
    [0x89, b'P', b'N', b'G', ..] => ImageFormat::Png,
    [0xff, 0xd8, 0xff, ..] => ImageFormat::Jpeg,
    [b'G', b'I', b'F', b'8', ..] => ImageFormat::Gif,
    [
      b'R',
      b'I',
      b'F',
      b'F',
      _,
      _,
      _,
      _,
      b'W',
      b'E',
      b'B',
      b'P',
      ..,
    ] => ImageFormat::Webp,
    [b'B', b'M', ..] => ImageFormat::Bmp,
    [b'I', b'I', 0x2a, 0, ..] | [b'M', b'M', 0, 0x2a, ..] => ImageFormat::Tiff,
    [0, 0, 1, 0, ..] => ImageFormat::Ico,
    _ => return None,
  })
}

/// `image(source)`: a file path under a read grant, or image bytes.
fn image_el<'js>(ctx: Ctx<'js>, source: Value<'js>) -> Result<Class<'js, crate::El>> {
  if let Some(path) = source.as_string() {
    let path = path.to_string()?;
    if path.contains("://") {
      return Err(Exception::throw_type(
        &ctx,
        "image() takes no URLs: fetch the bytes instead",
      ));
    }
    let path = PathBuf::from(path);
    if let Err(e) = plugin(&ctx)?.capabilities.open(&path, Access::Read) {
      return Err(Exception::throw_type(&ctx, &format!("image(): {e}")));
    }
    return new_el(&ctx, img(path));
  }
  let bytes = TypedArray::<u8>::from_js(&ctx, source)
    .map_err(|_| Exception::throw_type(&ctx, "image() takes a path or a Uint8Array"))?;
  // SAFETY: copied before any JS can detach or resize the buffer
  let bytes: Vec<u8> = unsafe { bytes.as_bytes() }.unwrap_or_default().to_vec();
  let format = image_format(&bytes).ok_or_else(|| {
    Exception::throw_type(
      &ctx,
      "image(): not a PNG, JPEG, WebP, GIF, BMP, TIFF or ICO",
    )
  })?;
  new_el(&ctx, img(Arc::new(Image::from_bytes(format, bytes))))
}

/// `list(id, count, get_key, render(index, cx))`. ponytail: `get_key` is unused until
/// `on_item_click` exists
fn list_el<'js>(
  ctx: Ctx<'js>,
  id: String,
  count: u32,
  _get_key: Function<'js>,
  render: Function<'js>,
) -> Result<Class<'js, crate::El>> {
  let view = frame_or_throw(&ctx)?.view.clone();
  let rt = runtime(&ctx)?;
  let state = rt.list(view.entity_id(), id, count as usize);
  let key = rt.slot(view.entity_id(), render, true);
  let el = gpui_kit::list(state, move |index, window, cx| {
    let Some(view) = view.upgrade() else {
      return div().into_any_element();
    };
    view
      .update(cx, |view, cx| {
        let args = |a: &mut Args<'_>| a.push_arg(index);
        view.render_content(key, Some(index), window, cx, args, |ctx, frame, value| {
          frame.child(ctx, Child::from_js(ctx, value)?)
        })
      })
      .unwrap_or_else(|e| format!("render failed: {e}").into_any_element())
  });
  new_el(&ctx, el)
}

/// `uniform_list(id, count, get_key, render(range, cx) => Element[])`
fn uniform_list_el<'js>(
  ctx: Ctx<'js>,
  id: String,
  count: u32,
  _get_key: Function<'js>,
  render: Function<'js>,
) -> Result<Class<'js, crate::El>> {
  let view = frame_or_throw(&ctx)?.view.clone();
  let key = runtime(&ctx)?.slot(view.entity_id(), render, true);
  let el = uniform_list(id, count as usize, move |range, window, cx| {
    let Some(view) = view.upgrade() else {
      return Vec::new();
    };
    view
      .update(cx, |view, cx| {
        let args = |a: &mut Args<'_>| {
          let r = Object::new(a.ctx().clone())?;
          r.set("start", range.start)?;
          r.set("end", range.end)?;
          a.push_arg(r)
        };
        view.render_content(key, None, window, cx, args, |ctx, frame, value| {
          let items: Vec<Child> = Vec::from_js(ctx, value)?;
          items.into_iter().map(|c| frame.child(ctx, c)).collect()
        })
      })
      .unwrap_or_else(|e| {
        tracing::warn!("uniform_list render failed: {e}");
        Vec::new()
      })
  });
  new_el(&ctx, el)
}

/// The `gpui-kit` module: views, elements, geometry
pub struct GpuiKit;

impl GpuiKit {
  pub const NAME: &str = "gpui-kit";
  const OWN: &[&str] = &[
    "View",
    "div",
    "svg",
    "image",
    "list",
    "uniform_list",
    "PathBuilder",
    "Background",
    "ScrollHandle",
  ];

  /// The TypeScript of every built-in module
  pub fn d_ts() -> String {
    let union = |names: &[&str]| -> String {
      names
        .iter()
        .map(|n| format!("\"{n}\""))
        .collect::<Vec<_>>()
        .join(" | ")
    };
    let families = |module: &str| -> String {
      FAMILIES
        .iter()
        .filter(|f| f.module == module)
        .map(|f| (f.d_ts)())
        .collect()
    };
    format!(
      r##"declare module "gpui-kit" {{
  export type SpacingToken = {spacing};
  export type RadiusToken = {radius};
  export type ColorToken = {colors};
  /** A theme token, `"#rrggbb"`, `"#rrggbbaa"` or `0xrrggbb`. */
  export type Color = ColorToken | `#${{string}}` | number;
  /** A bare number is pixels; a string carries its unit, or is a spacing token. */
  export type Length = number | `${{number}}px` | `${{number}}rem` | `${{number}}%` | "auto" | SpacingToken;
  export type DefiniteLength = number | `${{number}}px` | `${{number}}rem` | `${{number}}%` | SpacingToken;
  export type AbsoluteLength = number | `${{number}}px` | `${{number}}rem`;
  export type Props = Record<string, any>;
{events}{nullary}  export type NullaryStyles = {{ [Method in NullaryStyle]: () => Element }};
  /** An element of the render that built it: chain its methods, pass it to `child`. */
  export interface Element extends NullaryStyles {{
{el}  }}
  export type NativeElement = Element;
{rest}}}
declare var window: import("gpui-kit").Window;
declare module "gpui-base" {{
  import {{ ClickEvent, Context, Element, FocusHandle }} from "gpui-kit";
  /** A row, centered on the cross axis. */
  export function h_flex(): Element;
  /** A column. */
  export function v_flex(): Element;
{theme}{base}}}
declare module "gpui-component" {{
  import {{ ClickEvent, Context, Element, FocusHandle }} from "gpui-kit";
{component}}}
declare module "gpui-fps" {{
{fps}}}
"##,
      spacing = union(SPACING_TOKENS),
      radius = union(RADIUS_TOKENS),
      colors = union(COLOR_TOKENS),
      events = events::D_TS,
      nullary = nullary_d_ts(),
      el = el_d_ts(),
      rest = KIT_D_TS,
      theme = theme::d_ts(),
      base = families(GpuiBase::NAME),
      component = families(GpuiComponent::NAME),
      fps = families(GpuiFps::NAME),
    )
  }
}

impl ModuleDef for GpuiKit {
  fn declare<'js>(decl: &Declarations<'js>) -> Result<()> {
    declare(decl, Self::NAME, Self::OWN)
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
    // an empty base class, so `extends View` works without native subclassing
    let view = ctx.eval::<Value, _>("(class View { constructor(_props) {} })")?;
    let scroll = Class::<ScrollHandle>::create_constructor(ctx)?.expect("constructor");
    let own = vec![
      ("View", view),
      ("div", func(ctx, |ctx: Ctx<'js>| new_el(&ctx, div()))?),
      ("svg", func(ctx, svg_el)?),
      ("image", func(ctx, image_el)?),
      ("list", func(ctx, list_el)?),
      ("uniform_list", func(ctx, uniform_list_el)?),
      ("PathBuilder", path_builders(ctx)?.into_value()),
      ("Background", backgrounds(ctx)?.into_value()),
      ("ScrollHandle", scroll.into_value()),
    ];
    evaluate(ctx, exports, Self::NAME, own)
  }
}

/// The `gpui-base` module: layout helpers and the theme
pub struct GpuiBase;

impl GpuiBase {
  pub const NAME: &str = "gpui-base";
}

impl ModuleDef for GpuiBase {
  fn declare<'js>(decl: &Declarations<'js>) -> Result<()> {
    declare(decl, Self::NAME, &["h_flex", "v_flex", "set_theme"])
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
    let own = vec![
      (
        "h_flex",
        func(ctx, |ctx: Ctx<'js>| new_el(&ctx, div().h_flex()))?,
      ),
      (
        "v_flex",
        func(ctx, |ctx: Ctx<'js>| new_el(&ctx, div().v_flex()))?,
      ),
      ("set_theme", func(ctx, theme::set_theme)?),
    ];
    evaluate(ctx, exports, Self::NAME, own)
  }
}

/// The `gpui-component` module: the component families
pub struct GpuiComponent;

impl GpuiComponent {
  pub const NAME: &str = "gpui-component";
}

impl ModuleDef for GpuiComponent {
  fn declare<'js>(decl: &Declarations<'js>) -> Result<()> {
    declare(decl, Self::NAME, &[])
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
    evaluate(ctx, exports, Self::NAME, Vec::new())
  }
}

/// The `gpui-fps` module: the performance HUD
pub struct GpuiFps;

impl GpuiFps {
  pub const NAME: &str = "gpui-fps";
}

impl ModuleDef for GpuiFps {
  fn declare<'js>(decl: &Declarations<'js>) -> Result<()> {
    declare(decl, Self::NAME, &[])
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
    evaluate(ctx, exports, Self::NAME, Vec::new())
  }
}

/// The hand-written rest of `gpui-kit`
const KIT_D_TS: &str = r#"  /** A retained child view from `cx.new`; a child wherever a child is taken. */
  export interface Entity {
    /** Runs the child's `update(props)` and renders it again. */
    set_props(props?: Props): void;
    release(): boolean;
  }
  /** A focus target the script keeps; give it to an element with `track_focus`. */
  export interface FocusHandle {
    focus(): void;
    is_focused(): boolean;
    release(): boolean;
  }
  /** A scroll position: `track_scroll(handle)` on the scrolled element. */
  export class ScrollHandle {
    constructor();
    static new(): ScrollHandle;
    /** How far it is scrolled, negative downwards and rightwards. */
    offset(): Point;
    max_offset(): Point;
    set_offset(x: number, y: number): void;
    scroll_to_item(index: number): void;
    scroll_to_bottom(): void;
    top_item(): number;
  }
  export interface TimerHandle {
    cancel(): void;
  }
  export interface Timer {
    /** Calls `cb(cx)` once after `ms`. */
    after(ms: number, cb: (cx: Context) => void): TimerHandle;
    /** Calls `cb(cx)` every `ms`. */
    every(ms: number, cb: (cx: Context) => void): TimerHandle;
  }
  /** One entry of `cx.bind_keys`. */
  export interface KeyBinding {
    /** The chord, e.g. `"cmd-s"`, or a sequence: `"ctrl-k ctrl-s"`. */
    keystroke: string;
    /** The action it dispatches, handled by `on_action`. */
    action: string;
    /** A key-context predicate matched against `key_context`; global when left out. */
    context?: string;
  }
  /** What a view's methods, handlers, timers and tasks get. */
  export interface Context {
    /** Renders the view again, or the retained child `target`. Throws in `render`. */
    notify(target?: Entity): void;
    /** Installs key bindings, all or none; returns how many. */
    bind_keys(bindings: KeyBinding[]): number;
    /** Stops this event reaching the handlers further out. */
    stop_propagation(): void;
    /** Undoes `stop_propagation()` in the same dispatch. */
    propagate(): void;
    phase(): "render" | "event" | "task";
    /** The theme's semantic tokens. */
    theme(): import("gpui-base").Theme;
    /** Opens an absolute http(s) URL with the system's handler. */
    open_url(url: string): void;
    read_from_clipboard(): string | undefined;
    write_to_clipboard(text: string): void;
    /** A focus target, made once in `init` or a handler, never in `render`. */
    focus_handle(): FocusHandle;
    /** Creates a retained child view; not in `render`. */
    new(Class: ViewClass, props?: Props): Entity;
    /** Calls `body(cx)` and returns what it returns, the promise of an async body. */
    spawn<T>(body: (cx: Context) => T): T;
    /** Resolves after `ms`. */
    sleep(ms?: number): Promise<void>;
    readonly timer: Timer;
  }
  export type AsyncContext = Context;
  /**
   * The base class of every view: default-export a subclass. `init` runs once, `render`
   * whenever the view is notified; never keep an element on the instance.
   */
  export abstract class View {
    constructor(props?: Props);
    init?(props: Props | undefined, cx: Context): void;
    /** Runs when the parent hands new props through `set_props`. */
    update?(props: Props | undefined): void;
    abstract render(cx: Context): Element | Entity | string;
  }
  export type ViewClass = new (props?: Props) => View;
  export function div(): Element;
  /** A vector icon tinted with the text color: an absolute path or an asset. */
  export function svg(path: string): Element;
  /** A full-color image: a file path or PNG, JPEG, WebP, GIF, BMP, TIFF or ICO bytes. */
  export function image(source: string | Uint8Array): Element;
  /** The visible items, `[start, end)`. */
  export interface ItemRange {
    start: number;
    end: number;
  }
  /** gpui's lazy list of rows of any height; size it, e.g. `size_full()`. */
  export function list(
    id: string,
    item_count: number,
    get_key: (index: number) => string,
    render: (index: number, cx: Context) => Element,
  ): Element;
  /** gpui's list of rows of one height, rendered a range at a time. */
  export function uniform_list(
    id: string,
    item_count: number,
    get_key: (index: number) => string,
    render: (range: ItemRange, cx: Context) => Element[],
  ): Element;
  /** Geometry from `PathBuilder.build()`, for `window.paint_path`. */
  export interface Path {}
  export interface PathBuilder {
    move_to(x: number, y: number): PathBuilder;
    line_to(x: number, y: number): PathBuilder;
    curve_to(to_x: number, to_y: number, control_x: number, control_y: number): PathBuilder;
    cubic_bezier_to(to_x: number, to_y: number, control_a_x: number, control_a_y: number, control_b_x: number, control_b_y: number): PathBuilder;
    arc_to(radius_x: number, radius_y: number, rotation: number, large_arc: boolean, sweep: boolean, to_x: number, to_y: number): PathBuilder;
    add_polygon(points: ReadonlyArray<readonly [number, number]>, closed?: boolean): PathBuilder;
    close(): PathBuilder;
    dash_array(values: readonly number[]): PathBuilder;
    build(): Path;
  }
  export const PathBuilder: {
    fill(): PathBuilder;
    stroke(width: number): PathBuilder;
  };
  export interface BackgroundStop {}
  export interface Background {
    opacity(factor: number): Background;
    color_space(space: "srgb" | "oklab"): Background;
  }
  export const Background: {
    solid(color: Color): Background;
    stop(color: Color, percentage: number): BackgroundStop;
    linear_gradient(angle: number, from: Color | BackgroundStop, to: Color | BackgroundStop): Background;
    pattern_slash(color: Color, width: number, interval: number): Background;
    checkerboard(color: Color, size: number): Background;
  };
  export interface DialogOptions {
    escape_dismissable?: boolean;
    backdrop_dismissable?: boolean;
  }
  export interface ToastOptions {
    title: string;
    description?: string;
    level?: "info" | "success" | "warning" | "error";
    /** `null` keeps it up until removed. */
    timeout?: number | null;
    id?: string;
  }
  /** The window the view draws into, as the `window` global. Overlays open from handlers. */
  export interface Window {
    /** Opens a dialog with what `content()` renders, again on every redraw. */
    open_dialog(content: () => Element, options?: DialogOptions): void;
    close_dialog(): void;
    close_all_dialogs(): void;
    has_active_dialog(): boolean;
    /** Opens the sheet on the right. */
    open_sheet(content: () => Element): void;
    open_sheet_at(placement: "top" | "bottom" | "left" | "right", content: () => Element): void;
    close_sheet(): void;
    has_active_sheet(): boolean;
    /** Posts a toast; returns its id. */
    push_toast(options: ToastOptions): string;
    remove_toast(id: string): void;
    clear_toasts(): void;
    /** Dispatches an action down the focus path, as its key binding would. */
    dispatch_action(action: string): void;
    rem_size(): number;
    line_height(): number;
    viewport_size(): Size;
    bounds(): Bounds;
    mouse_position(): Point;
    appearance(): "light" | "dark";
    is_window_active(): boolean;
    is_fullscreen(): boolean;
    is_maximized(): boolean;
    set_rem_size(size: number): void;
    /** Redraws every view of the window; `cx.notify()` is almost always what you want. */
    refresh(): void;
    focus_next(): void;
    focus_prev(): void;
    activate_window(): void;
    minimize_window(): void;
    zoom_window(): void;
    toggle_fullscreen(): void;
    /** An element painting `path` with `fill`, in its own coordinates. */
    paint_path(path: Path, fill: Color | Background): Element;
  }
"#;

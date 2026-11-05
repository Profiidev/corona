//! Overlays: surfaces anchored to a trigger (`Popover`, `HoverCard`, `Popup`), window effects a
//! button opens (`Dialog`, `AlertDialog`, `Sheet`, `Notification`) and the inline `Alert`.
//! gpui-base's unstyled surfaces are exported from `"corona/base"` ([`BASE`]), gpui-component's
//! from `"corona/component"` ([`FAMILY`]).
//!
//! A surface's `content(element)` belongs to the render that built it. A window effect outlives
//! that render, so its options sit in a JS object its button reads when clicked; from then on
//! its content fn and callbacks are kept as view slots, like those of `window.open_dialog`.

// an overlay moves through each builder method once, by value, as gpui's builders do
#![allow(clippy::large_enum_variant, clippy::result_large_err)]

use std::{any::Any, cell::Cell, rc::Rc, time::Duration};

use gpui_kit::{
  Anchor, AnyElement, App, ClickEvent, InteractiveElement, Interactivity, IntoElement, MouseButton,
  ParentElement, SharedString, StyleRefinement, Styled, WeakEntity, Window,
  base::{self, Placement},
  component::{
    Sizable, WindowExt,
    alert::Alert,
    button::{Button, ButtonVariants},
    hover_card::HoverCard,
    notification::{Notification, NotificationType},
    popover::Popover,
  },
  div,
};
use rquickjs::{
  Class, Ctx, Exception, FromJs, Function, IntoJs, Object, Result, Value, function::This,
};

use super::{Family, SizeArg, ctor, declare};
use crate::{
  Child, El, Id, Node, On, Opt, ScriptView, TsType, frame, frame_or_throw, new_el,
  runtime::{SlotGuard, runtime},
  view::{current_view, window_or_throw},
  with_node,
};

/// Every overlay element; one type, so methods several kinds share apply to each
enum Overlay {
  Popover(base::Popover),
  HoverCard(base::HoverCard),
  Popup(base::Popup),
  CPopover(Popover),
  CHoverCard(HoverCard),
  Effect(Effect),
  Alert(Alert),
}

/// A window effect's button; `spec` indexes the frame handler that opens it
struct Effect {
  kind: &'static str,
  button: Button,
  spec: usize,
}

const KINDS: &[&str] = &[
  "Popover",
  "HoverCard",
  "Popup",
  "Dialog",
  "AlertDialog",
  "Sheet",
  "Notification",
  "Alert",
];
const EFFECTS: &[&str] = &["Dialog", "AlertDialog", "Sheet", "Notification"];

impl Node for Overlay {
  fn kind(&self) -> &'static str {
    match self {
      Self::Popover(_) | Self::CPopover(_) => "Popover",
      Self::HoverCard(_) | Self::CHoverCard(_) => "HoverCard",
      Self::Popup(_) => "Popup",
      Self::Effect(e) => e.kind,
      Self::Alert(_) => "Alert",
    }
  }
  fn style(&mut self) -> Option<&mut StyleRefinement> {
    Some(match self {
      Self::Popover(p) => p.style(),
      Self::Popup(p) => p.style(),
      Self::CPopover(p) => p.style(),
      Self::CHoverCard(h) => h.style(),
      Self::Effect(e) => Styled::style(&mut e.button),
      Self::Alert(a) => a.style(),
      Self::HoverCard(_) => return None,
    })
  }
  fn interactivity(&mut self) -> Option<&mut Interactivity> {
    match self {
      Self::Popup(p) => Some(p.interactivity()),
      _ => None,
    }
  }
  fn add_child(&mut self, child: AnyElement) -> bool {
    match self {
      Self::CPopover(p) => p.extend([child]),
      Self::CHoverCard(h) => h.extend([child]),
      _ => return false,
    }
    true
  }
  fn into_element(self: Box<Self>) -> AnyElement {
    match *self {
      Self::Popover(p) => p.into_any_element(),
      Self::HoverCard(h) => h.into_any_element(),
      Self::Popup(p) => p.into_any_element(),
      Self::CPopover(p) => p.into_any_element(),
      Self::CHoverCard(h) => h.into_any_element(),
      Self::Effect(e) => e.button.into_any_element(),
      Self::Alert(a) => a.into_any_element(),
    }
  }
  fn into_any(self: Box<Self>) -> Box<dyn Any> {
    self
  }
}

/// Applies a method to the overlay; `f` hands back the kinds it does not apply to.
fn ov(
  c: &Ctx<'_>,
  n: &mut Box<dyn Node>,
  f: impl FnOnce(Overlay) -> std::result::Result<Overlay, Overlay>,
) -> Result<()> {
  let kind = n.kind();
  let mut applies = true;
  crate::map(c, n, |o: Overlay| {
    f(o).unwrap_or_else(|o| {
      applies = false;
      o
    })
  })?;
  match applies {
    true => Ok(()),
    false => Err(crate::miss(c, kind)),
  }
}

/// Sets option `key` of a window effect of `kinds`; `f` applies the method to other overlays.
fn opt<'js>(
  c: &Ctx<'js>,
  n: &mut Box<dyn Node>,
  kinds: &[&str],
  key: &str,
  value: impl IntoJs<'js>,
  f: impl FnOnce(Overlay) -> std::result::Result<Overlay, Overlay>,
) -> Result<()> {
  let mut spec = None;
  ov(c, n, |o| match o {
    Overlay::Effect(e) if kinds.contains(&e.kind) => {
      spec = Some(e.spec);
      Ok(Overlay::Effect(e))
    }
    o => f(o),
  })?;
  if let Some(i) = spec {
    let open = frame().handlers[i].clone().restore(c)?;
    open.get::<_, Object>("spec")?.set(key, value)?;
  }
  Ok(())
}

/// Arg types naming one of a few values
macro_rules! names {
  ($($ty:ident($inner:ty) $what:literal { $($name:literal => $value:expr),* $(,)? })*) => {$(
    struct $ty(&'static str);

    impl<'js> FromJs<'js> for $ty {
      fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
        let name = String::from_js(ctx, value)?;
        match name.as_str() {
          $($name => Ok(Self($name)),)*
          _ => Err(Exception::throw_type(
            ctx,
            &crate::element::style::unknown($what, &name, &[$($name),*]),
          )),
        }
      }
    }

    impl TsType for $ty {
      fn ts() -> String {
        [$(concat!("\"", $name, "\"")),*].join(" | ")
      }
    }

    impl $ty {
      /// The value of a name `from_js` accepted
      fn named(name: &str) -> $inner {
        match name {
          $($name => $value,)*
          _ => unreachable!("checked by from_js"),
        }
      }
    }
  )*};
}

names! {
  Anc(Anchor) "anchor" {
    "top_left" => Anchor::TopLeft,
    "top_center" => Anchor::TopCenter,
    "top_right" => Anchor::TopRight,
    "bottom_left" => Anchor::BottomLeft,
    "bottom_center" => Anchor::BottomCenter,
    "bottom_right" => Anchor::BottomRight,
    "left_center" => Anchor::LeftCenter,
    "right_center" => Anchor::RightCenter,
  }
  Place(Placement) "placement" {
    "top" => Placement::Top,
    "right" => Placement::Right,
    "bottom" => Placement::Bottom,
    "left" => Placement::Left,
  }
  Level(NotificationType) "notification type" {
    "info" => NotificationType::Info,
    "success" => NotificationType::Success,
    "warning" => NotificationType::Warning,
    "error" => NotificationType::Error,
  }
  Press(MouseButton) "mouse button" {
    "left" => MouseButton::Left,
    "right" => MouseButton::Right,
    "middle" => MouseButton::Middle,
  }
}

/// Milliseconds as a delay
struct Ms(Duration);

impl<'js> FromJs<'js> for Ms {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    super::misc::millis(ctx, f64::from_js(ctx, value)?).map(Ms)
  }
}

impl TsType for Ms {
  fn ts() -> String {
    "number".into()
  }
}

/// A window effect callback, `(cx) => void`
struct Cb<'js>(Function<'js>);

impl<'js> FromJs<'js> for Cb<'js> {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    Function::from_js(ctx, value).map(Cb)
  }
}

impl TsType for Cb<'_> {
  fn ts() -> String {
    "(cx: Context) => void".into()
  }
}

/// What `content` takes: an element for a surface, a fn for a dialog or sheet
enum Content<'js> {
  El(Child<'js>),
  Fn(Function<'js>),
}

impl<'js> FromJs<'js> for Content<'js> {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    match value.as_function() {
      Some(f) => Ok(Self::Fn(f.clone())),
      None => Child::from_js(ctx, value).map(Self::El),
    }
  }
}

impl TsType for Content<'_> {
  fn ts() -> String {
    "Element | (() => Element)".into()
  }
}

mod methods {
  use super::*;
  use Overlay::*;

  crate::el_methods! {
    kinds: KINDS;
    /// The element a `corona/base` `Popover` or `HoverCard` draws while closed, and that opens it.
    fn trigger(element: Child<'js>) for ["HoverCard@base", "Popover@base"] |c, n| {
      let t = frame().child(c, element)?;
      ov(c, n, |o| match o {
        Popover(p) => Ok(Popover(p.trigger_with(move |_, _, _| t))),
        HoverCard(h) => Ok(HoverCard(h.trigger(t))),
        o => Err(o),
      })
    };
    /// The element that opens a `corona/component` `HoverCard` when hovered.
    fn trigger_element(element: Child<'js>) for ["HoverCard"] |c, n| {
      let t = frame().child(c, element)?;
      ov(c, n, |o| match o {
        CHoverCard(h) => Ok(CHoverCard(h.trigger(t))),
        o => Err(o),
      })
    };
    /// What a `Popover`, `HoverCard` or `Popup` shows while open: an element of this render.
    fn content(content: Content<'js> as "Element") for ["HoverCard", "HoverCard@base", "Popover", "Popover@base", "Popup@base"] |c, n| match content {
      Content::Fn(_) => Err(crate::miss(c, n.kind())),
      Content::El(child) => {
        let el = frame().child(c, child)?;
        ov(c, n, |o| match o {
          Popover(p) => Ok(Popover(p.content(move |_, _, _| el))),
          HoverCard(h) => Ok(HoverCard(h.content(move |_, _, _| div().id("content").child(el)))),
          Popup(p) => Ok(Popup(p.content(el))),
          CPopover(p) => Ok(CPopover(p.child(el))),
          CHoverCard(h) => Ok(CHoverCard(h.child(el))),
          o => Err(o),
        })
      }
    };
    /// What a `Dialog` or `Sheet` shows: it outlives the render, so it takes a fn rendering its
    /// content instead.
    fn content(content: Content<'js> as "() => Element") for ["Dialog", "Sheet"] |c, n| match content {
      Content::Fn(f) => opt(c, n, &["Dialog", "Sheet"], "content", f, Err),
      Content::El(_) => Err(crate::miss(c, n.kind())),
    };
    /// Whether a `Popover` is open; set, the script holds the state through `on_open_change`.
    fn open(value: bool) for ["Popover", "Popover@base"] |c, n| ov(c, n, |o| match o {
      Popover(p) => Ok(Popover(p.open(value))),
      CPopover(p) => Ok(CPopover(p.open(value))),
      o => Err(o),
    });
    /// Whether an uncontrolled `Popover` starts open.
    fn default_open(value: bool) for ["Popover", "Popover@base"] |c, n| ov(c, n, |o| match o {
      Popover(p) => Ok(Popover(p.default_open(value))),
      CPopover(p) => Ok(CPopover(p.default_open(value))),
      o => Err(o),
    });
    /// Whether a press outside an open `Popover` closes it. Default `true`.
    fn overlay_closable(value: bool) for ["Popover", "Popover@base"] |c, n| ov(c, n, |o| match o {
      Popover(p) => Ok(Popover(p.overlay_closable(value))),
      CPopover(p) => Ok(CPopover(p.overlay_closable(value))),
      o => Err(o),
    });
    /// Which pointer button opens a `Popover`. Default `"left"`.
    fn mouse_button(button: Press) for ["Popover", "Popover@base"] |c, n| ov(c, n, |o| match o {
      Popover(p) => Ok(Popover(p.mouse_button(Press::named(button.0)))),
      CPopover(p) => Ok(CPopover(p.mouse_button(Press::named(button.0)))),
      o => Err(o),
    });
    /// Which corner of a `Popover`, `HoverCard` or `Popup` is pinned to its trigger.
    fn anchor(anchor: Anc) for ["HoverCard", "HoverCard@base", "Popover", "Popover@base", "Popup@base"] |c, n| anchor_to(c, n, Anc::named(anchor.0));
    /// Same as `anchor`.
    fn card_anchor(anchor: Anc) for ["HoverCard", "HoverCard@base", "Popover", "Popover@base", "Popup@base"] |c, n| anchor_to(c, n, Anc::named(anchor.0));
    /// How long the pointer rests on a `HoverCard`'s trigger before it opens. Default 600.
    fn open_delay(ms: Ms) for ["HoverCard", "HoverCard@base"] |c, n| ov(c, n, |o| match o {
      HoverCard(h) => Ok(HoverCard(h.open_delay(ms.0))),
      CHoverCard(h) => Ok(CHoverCard(h.open_delay(ms.0))),
      o => Err(o),
    });
    /// How long a `HoverCard` stays open after the pointer left it and its trigger. Default 300.
    fn close_delay(ms: Ms) for ["HoverCard", "HoverCard@base"] |c, n| ov(c, n, |o| match o {
      HoverCard(h) => Ok(HoverCard(h.close_delay(ms.0))),
      CHoverCard(h) => Ok(CHoverCard(h.close_delay(ms.0))),
      o => Err(o),
    });
    /// Whether a `corona/component` `Popover` or `HoverCard` draws its surface. Default `true`.
    fn appearance(value: bool) for ["HoverCard", "Popover"] |c, n| ov(c, n, |o| match o {
      CPopover(p) => Ok(CPopover(p.appearance(value))),
      CHoverCard(h) => Ok(CHoverCard(h.appearance(value))),
      o => Err(o),
    });
    /// `handler(open, cx)` when a `Popover` or `HoverCard` opens or closes by itself.
    fn on_open_change(handler: On<'js, bool>) for ["HoverCard", "HoverCard@base", "Popover", "Popover@base"] |c, n| {
      let l = frame().listener(handler.0, |open: &bool| *open);
      ov(c, n, |o| match o {
        Popover(p) => Ok(Popover(p.on_open_change(l))),
        HoverCard(h) => Ok(HoverCard(h.on_open_change(l))),
        CPopover(p) => Ok(CPopover(p.on_open_change(l))),
        CHoverCard(h) => Ok(CHoverCard(h.on_open_change(l))),
        o => Err(o),
      })
    };
    /// The title of a window effect or an `Alert`.
    fn title(text: String) for ["Alert", "AlertDialog", "Dialog", "Notification", "Sheet"] |c, n| opt(c, n, EFFECTS, "title", text.clone(), |o| match o {
      Alert(a) => Ok(Alert(a.title(text))),
      o => Err(o),
    });
    /// The text under an `AlertDialog`'s title.
    fn description(text: String) for ["AlertDialog"] |c, n| opt(c, n, &["AlertDialog"], "description", text, Err);
    /// The text under a `Notification`'s title.
    fn message(text: String) for ["Notification"] |c, n| opt(c, n, &["Notification"], "description", text, Err);
    /// Whether an `AlertDialog` has a cancel button. Default `false`.
    fn show_cancel(value: bool) for ["AlertDialog"] |c, n| opt(c, n, &["AlertDialog"], "show_cancel", value, Err);
    /// The window edge a `Sheet` slides in from. Default `"right"`.
    fn placement(at: Place) for ["Sheet"] |c, n| opt(c, n, &["Sheet"], "placement", at.0, Err);
    /// Whether a `Notification` hides by itself. Default `true`.
    fn autohide(value: bool) for ["Notification"] |c, n| opt(c, n, &["Notification"], "autohide", value, Err);
    /// `handler(cx)` on a `Dialog`'s or `AlertDialog`'s OK button; the dialog closes after.
    fn on_ok(handler: Cb<'js>) for ["AlertDialog", "Dialog"] |c, n| opt(c, n, &["Dialog", "AlertDialog"], "on_ok", handler.0, Err);
    /// `handler(cx)` on a `Dialog`'s or `AlertDialog`'s cancel button; the dialog closes after.
    fn on_cancel(handler: Cb<'js>) for ["AlertDialog", "Dialog"] |c, n| {
      opt(c, n, &["Dialog", "AlertDialog"], "on_cancel", handler.0, Err)
    };
    /// `handler(cx)` once a window effect closed, however it was closed.
    fn on_close(handler: Cb<'js>) for ["AlertDialog", "Dialog", "Notification", "Sheet"] |c, n| opt(c, n, EFFECTS, "on_close", handler.0, Err);
    /// `handler(cx)` on a click on a `Notification`.
    fn on_click(handler: Cb<'js>) for ["Notification"] |c, n| opt(c, n, &["Notification"], "on_click", handler.0, Err);
    /// Draws an `Alert` full width, without border, radius or title.
    fn banner() for ["Alert"] |c, n| ov(c, n, |o| match o {
      Alert(a) => Ok(Alert(a.banner())),
      o => Err(o),
    });
    /// Whether an `Alert` is drawn. Default `true`.
    fn visible(value: Opt<bool>) for ["Alert"] |c, n| ov(c, n, |o| match o {
      Alert(a) => Ok(Alert(a.visible(value.0.unwrap_or(true)))),
      o => Err(o),
    });
    /// An `Alert`'s semantic size.
    fn size(size: SizeArg<'js>) for ["Alert"] |c, n| {
      let s = size.get(c, &**n)?;
      ov(c, n, |o| match o {
        Alert(a) => Ok(Alert(a.with_size(s))),
        o => Err(o),
      })
    };
  }

  fn anchor_to(c: &Ctx<'_>, n: &mut Box<dyn Node>, anchor: Anchor) -> Result<()> {
    ov(c, n, |o| match o {
      Popover(p) => Ok(Popover(p.anchor(anchor))),
      HoverCard(h) => Ok(HoverCard(h.anchor(anchor))),
      Popup(p) => Ok(Popup(p.anchor(anchor))),
      CPopover(p) => Ok(CPopover(p.anchor(anchor))),
      CHoverCard(h) => Ok(CHoverCard(h.anchor(anchor))),
      o => Err(o),
    })
  }

  /// `type` is a Rust keyword, so it has no line in the table
  pub(super) fn notification_type<'js>(
    this: This<Class<'js, El>>,
    level: Level,
  ) -> Result<Class<'js, El>> {
    with_node(&this.0, |c, n| {
      opt(c, n, &["Notification"], "type", level.0, Err)
    })?;
    Ok(this.0)
  }

  pub(super) const TYPE_D_TS: &str = "    /** The kind of a `Notification`. Default `\"info\"`. */\n    type(this: Element<\"Notification\">, level: \"info\" | \"success\" | \"warning\" | \"error\"): this;\n";
}

fn install(proto: &Object<'_>) -> Result<()> {
  methods::install(proto)?;
  let level = Function::new(proto.ctx().clone(), methods::notification_type)?;
  crate::define(proto, "type", Some(KINDS), level)
}

fn el_d_ts() -> String {
  methods::d_ts() + methods::TYPE_D_TS
}

/// What a window effect keeps once open: its view and the slot of `on_effect_error`
#[derive(Clone)]
struct Kept {
  view: WeakEntity<ScriptView>,
  error: Option<u64>,
  failed: Rc<Cell<bool>>,
}

impl Kept {
  /// Calls the kept fn `key` with `args`, then the `cx`
  fn call(
    &self,
    key: Option<u64>,
    window: &mut Window,
    cx: &mut App,
    arg: Option<String>,
  ) -> Option<AnyElement> {
    let (key, view) = (key?, self.view.upgrade()?);
    let out = view.update(cx, |view, cx| {
      view.render_content(
        key,
        None,
        window,
        cx,
        |args| match arg {
          Some(arg) => args.push_arg(arg),
          None => Ok(()),
        },
        |ctx, frame, value| match value.is_undefined() {
          true => Ok(None),
          false => frame.child(ctx, Child::from_js(ctx, value)?).map(Some),
        },
      )
    });
    out.unwrap_or_else(|e| {
      let message = format!("{e}");
      tracing::warn!("window effect: {message}");
      if !self.failed.replace(true) {
        self.call(self.error, window, cx, Some(message.clone()));
      }
      Some(message.into_any_element())
    })
  }

  fn fire(&self, key: Option<u64>, window: &mut Window, cx: &mut App) {
    self.call(key, window, cx, None);
  }

  fn content(&self, key: Option<u64>, window: &mut Window, cx: &mut App) -> AnyElement {
    (self.call(key, window, cx, None)).unwrap_or_else(|| div().into_any_element())
  }
}

/// Marks the notifications window effects post
struct Note;

/// Opens the window effect `spec` describes, from its button's click.
fn open_effect<'js>(ctx: &Ctx<'js>, spec: &Object<'js>) -> Result<()> {
  let window = window_or_throw(ctx)?;
  let view = current_view().ok_or_else(|| Exception::throw_type(ctx, "no view here"))?;
  let rt = runtime(ctx)?;
  let keep = |key: &str| -> Result<Option<u64>> {
    Ok((spec.get::<_, Option<Function>>(key)?).map(|f| rt.slot(view.entity_id(), f, false)))
  };
  let text = |key: &str| -> Result<Option<SharedString>> {
    Ok(spec.get::<_, Option<String>>(key)?.map(Into::into))
  };
  let flag = |key: &str, default| -> Result<bool> {
    Ok(spec.get::<_, Option<bool>>(key)?.unwrap_or(default))
  };
  let kind: String = spec.get("kind")?;
  let (title, description) = (text("title")?, text("description")?);
  let (ok, cancel, close) = (keep("on_ok")?, keep("on_cancel")?, keep("on_close")?);
  let kept = Kept {
    view: view.clone(),
    error: keep("on_effect_error")?,
    failed: Rc::default(),
  };
  let app = crate::app();
  match kind.as_str() {
    "Dialog" => {
      let content = keep("content")?;
      let guard = SlotGuard::new(
        &rt,
        view.entity_id(),
        [ok, cancel, close, kept.error, content],
      );
      window.open_dialog(app, move |mut dialog, window, cx| {
        let _ = &guard;
        if let Some(title) = title.clone() {
          dialog = dialog.title(title);
        }
        let (k1, k2, k3) = (kept.clone(), kept.clone(), kept.clone());
        dialog
          .child(kept.content(content, window, cx))
          .on_ok(move |_, window, cx| {
            k1.fire(ok, window, cx);
            true
          })
          .on_cancel(move |_, window, cx| {
            k2.fire(cancel, window, cx);
            true
          })
          .on_close(move |_, window, cx| k3.fire(close, window, cx))
      });
    }
    "AlertDialog" => {
      let show_cancel = flag("show_cancel", false)?;
      let guard = SlotGuard::new(&rt, view.entity_id(), [ok, cancel, close, kept.error]);
      window.open_alert_dialog(app, move |mut alert, _, _| {
        let _ = &guard;
        if let Some(title) = title.clone() {
          alert = alert.title(title);
        }
        if let Some(description) = description.clone() {
          alert = alert.description(description);
        }
        let (k1, k2, k3) = (kept.clone(), kept.clone(), kept.clone());
        alert
          .show_cancel(show_cancel)
          .on_ok(move |_, window, cx| {
            k1.fire(ok, window, cx);
            true
          })
          .on_cancel(move |_, window, cx| {
            k2.fire(cancel, window, cx);
            true
          })
          .on_close(move |_, window, cx| k3.fire(close, window, cx))
      });
    }
    "Sheet" => {
      let content = keep("content")?;
      let at = (spec.get::<_, Option<String>>("placement")?)
        .map_or(Placement::Right, |p| Place::named(&p));
      let guard = SlotGuard::new(
        &rt,
        view.entity_id(),
        [ok, cancel, close, kept.error, content],
      );
      window.open_sheet_at(at, app, move |mut sheet, window, cx| {
        let _ = &guard;
        if let Some(title) = title.clone() {
          sheet = sheet.title(title);
        }
        let k = kept.clone();
        sheet
          .child(kept.content(content, window, cx))
          .on_close(move |_, window, cx| k.fire(close, window, cx))
      });
    }
    _ => {
      let level = (spec.get::<_, Option<String>>("type")?)
        .map_or(NotificationType::Info, |t| Level::named(&t));
      let click = keep("on_click")?;
      let guard = SlotGuard::new(
        &rt,
        view.entity_id(),
        [ok, cancel, close, kept.error, click],
      );
      let id: String = spec.get("id")?;
      let mut note = Notification::new()
        .id1::<Note>(id)
        .with_type(level)
        .autohide(flag("autohide", true)?);
      if let Some(title) = title {
        note = note.title(title);
      }
      if let Some(message) = description {
        note = note.message(message);
      }
      let k = kept.clone();
      let note = note
        .on_click(move |_, window, cx| k.fire(click, window, cx))
        .on_close(move |window, cx| {
          let _ = &guard;
          kept.fire(close, window, cx)
        });
      window.push_notification(note, app);
    }
  }
  Ok(())
}

/// A window effect: a button that opens it, `on_effect_error(message, cx)` when its content
/// fails to render
fn effect<'js>(
  ctx: Ctx<'js>,
  kind: &'static str,
  id: String,
  label: String,
  on_error: Opt<Function<'js>>,
) -> Result<Class<'js, El>> {
  let spec = Object::new(ctx.clone())?;
  spec.set("kind", kind)?;
  spec.set("id", id.clone())?;
  spec.set("on_effect_error", on_error.0)?;
  let s = spec.clone();
  let open = Function::new(ctx.clone(), move |ctx: Ctx<'js>| open_effect(&ctx, &s))?;
  open.set("spec", spec)?;
  let frame = frame_or_throw(&ctx)?;
  let click = frame.listener(open, |_: &ClickEvent| ());
  let effect = Effect {
    kind,
    spec: frame.handlers.len() - 1,
    button: Button::new(id).label(label).on_click(click),
  };
  new_el(&ctx, Overlay::Effect(effect))
}

/// Makes an alert from its id and message
type MakeAlert = fn(Id, String) -> Alert;

const ALERTS: &[(&str, MakeAlert)] = &[
  ("Alert", |id, m| Alert::new(id.into_inner(), m)),
  ("InfoAlert", |id, m| Alert::info(id.into_inner(), m)),
  ("SuccessAlert", |id, m| Alert::success(id.into_inner(), m)),
  ("WarningAlert", |id, m| Alert::warning(id.into_inner(), m)),
  ("ErrorAlert", |id, m| Alert::error(id.into_inner(), m)),
];

fn exports<'js>(ctx: &Ctx<'js>) -> Result<Vec<(&'static str, Value<'js>)>> {
  let mut out = vec![
    ctor(
      ctx,
      "Popover",
      |ctx: Ctx<'js>, id: String, label: String| {
        let trigger = Button::new(format!("popover-trigger:{id}"))
          .ghost()
          .label(label);
        new_el(&ctx, Overlay::CPopover(Popover::new(id).trigger(trigger)))
      },
    )?,
    ctor(ctx, "HoverCard", |ctx: Ctx<'js>, id: Id| {
      new_el(&ctx, Overlay::CHoverCard(HoverCard::new(id.into_inner())))
    })?,
  ];
  out.push(ctor(
    ctx,
    "Tooltip",
    |ctx: Ctx<'js>, id: String, label: String, text: String| {
      new_el(&ctx, Button::new(id).label(label).tooltip(text))
    },
  )?);
  for kind in EFFECTS {
    let f = move |ctx, id, label, on_error| effect(ctx, kind, id, label, on_error);
    out.push(ctor(ctx, kind, f)?);
  }
  for (name, make) in ALERTS {
    let f =
      move |ctx: Ctx<'js>, id: Id, message: String| new_el(&ctx, Overlay::Alert(make(id, message)));
    out.push(ctor(ctx, name, f)?);
  }
  Ok(out)
}

fn base_exports<'js>(ctx: &Ctx<'js>) -> Result<Vec<(&'static str, Value<'js>)>> {
  Ok(vec![
    ctor(ctx, "Popover", |ctx: Ctx<'js>, id: Id| {
      new_el(&ctx, Overlay::Popover(base::Popover::new(id.into_inner())))
    })?,
    ctor(ctx, "HoverCard", |ctx: Ctx<'js>, id: Id| {
      new_el(
        &ctx,
        Overlay::HoverCard(base::HoverCard::new(id.into_inner())),
      )
    })?,
    ctor(
      ctx,
      "Popup",
      |ctx: Ctx<'js>, id: Id, trigger: Child<'js>| {
        let trigger = frame_or_throw(&ctx)?.child(&ctx, trigger)?;
        new_el(
          &ctx,
          Overlay::Popup(base::Popup::new(id.into_inner(), trigger)),
        )
      },
    )?,
  ])
}

const EFFECT_ARGS: &str = "id: string, label: string, \
  on_effect_error?: (message: string, cx: Context) => void";

pub(crate) const FAMILY: Family = Family {
  module: "corona/component",
  names: &[
    "Popover",
    "HoverCard",
    "Tooltip",
    "Dialog",
    "AlertDialog",
    "Sheet",
    "Notification",
    "Alert",
    "InfoAlert",
    "SuccessAlert",
    "WarningAlert",
    "ErrorAlert",
  ],
  exports,
  methods: install,
  d_ts: || {
    let mut out = declare(
      "A ghost button labelled `label` opening a styled surface: `content(..)`, `open(..)`, `on_open_change(..)`.",
      "Popover",
      "id: string, label: string",
    );
    out += &declare(
      "A styled surface opened by hovering `trigger_element(..)`.",
      "HoverCard",
      "id: string | number",
    );
    out += &declare(
      "A button `label` showing `text` on hover.",
      "Tooltip",
      "id: string, label: string, text: string",
    );
    for (kind, what) in [
      ("Dialog", "a dialog of `title(..)` and `content(() => ..)`"),
      (
        "AlertDialog",
        "an alert of `title(..)` and `description(..)`",
      ),
      ("Sheet", "a sheet of `content(() => ..)` at `placement(..)`"),
      ("Notification", "a toast of `title(..)` and `message(..)`"),
    ] {
      out += &declare(
        &format!("A button `label` opening {what}."),
        kind,
        EFFECT_ARGS,
      );
    }
    for (name, _) in ALERTS {
      out += &declare(
        "An inline message.",
        name,
        "id: string | number, message: string",
      );
    }
    out
  },
  el_d_ts,
};

pub(crate) const BASE: Family = Family {
  module: "corona/base",
  names: &["Popover", "HoverCard", "Popup"],
  exports: base_exports,
  methods: |_| Ok(()),
  d_ts: || {
    declare(
      "An unstyled surface anchored to `trigger(..)`, opened by a press: `content(..)`, `open(..)`, `on_open_change(..)`.",
      "Popover",
      "id: string | number",
    ) + &declare(
      "An unstyled surface anchored to `trigger(..)`, opened by resting the pointer on it.",
      "HoverCard",
      "id: string | number",
    ) + &declare(
      "The bare anchored surface: shows `content(..)` while it is set, anchored to `trigger`.",
      "Popup",
      "id: string | number, trigger: Element",
    )
  },
  el_d_ts: String::new,
};

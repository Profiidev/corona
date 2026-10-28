//! The `window` global (dialogs, sheets, toasts, actions, measurements) and [`ScriptRoot`],
//! the content view a host mounts a plugin view in. Overlays need the window's root to be
//! `gpui_component::Root`, which hosts dialogs, sheets and notifications.

use gpui_kit::{
  AnyElement, App, Context, Entity, Hsla, IntoElement, ParentElement, Render, StyleRefinement,
  Styled, WeakEntity, Window, WindowAppearance,
  base::Placement,
  component::{
    WindowExt,
    notification::{Notification, NotificationType},
  },
  div,
  prelude::FluentBuilder,
};
use rquickjs::{Class, Ctx, Exception, FromJs, Function, JsLifetime, Object, Result, class::Trace};

use crate::{
  Child, El, Opt, ScriptView,
  background::{JsPath, paint_path},
  events::{Pt, ScriptAction},
  runtime::runtime,
  style::Fill,
  view::{current_view, window_or_throw},
};

/// Marks the toasts scripts post, for removal by id
struct ScriptToast;

/// The view of the running call and the key of `content` kept for it
fn keep<'js>(ctx: &Ctx<'js>, content: Function<'js>) -> Result<(WeakEntity<ScriptView>, u64)> {
  let view = current_view()
    .ok_or_else(|| Exception::throw_type(ctx, "no view here: call this from an event handler"))?;
  if crate::scope::try_frame().is_some() {
    return Err(Exception::throw_type(
      ctx,
      "overlays open from event handlers, not render",
    ));
  }
  let key = runtime(ctx)?.slot(view.entity_id(), content, false);
  Ok((view, key))
}

/// Renders the content kept as `key` for `view`
fn content(
  view: &WeakEntity<ScriptView>,
  key: u64,
  window: &mut Window,
  cx: &mut App,
) -> AnyElement {
  let Some(view) = view.upgrade() else {
    return div().into_any_element();
  };
  view
    .update(cx, |view, cx| {
      view.render_content(
        key,
        None,
        window,
        cx,
        |_| Ok(()),
        |ctx, frame, value| frame.child(ctx, Child::from_js(ctx, value)?),
      )
    })
    .unwrap_or_else(|e| format!("render failed: {e}").into_any_element())
}

fn placement(ctx: &Ctx<'_>, name: &str) -> Result<Placement> {
  Ok(match name {
    "top" => Placement::Top,
    "bottom" => Placement::Bottom,
    "left" => Placement::Left,
    "right" => Placement::Right,
    _ => {
      let names = ["top", "bottom", "left", "right"];
      return Err(Exception::throw_type(
        ctx,
        &crate::style::unknown("placement", name, &names),
      ));
    }
  })
}

#[derive(rquickjs::IntoJs)]
pub struct Size {
  width: f32,
  height: f32,
}

#[derive(rquickjs::IntoJs)]
pub struct Bounds {
  x: f32,
  y: f32,
  width: f32,
  height: f32,
}

/// The `window` global
#[derive(Trace, JsLifetime, Default)]
#[rquickjs::class(rename = "Window")]
pub struct ScriptWindow {}

#[rquickjs::methods]
impl ScriptWindow {
  /// Opens a dialog with what `content()` renders, again on every redraw.
  pub fn open_dialog<'js>(
    &self,
    ctx: Ctx<'js>,
    content_fn: Function<'js>,
    options: Opt<Object<'js>>,
  ) -> Result<()> {
    let window = window_or_throw(&ctx)?;
    let (view, key) = keep(&ctx, content_fn)?;
    let get = |name: &str| -> Result<bool> {
      Ok(match &options.0 {
        Some(o) => o.get::<_, Option<bool>>(name)?.unwrap_or(true),
        None => true,
      })
    };
    let (escape, backdrop) = (get("escape_dismissable")?, get("backdrop_dismissable")?);
    window.open_dialog(crate::app(), move |dialog, window, cx| {
      dialog
        .keyboard(escape)
        .overlay_closable(backdrop)
        .child(content(&view, key, window, cx))
    });
    Ok(())
  }

  pub fn close_dialog(&self, ctx: Ctx<'_>) -> Result<()> {
    window_or_throw(&ctx)?.close_dialog(crate::app());
    Ok(())
  }

  pub fn close_all_dialogs(&self, ctx: Ctx<'_>) -> Result<()> {
    window_or_throw(&ctx)?.close_all_dialogs(crate::app());
    Ok(())
  }

  pub fn has_active_dialog(&self, ctx: Ctx<'_>) -> Result<bool> {
    Ok(window_or_throw(&ctx)?.has_active_dialog(crate::app()))
  }

  /// Opens the sheet on the right with what `content()` renders.
  pub fn open_sheet<'js>(&self, ctx: Ctx<'js>, content_fn: Function<'js>) -> Result<()> {
    self.open_sheet_at(ctx, "right".into(), content_fn)
  }

  /// Opens the sheet at `placement`: `"top"`, `"bottom"`, `"left"` or `"right"`.
  pub fn open_sheet_at<'js>(
    &self,
    ctx: Ctx<'js>,
    at: String,
    content_fn: Function<'js>,
  ) -> Result<()> {
    let window = window_or_throw(&ctx)?;
    let at = placement(&ctx, &at)?;
    let (view, key) = keep(&ctx, content_fn)?;
    window.open_sheet_at(at, crate::app(), move |sheet, window, cx| {
      sheet.child(content(&view, key, window, cx))
    });
    Ok(())
  }

  pub fn close_sheet(&self, ctx: Ctx<'_>) -> Result<()> {
    window_or_throw(&ctx)?.close_sheet(crate::app());
    Ok(())
  }

  pub fn has_active_sheet(&self, ctx: Ctx<'_>) -> Result<bool> {
    Ok(window_or_throw(&ctx)?.has_active_sheet(crate::app()))
  }

  /// Posts a toast `{ title, description?, level?, timeout?, id? }`; returns its id.
  pub fn push_toast<'js>(&self, ctx: Ctx<'js>, options: Object<'js>) -> Result<String> {
    let window = window_or_throw(&ctx)?;
    let title: String = options.get("title")?;
    let level = match options.get::<_, Option<String>>("level")?.as_deref() {
      None | Some("info") => NotificationType::Info,
      Some("success") => NotificationType::Success,
      Some("warning") => NotificationType::Warning,
      Some("error") => NotificationType::Error,
      Some(other) => {
        let names = ["info", "success", "warning", "error"];
        return Err(Exception::throw_type(
          &ctx,
          &crate::style::unknown("level", other, &names),
        ));
      }
    };
    let id = match options.get::<_, Option<String>>("id")? {
      Some(id) => id,
      None => {
        let rt = runtime(&ctx)?;
        format!("toast-{}", rt.slot_id())
      }
    };
    let mut note = Notification::new()
      .title(title)
      .with_type(level)
      .id1::<ScriptToast>(id.clone());
    if let Some(description) = options.get::<_, Option<String>>("description")? {
      note = note.message(description);
    }
    // `timeout: null` keeps it up; gpui-component picks the duration otherwise
    let keep_up =
      options.contains_key("timeout")? && options.get::<_, rquickjs::Value>("timeout")?.is_null();
    window.push_notification(note.autohide(!keep_up), crate::app());
    Ok(id)
  }

  pub fn remove_toast(&self, ctx: Ctx<'_>, id: String) -> Result<()> {
    window_or_throw(&ctx)?.remove_notification1::<ScriptToast>(id, crate::app());
    Ok(())
  }

  pub fn clear_toasts(&self, ctx: Ctx<'_>) -> Result<()> {
    window_or_throw(&ctx)?.clear_notifications(crate::app());
    Ok(())
  }

  /// Dispatches the action named `action` down the focus path, as its key binding would.
  pub fn dispatch_action(&self, ctx: Ctx<'_>, action: String) -> Result<()> {
    let action = Box::new(ScriptAction(action.into()));
    window_or_throw(&ctx)?.dispatch_action(action, crate::app());
    Ok(())
  }

  /// The pixels one `rem` is.
  pub fn rem_size(&self, ctx: Ctx<'_>) -> Result<f32> {
    Ok(window_or_throw(&ctx)?.rem_size().into())
  }

  pub fn line_height(&self, ctx: Ctx<'_>) -> Result<f32> {
    Ok(window_or_throw(&ctx)?.line_height().into())
  }

  /// The drawable area.
  pub fn viewport_size(&self, ctx: Ctx<'_>) -> Result<Size> {
    let size = window_or_throw(&ctx)?.viewport_size();
    Ok(Size {
      width: size.width.into(),
      height: size.height.into(),
    })
  }

  /// Where the window is on screen, and how big.
  pub fn bounds(&self, ctx: Ctx<'_>) -> Result<Bounds> {
    let b = window_or_throw(&ctx)?.bounds();
    Ok(Bounds {
      x: b.origin.x.into(),
      y: b.origin.y.into(),
      width: b.size.width.into(),
      height: b.size.height.into(),
    })
  }

  pub fn mouse_position(&self, ctx: Ctx<'_>) -> Result<Pt> {
    Ok(window_or_throw(&ctx)?.mouse_position().into())
  }

  /// `"light"` or `"dark"`
  pub fn appearance(&self, ctx: Ctx<'_>) -> Result<&'static str> {
    Ok(match window_or_throw(&ctx)?.appearance() {
      WindowAppearance::Dark | WindowAppearance::VibrantDark => "dark",
      WindowAppearance::Light | WindowAppearance::VibrantLight => "light",
    })
  }

  pub fn is_window_active(&self, ctx: Ctx<'_>) -> Result<bool> {
    Ok(window_or_throw(&ctx)?.is_window_active())
  }

  pub fn is_fullscreen(&self, ctx: Ctx<'_>) -> Result<bool> {
    Ok(window_or_throw(&ctx)?.is_fullscreen())
  }

  pub fn is_maximized(&self, ctx: Ctx<'_>) -> Result<bool> {
    Ok(window_or_throw(&ctx)?.is_maximized())
  }

  pub fn set_rem_size(&self, ctx: Ctx<'_>, size: f32) -> Result<()> {
    window_or_throw(&ctx)?.set_rem_size(gpui_kit::px(size));
    Ok(())
  }

  /// Redraws every view of the window; `cx.notify()` is almost always what you want.
  pub fn refresh(&self, ctx: Ctx<'_>) -> Result<()> {
    window_or_throw(&ctx)?.refresh();
    Ok(())
  }

  pub fn focus_next(&self, ctx: Ctx<'_>) -> Result<()> {
    window_or_throw(&ctx)?.focus_next(crate::app());
    Ok(())
  }

  pub fn focus_prev(&self, ctx: Ctx<'_>) -> Result<()> {
    window_or_throw(&ctx)?.focus_prev(crate::app());
    Ok(())
  }

  pub fn activate_window(&self, ctx: Ctx<'_>) -> Result<()> {
    window_or_throw(&ctx)?.activate_window();
    Ok(())
  }

  pub fn minimize_window(&self, ctx: Ctx<'_>) -> Result<()> {
    window_or_throw(&ctx)?.minimize_window();
    Ok(())
  }

  pub fn zoom_window(&self, ctx: Ctx<'_>) -> Result<()> {
    window_or_throw(&ctx)?.zoom_window();
    Ok(())
  }

  pub fn toggle_fullscreen(&self, ctx: Ctx<'_>) -> Result<()> {
    window_or_throw(&ctx)?.toggle_fullscreen();
    Ok(())
  }

  /// An element painting `path` with `fill`, in its own coordinates.
  pub fn paint_path<'js>(
    &self,
    ctx: Ctx<'js>,
    path: Class<'js, JsPath>,
    fill: Fill,
  ) -> Result<Class<'js, El>> {
    paint_path(&ctx, &path.borrow(), fill.0)
  }
}

/// The content a host mounts a plugin view in: what it paints under the view and whether
/// it fills the window. Filling, it is cached at full size, so frames in which the view did
/// not notify repaint without entering JS.
pub struct ScriptRoot {
  view: Entity<ScriptView>,
  background: Option<Hsla>,
  fill: bool,
}

impl ScriptRoot {
  pub fn new(view: Entity<ScriptView>) -> Self {
    Self {
      view,
      background: None,
      fill: true,
    }
  }

  pub fn view(&self) -> &Entity<ScriptView> {
    &self.view
  }

  /// What is painted under the view; none leaves the window's own background.
  pub fn set_background(&mut self, background: Option<Hsla>, cx: &mut Context<Self>) {
    self.background = background;
    cx.notify();
  }

  /// Whether it fills the window (the default) or is as big as its content.
  pub fn set_fill(&mut self, fill: bool, cx: &mut Context<Self>) {
    self.fill = fill;
    cx.notify();
  }

  pub fn background(&self) -> Option<Hsla> {
    self.background
  }

  pub fn fills(&self) -> bool {
    self.fill
  }
}

impl Render for ScriptRoot {
  fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
    let view = self.view.clone();
    div()
      .when_some(self.background, |root, bg| root.bg(bg))
      .map(|root| match self.fill {
        true => root
          .size_full()
          .child(view.cached(StyleRefinement::default().size_full())),
        // cached at full size it would have no size of its own
        false => root.child(view),
      })
  }
}

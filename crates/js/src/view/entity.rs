//! Retained entities: state a script keeps across renders (`ScrollHandle` here; `InputState`,
//! `SliderState`, .. bound in `components/`), and `.on(event, handler)` on them.
//!
//! A state's JS class holds a [`Retained`], which keeps the gpui entity and the handlers its
//! `.on` registered. The handlers live in the runtime, not in the JS object, so releasing the
//! view frees them.

use std::rc::Weak;

use gpui_kit::{App, Entity, EventEmitter};
use rquickjs::{
  CatchResultExt, Ctx, Exception, Function, IntoJs, JsLifetime, Persistent, Result, class::Trace,
};

use crate::{
  Enter, Named,
  element::events::Pt,
  runtime::{Runtime, runtime},
  view::{ViewCx, current_view},
  with,
};

/// A gpui entity a script holds, with its `.on` subscriptions
pub struct Retained<T: 'static> {
  entity: Option<Entity<T>>,
  subs: Vec<u32>,
  rt: Weak<Runtime>,
}

impl<T: 'static> Retained<T> {
  pub fn new(ctx: &Ctx<'_>, entity: Entity<T>) -> Result<Self> {
    Ok(Self {
      entity: Some(entity),
      subs: Vec::new(),
      rt: std::rc::Rc::downgrade(&runtime(ctx)?),
    })
  }

  /// The entity, or a JS error once released
  pub fn entity(&self, ctx: &Ctx<'_>) -> Result<&Entity<T>> {
    (self.entity.as_ref()).ok_or_else(|| Exception::throw_type(ctx, "the state was released"))
  }

  /// `.on(name, handler)`: calls `handler(payload, cx)` for each gpui event `map` names `name`.
  /// False for an event name `map` never produces, per `names`.
  pub fn on<'js, E: 'static, P: for<'a> IntoJs<'a> + 'static>(
    &mut self,
    ctx: &Ctx<'js>,
    names: &[&str],
    name: String,
    handler: Function<'js>,
    map: impl Fn(&E, &App) -> Option<(&'static str, P)> + 'static,
  ) -> Result<bool>
  where
    T: EventEmitter<E>,
  {
    if !names.contains(&name.as_str()) {
      return Ok(false);
    }
    let rt = runtime(ctx)?;
    let handler = Persistent::save(ctx, handler);
    let view = current_view();
    let weak = self.rt.clone();
    let sub = crate::app().subscribe(self.entity(ctx)?, move |_, event: &E, cx| {
      let Some((_, payload)) = map(event, cx).filter(|(n, _)| *n == name) else {
        return;
      };
      let Some(rt) = weak.upgrade() else {
        return;
      };
      let _enter = Enter::new(cx, None, None);
      with(rt.ctx(), |ctx| {
        let call = || {
          let cx = view.clone().map(ViewCx::for_view);
          handler.clone().restore(&ctx)?.call::<_, ()>((payload, cx))
        };
        if let Err(e) = call().catch(&ctx) {
          tracing::warn!("state handler: {e}");
        }
      });
    });
    self.subs.push(rt.keep(sub));
    Ok(true)
  }

  /// Drops the entity and its handlers; false if released already.
  pub fn release(&mut self) -> bool {
    if let Some(rt) = self.rt.upgrade() {
      self.subs.drain(..).for_each(|id| rt.unkeep(id));
    }
    self.entity.take().is_some()
  }
}

/// A scroll position a script reads and drives: `track_scroll(handle)` on the element
#[derive(Trace, JsLifetime, Default)]
#[rquickjs::class]
pub struct ScrollHandle {
  #[qjs(skip_trace)]
  pub(crate) handle: gpui_kit::ScrollHandle,
}

impl Named for ScrollHandle {
  const TS: &'static str = "ScrollHandle";
}

#[rquickjs::methods]
impl ScrollHandle {
  #[qjs(constructor)]
  pub fn new() -> Self {
    Self::default()
  }

  /// Same as `new ScrollHandle()`.
  #[qjs(static, rename = "new")]
  pub fn create() -> Self {
    Self::default()
  }

  /// How far it is scrolled, negative downwards and rightwards.
  pub fn offset(&self) -> Pt {
    self.handle.offset().into()
  }

  /// How far it can scroll.
  pub fn max_offset(&self) -> Pt {
    self.handle.max_offset().into()
  }

  pub fn set_offset(&self, x: f32, y: f32) {
    let p = gpui_kit::point(gpui_kit::px(x), gpui_kit::px(y));
    self.handle.set_offset(p);
  }

  /// Scrolls child `index` into view.
  pub fn scroll_to_item(&self, index: u32) {
    self.handle.scroll_to_item(index as usize);
  }

  pub fn scroll_to_bottom(&self) {
    self.handle.scroll_to_bottom();
  }

  /// The first visible child.
  pub fn top_item(&self) -> u32 {
    self.handle.top_item() as u32
  }
}

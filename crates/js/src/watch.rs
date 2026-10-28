//! Calling back into JS when the shell changes: `subscribe(cb)` and view re-renders.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use gpui_kit::{App, Entity, Subscription};
use rquickjs::{AsyncContext, CatchResultExt, Ctx, Function, IntoJs, Persistent, Result, Value};

use crate::{Enter, scope::try_frame, with};

/// A JS fn kept to be called later, from a gpui observer or task.
#[derive(Clone)]
pub struct Callback {
  f: Persistent<Function<'static>>,
  // last: holds the runtime until the fn is freed
  ctx: AsyncContext,
}

impl Callback {
  /// Calls the fn with the value `arg` makes, in a scope of `cx`; logs what it throws.
  pub fn call(&self, cx: &mut App, arg: impl for<'js> FnOnce(Ctx<'js>) -> Result<Value<'js>>) {
    let _enter = Enter::new(cx, None, None);
    with(&self.ctx, |ctx| {
      let call = || {
        let f = self.f.clone().restore(&ctx)?;
        f.call::<_, ()>((arg(ctx.clone())?,))
      };
      if let Err(e) = call().catch(&ctx) {
        tracing::warn!("script callback: {e}");
      }
    });
  }
}

/// A plugin's subscriptions: shared by its modules and owned by its runtime, which
/// [`clear`](Self::clear)s it before it drops itself, as it holds JS fns.
#[derive(Clone, Default)]
pub struct Watches(Rc<RefCell<State>>);

#[derive(Default)]
struct State {
  next: u32,
  subs: HashMap<u32, (Subscription, Callback)>,
  ctx: Option<AsyncContext>,
}

impl Watches {
  /// The context the plugin runs in, set once its runtime exists.
  pub fn set_context(&self, ctx: AsyncContext) {
    self.0.borrow_mut().ctx = Some(ctx);
  }

  /// Keeps `f` to call it later.
  pub fn callback(&self, f: Function<'_>) -> Callback {
    let ctx = self.0.borrow().ctx.clone();
    Callback {
      f: Persistent::save(&f.ctx().clone(), f),
      ctx: ctx.expect("watches used before their runtime exists"),
    }
  }

  /// Calls `cb(read(cx))` on every change of `entity`, until the returned fn is called.
  pub fn subscribe<'js, E: 'static, R: for<'a> IntoJs<'a>>(
    &self,
    entity: &Entity<E>,
    cb: Function<'js>,
    read: impl Fn(&App) -> R + 'static,
  ) -> Result<Function<'js>> {
    let ctx = cb.ctx().clone();
    let callback = self.callback(cb);
    let id = {
      let mut state = self.0.borrow_mut();
      state.next += 1;
      state.next
    };
    let watches = self.clone();
    let sub = crate::app().observe(entity, move |_, cx| {
      // cloned out, the fn may unsubscribe
      let callback = watches.0.borrow().subs.get(&id).map(|(_, cb)| cb.clone());
      if let Some(callback) = callback {
        callback.call(cx, |ctx| read(crate::app()).into_js(&ctx));
      }
    });
    self.0.borrow_mut().subs.insert(id, (sub, callback));

    let watches = self.clone();
    Function::new(ctx, move || {
      let removed = watches.0.borrow_mut().subs.remove(&id);
      drop(removed);
    })
  }

  /// Drops every subscription and JS fn, so the runtime can be freed.
  pub fn clear(&self) {
    let state = std::mem::take(&mut *self.0.borrow_mut());
    drop(state);
  }
}

/// Renders the view that is rendering again once `entity` changes. Does nothing outside a
/// render.
pub fn track<T: 'static>(entity: &Entity<T>) {
  if let Some(frame) = try_frame() {
    frame.reads.entry(entity.entity_id()).or_insert_with(|| {
      let entity = entity.clone();
      Box::new(move |cx| cx.observe(&entity, |_, _, cx| cx.notify()))
    });
  }
}

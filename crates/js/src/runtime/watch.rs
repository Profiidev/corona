//! Calling back into JS when the shell changes: `subscribe(cb)` and view re-renders.

use std::{cell::RefCell, collections::HashMap, marker::PhantomData, rc::Rc};

use gpui_kit::{App, Entity, Subscription};
use rquickjs::{
  AsyncContext, CatchResultExt, Ctx, FromJs, Function, IntoJs, Persistent, Result, Value,
};
use specta::{Type, Types, datatype::DataType};
use specta_typescript::{Typescript, define, primitives};

use crate::{
  Enter,
  runtime::{host::floats, runtime, scope::try_frame},
  with,
};

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
  /// JS fns a host module calls itself, by id
  handlers: HashMap<u32, Callback>,
  next_handler: u32,
  /// Kept until the runtime ends
  held: Vec<Subscription>,
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

  /// Keeps `f` for [`call_handler`](Self::call_handler) until released or the runtime ends.
  pub fn register_handler(&self, f: Function<'_>) -> u32 {
    let callback = self.callback(f);
    let mut state = self.0.borrow_mut();
    state.next_handler += 1;
    let id = state.next_handler;
    state.handlers.insert(id, callback);
    id
  }

  pub fn release_handler(&self, id: u32) {
    let removed = self.0.borrow_mut().handlers.remove(&id);
    drop(removed);
  }

  /// Keeps `sub` until the runtime ends.
  pub fn hold(&self, sub: Subscription) {
    self.0.borrow_mut().held.push(sub);
  }

  /// Calls handler `id` with `arg`, JSON, and waits for a promise it returns. `settle` gets
  /// the value, or the error, in the runtime's context; it never runs when the runtime is
  /// gone first.
  pub fn call_handler(
    &self,
    id: u32,
    arg: &str,
    cx: &mut App,
    settle: impl for<'js> FnOnce(&Ctx<'js>, std::result::Result<Value<'js>, String>, &mut App) + 'static,
  ) {
    let (callback, ctx) = {
      let state = self.0.borrow();
      (state.handlers.get(&id).cloned(), state.ctx.clone())
    };
    let Some(ctx) = ctx else {
      return;
    };
    let _enter = Enter::new(cx, None, None);
    with(&ctx, |ctx| {
      let call = || -> Result<Value<'_>> {
        let Some(callback) = &callback else {
          return Err(rquickjs::Exception::throw_message(
            &ctx,
            "the handler is gone",
          ));
        };
        let f = callback.f.clone().restore(&ctx)?;
        f.call((ctx.json_parse(arg)?,))
      };
      let value = match call().catch(&ctx) {
        Ok(value) => value,
        Err(e) => return settle(&ctx, Err(e.to_string()), &mut crate::app()),
      };
      let Some(promise) = value.as_promise().cloned() else {
        return settle(&ctx, Ok(value), &mut crate::app());
      };
      let ctx2 = ctx.clone();
      ctx.spawn(async move {
        let settled = promise
          .into_future::<Value>()
          .await
          .catch(&ctx2)
          .map_err(|e| e.to_string());
        if let Some(mut cx) = crate::runtime::scope::try_app() {
          settle(&ctx2, settled, &mut cx);
        }
      });
    });
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
    let id = self.next_id();
    let watches = self.clone();
    let sub = crate::app().observe(entity, move |_, cx| {
      // cloned out, the fn may unsubscribe
      let callback = watches.0.borrow().subs.get(&id).map(|(_, cb)| cb.clone());
      if let Some(callback) = callback {
        callback.call(cx, |ctx| read(&crate::app()).into_js(&ctx));
      }
    });
    self.0.borrow_mut().subs.insert(id, (sub, callback));
    Unsubscribe {
      watches: self.clone(),
      id,
    }
    .into_js(&ctx)?
    .into_function()
    .ok_or(rquickjs::Error::Unknown)
  }

  fn next_id(&self) -> u32 {
    let mut state = self.0.borrow_mut();
    state.next += 1;
    state.next
  }

  /// Drops every subscription and JS fn, so the runtime can be freed.
  pub fn clear(&self) {
    let state = std::mem::take(&mut *self.0.borrow_mut());
    drop(state);
  }
}

/// A JS fn a host fn takes to call later with `T`, typed `(value: T) => void`.
pub struct Listener<T> {
  callback: Callback,
  watches: Watches,
  _value: PhantomData<fn(T)>,
}

impl<T> Clone for Listener<T> {
  fn clone(&self) -> Self {
    Self {
      callback: self.callback.clone(),
      watches: self.watches.clone(),
      _value: PhantomData,
    }
  }
}

impl<T: for<'js> IntoJs<'js>> Listener<T> {
  /// Calls the fn with `value`; logs what it throws.
  pub fn call(&self, cx: &mut App, value: T) {
    self.callback.call(cx, |ctx| value.into_js(&ctx));
  }
}

impl<T> Listener<T> {
  /// Keeps `sub`, which calls this listener, until the script calls the returned fn or its
  /// runtime is gone.
  pub fn keep(self, sub: Subscription) -> Unsubscribe {
    let id = self.watches.next_id();
    (self.watches.0.borrow_mut().subs).insert(id, (sub, self.callback));
    Unsubscribe {
      watches: self.watches,
      id,
    }
  }
}

impl<'js, T> FromJs<'js> for Listener<T> {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    let f = Function::from_js(ctx, value)?;
    let watches = runtime(ctx)?.watches.clone();
    Ok(Self {
      callback: watches.callback(f),
      watches,
      _value: PhantomData,
    })
  }
}

impl<T: Type> Type for Listener<T> {
  fn definition(types: &mut Types) -> DataType {
    let mut dt = T::definition(types);
    floats(&mut dt);
    let ts = primitives::reference(&Typescript::default(), types, &dt).expect("host type");
    DataType::Reference(define(format!("(value: {ts}) => void")))
  }
}

/// Stops a [`Listener`]: a JS fn typed `() => void`.
pub struct Unsubscribe {
  watches: Watches,
  id: u32,
}

impl<'js> IntoJs<'js> for Unsubscribe {
  fn into_js(self, ctx: &Ctx<'js>) -> Result<Value<'js>> {
    let Self { watches, id } = self;
    let stop = Function::new(ctx.clone(), move || {
      let removed = watches.0.borrow_mut().subs.remove(&id);
      drop(removed);
    })?;
    Ok(stop.into_value())
  }
}

impl Type for Unsubscribe {
  fn definition(_: &mut Types) -> DataType {
    DataType::Reference(define("() => void"))
  }
}

/// Renders the view that is rendering again once `entity` changes. Does nothing outside a
/// render.
pub fn track<T: 'static>(entity: &Entity<T>) {
  if let Some(mut frame) = try_frame() {
    frame.reads.entry(entity.entity_id()).or_insert_with(|| {
      let entity = entity.clone();
      Box::new(move |cx| cx.observe(&entity, |_, _, cx| cx.notify()))
    });
  }
}

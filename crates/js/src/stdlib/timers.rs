//! Timers on the gpui clock: `setTimeout` and friends, `sleep(ms)` and `timer.after/every`.
//! They run as futures of their runtime, so dropping it cancels them.

use std::{
  cell::{Cell, RefCell},
  collections::HashMap,
  rc::Rc,
  time::Duration,
};

use rquickjs::{
  CatchResultExt, Ctx, Function, Object, Promise, Result, Value,
  function::{Opt, Rest, This},
};

use super::io::promise;
use crate::app;

pub(crate) fn duration(ms: Option<f64>) -> Duration {
  Duration::from_millis(ms.filter(|ms| ms.is_finite() && *ms > 0.).unwrap_or(0.) as u64)
}

/// Calls `f(...args)` after `delay`, again every `delay` when `repeat`, until `cancelled`
/// is set. Sets `cancelled` once a one-shot ran.
fn schedule<'js>(
  ctx: &Ctx<'js>,
  f: Function<'js>,
  delay: Duration,
  repeat: bool,
  args: Vec<Value<'js>>,
  cancelled: Rc<Cell<bool>>,
) {
  let ctx2 = ctx.clone();
  // an interval of 0 would spin the executor
  let delay = if repeat {
    delay.max(Duration::from_millis(4))
  } else {
    delay
  };
  ctx.spawn(async move {
    loop {
      app().background_executor().timer(delay).await;
      if cancelled.get() {
        break;
      }
      if let Err(e) = f.call::<_, ()>((Rest(args.clone()),)).catch(&ctx2) {
        tracing::warn!("timer callback: {e}");
      }
      if !repeat {
        cancelled.set(true);
        break;
      }
    }
  });
}

/// The last id, and the cancel flags of the live timers by id
type Live = (u32, HashMap<u32, Rc<Cell<bool>>>);

/// Installs `setTimeout`, `setInterval`, `clearTimeout` and `clearInterval`.
pub(crate) fn install<'js>(ctx: &Ctx<'js>) -> Result<()> {
  let live: Rc<RefCell<Live>> = Rc::default();
  for (name, repeat) in [("setTimeout", false), ("setInterval", true)] {
    let live = live.clone();
    let set = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'js>, f: Function<'js>, ms: Opt<f64>, args: Rest<Value<'js>>| {
        let cancelled = Rc::new(Cell::new(false));
        let id = {
          let mut live = live.borrow_mut();
          live.1.retain(|_, c| !c.get());
          live.0 += 1;
          let id = live.0;
          live.1.insert(id, cancelled.clone());
          id
        };
        schedule(&ctx, f, duration(ms.0), repeat, args.0, cancelled);
        id
      },
    )?;
    ctx.globals().set(name, set)?;
  }
  let clear = Function::new(ctx.clone(), move |id: Opt<Value<'_>>| {
    let id = id.0.and_then(|v| v.as_number()).unwrap_or(-1.) as u32;
    if let Some(cancelled) = live.borrow_mut().1.remove(&id) {
      cancelled.set(true);
    }
  })?;
  ctx.globals().set("clearTimeout", clear.clone())?;
  ctx.globals().set("clearInterval", clear)
}

/// `sleep(ms)`: resolves after `ms`
pub(crate) fn sleep<'js>(ctx: Ctx<'js>, ms: Opt<f64>) -> Result<Promise<'js>> {
  let delay = duration(ms.0);
  promise(&ctx, async move {
    app().background_executor().timer(delay).await;
    Ok(())
  })
}

/// `{ after(ms, handler), every(ms, handler) }`, calling `handler(cx)`; both return a task
/// with `cancel()` and `is_done()`. `cx` is a property: a JS value a Rust closure held would
/// hide a cycle from the GC.
pub(crate) fn timer<'js>(ctx: &Ctx<'js>, cx: Value<'js>) -> Result<Object<'js>> {
  let o = Object::new(ctx.clone())?;
  o.set("cx", cx)?;
  for (name, repeat) in [("after", false), ("every", true)] {
    let start = Function::new(
      ctx.clone(),
      move |ctx: Ctx<'js>,
            this: This<Value<'js>>,
            ms: f64,
            f: Function<'js>|
            -> Result<Object<'js>> {
        let cx = match this.0.as_object() {
          Some(timer) => timer.get("cx")?,
          None => Value::new_undefined(ctx.clone()),
        };
        let cancelled = Rc::new(Cell::new(false));
        schedule(
          &ctx,
          f,
          duration(Some(ms)),
          repeat,
          vec![cx],
          cancelled.clone(),
        );
        let task = Object::new(ctx.clone())?;
        let c = cancelled.clone();
        task.set("cancel", Function::new(ctx.clone(), move || c.set(true))?)?;
        task.set(
          "is_done",
          Function::new(ctx.clone(), move || cancelled.get())?,
        )?;
        Ok(task)
      },
    )?;
    o.set(name, start)?;
  }
  Ok(o)
}

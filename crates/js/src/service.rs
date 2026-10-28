//! A plugin service: a file whose default export is `main(cx)`, run without a window. A
//! failing `main` is logged; dropping the service cancels its timers and kills its
//! children.

use std::{cell::RefCell, path::Path, rc::Rc};

use anyhow::{Result, anyhow, bail};
use gpui_kit::{App, Task};
use rquickjs::{CatchResultExt, Function, Object, Promise, Value};

use crate::{
  Enter, ModuleBuilder, PluginRef, Watches,
  reload::watch,
  runtime::{self, Main, Runtime},
  stdlib::timers,
  with,
};

/// A running service
pub struct Service {
  _rt: Rc<Runtime>,
}

impl Service {
  /// Runs `main(cx)` of `entry`, whose `cx` has `sleep(ms)` and `timer.after/every`. Errs when
  /// the file does not load or export a function.
  pub fn start(
    entry: &Path,
    plugin: Rc<PluginRef>,
    modules: Vec<ModuleBuilder>,
    watches: Watches,
    cx: &mut App,
  ) -> Result<Self> {
    let id = plugin.id.clone();
    let rt = runtime::new(plugin, modules, watches, cx)?;
    let ctx = rt.ctx().clone();
    let _enter = Enter::new(cx, None, None);
    let started = with(&ctx, |ctx| {
      let run = || {
        let module = runtime::eval(&ctx, Main::File(entry))?;
        let main: Value = module.get("default")?;
        let Some(main) = main.into_function() else {
          return Ok(false);
        };
        let service_cx = Object::new(ctx.clone())?;
        service_cx.set("sleep", Function::new(ctx.clone(), timers::sleep)?)?;
        service_cx.set(
          "timer",
          timers::timer(&ctx, service_cx.clone().into_value())?,
        )?;
        match main.call::<_, Value>((service_cx,)).catch(&ctx) {
          Ok(out) => {
            if let Some(promise) = out.into_promise() {
              log_rejection(&ctx, promise, id.clone());
            }
          }
          Err(e) => tracing::warn!("service `{id}` failed: {e}"),
        }
        Ok(true)
      };
      run().catch(&ctx).map_err(|e| anyhow!("{e}"))
    });
    let service = Self { _rt: rt };
    if !started? {
      bail!("a service must `export default` a function `main(cx)`");
    }
    Ok(service)
  }

  /// Starts the service `start` makes, and again whenever a source in `dir` changes. A
  /// failed start is logged and retried on the next change.
  pub fn watch(
    dir: &Path,
    start: impl Fn(&mut App) -> Result<Service> + 'static,
    cx: &mut App,
  ) -> Task<()> {
    let running = Rc::new(RefCell::new(None));
    let restart = move |cx: &mut App| {
      // the old one stops before the new one starts
      running.borrow_mut().take();
      match start(cx) {
        Ok(service) => *running.borrow_mut() = Some(service),
        Err(e) => tracing::warn!("service not started: {e:#}"),
      }
    };
    restart(cx);
    watch(dir.to_path_buf(), cx, restart)
  }
}

fn log_rejection<'js>(ctx: &rquickjs::Ctx<'js>, promise: Promise<'js>, id: String) {
  let ctx2 = ctx.clone();
  ctx.spawn(async move {
    if let Err(e) = promise.into_future::<Value>().await.catch(&ctx2) {
      tracing::warn!("service `{id}` failed: {e}");
    }
  });
}

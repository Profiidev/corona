//! A plugin view: `class X extends View { init(props, cx); render(cx) }`, rendered through
//! the [`Frame`] arena.

use anyhow::{Result, anyhow};
use gpui_kit::{AnyElement, Context, EntityId, IntoElement, Render, Task, Window, div, prelude::*};
use rquickjs::{
  AsyncContext, AsyncRuntime, CatchResultExt, Class, Ctx, Function, JsLifetime, Module, Object,
  Persistent, Value,
  class::Trace,
  function::{Constructor, Func, This},
  loader::{BuiltinResolver, ModuleLoader},
  module::{Declarations, Exports, ModuleDef},
};

use crate::{Enter, Frame, app, el::new_el, el_d_ts, now, with};

/// The `cx` a view's methods and handlers get
#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct ViewCx {
  #[qjs(skip_trace)]
  view: EntityId,
}

#[rquickjs::methods]
impl ViewCx {
  /// Renders the view again.
  pub fn notify(&self) {
    app().notify(self.view);
  }
}

/// The `gpui-kit` module: `View` and `div`
pub struct GpuiKit;

impl GpuiKit {
  pub const NAME: &str = "gpui-kit";

  pub fn d_ts() -> String {
    let el: String = el_d_ts().lines().map(|l| format!("  {l}\n")).collect();
    format!(
      r#"declare module "gpui-kit" {{
{el}  export function div(): El;
  export interface ViewCx {{
    /** Renders the view again. */
    notify(): void;
  }}
  export abstract class View {{
    init?(props: unknown, cx: ViewCx): void;
    abstract render(cx: ViewCx): El;
  }}
}}
"#
    )
  }
}

impl ModuleDef for GpuiKit {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("View")?.declare("div")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    // an empty base class, so `extends View` works without native subclassing
    exports.export("View", ctx.eval::<Value, _>("(class View {})")?)?;
    exports.export("div", Func::from(new_el))?;
    Ok(())
  }
}

/// Runs a plugin view module in its own runtime.
pub struct ScriptView {
  instance: Persistent<Object<'static>>,
  handlers: Vec<Persistent<Function<'static>>>,
  _drive: Task<()>,
  #[cfg(test)]
  pub(crate) text: String,
  // last: `Persistent` handles must be freed before the runtime
  ctx: AsyncContext,
}

impl ScriptView {
  /// Evaluates `source`, whose default export extends `View`, and calls its `init`. The
  /// module can import `gpui-kit` and whatever is installed on `resolver` and `loader`.
  pub fn new(
    source: &str,
    mut resolver: BuiltinResolver,
    mut loader: ModuleLoader,
    window: &mut Window,
    cx: &mut Context<Self>,
  ) -> Result<Self> {
    resolver.add_module(GpuiKit::NAME);
    loader.add_module(GpuiKit::NAME, GpuiKit);
    let rt = AsyncRuntime::new()?;
    now(rt.set_loader(resolver, loader));
    let ctx = now(AsyncContext::full(&rt))?;

    // drives async host fns and the promise jobs they resolve, inside the scope
    let mut drive = Box::pin(rt.drive());
    let drive = cx.spawn(async move |_, cx| {
      std::future::poll_fn(|task| {
        cx.update(|cx| {
          let _enter = Enter::new(cx, None, None);
          drive.as_mut().poll(task)
        })
      })
      .await
    });

    let view = cx.entity_id();
    let _enter = Enter::new(cx, Some(window), None);
    let instance = with(&ctx, |ctx| {
      let run = || {
        let (module, done) = Module::declare(ctx.clone(), "main", source)?.eval()?;
        done.finish::<()>()?;
        let class: Constructor = module.get("default")?;
        let instance: Object = class.construct(())?;
        if let Ok(init) = instance.get::<_, Function>("init") {
          init.call::<_, ()>((
            This(instance.clone()),
            Value::new_undefined(ctx.clone()),
            ViewCx { view },
          ))?;
        }
        Ok(Persistent::save(&ctx, instance))
      };
      run().catch(&ctx).map_err(|e| anyhow!("{e}"))
    })?;

    Ok(Self {
      instance,
      handlers: Vec::new(),
      _drive: drive,
      #[cfg(test)]
      text: String::new(),
      ctx,
    })
  }

  /// Calls click handler `index` of the last render.
  pub(crate) fn dispatch(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
    let Some(handler) = self.handlers.get(index).cloned() else {
      return;
    };
    let view = cx.entity_id();
    let _enter = Enter::new(cx, Some(window), None);
    with(&self.ctx, |ctx| {
      let call = || {
        handler
          .restore(&ctx)?
          .call::<_, ()>((Value::new_undefined(ctx.clone()), ViewCx { view }))
      };
      if let Err(e) = call().catch(&ctx) {
        tracing::warn!("click handler: {e}");
      }
    });
  }

  fn render_js(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Result<AnyElement> {
    let view = cx.entity_id();
    let mut frame = Frame::new(cx.entity().downgrade());
    let enter = Enter::new(cx, Some(window), Some(&mut frame));
    let root = with(&self.ctx, |ctx| {
      let run = || {
        let instance = self.instance.clone().restore(&ctx)?;
        let render: Function = instance.get("render")?;
        let root: Class<crate::El> = render.call((This(instance), ViewCx { view }))?;
        let id = root.borrow().id;
        crate::frame().take(&ctx, id)
      };
      run().catch(&ctx).map_err(|e| anyhow!("{e}"))
    });
    drop(enter);
    let root = root?;
    #[cfg(test)]
    {
      self.text = std::mem::take(&mut frame.text);
    }
    self.handlers = std::mem::take(&mut frame.handlers);
    Ok(root.into_any_element())
  }
}

impl Render for ScriptView {
  fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    let el = self
      .render_js(window, cx)
      .unwrap_or_else(|e| format!("render failed: {e}").into_any_element());
    div().size_full().child(el)
  }
}

// `JS_FreeRuntime` aborts on live handles, see the field order
impl Drop for ScriptView {
  fn drop(&mut self) {
    self.handlers.clear();
  }
}

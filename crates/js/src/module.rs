//! Native ES modules built at runtime, and the `.d.ts` that types them.

use std::{collections::HashMap, rc::Rc};

use gpui_kit::{App, Entity};
use rquickjs::{
  Ctx, Error, Function, IntoJs, JsLifetime, Module, Result, Value,
  loader::{ImportAttributes, Loader},
  module::{Declarations, Exports, ModuleDef},
};
use specta::{
  Type, Types,
  datatype::{DataType, Function as FnType},
};
use specta_typescript::{Typescript, primitives};

use crate::{
  Watches, app,
  el::jsdoc,
  host::{camel, floats},
  watch::track,
};

/// Makes one export's value in the context importing the module
type Export = Rc<dyn for<'js> Fn(&Ctx<'js>) -> Result<Value<'js>>>;

/// A host module under construction: its exports, their TypeScript declarations, and the
/// plugin's [`Watches`] its signals subscribe through.
pub struct ModuleBuilder {
  name: String,
  exports: Vec<(String, Export)>,
  /// Rendered `export` lines, in order
  decls: Vec<String>,
  types: Types,
  watches: Watches,
}

impl ModuleBuilder {
  /// The module imported as `name`
  pub fn new(name: impl Into<String>, watches: &Watches) -> Self {
    Self {
      name: name.into(),
      exports: Vec::new(),
      decls: Vec::new(),
      types: Types::default(),
      watches: watches.clone(),
    }
  }

  pub fn name(&self) -> &str {
    &self.name
  }

  /// Exports a host fn under the camelCase of its name; [`host_module!`](crate::host_module)
  /// calls this.
  pub fn func(
    &mut self,
    datatype: fn(&mut Types) -> FnType,
    export: impl for<'js> Fn(&Ctx<'js>) -> Result<Value<'js>> + 'static,
  ) -> &mut Self {
    let name = self.declare(datatype);
    self.export(name, export)
  }

  fn export(
    &mut self,
    name: String,
    export: impl for<'js> Fn(&Ctx<'js>) -> Result<Value<'js>> + 'static,
  ) -> &mut Self {
    self.exports.push((name, Rc::new(export)));
    self
  }

  /// Types a host fn without exporting it, returns its JS name.
  pub fn declare(&mut self, datatype: fn(&mut Types) -> FnType) -> String {
    let mut f = datatype(&mut self.types);
    let name = camel(&f.name);
    f.args.iter_mut().for_each(|(_, dt)| floats(dt));
    f.result.iter_mut().for_each(floats);
    // trailing nullable params may be left out (`Opt`), they read as null
    let required = f
      .args
      .iter()
      .rposition(|(_, dt)| !matches!(dt, DataType::Nullable(_)))
      .map_or(0, |i| i + 1);
    let args: Vec<String> = f
      .args
      .iter()
      .enumerate()
      .map(|(i, (arg, dt))| {
        let optional = if i < required { "" } else { "?" };
        format!("{}{optional}: {}", camel(arg), self.ts(dt))
      })
      .collect();
    let mut result = f.result.as_ref().map_or("void".into(), |dt| self.ts(dt));
    if f.asyncness {
      result = format!("Promise<{result}>");
    }
    self.decls.push(format!(
      "{}  export function {name}({}): {result};\n",
      jsdoc_block(&f.docs),
      args.join(", ")
    ));
    name
  }

  /// Exports `name` as a `Signal<R>`: calling it returns `read(cx)`, and a view that calls it
  /// while rendering renders again once `entity` changes. `name.subscribe(cb)` calls
  /// `cb(read(cx))` on every change of `entity` and returns the unsubscribe fn.
  pub fn watch<E: 'static, R>(
    &mut self,
    name: &str,
    docs: &str,
    entity: impl Fn(&App) -> Entity<E> + 'static,
    read: impl Fn(&App) -> R + 'static,
  ) -> &mut Self
  where
    R: Type + for<'js> IntoJs<'js> + 'static,
  {
    let mut dt = R::definition(&mut self.types);
    floats(&mut dt);
    let ty = self.ts(&dt);
    self.decls.push(format!(
      "{}  export const {name}: Signal<{ty}>;\n",
      jsdoc_block(docs)
    ));

    let (entity, read) = (Rc::new(entity), Rc::new(read));
    let watches = self.watches.clone();
    self.export(name.into(), move |ctx| {
      signal(ctx, &entity, &read, &watches)
    })
  }

  fn ts(&self, dt: &DataType) -> String {
    primitives::reference(&Typescript::default(), &self.types, dt).expect("host type")
  }

  /// `declare module "name" { .. }` with the types its exports use
  fn d_ts(&self) -> String {
    let mut types = self.types.clone();
    types.iter_mut(|ndt| ndt.ty.iter_mut().for_each(floats));
    let types = primitives::export(
      &Typescript::default(),
      &types,
      types.into_sorted_iter(),
      "  ",
    )
    .expect("host types")
    // specta indents fields with tabs
    .replace('\t', "  ");
    format!(
      "declare module \"{}\" {{\n{types}{}}}\n",
      self.name,
      self.decls.concat()
    )
  }
}

/// `get()`, with `get.subscribe(cb)`
fn signal<'js, E: 'static, R: for<'a> IntoJs<'a> + 'static>(
  ctx: &Ctx<'js>,
  entity: &Rc<impl Fn(&App) -> Entity<E> + 'static>,
  read: &Rc<impl Fn(&App) -> R + 'static>,
  watches: &Watches,
) -> Result<Value<'js>> {
  let (e, r) = (entity.clone(), read.clone());
  let get = Function::new(ctx.clone(), move || {
    let cx = app();
    track(&e(cx));
    r(cx)
  })?;
  let (e, r, watches) = (entity.clone(), read.clone(), watches.clone());
  let subscribe = Function::new(ctx.clone(), move |cb: Function<'js>| {
    let r = r.clone();
    watches.subscribe(&e(app()), cb, move |cx| r(cx))
  })?;
  get.set("subscribe", subscribe)?;
  Ok(get.into_value())
}

fn jsdoc_block(docs: &str) -> String {
  let lines: Vec<String> = docs.lines().map(|l| format!(" {}", l.trim())).collect();
  jsdoc(&lines.iter().map(String::as_str).collect::<Vec<_>>(), "  ")
}

/// The `.d.ts` of `modules`, with the `Signal` their watched values are.
pub fn d_ts<'a>(modules: impl IntoIterator<Item = &'a ModuleBuilder>) -> String {
  let mut out = String::from(
    r#"/** A value of the shell: call it for the current one. */
interface Signal<T> {
  (): T;
  /** Calls `cb` with every new value; returns the fn that stops it. */
  subscribe(cb: (value: T) => void): () => void;
}
"#,
  );
  for module in modules {
    out.push_str(&module.d_ts());
  }
  out
}

/// The host modules of a runtime by name, kept in its context for [`HostDef`].
pub(crate) struct HostModules(HashMap<String, Vec<(String, Export)>>);

unsafe impl<'js> JsLifetime<'js> for HostModules {
  type Changed<'to> = HostModules;
}

impl HostModules {
  pub(crate) fn new(modules: Vec<ModuleBuilder>) -> Self {
    Self(modules.into_iter().map(|m| (m.name, m.exports)).collect())
  }

  pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
    self.0.keys().map(String::as_str)
  }
}

/// Loads the [`HostModules`] of the context.
pub(crate) struct HostLoader;

impl Loader for HostLoader {
  fn load<'js>(
    &mut self,
    ctx: &Ctx<'js>,
    name: &str,
    _: Option<ImportAttributes<'js>>,
  ) -> Result<Module<'js>> {
    match ctx.userdata::<HostModules>() {
      Some(modules) if modules.0.contains_key(name) => {
        drop(modules);
        Module::declare_def::<HostDef, _>(ctx.clone(), name)
      }
      _ => Err(Error::new_loading(name)),
    }
  }
}

/// One host module, found by its name in the context's [`HostModules`]
struct HostDef;

impl HostDef {
  fn exports<'js, R>(
    ctx: &Ctx<'js>,
    module: &Module<'js>,
    f: impl FnOnce(&[(String, Export)]) -> Result<R>,
  ) -> Result<R> {
    let name: String = module.name()?;
    let modules = ctx.userdata::<HostModules>().ok_or(Error::Unknown)?;
    let exports = modules
      .0
      .get(&name)
      .ok_or_else(|| Error::new_loading(&name))?;
    f(exports)
  }
}

impl ModuleDef for HostDef {
  fn declare<'js>(decl: &Declarations<'js>) -> Result<()> {
    Self::exports(decl.module().ctx(), decl.module(), |exports| {
      exports
        .iter()
        .try_for_each(|(name, _)| decl.declare(name.as_str()).map(drop))
    })
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
    // make the values first: a value may need the context's userdata itself
    let values = Self::exports(ctx, exports.module(), |list| {
      Ok(
        list
          .iter()
          .map(|(name, make)| (name.clone(), make.clone()))
          .collect::<Vec<_>>(),
      )
    })?;
    for (name, make) in values {
      exports.export(name, make(ctx)?)?;
    }
    Ok(())
  }
}

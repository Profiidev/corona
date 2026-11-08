//! The plugin a runtime runs, reachable from host fns as the injected [`Plugin`] param.

use std::{ops::Deref, rc::Rc};

use rquickjs::{
  Ctx, Exception, JsLifetime, Result,
  function::{FromParam, ParamRequirement, ParamsAccessor},
};
use specta::{Types, datatype::DataType, function::FunctionArg};

use crate::PluginRef;

/// Kept in the context of each runtime
pub(crate) struct PluginData(pub Rc<PluginRef>);

// SAFETY: holds no JS value, so it is the same type for every `'js`
unsafe impl<'js> JsLifetime<'js> for PluginData {
  type Changed<'to> = PluginData;
}

/// The plugin that calls, injected into a host fn without taking a JS argument, like
/// [`Cx`](crate::Cx). Throws in a runtime that runs no plugin.
pub struct Plugin(pub Rc<PluginRef>);

impl Deref for Plugin {
  type Target = PluginRef;
  fn deref(&self) -> &PluginRef {
    &self.0
  }
}

impl<'js> FromParam<'js> for Plugin {
  fn param_requirement() -> ParamRequirement {
    ParamRequirement::none()
  }

  fn from_param<'a>(params: &mut ParamsAccessor<'a, 'js>) -> Result<Self> {
    plugin(params.ctx()).map(Plugin)
  }
}

impl FunctionArg for Plugin {
  fn to_datatype(_: &mut Types) -> Option<DataType> {
    None
  }
}

/// The plugin of the runtime `ctx` belongs to
pub(crate) fn plugin(ctx: &Ctx<'_>) -> Result<Rc<PluginRef>> {
  match ctx.userdata::<PluginData>() {
    Some(data) => Ok(data.0.clone()),
    None => Err(Exception::throw_message(ctx, "no plugin runs here")),
  }
}

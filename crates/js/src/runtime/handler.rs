//! Plugin functions a host module calls later, and specs with such functions in them

use std::{cell::RefCell, marker::PhantomData, rc::Rc};

use rquickjs::{Ctx, Exception, FromJs, Function, Object, Result, Value};
use serde::{Deserialize, Deserializer, de::DeserializeOwned};
use specta::{Type, Types, datatype::DataType};
use specta_typescript::{Typescript, define, primitives};

use crate::{
  Watches,
  runtime::{host::floats, runtime},
};

/// A plugin function the host calls with [`Watches::call_handler`] by id. As a host fn param
/// it is the function, typed `(arg: A) => R | void | Promise<R | void>`; in a spec that
/// [`Spec`] reads it is the id the function was registered as.
pub struct Handler<A, R> {
  pub id: u32,
  _types: PhantomData<fn(A) -> R>,
}

impl<A, R> Clone for Handler<A, R> {
  fn clone(&self) -> Self {
    *self
  }
}

impl<A, R> Copy for Handler<A, R> {}

impl<A, R> std::fmt::Debug for Handler<A, R> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "Handler({})", self.id)
  }
}

impl<A, R> Handler<A, R> {
  pub fn new(id: u32) -> Self {
    Self {
      id,
      _types: PhantomData,
    }
  }
}

impl<'de, A, R> Deserialize<'de> for Handler<A, R> {
  fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
    u32::deserialize(deserializer).map(Self::new)
  }
}

impl<'js, A, R> FromJs<'js> for Handler<A, R> {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    let f = Function::from_js(ctx, value)?;
    Ok(Self::new(runtime(ctx)?.watches.register_handler(f)))
  }
}

impl<A: Type, R: Type> Type for Handler<A, R> {
  fn definition(types: &mut Types) -> DataType {
    let mut arg = A::definition(types);
    let mut ret = R::definition(types);
    floats(&mut arg);
    floats(&mut ret);
    let ts = Typescript::default();
    // `()` is no argument
    let arg = match &arg {
      DataType::Tuple(t) if t.elements.is_empty() => String::new(),
      arg => format!(
        "arg: {}",
        primitives::reference(&ts, types, arg).expect("host type")
      ),
    };
    let ret = primitives::reference(&ts, types, &ret).expect("host type");
    DataType::Reference(define(format!(
      "(({arg}) => {ret} | void | Promise<{ret} | void>)"
    )))
  }
}

/// A spec the plugin wrote, read as `T`. JSON has no functions: each one in it is registered
/// as a handler and read as its id (see [`Handler`]), and `handlers` has them all.
pub struct Spec<T> {
  pub value: T,
  pub handlers: Vec<u32>,
}

impl<'js, T: DeserializeOwned> FromJs<'js> for Spec<T> {
  fn from_js(ctx: &Ctx<'js>, value: Value<'js>) -> Result<Self> {
    let watches = runtime(ctx)?.watches.clone();
    let sink = Rc::default();
    let text = encode(ctx, &watches, value, &sink)?;
    match serde_json::from_str(&text) {
      Ok(value) => Ok(Self {
        value,
        handlers: sink.take(),
      }),
      Err(e) => {
        sink
          .take()
          .iter()
          .for_each(|id| watches.release_handler(*id));
        Err(Exception::throw_type(ctx, &e.to_string()))
      }
    }
  }
}

impl<T: Type> Type for Spec<T> {
  fn definition(types: &mut Types) -> DataType {
    T::definition(types)
  }
}

/// `value` as JSON, with each function in it registered as a handler and written as its id,
/// which is added to `sink`
pub fn encode<'js>(
  ctx: &Ctx<'js>,
  watches: &Watches,
  value: Value<'js>,
  sink: &Rc<RefCell<Vec<u32>>>,
) -> Result<String> {
  let (watches, sink) = (watches.clone(), sink.clone());
  let replacer = Function::new(
    ctx.clone(),
    move |ctx: Ctx<'js>, _key: String, value: Value<'js>| -> Result<Value<'js>> {
      let Some(f) = value.as_function() else {
        return Ok(value);
      };
      let id = watches.register_handler(f.clone());
      sink.borrow_mut().push(id);
      Ok(Value::new_int(ctx, id as i32))
    },
  )?;
  let stringify: Function = ctx.globals().get::<_, Object>("JSON")?.get("stringify")?;
  let text: Option<String> = stringify.call((value, replacer))?;
  Ok(text.unwrap_or_else(|| "null".into()))
}

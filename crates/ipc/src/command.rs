use std::{future::Future, pin::Pin};

use anyhow::Result;
use gpui_kit::App;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

#[derive(Serialize, Deserialize)]
pub struct IpcPayload {
  pub command: String,
  pub data: Value,
}

pub trait IpcCommand {
  const COMMAND: &'static str;

  type Payload: Serialize + DeserializeOwned;
  type Response: Serialize + DeserializeOwned + 'static;

  fn handle(payload: Self::Payload, cx: &mut App) -> Result<Self::Response>;

  /// The answer, which may come later; `handle` right away by default
  fn reply(payload: Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    Box::pin(std::future::ready(Self::handle(payload, cx)))
  }
}

pub type Reply<T> = Pin<Box<dyn Future<Output = Result<T>>>>;

pub trait IpcCommandSend: IpcCommand {
  fn send(payload: Self::Payload) -> Result<Self::Response> {
    let data = serde_json::to_value(payload)?;
    let res = crate::client::send(Self::COMMAND, data)?;
    Ok(serde_json::from_value(res)?)
  }
}

impl<T: IpcCommand> IpcCommandSend for T {}

pub(crate) type Handler = Box<dyn Fn(Value, &mut App) -> Reply<Value>>;

pub(crate) fn erase<C: IpcCommand>() -> Handler {
  Box::new(|data, cx| {
    let reply = serde_json::from_value(data).map(|payload| C::reply(payload, cx));
    Box::pin(async move { Ok(serde_json::to_value(reply?.await?)?) })
  })
}

#[cfg(test)]
pub(crate) mod tests {
  use std::collections::HashMap;

  use anyhow::bail;
  use gpui_kit::{self as gpui, TestAppContext};
  use serde_json::json;

  use super::*;

  pub(crate) struct Shout;

  impl IpcCommand for Shout {
    const COMMAND: &'static str = "shout";
    type Payload = String;
    type Response = String;

    fn handle(payload: String, _: &mut App) -> Result<String> {
      if payload == "fail" {
        bail!("nope");
      }
      Ok(payload.to_uppercase())
    }
  }

  /// A response serde_json cannot turn into a value: map keys must be strings
  struct Unserializable;

  impl IpcCommand for Unserializable {
    const COMMAND: &'static str = "bad";
    type Payload = ();
    type Response = HashMap<(u8, u8), u8>;

    fn handle(_: (), _: &mut App) -> Result<Self::Response> {
      Ok(HashMap::from([((1, 2), 3)]))
    }
  }

  #[gpui::test]
  fn erased_handlers(cx: &mut TestAppContext) {
    cx.update(|cx| {
      let shout = erase::<Shout>();
      let block_on = smol::block_on::<Result<Value>>;
      assert_eq!(block_on(shout(json!("hi"), cx)).unwrap(), json!("HI"));
      // payload of the wrong type
      assert!(block_on(shout(json!(5), cx)).is_err());
      let err = block_on(shout(json!("fail"), cx)).unwrap_err();
      assert!(err.to_string().contains("nope"));
      assert!(block_on(erase::<Unserializable>()(Value::Null, cx)).is_err());
    });
  }
}

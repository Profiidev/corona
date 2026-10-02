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
  type Response: Serialize + DeserializeOwned;

  fn handle(payload: Self::Payload, cx: &mut App) -> Result<Self::Response>;
}

pub trait IpcCommandSend: IpcCommand {
  fn send(payload: Self::Payload) -> Result<Self::Response> {
    let data = serde_json::to_value(payload)?;
    let res = crate::client::send(Self::COMMAND, data)?;
    Ok(serde_json::from_value(res)?)
  }
}

impl<T: IpcCommand> IpcCommandSend for T {}

pub(crate) type Handler = Box<dyn Fn(Value, &mut App) -> Result<Value>>;

pub(crate) fn erase<C: IpcCommand>() -> Handler {
  Box::new(|data, cx| {
    let payload = serde_json::from_value(data)?;
    Ok(serde_json::to_value(C::handle(payload, cx)?)?)
  })
}

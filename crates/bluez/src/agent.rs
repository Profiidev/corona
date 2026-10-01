use anyhow::Result;
use bluez_zbus::{
  agent_manager1::AgentManager1Proxy,
  agent1::{self, Capability, Message},
};
use futures_channel::{mpsc, oneshot};
use zbus::{Connection, zvariant::ObjectPath};

const AGENT_PATH: &str = "/io/corona/bluez/agent";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairingKind {
  Confirm { passkey: u32 },
  Authorize,
  PinCode,
  Passkey,
  DisplayPasskey { passkey: u32 },
}

pub struct PairingRequest {
  pub device: String,
  pub kind: PairingKind,
  pub(crate) reply: Reply,
}

pub(crate) enum Reply {
  Accept(oneshot::Sender<bool>),
  PinCode(oneshot::Sender<Option<String>>),
  Passkey(oneshot::Sender<Option<u32>>),
  None,
}

pub(crate) enum AgentEvent {
  Request(PairingRequest),
  Cancel,
}

pub async fn register(conn: &Connection) -> Result<mpsc::Receiver<Message>> {
  let (agent, messages) = agent1::create();
  conn.object_server().at(AGENT_PATH, agent).await?;
  let path = ObjectPath::try_from(AGENT_PATH)?;
  let manager = AgentManager1Proxy::new(conn).await?;
  manager
    .register_agent(&path, Capability::KeyboardDisplay.into())
    .await?;
  manager.request_default_agent(&path).await?;
  Ok(messages)
}

pub(crate) fn event(message: Message) -> Option<AgentEvent> {
  let request = |device: zbus::zvariant::OwnedObjectPath, kind, reply| {
    Some(AgentEvent::Request(PairingRequest {
      device: device.to_string(),
      kind,
      reply,
    }))
  };
  match message {
    Message::RequestConfirmation {
      device,
      passkey,
      response,
    } => request(
      device,
      PairingKind::Confirm { passkey },
      Reply::Accept(response),
    ),
    Message::RequestAuthorization { device, response } => {
      request(device, PairingKind::Authorize, Reply::Accept(response))
    }
    Message::RequestPinCode { device, response } => {
      request(device, PairingKind::PinCode, Reply::PinCode(response))
    }
    Message::RequestPasskey { device, response } => {
      request(device, PairingKind::Passkey, Reply::Passkey(response))
    }
    Message::DisplayPasskey {
      device, passkey, ..
    } => request(device, PairingKind::DisplayPasskey { passkey }, Reply::None),
    Message::DisplayPinCode { device, pincode } => pincode
      .parse()
      .ok()
      .and_then(|passkey| request(device, PairingKind::DisplayPasskey { passkey }, Reply::None)),
    Message::Cancel | Message::Release => Some(AgentEvent::Cancel),
    Message::AuthorizeService { .. } => None,
  }
}

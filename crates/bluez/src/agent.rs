use anyhow::Result;
use bluez_zbus::{
  agent_manager1::AgentManager1Proxy,
  agent1::{Capability, Message},
};
use futures_channel::{mpsc, oneshot};
use zbus::{
  Connection,
  zvariant::{ObjectPath, OwnedObjectPath},
};

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

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.bluez.Error")]
enum AgentError {
  #[zbus(error)]
  ZBus(zbus::Error),
  Rejected(String),
  Canceled(String),
}

/// `org.bluez.Agent1`. Every method takes `&self`, so a `Cancel` reaches the shell while a
/// prompt still waits for its answer, which an agent taking `&mut self` would hold off.
struct Agent {
  messages: mpsc::UnboundedSender<Message>,
}

impl Agent {
  fn send(&self, message: Message) -> Result<(), AgentError> {
    self
      .messages
      .unbounded_send(message)
      .map_err(|_| AgentError::Canceled("the shell is gone".into()))
  }

  async fn ask<T>(
    &self,
    message: impl FnOnce(oneshot::Sender<T>) -> Message,
  ) -> Result<T, AgentError> {
    let (response, answer) = oneshot::channel();
    self.send(message(response))?;
    answer
      .await
      .map_err(|_| AgentError::Canceled("the prompt was closed".into()))
  }
}

#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
  fn release(&self) -> Result<(), AgentError> {
    self.send(Message::Release)
  }

  async fn request_pin_code(&self, device: OwnedObjectPath) -> Result<String, AgentError> {
    self
      .ask(|response| Message::RequestPinCode { device, response })
      .await?
      .ok_or_else(|| AgentError::Rejected("no PIN".into()))
  }

  fn display_pin_code(&self, device: OwnedObjectPath, pincode: String) -> Result<(), AgentError> {
    self.send(Message::DisplayPinCode { device, pincode })
  }

  async fn request_passkey(&self, device: OwnedObjectPath) -> Result<u32, AgentError> {
    self
      .ask(|response| Message::RequestPasskey { device, response })
      .await?
      .ok_or_else(|| AgentError::Rejected("no passkey".into()))
  }

  fn display_passkey(
    &self,
    device: OwnedObjectPath,
    passkey: u32,
    entered: u16,
  ) -> Result<(), AgentError> {
    self.send(Message::DisplayPasskey {
      device,
      passkey,
      entered,
    })
  }

  async fn request_confirmation(
    &self,
    device: OwnedObjectPath,
    passkey: u32,
  ) -> Result<(), AgentError> {
    let accepted = self
      .ask(|response| Message::RequestConfirmation {
        device,
        passkey,
        response,
      })
      .await?;
    accepted
      .then_some(())
      .ok_or_else(|| AgentError::Rejected("not confirmed".into()))
  }

  async fn request_authorization(&self, device: OwnedObjectPath) -> Result<(), AgentError> {
    let accepted = self
      .ask(|response| Message::RequestAuthorization { device, response })
      .await?;
    accepted
      .then_some(())
      .ok_or_else(|| AgentError::Rejected("not authorized".into()))
  }

  /// A device that is not trusted wants a service: asked like a pairing, never assumed
  async fn authorize_service(
    &self,
    device: OwnedObjectPath,
    _uuid: String,
  ) -> Result<(), AgentError> {
    self.request_authorization(device).await
  }

  fn cancel(&self) -> Result<(), AgentError> {
    self.send(Message::Cancel)
  }
}

pub async fn register(conn: &Connection) -> Result<mpsc::UnboundedReceiver<Message>> {
  let (sender, messages) = mpsc::unbounded();
  conn
    .object_server()
    .at(AGENT_PATH, Agent { messages: sender })
    .await?;
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

#[cfg(test)]
mod tests {
  use zbus::zvariant::OwnedObjectPath;

  use super::*;

  fn dev() -> OwnedObjectPath {
    OwnedObjectPath::try_from("/org/bluez/hci0/dev_AA").unwrap()
  }

  fn request(message: Message) -> PairingRequest {
    match event(message) {
      Some(AgentEvent::Request(request)) => request,
      _ => panic!("not a request"),
    }
  }

  #[test]
  fn messages_become_requests() {
    let (response, mut confirmed) = oneshot::channel();
    let confirm = request(Message::RequestConfirmation {
      device: dev(),
      passkey: 123456,
      response,
    });
    assert_eq!(confirm.device, "/org/bluez/hci0/dev_AA");
    assert_eq!(confirm.kind, PairingKind::Confirm { passkey: 123456 });
    let Reply::Accept(reply) = confirm.reply else {
      panic!()
    };
    reply.send(true).unwrap();
    assert_eq!(confirmed.try_recv().unwrap(), Some(true));

    let (response, _) = oneshot::channel();
    let authorize = request(Message::RequestAuthorization {
      device: dev(),
      response,
    });
    assert_eq!(authorize.kind, PairingKind::Authorize);
    assert!(matches!(authorize.reply, Reply::Accept(_)));

    let (response, _) = oneshot::channel();
    let pin = request(Message::RequestPinCode {
      device: dev(),
      response,
    });
    assert_eq!(pin.kind, PairingKind::PinCode);
    assert!(matches!(pin.reply, Reply::PinCode(_)));

    let (response, _) = oneshot::channel();
    let passkey = request(Message::RequestPasskey {
      device: dev(),
      response,
    });
    assert_eq!(passkey.kind, PairingKind::Passkey);
    assert!(matches!(passkey.reply, Reply::Passkey(_)));

    let shown = request(Message::DisplayPasskey {
      device: dev(),
      passkey: 42,
      entered: 3,
    });
    assert_eq!(shown.kind, PairingKind::DisplayPasskey { passkey: 42 });
    assert!(matches!(shown.reply, Reply::None));

    let pin_shown = request(Message::DisplayPinCode {
      device: dev(),
      pincode: "001234".into(),
    });
    assert_eq!(
      pin_shown.kind,
      PairingKind::DisplayPasskey { passkey: 1234 }
    );

    let pin_zeros = request(Message::DisplayPinCode {
      device: dev(),
      pincode: "0000".into(),
    });
    assert_eq!(pin_zeros.kind, PairingKind::DisplayPasskey { passkey: 0 });
  }

  #[test]
  fn other_messages() {
    assert!(matches!(event(Message::Cancel), Some(AgentEvent::Cancel)));
    assert!(matches!(event(Message::Release), Some(AgentEvent::Cancel)));
    assert!(
      event(Message::AuthorizeService {
        device: dev(),
        uuid: "0000110b".into(),
      })
      .is_none()
    );
    // not a number: alphanumeric PIN code fails to parse into u32, dropping message so pairing hangs
    assert!(
      event(Message::DisplayPinCode {
        device: dev(),
        pincode: "abcd".into(),
      })
      .is_none()
    );
    assert!(
      event(Message::DisplayPinCode {
        device: dev(),
        pincode: "ABCD".into(),
      })
      .is_none()
    );
  }
}

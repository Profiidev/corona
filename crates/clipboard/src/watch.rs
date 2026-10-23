//! Follows the clipboard through the data control protocol, `ext` or else `wlr`

use std::{
  collections::HashMap,
  io::Read,
  os::fd::{AsFd, OwnedFd},
};

use anyhow::{Context, Result, bail};
use rustix::pipe::{PipeFlags, pipe_with};
use wayland_client::{
  Connection, Dispatch, Proxy, QueueHandle,
  backend::ObjectId,
  event_created_child,
  globals::{GlobalListContents, registry_queue_init},
  protocol::{wl_registry, wl_seat::WlSeat},
};

/// What was copied: the bytes of the chosen type
pub struct Captured {
  pub mime: String,
  pub bytes: Vec<u8>,
}

/// Larger selections are not read
const MAX_BYTES: u64 = 64 * 1024 * 1024;

const TEXT: [&str; 5] = [
  "text/plain;charset=utf-8",
  "text/plain",
  "UTF8_STRING",
  "STRING",
  "TEXT",
];

/// The type to keep from what the selection offers; password managers mark theirs
pub fn choose(mimes: &[String]) -> Option<&str> {
  if mimes
    .iter()
    .any(|m| m == "x-kde-passwordManagerHint" || m.starts_with("x-kde-passwordManagerHint"))
  {
    return None;
  }
  let text = TEXT
    .iter()
    .find_map(|t| mimes.iter().find(|m| m.as_str() == *t));
  let image = ["image/png", "image/jpeg", "image/webp", "image/gif"]
    .iter()
    .find_map(|t| mimes.iter().find(|m| m.as_str() == *t));
  text.or(image).map(String::as_str)
}

struct State {
  offers: HashMap<ObjectId, Vec<String>>,
  tx: flume::Sender<Captured>,
  conn: Connection,
}

impl State {
  /// Reads the selection on its own thread, a slow or stuck source must not stop the watch
  fn read(&self, mime: &str, receive: impl FnOnce(&str, &OwnedFd)) -> Result<()> {
    let (read, write) = pipe_with(PipeFlags::CLOEXEC)?;
    receive(mime, &write);
    self.conn.flush()?;
    drop(write);
    let (tx, mime) = (self.tx.clone(), mime.to_string());
    std::thread::spawn(move || {
      let mut bytes = Vec::new();
      let file = std::fs::File::from(read);
      if file.take(MAX_BYTES + 1).read_to_end(&mut bytes).is_ok()
        && !bytes.is_empty()
        && (bytes.len() as u64) <= MAX_BYTES
      {
        let _ = tx.send(Captured { mime, bytes });
      }
    });
    Ok(())
  }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
  fn event(
    _: &mut Self,
    _: &wl_registry::WlRegistry,
    _: wl_registry::Event,
    _: &GlobalListContents,
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
  }
}

impl Dispatch<WlSeat, ()> for State {
  fn event(
    _: &mut Self,
    _: &WlSeat,
    _: <WlSeat as Proxy>::Event,
    _: &(),
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
  }
}

/// The same handling for both protocols, which differ only in names
macro_rules! data_control {
  ($manager:ty, $device_mod:ident, $device:ty, $offer_mod:ident, $offer:ty) => {
    impl Dispatch<$manager, ()> for State {
      fn event(
        _: &mut Self,
        _: &$manager,
        _: <$manager as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
      ) {
      }
    }

    impl Dispatch<$device, ()> for State {
      fn event(
        state: &mut Self,
        _: &$device,
        event: $device_mod::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
      ) {
        match event {
          $device_mod::Event::DataOffer { id } => {
            state.offers.insert(id.id(), Vec::new());
          }
          $device_mod::Event::Selection { id: Some(offer) } => {
            let mimes = state.offers.remove(&offer.id()).unwrap_or_default();
            if let Some(mime) = choose(&mimes) {
              let read = state.read(mime, |mime, fd| offer.receive(mime.to_string(), fd.as_fd()));
              if let Err(e) = read {
                tracing::warn!("clipboard: {e:#}");
              }
            }
            offer.destroy();
          }
          $device_mod::Event::PrimarySelection { id: Some(offer) } => {
            state.offers.remove(&offer.id());
            offer.destroy();
          }
          _ => {}
        }
      }

      event_created_child!(State, $device, [
        $device_mod::EVT_DATA_OFFER_OPCODE => ($offer, ()),
      ]);
    }

    impl Dispatch<$offer, ()> for State {
      fn event(
        state: &mut Self,
        offer: &$offer,
        event: $offer_mod::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
      ) {
        if let $offer_mod::Event::Offer { mime_type } = event {
          state.offers.entry(offer.id()).or_default().push(mime_type);
        }
      }
    }
  };
}

use wayland_protocols::ext::data_control::v1::client::{
  ext_data_control_device_v1::{self, ExtDataControlDeviceV1},
  ext_data_control_manager_v1::ExtDataControlManagerV1,
  ext_data_control_offer_v1::{self, ExtDataControlOfferV1},
};
use wayland_protocols_wlr::data_control::v1::client::{
  zwlr_data_control_device_v1::{self, ZwlrDataControlDeviceV1},
  zwlr_data_control_manager_v1::ZwlrDataControlManagerV1,
  zwlr_data_control_offer_v1::{self, ZwlrDataControlOfferV1},
};

data_control!(
  ExtDataControlManagerV1,
  ext_data_control_device_v1,
  ExtDataControlDeviceV1,
  ext_data_control_offer_v1,
  ExtDataControlOfferV1
);
data_control!(
  ZwlrDataControlManagerV1,
  zwlr_data_control_device_v1,
  ZwlrDataControlDeviceV1,
  zwlr_data_control_offer_v1,
  ZwlrDataControlOfferV1
);

/// Sends every new selection to `tx` until the connection ends; blocks, run it on a thread
pub fn run(tx: flume::Sender<Captured>) -> Result<()> {
  let conn = Connection::connect_to_env().context("no Wayland display")?;
  let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
  let qh = queue.handle();
  let seat: WlSeat = globals.bind(&qh, 1..=8, ())?;
  let mut state = State {
    offers: HashMap::new(),
    tx,
    conn: conn.clone(),
  };
  // keep the device alive for the whole loop
  let _device: Box<dyn std::any::Any> =
    match globals.bind::<ExtDataControlManagerV1, _, _>(&qh, 1..=1, ()) {
      Ok(manager) => Box::new(manager.get_data_device(&seat, &qh, ())),
      Err(_) => match globals.bind::<ZwlrDataControlManagerV1, _, _>(&qh, 1..=2, ()) {
        Ok(manager) => Box::new(manager.get_data_device(&seat, &qh, ())),
        Err(_) => bail!("the compositor has no data control protocol"),
      },
    };
  loop {
    queue.blocking_dispatch(&mut state)?;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn mimes(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
  }

  #[test]
  fn picks_text_then_images() {
    assert_eq!(
      choose(&mimes(&["text/html", "text/plain", "UTF8_STRING"])),
      Some("text/plain")
    );
    assert_eq!(
      choose(&mimes(&["image/png", "text/html"])),
      Some("image/png")
    );
    assert_eq!(choose(&mimes(&["text/html"])), None);
    // password managers mark what nobody should keep
    assert_eq!(
      choose(&mimes(&["text/plain", "x-kde-passwordManagerHint"])),
      None
    );
  }
}

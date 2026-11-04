//! Follows the clipboard through the data control protocol, `ext` or else `wlr`

use std::{
  collections::HashMap,
  io::Read,
  os::fd::{AsFd, OwnedFd},
  sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
  },
  time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use rustix::event::{PollFd, PollFlags, poll};
use rustix::io::Errno;
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

/// Largest text entry kept, in bytes
pub(crate) const MAX_TEXT: usize = 1024 * 1024;
/// Hard cap on any image read, whatever the setting
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// A source that has not finished writing by then is dropped
const READ_TIMEOUT: Duration = Duration::from_secs(3);

/// What the history takes right now, so the watcher does not read what would be dropped
#[derive(Default)]
pub struct Wanted {
  enabled: AtomicBool,
  images: AtomicBool,
  image_bytes: AtomicU64,
}

impl Wanted {
  pub fn set(&self, enabled: bool, images: bool, image_bytes: u64) {
    self.enabled.store(enabled, Ordering::Relaxed);
    self.images.store(images, Ordering::Relaxed);
    self.image_bytes.store(image_bytes, Ordering::Relaxed);
  }
}

/// Everything the source writes up to `limit + 1`, None when it is not done by `deadline`
fn read_until(fd: OwnedFd, deadline: Instant, limit: u64) -> Option<Vec<u8>> {
  let mut bytes = Vec::new();
  let mut file = std::fs::File::from(fd);
  let mut chunk = [0u8; 64 * 1024];
  while (bytes.len() as u64) <= limit {
    let left = deadline.checked_duration_since(Instant::now())?;
    let mut fds = [PollFd::new(&file, PollFlags::IN)];
    let timeout = rustix::event::Timespec::try_from(left).ok()?;
    match poll(&mut fds, Some(&timeout)) {
      Ok(0) => return None,
      Ok(_) => {}
      Err(Errno::INTR) => continue,
      Err(_) => return None,
    }
    match file.read(&mut chunk) {
      Ok(0) => break,
      Ok(n) => bytes.extend_from_slice(&chunk[..n]),
      Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
      Err(_) => return None,
    }
  }
  Some(bytes)
}

const TEXT: [&str; 5] = [
  "text/plain;charset=utf-8",
  "text/plain",
  "UTF8_STRING",
  "STRING",
  "TEXT",
];

/// The types to read from what the selection offers: text, then an image. Password
/// managers mark what nobody should keep. Images only when `images`.
pub fn choose(mimes: &[String], images: bool) -> Vec<&str> {
  if mimes
    .iter()
    .any(|m| m.starts_with("x-kde-passwordManagerHint"))
  {
    return Vec::new();
  }
  let text = TEXT
    .iter()
    .find_map(|t| mimes.iter().find(|m| m.as_str() == *t));
  let image = ["image/png", "image/jpeg", "image/webp", "image/gif"]
    .iter()
    .filter(|_| images)
    .find_map(|t| mimes.iter().find(|m| m.as_str() == *t));
  text.into_iter().chain(image).map(String::as_str).collect()
}

/// Real text wins; an image when the text is empty, as apps that copy an image often
/// offer text types too
fn pick(read: Vec<(String, Vec<u8>)>) -> Option<Captured> {
  let text = |bytes: &[u8]| std::str::from_utf8(bytes).is_ok_and(|t| !t.trim().is_empty());
  read
    .into_iter()
    .find(|(mime, bytes)| !bytes.is_empty() && (mime.starts_with("image/") || text(bytes)))
    .map(|(mime, bytes)| Captured { mime, bytes })
}

struct State {
  offers: HashMap<ObjectId, Vec<String>>,
  tx: flume::Sender<Captured>,
  conn: Connection,
  wanted: Arc<Wanted>,
}

impl State {
  /// Reads the selection on its own thread, a slow or stuck source must not stop the watch
  fn read(&self, mimes: &[&str], receive: impl Fn(&str, &OwnedFd)) -> Result<()> {
    let mut pipes = Vec::new();
    for mime in mimes {
      let (read, write) = pipe_with(PipeFlags::CLOEXEC)?;
      receive(mime, &write);
      pipes.push((mime.to_string(), read, write));
    }
    self.conn.flush()?;
    let pipes: Vec<(String, OwnedFd)> = pipes
      .into_iter()
      .map(|(mime, read, write)| {
        drop(write);
        (mime, read)
      })
      .collect();
    let tx = self.tx.clone();
    let deadline = Instant::now() + READ_TIMEOUT;
    let image_bytes = self.wanted.image_bytes.load(Ordering::Relaxed);
    std::thread::spawn(move || {
      let read = pipes
        .into_iter()
        .filter_map(|(mime, fd)| {
          let limit = match mime.starts_with("image/") {
            true => image_bytes.min(MAX_BYTES),
            false => MAX_TEXT as u64,
          };
          let bytes = read_until(fd, deadline, limit)?;
          ((bytes.len() as u64) <= limit).then_some((mime, bytes))
        })
        .collect();
      if let Some(captured) = pick(read) {
        let _ = tx.send(captured);
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
            let wanted = match state.wanted.enabled.load(Ordering::Relaxed) {
              true => choose(&mimes, state.wanted.images.load(Ordering::Relaxed)),
              false => Vec::new(),
            };
            if !wanted.is_empty() {
              let read = state.read(&wanted, |mime, fd| offer.receive(mime.to_string(), fd.as_fd()));
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
pub fn run(tx: flume::Sender<Captured>, wanted: Arc<Wanted>) -> Result<()> {
  let conn = Connection::connect_to_env().context("no Wayland display")?;
  let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
  let qh = queue.handle();
  let seat: WlSeat = globals.bind(&qh, 1..=8, ())?;
  let mut state = State {
    offers: HashMap::new(),
    tx,
    conn: conn.clone(),
    wanted,
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
  fn reads_text_and_images() {
    assert_eq!(
      choose(&mimes(&["text/html", "text/plain", "UTF8_STRING"]), true),
      ["text/plain"]
    );
    assert_eq!(
      choose(&mimes(&["image/png", "text/html"]), true),
      ["image/png"]
    );
    assert_eq!(
      choose(&mimes(&["text/plain", "image/png"]), true),
      ["text/plain", "image/png"]
    );
    // images off: only the text
    assert_eq!(
      choose(&mimes(&["text/plain", "image/png"]), false),
      ["text/plain"]
    );
    assert!(choose(&mimes(&["text/html"]), true).is_empty());
    // password managers mark what nobody should keep
    assert!(choose(&mimes(&["text/plain", "x-kde-passwordManagerHint"]), true).is_empty());
  }

  #[test]
  fn empty_text_gives_way_to_the_image() {
    let read = |pairs: &[(&str, &[u8])]| {
      pick(
        pairs
          .iter()
          .map(|(m, b)| (m.to_string(), b.to_vec()))
          .collect(),
      )
      .map(|c| c.mime)
    };
    assert_eq!(
      read(&[("text/plain", b""), ("image/png", b"png")]).as_deref(),
      Some("image/png")
    );
    assert_eq!(
      read(&[("text/plain", b"  "), ("image/png", b"png")]).as_deref(),
      Some("image/png")
    );
    assert_eq!(
      read(&[("text/plain", b"hi"), ("image/png", b"png")]).as_deref(),
      Some("text/plain")
    );
    assert_eq!(read(&[("text/plain", b"")]), None);
  }

  fn pipe_with_bytes(n: usize, keep_open: bool) -> OwnedFd {
    let (read, write) = pipe_with(PipeFlags::CLOEXEC).unwrap();
    std::thread::spawn(move || {
      let mut file = std::fs::File::from(write);
      let _ = std::io::Write::write_all(&mut file, &vec![0u8; n]);
      if keep_open {
        std::thread::sleep(Duration::from_millis(500));
      }
    });
    read
  }

  #[test]
  fn read_until_caps_and_times_out() {
    let soon = || Instant::now() + Duration::from_secs(2);
    let exact = read_until(pipe_with_bytes(100, false), soon(), 100).unwrap();
    assert_eq!(exact.len(), 100);
    let over = read_until(pipe_with_bytes(110, false), soon(), 100).unwrap();
    assert!(over.len() > 100);
    let stuck = Instant::now() + Duration::from_millis(50);
    assert!(read_until(pipe_with_bytes(10, true), stuck, 100).is_none());
  }
}

use std::{
  collections::HashMap,
  ffi::OsString,
  hash::{DefaultHasher, Hash, Hasher},
  os::unix::ffi::OsStringExt,
  path::PathBuf,
  sync::atomic::{AtomicU32, Ordering},
  time::SystemTime,
};

use anyhow::{Context, Result, ensure};
use image::RgbaImage;
use zbus::{interface, object_server::SignalEmitter, zvariant::OwnedValue};

use crate::state::{self, Notification, NotificationImage, Urgency};

pub(crate) const PATH: &str = "/org/freedesktop/Notifications";
pub(crate) const NAME: &str = "org.freedesktop.Notifications";

#[derive(Clone, Copy)]
#[repr(u32)]
pub(crate) enum CloseReason {
  Dismissed = 2,
  Closed = 3,
}

pub(crate) enum Event {
  Notify(Box<Notification>),
  Close(u32),
}

pub(crate) struct Server {
  pub events: flume::Sender<Event>,
  pub next_id: AtomicU32,
}

impl Server {
  /// counts up from 1, skipping 0 which the spec reserves for "no notification"
  fn fresh_id(&self) -> u32 {
    let next = |id: u32| Some(id.checked_add(1).unwrap_or(1));
    self
      .next_id
      .try_update(Ordering::Relaxed, Ordering::Relaxed, next)
      .unwrap_or(1)
  }
}

/// The spec says byte, plenty of clients send another integer type
fn urgency(hints: &HashMap<String, OwnedValue>) -> Option<i64> {
  hint::<u8>(hints, "urgency")
    .map(i64::from)
    .or_else(|| hint::<i32>(hints, "urgency").map(i64::from))
    .or_else(|| hint::<u32>(hints, "urgency").map(i64::from))
    .or_else(|| hint::<i16>(hints, "urgency").map(i64::from))
    .or_else(|| hint::<u16>(hints, "urgency").map(i64::from))
    .or_else(|| hint::<i64>(hints, "urgency"))
    .or_else(|| hint::<u64>(hints, "urgency").and_then(|u| i64::try_from(u).ok()))
}

fn hint<T: TryFrom<OwnedValue>>(hints: &HashMap<String, OwnedValue>, key: &str) -> Option<T> {
  T::try_from(hints.get(key)?.try_clone().ok()?).ok()
}

/// `(iiibiiay)`: width, height, rowstride, has alpha, bits per sample, channels, pixels
type ImageData = (i32, i32, i32, bool, i32, i32, Vec<u8>);

/// The spec's priority: raw image data (under any of its three names), then
/// `image-path`, then the `app_icon` argument
fn image(hints: &HashMap<String, OwnedValue>, app_icon: &str) -> Option<NotificationImage> {
  let data = ["image-data", "image_data", "icon_data"]
    .iter()
    .find_map(|key| hint::<ImageData>(hints, key));
  if let Some(data) = data {
    match rgba(data).and_then(|image| image_file(&image)) {
      Ok(path) => return Some(NotificationImage::Path(path)),
      Err(e) => tracing::warn!("ignoring notification image data: {e:?}"),
    }
  }
  hint::<String>(hints, "image-path")
    .and_then(|path| icon(&path))
    .or_else(|| icon(app_icon))
}

/// a `file://` uri, an absolute path or a theme icon name
fn icon(text: &str) -> Option<NotificationImage> {
  if let Some(path) = text.strip_prefix("file://") {
    let path = path.strip_prefix("localhost").unwrap_or(path);
    return Some(NotificationImage::Path(PathBuf::from(OsString::from_vec(
      percent_decode(path),
    ))));
  }
  match text {
    "" => None,
    path if path.starts_with('/') => Some(NotificationImage::Path(path.into())),
    name => Some(NotificationImage::Name(name.into())),
  }
}

fn rgba((width, height, rowstride, _, bits, channels, data): ImageData) -> Result<RgbaImage> {
  ensure!(bits == 8, "{bits} bits per sample");
  ensure!(matches!(channels, 3 | 4), "{channels} channels");
  let (w, h, stride, channels) = (
    usize::try_from(width)?,
    usize::try_from(height)?,
    usize::try_from(rowstride)?,
    channels as usize,
  );
  ensure!(w > 0 && h > 0 && stride >= w * channels, "bad image size");
  // the last row may be cut down to its pixels
  let needed = stride * (h - 1) + w * channels;
  ensure!(
    data.len() >= needed,
    "{} bytes, {needed} needed",
    data.len()
  );
  let mut pixels = Vec::with_capacity(w * h * 4);
  for row in data.chunks(stride).take(h) {
    for px in row[..w * channels].chunks_exact(channels) {
      pixels.extend_from_slice(&px[..3]);
      pixels.push(px.get(3).copied().unwrap_or(u8::MAX));
    }
  }
  RgbaImage::from_raw(width as u32, height as u32, pixels).context("image buffer")
}

/// saved once per distinct picture, apps resend the same avatar a lot
fn image_file(image: &RgbaImage) -> Result<PathBuf> {
  let mut hasher = DefaultHasher::new();
  image.dimensions().hash(&mut hasher);
  image.as_raw().hash(&mut hasher);
  let path = cache_dir().join(format!("{:016x}.png", hasher.finish()));
  if !path.exists() {
    std::fs::create_dir_all(cache_dir())?;
    image.save(&path)?;
  }
  Ok(path)
}

fn cache_dir() -> PathBuf {
  dirs::runtime_dir()
    .unwrap_or_else(std::env::temp_dir)
    .join("corona")
    .join("notifications")
}

/// `%20` and friends as their bytes; a stray `%` stays as it is
fn percent_decode(text: &str) -> Vec<u8> {
  let bytes = text.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    let hex = bytes
      .get(i + 1..i + 3)
      .and_then(|h| std::str::from_utf8(h).ok())
      .and_then(|h| u8::from_str_radix(h, 16).ok());
    match (bytes[i], hex) {
      (b'%', Some(byte)) => {
        out.push(byte);
        i += 3;
      }
      (byte, _) => {
        out.push(byte);
        i += 1;
      }
    }
  }
  out
}

#[interface(name = "org.freedesktop.Notifications")]
impl Server {
  fn get_capabilities(&self) -> Vec<&'static str> {
    vec![
      "body",
      "actions",
      "persistence",
      "icon-static",
      "body-markup",
      "body-hyperlinks",
      "inline-reply",
    ]
  }

  #[allow(clippy::too_many_arguments)]
  fn notify(
    &self,
    app_name: String,
    replaces_id: u32,
    app_icon: String,
    summary: String,
    body: String,
    actions: Vec<String>,
    hints: HashMap<String, OwnedValue>,
    expire_timeout: i32,
  ) -> u32 {
    // only an id this server handed out is replaced, any other one gets a new id
    let issued = |id: u32| id != 0 && id < self.next_id.load(Ordering::Relaxed);
    let id = match replaces_id {
      id if issued(id) => id,
      _ => self.fresh_id(),
    };
    let urgency = match urgency(&hints) {
      Some(0) => Urgency::Low,
      Some(2) => Urgency::Critical,
      _ => Urgency::Normal,
    };
    let notification = Notification {
      id,
      app_name,
      image: image(&hints, &app_icon),
      app_icon,
      summary,
      body,
      actions: state::actions(actions),
      urgency,
      desktop_entry: hint(&hints, "desktop-entry"),
      reply_placeholder: hint(&hints, "x-kde-reply-placeholder-text"),
      resident: hint(&hints, "resident").unwrap_or(false),
      expire_timeout,
      time: SystemTime::now(),
      read: false,
    };
    let _ = self.events.send(Event::Notify(Box::new(notification)));
    id
  }

  async fn close_notification(
    &self,
    id: u32,
    #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
  ) -> zbus::fdo::Result<()> {
    let _ = self.events.send(Event::Close(id));
    Self::notification_closed(&emitter, id, CloseReason::Closed as u32).await?;
    Ok(())
  }

  fn get_server_information(&self) -> (&'static str, &'static str, &'static str, &'static str) {
    ("corona", "corona", env!("CARGO_PKG_VERSION"), "1.2")
  }

  #[zbus(signal)]
  pub(crate) async fn notification_closed(
    emitter: &SignalEmitter<'_>,
    id: u32,
    reason: u32,
  ) -> zbus::Result<()>;

  #[zbus(signal)]
  pub(crate) async fn action_invoked(
    emitter: &SignalEmitter<'_>,
    id: u32,
    action_key: &str,
  ) -> zbus::Result<()>;

  #[zbus(signal)]
  pub(crate) async fn notification_replied(
    emitter: &SignalEmitter<'_>,
    id: u32,
    text: &str,
  ) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
  use zbus::zvariant::Value;

  use super::*;

  fn server(next_id: u32) -> (Server, flume::Receiver<Event>) {
    let (events, received) = flume::unbounded();
    (
      Server {
        events,
        next_id: AtomicU32::new(next_id),
      },
      received,
    )
  }

  fn hints(pairs: Vec<(&str, Value<'_>)>) -> HashMap<String, OwnedValue> {
    pairs
      .into_iter()
      .map(|(k, v)| (k.to_string(), v.try_to_owned().unwrap()))
      .collect()
  }

  fn notify(server: &Server, replaces_id: u32, hints: HashMap<String, OwnedValue>) -> u32 {
    server.notify(
      "app".into(),
      replaces_id,
      "icon".into(),
      "summary".into(),
      "body".into(),
      vec!["default".into(), "Open".into()],
      hints,
      -1,
    )
  }

  fn received(rx: &flume::Receiver<Event>) -> Notification {
    match rx.try_recv().unwrap() {
      Event::Notify(n) => *n,
      Event::Close(id) => panic!("closed {id}"),
    }
  }

  #[test]
  fn ids_count_up_unless_replacing() {
    let (server, rx) = server(1);
    assert_eq!(notify(&server, 0, HashMap::new()), 1);
    assert_eq!(notify(&server, 0, HashMap::new()), 2);
    assert_eq!(notify(&server, 1, HashMap::new()), 1);
    assert_eq!(notify(&server, 0, HashMap::new()), 3);
    assert_eq!(rx.len(), 4);
    let first = received(&rx);
    assert_eq!(
      (
        first.app_name.as_str(),
        first.app_icon.as_str(),
        first.summary.as_str(),
        first.body.as_str()
      ),
      ("app", "icon", "summary", "body")
    );
    assert_eq!(first.actions[0].label, "Open");
    assert!(!first.read && !first.resident);
  }

  #[test]
  fn hint_values() {
    let (server, rx) = server(1);
    for (value, urgency) in [
      (0u8, Urgency::Low),
      (1, Urgency::Normal),
      (2, Urgency::Critical),
      (7, Urgency::Normal),
    ] {
      notify(&server, 0, hints(vec![("urgency", value.into())]));
      assert_eq!(received(&rx).urgency, urgency);
    }
    notify(
      &server,
      0,
      hints(vec![
        ("desktop-entry", "firefox".into()),
        ("resident", true.into()),
      ]),
    );
    let n = received(&rx);
    assert_eq!(
      (n.desktop_entry.as_deref(), n.resident),
      (Some("firefox"), true)
    );
    // mistyped hints are ignored
    notify(
      &server,
      0,
      hints(vec![
        ("desktop-entry", 5u32.into()),
        ("resident", "yes".into()),
      ]),
    );
    let n = received(&rx);
    assert_eq!((n.desktop_entry, n.resident), (None, false));
  }

  #[test]
  fn urgency_accepts_any_integer() {
    let (server, rx) = server(1);
    let values: Vec<Value<'_>> = vec![
      2i32.into(),
      2u32.into(),
      2i16.into(),
      2u16.into(),
      2i64.into(),
      2u64.into(),
    ];
    for value in values {
      notify(&server, 0, hints(vec![("urgency", value)]));
      assert_eq!(received(&rx).urgency, Urgency::Critical);
    }
    notify(&server, 0, hints(vec![("urgency", 0i32.into())]));
    assert_eq!(received(&rx).urgency, Urgency::Low);
    notify(&server, 0, hints(vec![("urgency", "2".into())]));
    assert_eq!(received(&rx).urgency, Urgency::Normal);
  }

  #[test]
  fn unknown_replace_ids_get_a_fresh_id() {
    let (server, _rx) = server(1);
    let foreign = notify(&server, 2, HashMap::new());
    let fresh = notify(&server, 0, HashMap::new());
    let next = notify(&server, 0, HashMap::new());
    assert!(
      foreign != fresh && foreign != next,
      "ids {foreign} {fresh} {next}"
    );
  }

  #[test]
  fn ids_never_wrap_to_zero() {
    let (server, _rx) = server(u32::MAX);
    assert_eq!(notify(&server, 0, HashMap::new()), u32::MAX);
    assert_eq!(notify(&server, 0, HashMap::new()), 1);
    assert_eq!(notify(&server, 0, HashMap::new()), 2);
  }

  #[test]
  fn id_wraparound_and_replaces_id_across_boundary() {
    let (server, _rx) = server(u32::MAX);
    let id_max = notify(&server, 0, HashMap::new());
    assert_eq!(id_max, u32::MAX);
    let id_1 = notify(&server, 0, HashMap::new());
    assert_eq!(id_1, 1);
    let id_2 = notify(&server, 0, HashMap::new());
    assert_eq!(id_2, 2);

    // After wrapping around, next_id is 3.
    // replaces_id for 1 succeeds because 1 < 3.
    let rep_1 = notify(&server, 1, HashMap::new());
    assert_eq!(rep_1, 1);

    // replaces_id for u32::MAX fails because u32::MAX < 3 is false!
    // So it gets a fresh ID (3).
    let rep_max = notify(&server, u32::MAX, HashMap::new());
    assert_eq!(rep_max, 3);
  }

  #[test]
  fn replaces_id_for_issued_id_even_if_closed() {
    let (server, rx) = server(1);
    let id1 = notify(&server, 0, HashMap::new());
    assert_eq!(id1, 1);
    let _ = rx.drain();
    // Replacing id 1 is accepted by server because 1 < next_id (2)
    let rep = notify(&server, 1, HashMap::new());
    assert_eq!(rep, 1);
    let n = received(&rx);
    assert_eq!(n.id, 1);
  }

  #[test]
  fn expire_timeout_is_stored() {
    let (server, rx) = server(1);
    for timeout in [-1, 0, 1000, i32::MAX, i32::MIN] {
      let id = server.notify(
        "app".into(),
        0,
        "".into(),
        "title".into(),
        "body".into(),
        vec![],
        HashMap::new(),
        timeout,
      );
      assert!(id > 0);
      let n = received(&rx);
      assert_eq!((n.id, n.expire_timeout), (id, timeout));
    }
  }

  #[test]
  fn server_information() {
    let (server, _) = server(1);
    assert_eq!(
      server.get_capabilities(),
      [
        "body",
        "actions",
        "persistence",
        "icon-static",
        "body-markup",
        "body-hyperlinks",
        "inline-reply"
      ]
    );
    let (name, vendor, version, spec) = server.get_server_information();
    assert_eq!((name, vendor, spec), ("corona", "corona", "1.2"));
    assert_eq!(version, env!("CARGO_PKG_VERSION"));
  }

  #[test]
  fn image_priority() {
    let (server, rx) = server(1);
    let pixels: Value<'_> = (1i32, 1i32, 4i32, true, 8i32, 4i32, vec![1u8, 2, 3, 4]).into();
    // data beats image-path beats app_icon, under any of its names
    for key in ["image-data", "image_data", "icon_data"] {
      notify(
        &server,
        0,
        hints(vec![
          (key, pixels.try_clone().unwrap()),
          ("image-path", "/a.png".into()),
        ]),
      );
      let Some(NotificationImage::Path(path)) = received(&rx).image else {
        panic!("{key} not saved");
      };
      assert_eq!(path.extension().unwrap(), "png");
      assert_eq!(
        image::open(&path).unwrap().to_rgba8().as_raw(),
        &[1, 2, 3, 4]
      );
    }
    notify(
      &server,
      0,
      hints(vec![("image-path", "file:///a%20b.png".into())]),
    );
    assert_eq!(
      received(&rx).image,
      Some(NotificationImage::Path("/a b.png".into()))
    );
    notify(
      &server,
      0,
      hints(vec![("image-path", "dialog-warning".into())]),
    );
    assert_eq!(
      received(&rx).image,
      Some(NotificationImage::Name("dialog-warning".into()))
    );
    // the test notify sends app_icon "icon"
    notify(&server, 0, HashMap::new());
    assert_eq!(
      received(&rx).image,
      Some(NotificationImage::Name("icon".into()))
    );
    // broken data falls through to the next source
    let short: Value<'_> = (2i32, 2i32, 8i32, true, 8i32, 4i32, vec![0u8; 3]).into();
    notify(&server, 0, hints(vec![("image-data", short)]));
    assert_eq!(
      received(&rx).image,
      Some(NotificationImage::Name("icon".into()))
    );
    assert_eq!(icon(""), None);
    assert_eq!(
      icon("/usr/a.svg"),
      Some(NotificationImage::Path("/usr/a.svg".into()))
    );
  }

  #[test]
  fn image_data_rowstride_and_channels() {
    // 2x2 rgb rows padded to 8 bytes, the last row unpadded
    let data = vec![1, 2, 3, 4, 5, 6, 0, 0, 7, 8, 9, 10, 11, 12];
    let image = rgba((2, 2, 8, false, 8, 3, data)).unwrap();
    assert_eq!(
      image.as_raw(),
      &[1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
    );
    assert!(rgba((1, 1, 4, true, 16, 4, vec![0; 8])).is_err());
    assert!(rgba((1, 1, 2, true, 8, 4, vec![0; 4])).is_err());
    assert!(rgba((0, 1, 4, true, 8, 4, vec![])).is_err());
    assert!(rgba((-1, 1, 4, true, 8, 4, vec![0; 4])).is_err());
    assert!(rgba((1, 1, 4, true, 8, 2, vec![0; 4])).is_err());
  }

  #[test]
  fn reply_placeholder_hint() {
    let (server, rx) = server(1);
    notify(
      &server,
      0,
      hints(vec![("x-kde-reply-placeholder-text", "Reply…".into())]),
    );
    assert_eq!(received(&rx).reply_placeholder.as_deref(), Some("Reply…"));
  }
}

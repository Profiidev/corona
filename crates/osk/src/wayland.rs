//! The keyboard's own Wayland connection, on a thread of its own: the input
//! method says which text field has focus, the virtual keyboard types into it

use std::{
  io::Write,
  os::fd::{AsFd, OwnedFd},
  sync::Arc,
  time::Instant,
};

use anyhow::{Context, Result};
use rustix::{
  event::{EventfdFlags, PollFd, PollFlags, eventfd, poll},
  fs::{MemfdFlags, memfd_create},
};
use wayland_client::{
  Connection, Dispatch, QueueHandle, delegate_noop,
  globals::{GlobalListContents, registry_queue_init},
  protocol::{wl_keyboard::KeymapFormat, wl_registry, wl_seat::WlSeat},
};
use wayland_protocols::wp::text_input::zv3::client::zwp_text_input_v3::{
  ContentHint, ContentPurpose,
};
use wayland_protocols_misc::{
  zwp_input_method_v2::client::{
    zwp_input_method_manager_v2::ZwpInputMethodManagerV2,
    zwp_input_method_v2::{self, ZwpInputMethodV2},
  },
  zwp_virtual_keyboard_v1::client::{
    zwp_virtual_keyboard_manager_v1::ZwpVirtualKeyboardManagerV1,
    zwp_virtual_keyboard_v1::ZwpVirtualKeyboardV1,
  },
};

use crate::{
  keymap::{Keymap, Mods},
  layouts::Special,
};

const PRESSED: u32 = 1;
const RELEASED: u32 = 0;

pub enum Command {
  /// Compiles and sends the keymap of an xkb layout like `de`
  Keymap(&'static str),
  /// Committed through the input method when it is active and nothing is
  /// held, else pressed key by key
  Type { text: String, mods: Mods },
  /// An evdev key with modifiers
  Key { code: u32, mods: Mods },
  /// Replaces `delete` right before the cursor with `insert`
  Replace { delete: String, insert: String },
}

/// The focused text field
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Field {
  /// The text before the cursor, when the app tells
  pub before_cursor: Option<String>,
  /// A password, PIN, terminal, URL or number: nothing is suggested or learned
  pub private: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
  /// A text field got focus or changed, none when focus left text
  Field(Option<Field>),
  /// Another input method holds the seat; typing still works, focus is unknown
  Unavailable,
}

/// Sends to the thread and wakes it, so it never polls
pub struct Commands {
  tx: flume::Sender<Command>,
  wake: Arc<OwnedFd>,
}

impl Commands {
  pub fn send(&self, command: Command) {
    if self.tx.send(command).is_ok() {
      let _ = rustix::io::write(&*self.wake, &1u64.to_ne_bytes());
    }
  }
}

impl Drop for Commands {
  fn drop(&mut self) {
    // the thread sees the closed channel when it wakes
    let _ = rustix::io::write(&*self.wake, &1u64.to_ne_bytes());
  }
}

/// Commands that go nowhere but the returned queue
#[cfg(test)]
pub(crate) fn channel() -> (Commands, flume::Receiver<Command>) {
  let (tx, rx) = flume::unbounded();
  let wake = Arc::new(eventfd(0, EventfdFlags::CLOEXEC).unwrap());
  (Commands { tx, wake }, rx)
}

/// Connects on a new thread; `input_method` asks for the seat's input method too
pub fn spawn(input_method: bool, events: flume::Sender<Event>) -> Result<Commands> {
  let (tx, rx) = flume::unbounded();
  let wake = Arc::new(eventfd(0, EventfdFlags::CLOEXEC | EventfdFlags::NONBLOCK)?);
  let woken = wake.clone();
  std::thread::Builder::new()
    .name("wayland-osk".into())
    .spawn(move || {
      if let Err(e) = run(input_method, rx, &woken, events) {
        tracing::error!("on-screen keyboard stopped: {e:#}");
      }
    })?;
  Ok(Commands { tx, wake })
}

/// The input method's double-buffered state, applied on `done`
#[derive(Default)]
struct Pending {
  active: bool,
  surrounding: Option<(String, u32)>,
  hint: Option<ContentHint>,
  purpose: Option<ContentPurpose>,
}

impl Pending {
  fn field(&self) -> Field {
    let before_cursor = self
      .surrounding
      .as_ref()
      .and_then(|(text, cursor)| text.get(..*cursor as usize).map(str::to_string));
    let hidden = self
      .hint
      .is_some_and(|h| h.intersects(ContentHint::SensitiveData | ContentHint::HiddenText));
    let private = hidden
      || self.purpose.is_some_and(|p| {
        !matches!(
          p,
          ContentPurpose::Normal | ContentPurpose::Alpha | ContentPurpose::Name
        )
      });
    Field {
      before_cursor,
      private,
    }
  }
}

struct State {
  events: flume::Sender<Event>,
  pending: Pending,
  /// A text field has focus, as of the last `done`
  active: bool,
  /// `done` events so far, what a commit names
  serial: u32,
  input_method: Option<ZwpInputMethodV2>,
}

struct Typist {
  keyboard: ZwpVirtualKeyboardV1,
  keymap: Option<Keymap>,
  start: Instant,
}

impl Typist {
  fn set_keymap(&mut self, layout: &str) -> Result<()> {
    let keymap = Keymap::new(layout)?;
    let fd = memfd_create("corona-osk-keymap", MemfdFlags::CLOEXEC)?;
    let mut file = std::fs::File::from(fd);
    file.write_all(keymap.text.as_bytes())?;
    file.write_all(&[0])?;
    self.keyboard.keymap(
      KeymapFormat::XkbV1.into(),
      file.as_fd(),
      keymap.text.len() as u32 + 1,
    );
    self.keymap = Some(keymap);
    Ok(())
  }

  fn press(&self, code: u32, mods: Mods) {
    let Some(keymap) = &self.keymap else {
      tracing::warn!("no keymap yet, key {code} dropped");
      return;
    };
    let time = self.start.elapsed().as_millis() as u32;
    let mask = keymap.mask(mods);
    if mask != 0 {
      self.keyboard.modifiers(mask, 0, 0, 0);
    }
    self.keyboard.key(time, code, PRESSED);
    self.keyboard.key(time, code, RELEASED);
    if mask != 0 {
      self.keyboard.modifiers(0, 0, 0, 0);
    }
  }

  /// Presses the key of each character; ones the layout lacks go through the
  /// input method when it is active, else they are dropped
  fn type_keys(&self, text: &str, mods: Mods, state: &State) {
    for c in text.chars() {
      match self.keymap.as_ref().and_then(|k| k.key_for(c)) {
        Some((code, level)) => self.press(code, mods | level),
        None => match state.typing_field() {
          Some(input_method) => {
            input_method.commit_string(c.to_string());
            input_method.commit(state.serial);
          }
          // ponytail: no key and no input method; a temporary keymap with the character would type it
          None => tracing::debug!("no key types {c:?}"),
        },
      }
    }
  }

  fn run(&mut self, command: Command, state: &State) -> Result<()> {
    match command {
      Command::Keymap(layout) => self.set_keymap(layout)?,
      Command::Key { code, mods } => self.press(code, mods),
      Command::Type { text, mods } => match state.typing_field() {
        Some(input_method) if mods.is_empty() => {
          input_method.commit_string(text);
          input_method.commit(state.serial);
        }
        _ => self.type_keys(&text, mods, state),
      },
      Command::Replace { delete, insert } => match state.typing_field() {
        Some(input_method) => {
          input_method.delete_surrounding_text(delete.len() as u32, 0);
          input_method.commit_string(insert);
          input_method.commit(state.serial);
        }
        None => {
          let backspace = Special::Backspace.evdev().unwrap_or_default();
          for _ in delete.chars() {
            self.press(backspace, Mods::empty());
          }
          self.type_keys(&insert, Mods::empty(), state);
        }
      },
    }
    Ok(())
  }
}

impl State {
  /// The input method, when a text field takes text through it
  fn typing_field(&self) -> Option<&ZwpInputMethodV2> {
    self.input_method.as_ref().filter(|_| self.active)
  }
}

fn run(
  input_method: bool,
  commands: flume::Receiver<Command>,
  wake: &OwnedFd,
  events: flume::Sender<Event>,
) -> Result<()> {
  let conn = Connection::connect_to_env()?;
  let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
  let qh = queue.handle();
  let seat: WlSeat = globals.bind(&qh, 1..=8, ())?;
  let manager: ZwpVirtualKeyboardManagerV1 = globals
    .bind(&qh, 1..=1, ())
    .context("the compositor has no virtual keyboard protocol")?;
  let mut typist = Typist {
    keyboard: manager.create_virtual_keyboard(&seat, &qh, ()),
    keymap: None,
    start: Instant::now(),
  };
  let input_method = match input_method {
    true => match globals.bind::<ZwpInputMethodManagerV2, _, _>(&qh, 1..=1, ()) {
      Ok(manager) => Some(manager.get_input_method(&seat, &qh, ())),
      Err(_) => {
        let _ = events.send(Event::Unavailable);
        None
      }
    },
    false => None,
  };
  let mut state = State {
    events,
    pending: Pending::default(),
    active: false,
    serial: 0,
    input_method,
  };

  loop {
    while let Ok(command) = commands.try_recv() {
      if let Err(e) = typist.run(command, &state) {
        tracing::warn!("on-screen keyboard: {e:#}");
      }
    }
    if commands.is_disconnected() {
      return Ok(());
    }
    queue.dispatch_pending(&mut state)?;
    queue.flush()?;
    let Some(guard) = queue.prepare_read() else {
      continue;
    };
    let fd = guard.connection_fd();
    let mut fds = [
      PollFd::new(&fd, PollFlags::IN),
      PollFd::new(wake, PollFlags::IN),
    ];
    poll(&mut fds, None)?;
    let (wayland, woken) = (fds[0].revents(), fds[1].revents());
    if wayland.contains(PollFlags::IN) {
      guard.read()?;
    } else {
      drop(guard);
    }
    if woken.contains(PollFlags::IN) {
      let mut count = [0; 8];
      let _ = rustix::io::read(wake, &mut count);
    }
  }
}

impl Dispatch<ZwpInputMethodV2, ()> for State {
  fn event(
    state: &mut Self,
    input_method: &ZwpInputMethodV2,
    event: zwp_input_method_v2::Event,
    _: &(),
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
    use zwp_input_method_v2::Event as E;
    match event {
      // a new field starts from nothing
      E::Activate => {
        state.pending = Pending {
          active: true,
          ..Pending::default()
        }
      }
      E::Deactivate => state.pending.active = false,
      E::SurroundingText { text, cursor, .. } => state.pending.surrounding = Some((text, cursor)),
      E::ContentType { hint, purpose } => {
        state.pending.hint = hint.into_result().ok();
        state.pending.purpose = purpose.into_result().ok();
      }
      E::Done => {
        state.serial += 1;
        state.active = state.pending.active;
        let field = state.active.then(|| state.pending.field());
        let _ = state.events.send(Event::Field(field));
      }
      E::Unavailable => {
        input_method.destroy();
        state.input_method = None;
        state.active = false;
        let _ = state.events.send(Event::Unavailable);
      }
      _ => {}
    }
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

delegate_noop!(State: ignore WlSeat);
delegate_noop!(State: ZwpVirtualKeyboardManagerV1);
delegate_noop!(State: ZwpVirtualKeyboardV1);
delegate_noop!(State: ZwpInputMethodManagerV2);

#[cfg(test)]
mod tests {
  use std::{
    os::unix::fs::FileExt,
    sync::{
      Mutex,
      atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
  };

  use wayland_protocols::wp::text_input::zv3::server::zwp_text_input_v3::{
    ContentHint as Hint, ContentPurpose as Purpose,
  };
  use wayland_protocols_misc::{
    zwp_input_method_v2::server::{
      zwp_input_method_manager_v2::{self, ZwpInputMethodManagerV2 as ImManager},
      zwp_input_method_v2::{self as im, ZwpInputMethodV2 as Im},
    },
    zwp_virtual_keyboard_v1::server::{
      zwp_virtual_keyboard_manager_v1::{self, ZwpVirtualKeyboardManagerV1 as VkManager},
      zwp_virtual_keyboard_v1::{self as vk, ZwpVirtualKeyboardV1 as Vk},
    },
  };
  use wayland_server::{
    Client, DataInit, Dispatch as ServerDispatch, Display, DisplayHandle, GlobalDispatch,
    ListeningSocket, New, Resource,
    backend::{ClientData, ClientId, DisconnectReason},
    protocol::wl_seat::WlSeat as ServerSeat,
  };

  use super::*;

  /// What the client asked the compositor for, in order
  #[derive(Debug, PartialEq)]
  enum Seen {
    Keymap(String),
    Key(u32, u32),
    Modifiers(u32),
    Commit(String),
    Delete(u32),
    Done(u32),
  }

  #[derive(Clone, Default)]
  struct Shared {
    seen: Arc<Mutex<Vec<Seen>>>,
    input_method: Arc<Mutex<Option<Im>>>,
  }

  impl Shared {
    fn push(&self, seen: Seen) {
      self.seen.lock().unwrap().push(seen);
    }
  }

  struct NoData;
  impl ClientData for NoData {
    fn disconnected(&self, _: ClientId, _: DisconnectReason) {}
  }

  macro_rules! bind_global {
    ($($ty:ty),*) => {$(
      impl GlobalDispatch<$ty, ()> for Shared {
        fn bind(
          _: &mut Self,
          _: &DisplayHandle,
          _: &Client,
          resource: New<$ty>,
          _: &(),
          init: &mut DataInit<'_, Self>,
        ) {
          init.init(resource, ());
        }
      }
    )*};
  }
  bind_global!(ServerSeat, ImManager, VkManager);

  impl ServerDispatch<ServerSeat, ()> for Shared {
    fn request(
      _: &mut Self,
      _: &Client,
      _: &ServerSeat,
      _: <ServerSeat as Resource>::Request,
      _: &(),
      _: &DisplayHandle,
      _: &mut DataInit<'_, Self>,
    ) {
    }
  }

  impl ServerDispatch<ImManager, ()> for Shared {
    fn request(
      state: &mut Self,
      _: &Client,
      _: &ImManager,
      request: zwp_input_method_manager_v2::Request,
      _: &(),
      _: &DisplayHandle,
      init: &mut DataInit<'_, Self>,
    ) {
      if let zwp_input_method_manager_v2::Request::GetInputMethod { input_method, .. } = request {
        *state.input_method.lock().unwrap() = Some(init.init(input_method, ()));
      }
    }
  }

  impl ServerDispatch<Im, ()> for Shared {
    fn request(
      state: &mut Self,
      _: &Client,
      _: &Im,
      request: im::Request,
      _: &(),
      _: &DisplayHandle,
      _: &mut DataInit<'_, Self>,
    ) {
      match request {
        im::Request::CommitString { text } => state.push(Seen::Commit(text)),
        im::Request::DeleteSurroundingText { before_length, .. } => {
          state.push(Seen::Delete(before_length))
        }
        im::Request::Commit { serial } => state.push(Seen::Done(serial)),
        _ => {}
      }
    }
  }

  impl ServerDispatch<VkManager, ()> for Shared {
    fn request(
      _: &mut Self,
      _: &Client,
      _: &VkManager,
      request: zwp_virtual_keyboard_manager_v1::Request,
      _: &(),
      _: &DisplayHandle,
      init: &mut DataInit<'_, Self>,
    ) {
      if let zwp_virtual_keyboard_manager_v1::Request::CreateVirtualKeyboard { id, .. } = request {
        init.init(id, ());
      }
    }
  }

  impl ServerDispatch<Vk, ()> for Shared {
    fn request(
      state: &mut Self,
      _: &Client,
      _: &Vk,
      request: vk::Request,
      _: &(),
      _: &DisplayHandle,
      _: &mut DataInit<'_, Self>,
    ) {
      match request {
        vk::Request::Keymap { fd, size, .. } => {
          let mut text = vec![0; size as usize];
          std::fs::File::from(fd).read_exact_at(&mut text, 0).unwrap();
          let text = String::from_utf8_lossy(&text)
            .trim_end_matches('\0')
            .to_string();
          state.push(Seen::Keymap(text));
        }
        vk::Request::Key { key, state: s, .. } => state.push(Seen::Key(key, s)),
        vk::Request::Modifiers { mods_depressed, .. } => {
          state.push(Seen::Modifiers(mods_depressed))
        }
        _ => {}
      }
    }
  }

  /// A compositor on its own socket, `WAYLAND_DISPLAY` points at it.
  /// nextest runs each test in its own process, so setting the environment is safe.
  struct Compositor {
    shared: Shared,
    stop: Arc<AtomicBool>,
    _dir: tempfile::TempDir,
  }

  impl Compositor {
    fn start(with_input_method: bool) -> Self {
      let dir = tempfile::tempdir().unwrap();
      unsafe {
        std::env::set_var("XDG_RUNTIME_DIR", dir.path());
        std::env::set_var("WAYLAND_DISPLAY", "wayland-test");
        std::env::remove_var("WAYLAND_SOCKET");
      }
      let socket = ListeningSocket::bind("wayland-test").unwrap();
      let shared = Shared::default();
      let stop = Arc::new(AtomicBool::new(false));
      let (mut state, stopped) = (shared.clone(), stop.clone());
      thread::spawn(move || {
        let mut display = Display::<Shared>::new().unwrap();
        let handle = display.handle();
        handle.create_global::<Shared, ServerSeat, ()>(7, ());
        handle.create_global::<Shared, VkManager, ()>(1, ());
        if with_input_method {
          handle.create_global::<Shared, ImManager, ()>(1, ());
        }
        while !stopped.load(Ordering::Relaxed) {
          if let Some(stream) = socket.accept().unwrap() {
            display
              .handle()
              .insert_client(stream, Arc::new(NoData))
              .unwrap();
          }
          display.dispatch_clients(&mut state).unwrap();
          display.flush_clients().ok();
          thread::sleep(Duration::from_millis(2));
        }
      });
      Self {
        shared,
        stop,
        _dir: dir,
      }
    }

    /// Sends input method events once the client has asked for it
    fn input_method(&self, send: impl FnOnce(&Im)) {
      wait(|| self.shared.input_method.lock().unwrap().is_some());
      send(self.shared.input_method.lock().unwrap().as_ref().unwrap());
    }

    /// Waits for `count` requests and takes them
    fn take(&self, count: usize) -> Vec<Seen> {
      wait(|| self.shared.seen.lock().unwrap().len() >= count);
      std::mem::take(&mut *self.shared.seen.lock().unwrap())
    }
  }

  impl Drop for Compositor {
    fn drop(&mut self) {
      self.stop.store(true, Ordering::Relaxed);
    }
  }

  fn wait(done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
      assert!(Instant::now() < deadline, "timed out");
      thread::sleep(Duration::from_millis(2));
    }
  }

  fn next(events: &flume::Receiver<Event>) -> Event {
    events.recv_timeout(Duration::from_secs(10)).unwrap()
  }

  fn typed(text: &str, mods: Mods) -> Command {
    Command::Type {
      text: text.into(),
      mods,
    }
  }

  #[test]
  fn types_keys_without_a_focused_field() {
    let compositor = Compositor::start(true);
    let (tx, _events) = flume::unbounded();
    let commands = spawn(true, tx).unwrap();
    commands.send(Command::Keymap("us"));
    let Seen::Keymap(keymap) = &compositor.take(1)[0] else {
      panic!("no keymap first");
    };
    assert!(keymap.starts_with("xkb_keymap"));

    // evdev KEY_C with Ctrl, then KEY_A with Shift for `A`
    commands.send(typed("c", Mods::CTRL));
    commands.send(typed("A", Mods::empty()));
    commands.send(Command::Key {
      code: 14,
      mods: Mods::empty(),
    });
    assert_eq!(
      compositor.take(10),
      [
        Seen::Modifiers(4),
        Seen::Key(46, PRESSED),
        Seen::Key(46, RELEASED),
        Seen::Modifiers(0),
        Seen::Modifiers(1),
        Seen::Key(30, PRESSED),
        Seen::Key(30, RELEASED),
        Seen::Modifiers(0),
        Seen::Key(14, PRESSED),
        Seen::Key(14, RELEASED),
      ]
    );
  }

  #[test]
  fn commits_into_the_focused_field() {
    let compositor = Compositor::start(true);
    let (tx, events) = flume::unbounded();
    let commands = spawn(true, tx).unwrap();
    commands.send(Command::Keymap("us"));
    compositor.take(1);

    compositor.input_method(|im| {
      im.activate();
      im.surrounding_text("hello wör".into(), 10, 10);
      im.content_type(Hint::Spellcheck, Purpose::Normal);
      im.done();
    });
    assert_eq!(
      next(&events),
      Event::Field(Some(Field {
        before_cursor: Some("hello wör".into()),
        private: false,
      }))
    );

    // no key types ö on `us`, the input method does not need one
    commands.send(typed("ö", Mods::empty()));
    commands.send(Command::Replace {
      delete: "wör".into(),
      insert: "world ".into(),
    });
    assert_eq!(
      compositor.take(5),
      [
        Seen::Commit("ö".into()),
        Seen::Done(1),
        Seen::Delete(4),
        Seen::Commit("world ".into()),
        Seen::Done(1),
      ]
    );

    // only the content type changes: the rest of the state stays
    compositor.input_method(|im| {
      im.content_type(Hint::SensitiveData, Purpose::Password);
      im.done();
    });
    assert_eq!(
      next(&events),
      Event::Field(Some(Field {
        before_cursor: Some("hello wör".into()),
        private: true,
      }))
    );

    compositor.input_method(|im| {
      im.deactivate();
      im.done();
    });
    assert_eq!(next(&events), Event::Field(None));
    // without a field even plain text goes key by key, KEY_B
    commands.send(typed("b", Mods::empty()));
    assert_eq!(
      compositor.take(2),
      [Seen::Key(48, PRESSED), Seen::Key(48, RELEASED)]
    );
  }

  #[test]
  fn another_input_method_leaves_typing_working() {
    let compositor = Compositor::start(true);
    let (tx, events) = flume::unbounded();
    let commands = spawn(true, tx).unwrap();
    compositor.input_method(|im| im.unavailable());
    assert_eq!(next(&events), Event::Unavailable);
    commands.send(Command::Keymap("us"));
    commands.send(typed("b", Mods::empty()));
    assert_eq!(
      compositor.take(3)[1..],
      [Seen::Key(48, PRESSED), Seen::Key(48, RELEASED)]
    );
  }

  #[test]
  fn no_input_method_global_is_unavailable() {
    let _compositor = Compositor::start(false);
    let (tx, events) = flume::unbounded();
    let _commands = spawn(true, tx).unwrap();
    assert_eq!(next(&events), Event::Unavailable);
  }

  #[test]
  fn disabled_asks_for_no_input_method() {
    let compositor = Compositor::start(true);
    let (tx, events) = flume::unbounded();
    let commands = spawn(false, tx).unwrap();
    commands.send(Command::Keymap("us"));
    compositor.take(1);
    assert!(compositor.shared.input_method.lock().unwrap().is_none());
    assert!(events.try_recv().is_err());
  }
}

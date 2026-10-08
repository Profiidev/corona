//! A fake Hyprland: the command socket answers from a table, the event socket
//! sends what the test pushes.

use std::{
  collections::HashMap,
  io::{Read, Write},
  os::unix::net::{UnixListener, UnixStream},
  path::{Path, PathBuf},
  sync::{Arc, Mutex},
  thread,
};

pub(crate) const WORKSPACES: &str = r#"[
  {"address": "0x2", "type": "normal", "name": "2", "monitor": "DP-1", "monitorID": 1},
  {"address": "0x9", "type": "special", "name": "special:magic", "monitor": "eDP-1", "monitorID": 0},
  {"address": "0x1", "type": "normal", "name": "1", "monitor": "eDP-1", "monitorID": 0}
]"#;

pub(crate) const ACTIVE_WORKSPACE: &str =
  r#"{"address": "0x1", "type": "normal", "name": "1", "monitor": "eDP-1", "monitorID": 0}"#;

pub(crate) const MONITORS: &str = r#"[
  {"id": 1, "name": "DP-1", "width": 2560, "height": 1440, "refreshRate": 143.9, "x": 1920, "y": 0,
   "specialWorkspace": {"address": "", "name": ""},
   "activeWorkspace": {"address": "0x2", "name": "2"},
   "scale": 1.0, "focused": false, "disabled": false, "mirrorOf": "none"},
  {"id": 0, "name": "eDP-1", "width": 1920, "height": 1080, "refreshRate": 60.0, "x": 0, "y": 0,
   "specialWorkspace": {"address": "0x9", "name": "special:magic"},
   "activeWorkspace": {"address": "0x1", "name": "1"},
   "scale": 1.25, "focused": true, "disabled": false, "mirrorOf": "none"}
]"#;

pub(crate) const CLIENTS: &str = r#"[
  {"address": "0xb", "monitor": 1, "class": "kitty", "title": "zsh", "workspace": {"address": "0x2"},
   "at": [1920, 30], "size": [800, 600], "floating": true, "pinned": false, "fullscreen": 2,
   "hidden": false, "focusHistoryID": 1},
  {"address": "0xa", "monitor": 0, "class": "firefox", "title": "Fünf ✓", "workspace": {"address": "0x1"},
   "at": [0, 30], "size": [1920, 1050], "floating": false, "pinned": false, "fullscreen": 0,
   "hidden": false, "focusHistoryID": 0}
]"#;

pub(crate) const ACTIVE_WINDOW: &str = r#"{"address": "0xa", "monitor": 0, "class": "firefox",
  "title": "Fünf ✓", "workspace": {"address": "0x1"}, "at": [0, 30], "size": [1920, 1050],
  "floating": false, "pinned": false, "fullscreen": 0, "hidden": false, "focusHistoryID": 0}"#;

pub(crate) const DEVICES: &str =
  r#"{"mice": [], "keyboards": [{"active_keymap": "German", "main": true}]}"#;

pub(crate) struct FakeHyprland {
  pub dir: PathBuf,
  answers: Arc<Mutex<HashMap<String, Vec<u8>>>>,
  commands: Arc<Mutex<Vec<String>>>,
  events: Arc<Mutex<Option<UnixStream>>>,
  _tmp: tempfile::TempDir,
}

impl FakeHyprland {
  /// answers the data commands with the fixtures above and every eval with `ok`
  pub(crate) fn start() -> Self {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("hypr").join("sig");
    std::fs::create_dir_all(&dir).unwrap();
    let answers: HashMap<String, Vec<u8>> = [
      ("j/workspaces", WORKSPACES),
      ("j/activeworkspace", ACTIVE_WORKSPACE),
      ("j/monitors all", MONITORS),
      ("j/clients", CLIENTS),
      ("j/activewindow", ACTIVE_WINDOW),
      ("j/cursorpos", r#"{"x": -5, "y": 1200}"#),
      ("j/devices", DEVICES),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v.as_bytes().to_vec()))
    .collect();
    let fake = Self {
      dir,
      answers: Arc::new(Mutex::new(answers)),
      commands: Arc::default(),
      events: Arc::default(),
      _tmp: tmp,
    };

    let listener = UnixListener::bind(fake.dir.join(".socket.sock")).unwrap();
    let (answers, commands) = (fake.answers.clone(), fake.commands.clone());
    thread::spawn(move || {
      for stream in listener.incoming() {
        let Ok(mut stream) = stream else { return };
        // the client sends one command and waits for the end of the answer
        let mut buf = [0; 4096];
        let n = stream.read(&mut buf).unwrap_or(0);
        let command = String::from_utf8_lossy(&buf[..n]).to_string();
        commands.lock().unwrap().push(command.clone());
        let answer = match answers.lock().unwrap().get(&command) {
          Some(answer) => answer.clone(),
          None if command.starts_with("/eval ") => b"ok".to_vec(),
          None => b"unknown request".to_vec(),
        };
        stream.write_all(&answer).ok();
      }
    });

    fake.listen_events();
    fake
  }

  /// accepts event connections, the newest one gets the pushed lines
  fn listen_events(&self) {
    let listener = UnixListener::bind(self.dir.join(".socket2.sock")).unwrap();
    let events = self.events.clone();
    thread::spawn(move || {
      for stream in listener.incoming() {
        let Ok(stream) = stream else { return };
        *events.lock().unwrap() = Some(stream);
      }
    });
  }

  pub(crate) fn answer(&self, command: &str, answer: impl AsRef<[u8]>) {
    self
      .answers
      .lock()
      .unwrap()
      .insert(command.into(), answer.as_ref().to_vec());
  }

  pub(crate) fn commands(&self) -> Vec<String> {
    self.commands.lock().unwrap().clone()
  }

  pub(crate) fn connected(&self) -> bool {
    self.events.lock().unwrap().is_some()
  }

  pub(crate) fn push(&self, line: &str) {
    let mut events = self.events.lock().unwrap();
    let stream = events.as_mut().expect("a listener connected");
    stream.write_all(format!("{line}\n").as_bytes()).unwrap();
  }

  /// closes the event connection, as a restarting Hyprland would
  pub(crate) fn hang_up(&self) {
    *self.events.lock().unwrap() = None;
  }

  pub(crate) fn ipc(&self) -> super::command::Ipc {
    super::command::Ipc {
      cmd_socket: self.dir.join(".socket.sock"),
    }
  }

  pub(crate) fn runtime_dir(&self) -> &Path {
    self.dir.parent().unwrap().parent().unwrap()
  }
}

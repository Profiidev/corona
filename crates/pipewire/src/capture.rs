use std::{sync::Arc, time::SystemTime};

use dashmap::DashMap;
use pipewire::{node::NodeState, spa::utils::dict::DictRef};
use serde::Serialize;
use ts_rs::TS;

use crate::event::AudioEvent;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CaptureKind {
  Microphone,
  Camera,
  Screen,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Capture {
  pub id: u32,
  pub kind: CaptureKind,
  pub name: String,
  pub active: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CaptureAccess {
  id: u32,
  pub kind: CaptureKind,
  pub name: Option<String>,
  pub started: SystemTime,
  pub ended: Option<SystemTime>,
}

const LOG_LIMIT: usize = 50;

pub(crate) fn record(log: &mut Vec<CaptureAccess>, captures: &[Capture], now: SystemTime) -> bool {
  let active = |id: u32| captures.iter().any(|c| c.id == id && c.active);
  let mut changed = false;

  for entry in log.iter_mut().filter(|e| e.ended.is_none()) {
    if !active(entry.id) {
      entry.ended = Some(now);
      changed = true;
    }
  }

  for capture in captures.iter().filter(|c| c.active) {
    let ongoing = log.iter().any(|e| e.id == capture.id && e.ended.is_none());
    if ongoing {
      continue;
    }
    let name = (!capture.name.is_empty()).then(|| capture.name.clone());
    if name.is_some() {
      log.retain(|e| !(e.ended.is_none() && e.name.is_none() && e.kind == capture.kind));
    } else if captures
      .iter()
      .any(|c| c.active && c.kind == capture.kind && !c.name.is_empty())
    {
      continue;
    }
    log.insert(
      0,
      CaptureAccess {
        id: capture.id,
        kind: capture.kind,
        name,
        started: now,
        ended: None,
      },
    );
    changed = true;
  }

  // over the limit the oldest ended entries go, a capture still running stays
  while log.len() > LOG_LIMIT {
    match log.iter().rposition(|e| e.ended.is_some()) {
      Some(oldest) => _ = log.remove(oldest),
      None => break,
    }
  }
  changed
}

#[derive(Clone)]
struct Tracked {
  class: String,
  source: bool,
  kind: Option<CaptureKind>,
  name: String,
  running: bool,
  failed: bool,
}

impl Tracked {
  fn capture(&self, id: u32) -> Option<Capture> {
    let kind = self.kind?;
    let active = match (kind, self.source) {
      (CaptureKind::Screen, true) => !self.failed,
      _ => self.running,
    };
    Some(Capture {
      id,
      kind,
      name: self.name.clone(),
      active,
    })
  }
}

#[derive(Clone, Default)]
pub struct CaptureState {
  nodes: Arc<DashMap<u32, Tracked>>,
}

pub(crate) fn is_candidate(class: &str) -> bool {
  matches!(
    class,
    "Stream/Input/Audio" | "Stream/Input/Video" | "Video/Source" | "Stream/Output/Video"
  )
}

pub(crate) fn classify(class: &str, prop: impl Fn(&str) -> Option<String>) -> Option<CaptureKind> {
  let set = |key: &str| prop(key).as_deref() == Some("true");
  let device = prop("device.id").is_some() || prop("device.api").is_some();
  match class {
    "Stream/Input/Audio" if !set("stream.capture.sink") && !set("stream.monitor") => {
      Some(CaptureKind::Microphone)
    }
    "Video/Source" if device => Some(CaptureKind::Camera),
    "Video/Source" | "Stream/Output/Video" => Some(CaptureKind::Screen),
    "Stream/Input/Video" => match prop("media.role").as_deref() {
      Some("Screen") => Some(CaptureKind::Screen),
      Some("Camera") => Some(CaptureKind::Camera),
      _ => None,
    },
    _ => None,
  }
}

fn is_source(class: &str) -> bool {
  matches!(class, "Video/Source" | "Stream/Output/Video")
}

fn name(props: &DictRef) -> Option<String> {
  [
    "application.name",
    "media.name",
    "application.process.binary",
    "node.name",
  ]
  .into_iter()
  .find_map(|key| props.get(key))
  .map(str::to_string)
}

impl Tracked {
  fn apply(&mut self, props: &DictRef) {
    self.kind = classify(&self.class, |key| props.get(key).map(str::to_string));
    if !self.source
      && let Some(name) = name(props)
    {
      self.name = name;
    }
  }
}

impl CaptureState {
  pub(crate) fn insert(&self, id: u32, class: &str, props: &DictRef) {
    let mut tracked = Tracked {
      class: class.to_string(),
      source: is_source(class),
      kind: None,
      name: String::new(),
      running: false,
      failed: false,
    };
    tracked.apply(props);
    self.nodes.insert(id, tracked);
  }

  pub(crate) fn update(
    &self,
    id: u32,
    state: &NodeState,
    props: Option<&DictRef>,
    events: &flume::Sender<AudioEvent>,
  ) {
    let changed = self.nodes.get_mut(&id).is_some_and(|mut tracked| {
      let before = tracked.capture(id);
      tracked.running = matches!(state, NodeState::Running);
      tracked.failed = matches!(state, NodeState::Error(_));
      if let Some(props) = props {
        tracked.apply(props);
      }
      tracked.capture(id) != before
    });
    if changed {
      self.send(events);
    }
  }

  pub(crate) fn remove(&self, id: u32, events: &flume::Sender<AudioEvent>) -> bool {
    let Some((_, tracked)) = self.nodes.remove(&id) else {
      return false;
    };
    if tracked.kind.is_some() {
      self.send(events);
    }
    true
  }

  pub fn list(&self) -> Vec<Capture> {
    let mut captures: Vec<Capture> = self
      .nodes
      .iter()
      .filter_map(|t| t.capture(*t.key()))
      .collect();
    captures.sort_unstable_by_key(|c| c.id);
    captures
  }

  pub(crate) fn send(&self, events: &flume::Sender<AudioEvent>) {
    let _ = events.send(AudioEvent::Captures(self.list()));
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn kinds() {
    let none = |_: &str| None;
    assert_eq!(
      classify("Stream/Input/Audio", none),
      Some(CaptureKind::Microphone)
    );
    // recording what plays, like gpu-screen-recorder's desktop audio
    let monitor = |key: &str| (key == "stream.capture.sink").then(|| "true".to_string());
    assert_eq!(classify("Stream/Input/Audio", monitor), None);
    // a v4l2 camera has a device, the portal's screencast source (xdph) has none
    let camera = |key: &str| (key == "device.api").then(|| "v4l2".to_string());
    assert_eq!(classify("Video/Source", camera), Some(CaptureKind::Camera));
    assert_eq!(classify("Video/Source", none), Some(CaptureKind::Screen));
    assert_eq!(
      classify("Stream/Output/Video", none),
      Some(CaptureKind::Screen)
    );
    // OBS reading the screencast
    let screen = |key: &str| (key == "media.role").then(|| "Screen".to_string());
    assert_eq!(
      classify("Stream/Input/Video", screen),
      Some(CaptureKind::Screen)
    );
    assert_eq!(classify("Stream/Input/Video", none), None);
    assert_eq!(classify("Stream/Output/Audio", none), None);
  }

  fn capture(id: u32, kind: CaptureKind, name: &str, active: bool) -> Capture {
    Capture {
      id,
      kind,
      name: name.to_string(),
      active,
    }
  }

  #[test]
  fn log() {
    use CaptureKind::*;
    let (t0, t1, t2) = (
      SystemTime::UNIX_EPOCH,
      SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1),
      SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(2),
    );
    let mut log = Vec::new();

    // the portal source comes first, the app reading it replaces the unnamed entry
    assert!(record(&mut log, &[capture(1, Screen, "", true)], t0));
    let both = [
      capture(1, Screen, "", true),
      capture(2, Screen, "OBS", true),
    ];
    assert!(record(&mut log, &both, t0));
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].name.as_deref(), Some("OBS"));
    assert!(!record(&mut log, &both, t1));

    // a mic starts, the share ends
    assert!(record(
      &mut log,
      &[capture(3, Microphone, "Discord", true)],
      t2
    ));
    assert_eq!(log[0].kind, Microphone);
    assert_eq!(log[1].ended, Some(t2));
    assert_eq!(log[0].ended, None);
  }

  #[test]
  fn activity() {
    let tracked = |kind, source, running| Tracked {
      class: String::new(),
      source,
      kind: Some(kind),
      name: String::new(),
      running,
      failed: false,
    };
    let active = |t: Tracked| t.capture(0).unwrap().active;
    assert!(active(tracked(CaptureKind::Camera, true, true)));
    assert!(!active(tracked(CaptureKind::Camera, true, false)));
    // a paused screen share still shares
    assert!(active(tracked(CaptureKind::Screen, true, false)));
    assert!(!active(tracked(CaptureKind::Screen, false, false)));
  }

  use std::time::Duration;

  use crate::testing::props;

  #[test]
  fn candidates() {
    for class in [
      "Stream/Input/Audio",
      "Stream/Input/Video",
      "Video/Source",
      "Stream/Output/Video",
    ] {
      assert!(is_candidate(class), "{class}");
    }
    for class in [
      "Audio/Sink",
      "Audio/Source",
      "Stream/Output/Audio",
      "Video/Sink",
      "",
    ] {
      assert!(!is_candidate(class), "{class}");
    }
  }

  #[test]
  fn more_kinds() {
    let with = |pairs: &'static [(&'static str, &'static str)]| {
      move |key: &str| {
        pairs
          .iter()
          .find(|(k, _)| *k == key)
          .map(|(_, v)| v.to_string())
      }
    };
    assert_eq!(
      classify("Stream/Input/Audio", with(&[("stream.monitor", "true")])),
      None
    );
    // only "true" counts
    assert_eq!(
      classify("Stream/Input/Audio", with(&[("stream.monitor", "false")])),
      Some(CaptureKind::Microphone)
    );
    assert_eq!(
      classify("Video/Source", with(&[("device.id", "40")])),
      Some(CaptureKind::Camera)
    );
    // a device does not make a screencast output a camera
    assert_eq!(
      classify("Stream/Output/Video", with(&[("device.id", "40")])),
      Some(CaptureKind::Screen)
    );
    assert_eq!(
      classify("Stream/Input/Video", with(&[("media.role", "Camera")])),
      Some(CaptureKind::Camera)
    );
    assert_eq!(
      classify("Stream/Input/Video", with(&[("media.role", "Music")])),
      None
    );
  }

  fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
  }

  #[test]
  fn unnamed_captures_wait_for_a_named_one() {
    use CaptureKind::*;
    let mut log = Vec::new();
    // an app already reads the share: the unnamed source adds nothing
    let both = [
      capture(1, Screen, "", true),
      capture(2, Screen, "OBS", true),
    ];
    assert!(record(&mut log, &both, at(0)));
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].name.as_deref(), Some("OBS"));
    // inactive captures are not logged
    assert!(!record(
      &mut Vec::new(),
      &[capture(3, Camera, "x", false)],
      at(0)
    ));
    // a named capture of another kind leaves an unnamed one alone
    let mut log = Vec::new();
    record(&mut log, &[capture(1, Microphone, "", true)], at(0));
    record(
      &mut log,
      &[
        capture(1, Microphone, "", true),
        capture(2, Screen, "OBS", true),
      ],
      at(1),
    );
    assert_eq!(log.len(), 2);
    assert!(log.iter().all(|e| e.ended.is_none()));
  }

  #[test]
  fn sessions_end_and_restart() {
    use CaptureKind::*;
    let mut log = Vec::new();
    record(&mut log, &[capture(1, Camera, "Zoom", true)], at(0));
    assert!(record(
      &mut log,
      &[capture(1, Camera, "Zoom", false)],
      at(5)
    ));
    assert_eq!(log[0].ended, Some(at(5)));
    // ended stays ended
    assert!(!record(&mut log, &[], at(6)));
    assert_eq!(log[0].ended, Some(at(5)));
    // the same node running again is a new entry
    assert!(record(&mut log, &[capture(1, Camera, "Zoom", true)], at(7)));
    assert_eq!(log.len(), 2);
    assert_eq!((log[0].started, log[0].ended), (at(7), None));
  }

  #[test]
  fn the_log_is_capped() {
    use CaptureKind::*;
    let mut log = Vec::new();
    for i in 0..(LOG_LIMIT as u32 + 10) {
      record(
        &mut log,
        &[capture(i, Microphone, "app", true)],
        at(i.into()),
      );
    }
    assert_eq!(log.len(), LOG_LIMIT);
    // newest first
    assert_eq!(log[0].started, at(LOG_LIMIT as u64 + 9));
  }

  #[test]
  fn the_cap_keeps_running_captures() {
    use CaptureKind::*;
    let mut log = Vec::new();
    let long = capture(1000, Microphone, "call", true);
    record(&mut log, std::slice::from_ref(&long), at(0));
    for i in 0..LOG_LIMIT as u32 {
      record(
        &mut log,
        &[long.clone(), capture(i, Camera, "snap", true)],
        at(1 + u64::from(i)),
      );
      record(&mut log, std::slice::from_ref(&long), at(1 + u64::from(i)));
    }
    let call = log
      .iter()
      .find(|e| e.id == 1000)
      .expect("the call is still logged");
    assert_eq!(call.started, at(0));
  }

  #[test]
  fn tracked_nodes() {
    let (tx, rx) = flume::unbounded();
    let state = CaptureState::default();
    let mic = props(&[("application.name", "Discord"), ("node.name", "discord-in")]);
    state.insert(1, "Stream/Input/Audio", mic.dict());
    // a desktop audio recorder is tracked but no capture
    let monitor = props(&[("stream.capture.sink", "true")]);
    state.insert(2, "Stream/Input/Audio", monitor.dict());
    let screen = props(&[("application.name", "xdph")]);
    state.insert(3, "Video/Source", screen.dict());

    let list = state.list();
    assert_eq!(list.len(), 2);
    assert_eq!(
      list[0],
      capture(1, CaptureKind::Microphone, "Discord", false)
    );
    // a screen source is active until it fails, and keeps no name
    assert_eq!(list[1], capture(3, CaptureKind::Screen, "", true));

    state.update(1, &NodeState::Running, None, &tx);
    assert!(matches!(rx.try_recv().unwrap(), AudioEvent::Captures(c) if c[0].active));
    // running again is no change
    state.update(1, &NodeState::Running, None, &tx);
    assert!(rx.is_empty());
    // a rename is a change
    let renamed = props(&[("media.name", "Call")]);
    state.update(1, &NodeState::Running, Some(renamed.dict()), &tx);
    assert!(matches!(rx.try_recv().unwrap(), AudioEvent::Captures(c) if c[0].name == "Call"));
    // a source does not take names from its props
    state.update(3, &NodeState::Idle, Some(renamed.dict()), &tx);
    assert!(rx.is_empty());
    state.update(3, &NodeState::Error("gone"), None, &tx);
    assert!(matches!(rx.try_recv().unwrap(), AudioEvent::Captures(c) if !c[1].active));
    // the recorder turning into a mic by its props
    let mic_now = props(&[("node.name", "rec")]);
    state.update(2, &NodeState::Running, Some(mic_now.dict()), &tx);
    assert_eq!(state.list().len(), 3);
    rx.drain().for_each(drop);
    // unknown nodes are ignored
    state.update(9, &NodeState::Running, None, &tx);
    assert!(rx.is_empty());

    assert!(state.remove(1, &tx));
    assert_eq!(rx.drain().count(), 1);
    assert!(!state.remove(1, &tx));
    // removing an unclassified node needs no event
    let unclassified = props(&[("media.role", "Music")]);
    state.insert(4, "Stream/Input/Video", unclassified.dict());
    assert!(state.remove(4, &tx));
    assert!(rx.is_empty());
  }

  #[test]
  fn names_fall_back() {
    let name_of = |pairs: &[(&str, &str)]| name(props(pairs).dict());
    assert_eq!(name_of(&[]), None);
    assert_eq!(name_of(&[("node.name", "n")]).as_deref(), Some("n"));
    assert_eq!(
      name_of(&[("node.name", "n"), ("application.process.binary", "b")]).as_deref(),
      Some("b")
    );
    assert_eq!(
      name_of(&[("media.name", "m"), ("application.process.binary", "b")]).as_deref(),
      Some("m")
    );
    assert_eq!(
      name_of(&[("media.name", "m"), ("application.name", "a")]).as_deref(),
      Some("a")
    );
  }

  #[test]
  fn active_captures_exceeding_log_limit() {
    let mut log = Vec::new();
    let captures: Vec<_> = (0..55)
      .map(|i| Capture {
        id: i,
        kind: CaptureKind::Microphone,
        name: format!("App {i}"),
        active: true,
      })
      .collect();
    record(&mut log, &captures, SystemTime::now());
    assert_eq!(log.len(), 55);
  }

  #[test]
  fn screen_source_recovers_from_error_to_idle_active() {
    let (tx, rx) = flume::unbounded();
    let state = CaptureState::default();
    let screen = props(&[("application.name", "OBS")]);
    state.insert(3, "Stream/Output/Video", screen.dict());
    assert!(state.list()[0].active);

    state.update(3, &NodeState::Error("device disconnected"), None, &tx);
    assert!(!state.list()[0].active);
    rx.drain().for_each(drop);

    state.update(3, &NodeState::Idle, None, &tx);
    assert!(state.list()[0].active);
    assert_eq!(rx.drain().count(), 1);
  }
}

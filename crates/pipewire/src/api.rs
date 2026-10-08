use std::{collections::HashMap, time::SystemTime};

use anyhow::{Result, bail};
use corona_utils::entity::WriteChangedExt;
use gpui_kit::{App, AppContext, BorrowAppContext, Entity, Global};

use crate::{
  audio::PipewireAudio,
  capture::{Capture, CaptureAccess, CaptureKind, record},
  command::Command,
  event::AudioEvent,
  state::{AudioNode, NodeType, PipewireState},
};

pub struct Pipewire {
  commands: pipewire::channel::Sender<Command>,
  pub(super) state: PipewireState,
  pub sinks: Entity<Vec<AudioNode>>,
  pub sources: Entity<Vec<AudioNode>>,
  pub streams: Entity<Vec<AudioNode>>,
  pub default_sink: Entity<Option<AudioNode>>,
  pub default_source: Entity<Option<AudioNode>>,
  pub targets: Entity<HashMap<u32, u32>>,
  pub captures: Entity<Vec<Capture>>,
  pub capture_log: Entity<Vec<CaptureAccess>>,
}

impl Global for Pipewire {}

impl Pipewire {
  pub(super) fn new(
    cx: &mut App,
    commands: pipewire::channel::Sender<Command>,
    state: PipewireState,
    rx: flume::Receiver<AudioEvent>,
  ) -> Self {
    let audio = &state.audio;
    let pipewire = Self {
      sinks: cx.new(|_| audio.list(NodeType::Sink)),
      sources: cx.new(|_| audio.list(NodeType::Source)),
      streams: cx.new(|_| audio.list(NodeType::Stream)),
      default_sink: cx.new(|_| audio.default(NodeType::Sink)),
      default_source: cx.new(|_| audio.default(NodeType::Source)),
      targets: cx.new(|_| audio.resolved_targets()),
      captures: cx.new(|_| state.captures.list()),
      capture_log: cx.new(|_| {
        let mut log = Vec::new();
        record(&mut log, &state.captures.list(), SystemTime::now());
        log
      }),
      commands,
      state,
    };

    cx.spawn(async move |cx| {
      while let Ok(event) = rx.recv_async().await {
        cx.update(|cx| {
          cx.update_global::<Pipewire, _>(|pipewire, cx| match event {
            AudioEvent::Nodes(NodeType::Sink, nodes) => pipewire.sinks.write_changed(cx, nodes),
            AudioEvent::Nodes(NodeType::Source, nodes) => pipewire.sources.write_changed(cx, nodes),
            AudioEvent::Nodes(NodeType::Stream, nodes) => pipewire.streams.write_changed(cx, nodes),
            AudioEvent::DefaultSink(node) => pipewire.default_sink.write_changed(cx, node),
            AudioEvent::DefaultSource(node) => pipewire.default_source.write_changed(cx, node),
            AudioEvent::Targets(targets) => pipewire.targets.write_changed(cx, targets),
            AudioEvent::Captures(captures) => {
              pipewire.capture_log.update(cx, |log, cx| {
                if record(log, &captures, SystemTime::now()) {
                  cx.notify();
                }
              });
              pipewire.captures.write_changed(cx, captures);
            }
          });
        });
      }
    })
    .detach();

    pipewire
  }

  pub(super) fn send(&self, command: Command) -> Result<()> {
    if self.commands.send(command).is_err() {
      bail!("Failed to send command to pipewire");
    }
    Ok(())
  }

  pub fn audio(&self) -> PipewireAudio<'_> {
    PipewireAudio(self)
  }

  pub fn list_sinks<'c>(&self, cx: &'c App) -> &'c [AudioNode] {
    self.sinks.read(cx)
  }

  pub fn list_sources<'c>(&self, cx: &'c App) -> &'c [AudioNode] {
    self.sources.read(cx)
  }

  pub fn list_streams<'c>(&self, cx: &'c App) -> &'c [AudioNode] {
    self.streams.read(cx)
  }

  pub fn default_sink<'c>(&self, cx: &'c App) -> Option<&'c AudioNode> {
    self.default_sink.read(cx).as_ref()
  }

  pub fn default_source<'c>(&self, cx: &'c App) -> Option<&'c AudioNode> {
    self.default_source.read(cx).as_ref()
  }

  pub fn capture_log<'c>(&self, cx: &'c App) -> &'c [CaptureAccess] {
    self.capture_log.read(cx)
  }

  pub fn list_captures<'c>(&self, cx: &'c App) -> &'c [Capture] {
    self.captures.read(cx)
  }

  pub fn is_capturing(&self, kind: CaptureKind, cx: &App) -> bool {
    self
      .list_captures(cx)
      .iter()
      .any(|c| c.kind == kind && c.active)
  }

  pub fn capturing(&self, kind: CaptureKind, cx: &App) -> Vec<String> {
    let mut names: Vec<String> = self
      .list_captures(cx)
      .iter()
      .filter(|c| c.kind == kind && c.active && !c.name.is_empty())
      .map(|c| c.name.clone())
      .collect();
    names.sort();
    names.dedup();
    names
  }

  pub fn target(&self, stream: u32, cx: &App) -> Option<u32> {
    self.targets.read(cx).get(&stream).copied()
  }
}

#[cfg(test)]
mod tests {
  use std::{cell::Cell, rc::Rc};

  use gpui_kit::{self as gpui, TestAppContext};

  use super::*;
  use crate::{
    PipewireExt,
    command::Target,
    testing::{Commands, props},
  };

  fn node(id: u32, kind: NodeType, name: &str, pairs: &[(&str, &str)]) -> AudioNode {
    let mut all = vec![("node.name", name)];
    all.extend_from_slice(pairs);
    AudioNode::new(id, kind, props(&all).dict()).unwrap()
  }

  struct Running {
    events: flume::Sender<AudioEvent>,
    commands: Commands,
    state: PipewireState,
  }

  fn start(cx: &mut TestAppContext) -> Running {
    let state = PipewireState::new();
    state.audio.nodes.insert(
      54,
      node(54, NodeType::Sink, "speaker", &[("device.id", "47")]),
    );
    state
      .audio
      .nodes
      .insert(60, node(60, NodeType::Source, "mic", &[]));
    state
      .audio
      .nodes
      .insert(110, node(110, NodeType::Stream, "spotify", &[]));
    state
      .audio
      .defaults
      .insert(NodeType::Sink, "speaker".into());
    let screen = props(&[]);
    state.captures.insert(300, "Video/Source", screen.dict());
    let (tx, commands) = Commands::new();
    let (events, rx) = flume::unbounded();
    let thread_state = state.clone();
    cx.update(|cx| {
      let pipewire = Pipewire::new(cx, tx, thread_state, rx);
      cx.set_global(pipewire);
    });
    Running {
      events,
      commands,
      state,
    }
  }

  #[gpui::test]
  fn starts_from_the_state(cx: &mut TestAppContext) {
    let _running = start(cx);
    cx.read(|cx| {
      let pipewire = cx.pipewire();
      assert_eq!(pipewire.list_sinks(cx).len(), 1);
      assert_eq!(pipewire.list_sources(cx)[0].name, "mic");
      assert_eq!(pipewire.list_streams(cx)[0].id, 110);
      assert_eq!(pipewire.default_sink(cx).unwrap().id, 54);
      assert_eq!(pipewire.default_source(cx), None);
      assert_eq!(pipewire.target(110, cx), None);
      // the share that was running at startup is logged
      assert!(pipewire.is_capturing(CaptureKind::Screen, cx));
      assert_eq!(pipewire.capture_log(cx).len(), 1);
    });
  }

  #[gpui::test]
  fn events_update_the_entities(cx: &mut TestAppContext) {
    let running = start(cx);
    let sinks = cx.read(|cx| cx.pipewire().sinks.clone());
    let notified = Rc::new(Cell::new(0));
    let count = notified.clone();
    cx.update(|cx| {
      cx.observe(&sinks, move |_, _| count.set(count.get() + 1))
        .detach()
    });

    let headset = node(70, NodeType::Sink, "headset", &[]);
    let send = |event| running.events.send(event).unwrap();
    send(AudioEvent::Nodes(NodeType::Sink, vec![headset.clone()]));
    send(AudioEvent::Nodes(NodeType::Sink, vec![headset.clone()]));
    send(AudioEvent::Nodes(NodeType::Source, vec![]));
    send(AudioEvent::Nodes(NodeType::Stream, vec![]));
    send(AudioEvent::DefaultSink(Some(headset.clone())));
    send(AudioEvent::DefaultSource(Some(headset.clone())));
    send(AudioEvent::Targets(HashMap::from([(110, 70)])));
    cx.run_until_parked();
    // the same list again is no change
    assert_eq!(notified.get(), 1);
    cx.read(|cx| {
      let pipewire = cx.pipewire();
      assert_eq!(pipewire.list_sinks(cx), std::slice::from_ref(&headset));
      assert!(pipewire.list_sources(cx).is_empty() && pipewire.list_streams(cx).is_empty());
      assert_eq!(pipewire.default_sink(cx).unwrap().id, 70);
      assert_eq!(pipewire.default_source(cx).unwrap().id, 70);
      assert_eq!(pipewire.target(110, cx), Some(70));
      assert_eq!(pipewire.target(111, cx), None);
    });
  }

  fn capture(id: u32, kind: CaptureKind, name: &str, active: bool) -> Capture {
    Capture {
      id,
      kind,
      name: name.into(),
      active,
    }
  }

  #[gpui::test]
  fn captures_and_their_log(cx: &mut TestAppContext) {
    let running = start(cx);
    let log = cx.read(|cx| cx.pipewire().capture_log.clone());
    let notified = Rc::new(Cell::new(0));
    let count = notified.clone();
    cx.update(|cx| {
      cx.observe(&log, move |_, _| count.set(count.get() + 1))
        .detach()
    });

    let captures = vec![
      capture(1, CaptureKind::Microphone, "Zoom", true),
      capture(2, CaptureKind::Microphone, "Discord", true),
      capture(3, CaptureKind::Microphone, "Zoom", true),
      capture(4, CaptureKind::Microphone, "", true),
      capture(5, CaptureKind::Camera, "Zoom", false),
    ];
    running
      .events
      .send(AudioEvent::Captures(captures.clone()))
      .unwrap();
    cx.run_until_parked();
    cx.read(|cx| {
      let pipewire = cx.pipewire();
      // sorted, unique, named and active only
      assert_eq!(
        pipewire.capturing(CaptureKind::Microphone, cx),
        ["Discord", "Zoom"]
      );
      assert!(pipewire.capturing(CaptureKind::Camera, cx).is_empty());
      assert!(!pipewire.is_capturing(CaptureKind::Camera, cx));
      assert!(!pipewire.is_capturing(CaptureKind::Screen, cx));
      assert_eq!(pipewire.list_captures(cx), captures);
      // the screen share ended, three named mics started
      let log = pipewire.capture_log(cx);
      assert_eq!(log.len(), 4);
      assert!(
        log
          .iter()
          .any(|e| e.kind == CaptureKind::Screen && e.ended.is_some())
      );
    });
    assert_eq!(notified.get(), 1);
    // the same captures again change no log
    running.events.send(AudioEvent::Captures(captures)).unwrap();
    cx.run_until_parked();
    assert_eq!(notified.get(), 1);
  }

  #[gpui::test]
  fn audio_commands(cx: &mut TestAppContext) {
    let running = start(cx);
    // the sink has a card route once its info names the profile device
    running
      .state
      .audio
      .nodes
      .get_mut(&54)
      .unwrap()
      .profile_device = Some(1);
    cx.read(|cx| {
      let audio = cx.pipewire().audio();
      assert_eq!(audio.node(54).unwrap().name, "speaker");
      assert!(audio.node(1).is_none());
      audio.set_default(60).unwrap();
      audio.set_target(110, 54).unwrap();
      audio.reset_target(110).unwrap();
      audio.set_volumes(54, vec![0.5, 0.5]).unwrap();
      audio.set_mute(110, true).unwrap();
      for error in [
        audio.set_default(1),
        audio.set_target(110, 1),
        audio.set_volumes(1, vec![]),
        audio.set_mute(1, true),
      ] {
        assert_eq!(error.unwrap_err().to_string(), "No such audio node");
      }
    });
    let sent: Vec<String> = running
      .commands
      .take()
      .iter()
      .map(|c| format!("{c:?}"))
      .collect();
    assert_eq!(
      sent,
      [
        "SetDefault { kind: Source, name: \"mic\" }",
        "SetTarget { node: 110, name: Some(\"speaker\") }",
        "SetTarget { node: 110, name: None }",
        format!(
          "SetVolumes {{ target: {:?}, volumes: [0.5, 0.5] }}",
          Target::Route {
            node: 54,
            device: 47,
            profile_device: 1
          }
        )
        .as_str(),
        "SetMute { target: Node(110), mute: true }",
      ]
    );
  }

  #[gpui::test]
  fn a_device_without_a_profile_is_written_on_the_node(cx: &mut TestAppContext) {
    let running = start(cx);
    cx.read(|cx| cx.pipewire().audio().set_mute(54, false).unwrap());
    let sent = running.commands.take();
    assert!(matches!(
      sent.as_slice(),
      [Command::SetMute {
        target: Target::Node(54),
        mute: false
      }]
    ));
  }

  #[gpui::test]
  fn audio_commands_edge_cases(cx: &mut TestAppContext) {
    let running = start(cx);
    cx.read(|cx| {
      let audio = cx.pipewire().audio();
      // set_default on a Stream node ID sends Command::SetDefault with Stream kind
      audio.set_default(110).unwrap();
      // set_target with an invalid stream ID succeeds as long as target exists
      audio.set_target(999, 54).unwrap();
      // set_target with a Source node ID as the target succeeds
      audio.set_target(110, 60).unwrap();
      // set_volumes accepts empty or arbitrary float vectors
      audio.set_volumes(110, vec![]).unwrap();
      audio.set_volumes(110, vec![-0.5, f32::NAN, 10.0]).unwrap();
    });
    let sent: Vec<String> = running
      .commands
      .take()
      .iter()
      .map(|c| format!("{c:?}"))
      .collect();
    assert_eq!(
      sent,
      [
        "SetDefault { kind: Stream, name: \"spotify\" }",
        "SetTarget { node: 999, name: Some(\"speaker\") }",
        "SetTarget { node: 110, name: Some(\"mic\") }",
        "SetVolumes { target: Node(110), volumes: [] }",
        "SetVolumes { target: Node(110), volumes: [-0.5, NaN, 10.0] }",
      ]
    );
  }
}

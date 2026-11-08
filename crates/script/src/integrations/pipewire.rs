use corona_js::{
  Cx, Glob, Json, ModuleBuilder, PluginRef, ScriptModule, Throws, host_module, json, track,
};
use corona_pipewire::{
  AudioNode, Capture as PwCapture, CaptureAccess as PwCaptureAccess, CaptureKind, NodeType,
  PipewireExt,
};
use gpui_kit::App;
use serde::Serialize;
use specta::{Type, specta};

type Pw = Glob<corona_pipewire::Pipewire>;

// The script's view of an `AudioNode`: `volume` added, device ids left out.
#[derive(Serialize, Type)]
#[serde(rename = "AudioNode")]
struct Node {
  id: u32,
  serial: u64,
  kind: NodeType,
  name: String,
  description: String,
  nickname: Option<String>,
  volume: f32,
  volumes: Vec<f32>,
  mute: bool,
  app: Vec<String>,
}

impl From<&AudioNode> for Node {
  fn from(node: &AudioNode) -> Self {
    Self {
      id: node.id,
      serial: node.serial,
      kind: node.kind,
      name: node.name.clone(),
      description: node.description.clone(),
      nickname: node.nickname.clone(),
      volume: node.volume(),
      volumes: node.volumes.clone(),
      mute: node.mute,
      app: node.app.clone(),
    }
  }
}

#[derive(Serialize, Type)]
struct Capture {
  id: u32,
  kind: CaptureKind,
  name: String,
  active: bool,
}

impl From<&PwCapture> for Capture {
  fn from(capture: &PwCapture) -> Self {
    Self {
      id: capture.id,
      kind: capture.kind,
      name: capture.name.clone(),
      active: capture.active,
    }
  }
}

#[derive(Serialize, Type)]
struct CaptureAccess {
  kind: CaptureKind,
  name: Option<String>,
  started: f64,
  ended: Option<f64>,
}

fn unix(time: std::time::SystemTime) -> f64 {
  time
    .duration_since(std::time::UNIX_EPOCH)
    .map_or(0., |d| d.as_secs_f64())
}

impl From<&PwCaptureAccess> for CaptureAccess {
  fn from(access: &PwCaptureAccess) -> Self {
    Self {
      kind: access.kind,
      name: access.name.clone(),
      started: unix(access.started),
      ended: access.ended.map(unix),
    }
  }
}

fn nodes(nodes: &[AudioNode]) -> Vec<Node> {
  nodes.iter().map(Node::from).collect()
}

#[specta]
fn set_default(pipewire: Pw, id: u32) -> Throws<()> {
  pipewire.audio().set_default(id).into()
}

#[specta]
fn set_target(pipewire: Pw, stream: u32, sink: u32) -> Throws<()> {
  pipewire.audio().set_target(stream, sink).into()
}

#[specta]
fn reset_target(pipewire: Pw, stream: u32) -> Throws<()> {
  pipewire.audio().reset_target(stream).into()
}

#[specta]
fn set_volume(pipewire: Pw, id: u32, volume: f32) -> Throws<()> {
  pipewire.audio().set_level(id, volume).into()
}

#[specta]
fn set_mute(pipewire: Pw, id: u32, mute: bool) -> Throws<()> {
  pipewire.audio().set_mute(id, mute).into()
}

/// The id of the sink a stream is routed through, or null to follow the default.
#[specta]
fn target(cx: Cx, pipewire: Pw, stream: u32) -> Json<Option<u32>> {
  track(&cx.pipewire().targets);
  Json(pipewire.target(stream, &cx))
}

/// `corona/pipewire`
pub struct PipewireModule;

impl ScriptModule for PipewireModule {
  const NAME: &str = "pipewire";
  const DOC: &str = "Audio devices, streams and who records. Volumes are linear, 1 is 100%.";

  fn module(_: &PluginRef, m: &mut ModuleBuilder, _: &mut App) {
    m.watch(
      "listSinks",
      "",
      |cx| cx.pipewire().sinks.clone(),
      json(|cx| nodes(cx.pipewire().list_sinks(cx))),
    );
    m.watch(
      "listSources",
      "",
      |cx| cx.pipewire().sources.clone(),
      json(|cx| nodes(cx.pipewire().list_sources(cx))),
    );
    m.watch(
      "listStreams",
      "",
      |cx| cx.pipewire().streams.clone(),
      json(|cx| nodes(cx.pipewire().list_streams(cx))),
    );
    m.watch(
      "defaultSink",
      "",
      |cx| cx.pipewire().default_sink.clone(),
      json(|cx| cx.pipewire().default_sink(cx).map(Node::from)),
    );
    m.watch(
      "defaultSource",
      "",
      |cx| cx.pipewire().default_source.clone(),
      json(|cx| cx.pipewire().default_source(cx).map(Node::from)),
    );
    m.watch(
      "listCaptures",
      "",
      |cx| cx.pipewire().captures.clone(),
      json(|cx| {
        let captures = cx.pipewire().list_captures(cx);
        captures.iter().map(Capture::from).collect::<Vec<_>>()
      }),
    );
    m.watch(
      "captureLog",
      "",
      |cx| cx.pipewire().capture_log.clone(),
      json(|cx| {
        let log = cx.pipewire().capture_log(cx);
        log.iter().map(CaptureAccess::from).collect::<Vec<_>>()
      }),
    );
    host_module!(m, {
      sync: [target, set_default, set_target, reset_target, set_volume, set_mute],
    });
  }
}

#[cfg(test)]
mod tests {
  use std::time::{Duration, UNIX_EPOCH};

  use super::*;

  fn node(volumes: Vec<f32>) -> AudioNode {
    AudioNode {
      id: 7,
      serial: 70,
      kind: NodeType::Sink,
      name: "alsa_output".into(),
      description: "Speakers".into(),
      nickname: None,
      device: Some(1),
      profile_device: Some(2),
      volumes,
      mute: true,
      app: vec!["firefox".into()],
      pid: None,
      ready: true,
    }
  }

  #[test]
  fn volume_is_the_first_channel() {
    let json = serde_json::to_value(Node::from(&node(vec![0.4, 0.6]))).unwrap();
    assert_eq!(json["volume"], 0.4f32);
    assert_eq!(json["volumes"].as_array().unwrap().len(), 2);
    assert_eq!(json["id"], 7);
    assert_eq!(json["mute"], true);
    // device ids stay in the shell
    assert!(json.get("device").is_none());
    assert!(json.get("profile_device").is_none());

    let json = serde_json::to_value(Node::from(&node(vec![]))).unwrap();
    assert_eq!(json["volume"], 0.);
  }

  #[test]
  fn captures() {
    let capture = PwCapture {
      id: 3,
      kind: CaptureKind::Microphone,
      name: "obs".into(),
      active: true,
    };
    let json = serde_json::to_value(Capture::from(&capture)).unwrap();
    assert_eq!(json["id"], 3);
    assert_eq!(json["name"], "obs");
    assert_eq!(json["active"], true);
  }

  #[test]
  fn unix_times() {
    assert_eq!(unix(UNIX_EPOCH + Duration::from_millis(2500)), 2.5);
    assert_eq!(unix(UNIX_EPOCH), 0.);
    assert_eq!(unix(UNIX_EPOCH - Duration::from_secs(10)), 0.);
  }
}

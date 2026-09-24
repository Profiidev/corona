use gpui_kit::{App, Entity};
use gpui_shell::{HostArguments, HostModule, HostObject, HostResult, HostValue, with_current_app};

use crate::{
  integration::pipewire::{AudioNode, NodeType, Pipewire, PipewireExt},
  script::module::{Subscribe, Subscriptions, watch},
};

const DECLARATIONS: &str = r#"
export enum NodeKind {
  Sink = "sink",
  Source = "source",
  Stream = "stream",
}

export interface AudioNode {
  id: number;
  serial: number;
  kind: NodeKind;
  name: string;
  description: string;
  nickname: string | null;
  volume: number;
  volumes: number[];
  mute: boolean;
  app: string[];
}

export interface Error {
  message: string;
}

export function listSinks(): AudioNode[];
export function listSources(): AudioNode[];
export function listStreams(): AudioNode[];

export function defaultSink(): AudioNode | null;
export function defaultSource(): AudioNode | null;

/** The id of the sink a stream is routed through, or null to follow the default. */
export function target(stream: number): number | null;

export function setDefault(id: number): Error | null;
export function setTarget(stream: number, sink: number): Error | null;
export function resetTarget(stream: number): Error | null;
export function setVolume(id: number, volume: number): Error | null;
export function setMute(id: number, mute: boolean): Error | null;
"#;

impl From<NodeType> for HostValue {
  fn from(value: NodeType) -> Self {
    match value {
      NodeType::Sink => "sink",
      NodeType::Source => "source",
      NodeType::Stream => "stream",
    }
    .into()
  }
}

impl From<AudioNode> for HostValue {
  fn from(value: AudioNode) -> Self {
    HostObject::new()
      .field("id", value.id)
      .field("serial", value.serial)
      .field("kind", value.kind)
      .field("name", value.name)
      .field("description", value.description)
      .field("nickname", value.nickname)
      .field("volume", value.volumes.first().copied().unwrap_or(0.))
      .field("volumes", value.volumes)
      .field("mute", value.mute)
      .field("app", value.app)
      .into()
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Updates {
  Sinks,
  Sources,
  Streams,
  DefaultSink,
  DefaultSource,
  Targets,
}

impl From<Updates> for super::Updates {
  fn from(value: Updates) -> Self {
    super::Updates::Pipewire(value)
  }
}

fn read<W: 'static, R: Into<HostValue>>(
  reads: &Subscriptions,
  subs: &mut Vec<Subscribe>,
  update: Updates,
  entity: Entity<W>,
  read: impl Fn(&Pipewire, &App) -> R + 'static,
) -> impl Fn(&HostArguments) -> HostResult + 'static {
  let sub = watch(reads, update.into(), entity.clone());
  subs.push(sub);
  let reads = reads.clone();

  move |_| {
    reads.record(update.into());
    Ok(with_current_app(|cx| read(cx.pipewire(), cx).into()).unwrap_or(HostValue::Null))
  }
}

fn command(body: impl FnOnce(&Pipewire) -> anyhow::Result<()>) -> HostValue {
  with_current_app(|cx| match body(cx.pipewire()) {
    Ok(()) => HostValue::Null,
    Err(error) => HostObject::new().field("message", error.to_string()).into(),
  })
  .unwrap_or(HostValue::Null)
}

pub fn module(reads: &Subscriptions, subs: &mut Vec<Subscribe>, cx: &mut App) -> HostModule {
  HostModule::new("corona/pipewire")
    .declarations(DECLARATIONS)
    .function(
      "listSinks",
      read(
        reads,
        subs,
        Updates::Sinks,
        cx.pipewire().sinks.clone(),
        |pipewire, cx| pipewire.list_sinks(cx).to_vec(),
      ),
    )
    .function(
      "listSources",
      read(
        reads,
        subs,
        Updates::Sources,
        cx.pipewire().sources.clone(),
        |pipewire, cx| pipewire.list_sources(cx).to_vec(),
      ),
    )
    .function(
      "listStreams",
      read(
        reads,
        subs,
        Updates::Streams,
        cx.pipewire().streams.clone(),
        |pipewire, cx| pipewire.list_streams(cx).to_vec(),
      ),
    )
    .function(
      "defaultSink",
      read(
        reads,
        subs,
        Updates::DefaultSink,
        cx.pipewire().default_sink.clone(),
        |pipewire, cx| pipewire.default_sink(cx).cloned(),
      ),
    )
    .function(
      "defaultSource",
      read(
        reads,
        subs,
        Updates::DefaultSource,
        cx.pipewire().default_source.clone(),
        |pipewire, cx| pipewire.default_source(cx).cloned(),
      ),
    )
    .function("target", {
      let sub = watch(
        reads,
        Updates::Targets.into(),
        cx.pipewire().targets.clone(),
      );
      subs.push(sub);
      let reads = reads.clone();

      move |args: &HostArguments| {
        reads.record(Updates::Targets.into());
        let stream = args.integer(0)?;
        if stream < 0 || stream > u32::MAX as i64 {
          return Err(gpui_shell::HostError::new("stream id must be a valid u32"));
        }
        let stream = stream as u32;

        Ok(
          with_current_app(|cx| cx.pipewire().target(stream, cx).into()).unwrap_or(HostValue::Null),
        )
      }
    })
    .function("setDefault", |args| {
      let id = args.integer(0)? as u32;
      Ok(command(|pipewire| pipewire.audio().set_default(id)))
    })
    .function("setTarget", |args| {
      let stream = args.integer(0)? as u32;
      let sink = args.integer(1)? as u32;
      Ok(command(|pipewire| {
        pipewire.audio().set_target(stream, sink)
      }))
    })
    .function("resetTarget", |args| {
      let stream = args.integer(0)? as u32;
      Ok(command(|pipewire| pipewire.audio().reset_target(stream)))
    })
    .function("setVolume", |args| {
      let id = args.integer(0)? as u32;
      let volume = args.number(1)?;

      Ok(command(|pipewire| {
        let channels = pipewire
          .audio()
          .node(id)
          .map_or(1, |node| node.volumes.len().max(1));

        pipewire
          .audio()
          .set_volumes(id, vec![volume as f32; channels])
      }))
    })
    .function("setMute", |args| {
      let id = args.integer(0)? as u32;
      let mute = args.boolean(1)?;
      Ok(command(|pipewire| pipewire.audio().set_mute(id, mute)))
    })
}

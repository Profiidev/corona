//! Audio outputs and inputs as results, app volumes and muting the focused app

use std::rc::Rc;

use corona_desktop::entry::name_for_names;
use corona_ipc::IpcCommand;
use corona_launcher::{
  Accessory, Action, ArgSpec, Command, CommandSource, Hud, Icon, ListView, Meta, Outcome, Query,
  Section,
  kit::{ListSource, Notify, level_target, opener, percent, watch},
  registry, simple,
};
use corona_pipewire::{AudioNode, PipewireExt, volume::to_slider};
use gpui_kit::{App, SharedString, Subscription, assets::IconName};
use rust_i18n::t;

use crate::commands::{
  audio::{Kind, MoveStream, MuteFocused, SetDefault, StreamVolume},
  volume::Action as VolumeAction,
};

fn label(node: &AudioNode) -> String {
  node
    .nickname
    .clone()
    .unwrap_or_else(|| node.description.clone())
}

fn device_row(node: &AudioNode, kind: Kind, default: bool) -> Rc<dyn Command> {
  let n = node.clone();
  let id = node.id.to_string();
  simple(
    format!("audio:{}", node.name),
    move |_| Meta {
      title: match kind {
        Kind::Outputs => t!("app.launcher.devices.output", name = label(&n)),
        _ => t!("app.launcher.devices.input", name = label(&n)),
      }
      .into(),
      icon: Icon::Tile(
        match kind {
          Kind::Outputs => IconName::Speaker,
          _ => IconName::Mic,
        },
        160.,
      ),
      keywords: vec!["audio".into(), "sound".into(), "device".into()],
      section: Section::Results,
      accessories: match default {
        true => vec![Accessory::Icon(IconName::Check)],
        false => Vec::new(),
      },
      ..Default::default()
    },
    move |_| {
      let id = id.clone();
      vec![Action::sync(
        "default",
        t!("app.launcher.devices.make_default"),
        move |_, _, cx| {
          SetDefault::handle((kind, id.clone()), cx)?;
          Ok(Outcome::hud(t!("app.launcher.devices.switched")))
        },
      )]
    },
  )
}

fn observe(notify: Notify, cx: &mut App) -> Vec<Subscription> {
  let pw = cx.pipewire();
  let entities = (
    pw.sinks.clone(),
    pw.sources.clone(),
    pw.default_sink.clone(),
    pw.default_source.clone(),
  );
  vec![
    watch(&entities.0, &notify, cx),
    watch(&entities.1, &notify, cx),
    watch(&entities.2, &notify, cx),
    watch(&entities.3, &notify, cx),
  ]
}

/// One row per output and input, only once typing
struct Devices;

impl CommandSource for Devices {
  fn id(&self) -> SharedString {
    "audio:devices".into()
  }

  fn commands(&self, query: &Query, cx: &App) -> Vec<Rc<dyn Command>> {
    if query.is_empty() {
      return Vec::new();
    }
    let pw = cx.pipewire();
    let default_sink = pw.default_sink(cx).map(|n| n.id);
    let default_source = pw.default_source(cx).map(|n| n.id);
    pw.list_sinks(cx)
      .iter()
      .map(|n| device_row(n, Kind::Outputs, Some(n.id) == default_sink))
      .chain(
        pw.list_sources(cx)
          .iter()
          .map(|n| device_row(n, Kind::Inputs, Some(n.id) == default_source)),
      )
      .collect()
  }

  fn observe(&self, notify: Notify, cx: &mut App) -> Vec<Subscription> {
    observe(notify, cx)
  }
}

/// An app's name for a stream
fn app_name(stream: &AudioNode) -> String {
  name_for_names(stream.app.iter().map(String::as_str))
    .or_else(|| stream.app.get(2).cloned())
    .unwrap_or_else(|| stream.description.clone())
}

fn stream_row(stream: &AudioNode, sinks: &[AudioNode]) -> Rc<dyn Command> {
  let s = stream.clone();
  let sinks = sinks.to_vec();
  simple(
    format!("stream:{}", stream.serial),
    move |_| Meta {
      title: app_name(&s).into(),
      subtitle: Some(match s.mute {
        true => t!("app.launcher.audio.muted").into(),
        false => percent(to_slider(s.volume())).into(),
      }),
      icon: Icon::App(
        s.app
          .iter()
          .map(|a| SharedString::from(a.clone()))
          .collect(),
      ),
      section: Section::Results,
      accessories: vec![Accessory::Progress(to_slider(s.volume()))],
      args: vec![ArgSpec::level(t!("app.launcher.audio.level")).words(["mute"])],
      ..Default::default()
    },
    {
      let s = stream.clone();
      move |_| {
        let id = s.id.to_string();
        let current = to_slider(s.volume());
        let mut actions = vec![
          Action::sync("set", t!("app.launcher.audio.set"), {
            let id = id.clone();
            move |args, _, cx| {
              let action = match args.get(0) {
                Some(value) => match level_target(value, current) {
                  Some(level) => VolumeAction::Set(level),
                  None => VolumeAction::ToggleMute,
                },
                None => VolumeAction::ToggleMute,
              };
              StreamVolume::handle((id.clone(), action), cx)?;
              Ok(Outcome::KeepOpen)
            }
          })
          .keep_open(),
        ];
        for sink in &sinks {
          let (stream, sink_id, name) = (id.clone(), sink.id.to_string(), label(sink));
          actions.push(Action::sync(
            format!("move:{}", sink.id),
            t!("app.launcher.devices.play_on", name = name),
            move |_, _, cx| {
              MoveStream::handle((stream.clone(), sink_id.clone()), cx)?;
              Ok(Outcome::KeepOpen)
            },
          ));
        }
        actions
      }
    },
  )
}

pub fn init(cx: &mut App) {
  registry::register_source(Rc::new(Devices), cx);
  registry::register_all(
    [
      opener(
        "audio:apps",
        t!("app.launcher.devices.app_volume"),
        Icon::Tile(IconName::SlidersVertical, 160.),
        &["volume", "mixer", "stream"],
        |_| {
          ListView::new(
            t!("app.launcher.devices.app_volume"),
            t!("app.launcher.devices.search_apps"),
            Rc::new(
              ListSource::new("audio:streams", |_, cx| {
                let pw = cx.pipewire();
                let sinks = pw.list_sinks(cx).to_vec();
                pw.list_streams(cx)
                  .iter()
                  .map(|s| stream_row(s, &sinks))
                  .collect()
              })
              .observe(|notify, cx| {
                let (streams, sinks) = (cx.pipewire().streams.clone(), cx.pipewire().sinks.clone());
                vec![watch(&streams, &notify, cx), watch(&sinks, &notify, cx)]
              }),
            ),
          )
          .empty(t!("app.launcher.devices.no_streams"))
          .into_view()
        },
      ),
      simple(
        "audio:mute_focused",
        |_| Meta {
          title: t!("app.launcher.devices.mute_app").into(),
          icon: Icon::Tile(IconName::VolumeX, 160.),
          keywords: vec!["mute".into(), "audio".into(), "app".into()],
          ..Default::default()
        },
        |_| {
          vec![Action::sync(
            "mute",
            t!("app.launcher.devices.mute_app"),
            |_, _, cx| {
              let muted = MuteFocused::handle((), cx)?;
              Ok(Outcome::Close(Some(Hud::new(match muted {
                true => t!("app.launcher.audio.muted"),
                false => t!("app.launcher.devices.unmuted"),
              }))))
            },
          )]
        },
      ),
    ],
    cx,
  );
}

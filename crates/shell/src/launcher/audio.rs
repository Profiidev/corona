//! Output and microphone volume, mute and presets

use std::rc::Rc;

use anyhow::Result;
use corona_ipc::IpcCommand;
use corona_launcher::{
  Accessory, Action, ArgSpec, ArgValue, Args, Command, Hud, Icon, Meta, Outcome,
  kit::{Group, level_target, percent, watch},
  simple,
};
use corona_pipewire::{PipewireExt, volume::to_slider};
use corona_surface::panel::PanelState;
use gpui_kit::{App, SharedString, assets::IconName};
use rust_i18n::t;

use crate::commands::volume::{Action as VolumeAction, Device, Volume};

/// The default device's slider level and mute
fn level(device: Device, cx: &App) -> Option<(f32, bool)> {
  let pipewire = cx.pipewire();
  let node = match device {
    Device::Output => pipewire.default_sink(cx),
    Device::Input => pipewire.default_source(cx),
  }?;
  Some((to_slider(node.volume()), node.mute))
}

fn name(device: Device) -> SharedString {
  match device {
    Device::Output => t!("app.launcher.audio.volume").into(),
    Device::Input => t!("app.launcher.audio.mic").into(),
  }
}

/// What the inline argument does: a level, a step or mute
fn action(args: &Args, current: f32) -> Option<VolumeAction> {
  match args.get(0)? {
    ArgValue::Word(_) => Some(VolumeAction::ToggleMute),
    value => level_target(value, current).map(VolumeAction::Set),
  }
}

fn describe(device: Device, action: VolumeAction) -> SharedString {
  match action {
    VolumeAction::Set(level) => t!(
      "app.launcher.audio.set_to",
      name = name(device),
      level = percent(level)
    )
    .into(),
    _ => t!("app.launcher.audio.toggle_mute", name = name(device)).into(),
  }
}

fn run(device: Device, action: VolumeAction, cx: &mut App) -> Result<Outcome> {
  Volume::handle((device, action), cx)?;
  Ok(Outcome::Close(Some(
    Hud::new(describe(device, action)).icon(match device {
      Device::Output => IconName::Volume2,
      Device::Input => IconName::Mic,
    }),
  )))
}

struct LevelCommand(Device);

impl Command for LevelCommand {
  fn id(&self) -> SharedString {
    match self.0 {
      Device::Output => "volume".into(),
      Device::Input => "mic_volume".into(),
    }
  }

  fn meta(&self, cx: &App) -> Meta {
    let current = level(self.0, cx);
    Meta {
      title: name(self.0),
      subtitle: current.map(|(level, mute)| {
        match mute {
          true => t!("app.launcher.audio.muted"),
          false => percent(level).into(),
        }
        .into()
      }),
      icon: Icon::Tile(
        match self.0 {
          Device::Output => IconName::Volume2,
          Device::Input => IconName::Mic,
        },
        160.,
      ),
      keywords: match self.0 {
        Device::Output => vec!["vol".into(), "sound".into(), "audio".into()],
        Device::Input => vec!["microphone".into(), "input".into()],
      },
      accessories: current
        .map(|(level, _)| Accessory::Progress(level))
        .into_iter()
        .collect(),
      args: vec![
        ArgSpec::level(t!("app.launcher.audio.level"))
          .words(["mute"])
          .optional(),
      ],
      ..Default::default()
    }
  }

  fn describe(&self, args: &Args, cx: &App) -> Option<SharedString> {
    let (current, _) = level(self.0, cx)?;
    Some(describe(self.0, action(args, current)?))
  }

  fn actions(&self, _: &App) -> Vec<Action> {
    let device = self.0;
    let step = |id: &'static str, title: SharedString, delta: f32, keys: &'static str| {
      Action::sync(id, title, move |_, _, cx| {
        let (current, _) = level(device, cx).unwrap_or_default();
        Volume::handle(
          (device, VolumeAction::Set((current + delta).clamp(0., 1.))),
          cx,
        )?;
        Ok(Outcome::KeepOpen)
      })
      .shortcut(keys)
      .keep_open()
    };
    vec![
      Action::sync("set", t!("app.launcher.audio.set"), move |args, _, cx| {
        let (current, _) = level(device, cx).unwrap_or_default();
        match action(args, current) {
          Some(action) => run(device, action, cx),
          // without a level the audio page shows every device
          None => {
            PanelState::show("audio", false, cx)?;
            Ok(Outcome::close())
          }
        }
      }),
      step("up", t!("app.launcher.audio.up").into(), 0.05, "ctrl-up"),
      step(
        "down",
        t!("app.launcher.audio.down").into(),
        -0.05,
        "ctrl-down",
      ),
      Action::sync(
        "mute",
        t!("app.launcher.audio.toggle_mute", name = name(device)),
        move |_, _, cx| run(device, VolumeAction::ToggleMute, cx),
      ),
    ]
  }
}

pub fn init(cx: &mut App) {
  let mut commands: Vec<Rc<dyn Command>> = vec![
    Rc::new(LevelCommand(Device::Output)),
    Rc::new(LevelCommand(Device::Input)),
    simple(
      "mic_mute",
      |cx| {
        let muted = level(Device::Input, cx).is_some_and(|(_, m)| m);
        Meta {
          title: match muted {
            true => t!("app.launcher.audio.unmute_mic"),
            false => t!("app.launcher.audio.mute_mic"),
          }
          .into(),
          icon: Icon::Tile(
            if muted {
              IconName::Mic
            } else {
              IconName::MicOff
            },
            160.,
          ),
          keywords: vec!["microphone".into(), "mute".into()],
          ..Default::default()
        }
      },
      |_| {
        vec![Action::sync(
          "mute",
          t!("app.launcher.switch"),
          |_, _, cx| run(Device::Input, VolumeAction::ToggleMute, cx),
        )]
      },
    ),
  ];
  for preset in [0u8, 25, 50, 75, 100] {
    commands.push(simple(
      format!("volume:{preset}"),
      move |_| Meta {
        title: t!("app.launcher.audio.preset", level = preset).into(),
        icon: Icon::Tile(IconName::Volume1, 160.),
        keywords: vec!["volume".into(), "vol".into()],
        ..Default::default()
      },
      move |_| {
        vec![Action::sync(
          "set",
          t!("app.launcher.audio.set"),
          move |_, _, cx| {
            run(
              Device::Output,
              VolumeAction::Set(f32::from(preset) / 100.),
              cx,
            )
          },
        )]
      },
    ));
  }
  Group::new("audio", commands)
    .observe(|notify, cx| {
      let (sink, source) = (
        cx.pipewire().default_sink.clone(),
        cx.pipewire().default_source.clone(),
      );
      vec![watch(&sink, &notify, cx), watch(&source, &notify, cx)]
    })
    .register(cx);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn inline_levels() {
    let spec = [ArgSpec::level("level").words(["mute"])];
    let parse = |s| Args::parse(&spec, s).unwrap();
    assert!(
      matches!(action(&parse("40"), 0.1), Some(VolumeAction::Set(l)) if (l - 0.4).abs() < 1e-6)
    );
    assert!(
      matches!(action(&parse("+5"), 0.5), Some(VolumeAction::Set(l)) if (l - 0.55).abs() < 1e-6)
    );
    assert!(matches!(
      action(&parse("mute"), 0.5),
      Some(VolumeAction::ToggleMute)
    ));
  }
}

use anyhow::{Result, bail};
use gpui_kit::Global;
use pipewire::{channel::AttachedReceiver, main_loop::MainLoop};

use crate::integration::pipewire::state::PipewireState;

pub struct Pipewire(pipewire::channel::Sender<CommandBox>);

impl Pipewire {
  pub fn init(
    mainloop: &MainLoop,
    state: PipewireState,
  ) -> (Self, AttachedReceiver<'_, Box<dyn Command + 'static>>) {
    let (tx, rx) = pipewire::channel::channel();

    let rx = rx.attach(mainloop.loop_(), move |msg: Box<dyn Command + 'static>| {
      msg.execute(&state);
    });

    (Self(tx), rx)
  }

  pub fn list_audio_sinks(&self) -> Result<Vec<AudioSink>> {
    let (tx, rx) = flume::bounded(1);
    let cmd = Box::new(ListAudioSinks { res: tx });
    if self.0.send(cmd).is_err() {
      bail!("Failed to send cmd");
    }
    rx.recv().map_err(|e| e.into())
  }
}

impl Global for Pipewire {}

type CommandBox = Box<dyn Command>;

pub trait Command: Send {
  fn execute(&self, state: &PipewireState);
}

struct ListAudioSinks {
  res: flume::Sender<Vec<AudioSink>>,
}

#[derive(Debug)]
pub struct AudioSink {
  pub id: u32,
  pub name: String,
  pub description: String,
  pub nickname: Option<String>,
  pub device: u32,
  pub volumes: Vec<f32>,
  pub mute: bool,
}

impl Command for ListAudioSinks {
  fn execute(&self, state: &PipewireState) {
    let sinks = state
      .audio
      .sinks
      .iter()
      .map(|entry| AudioSink {
        id: entry.id,
        name: entry.name.clone(),
        description: entry.description.clone(),
        nickname: entry.nickname.clone(),
        device: entry.device,
        volumes: entry.volumes.clone(),
        mute: entry.mute,
      })
      .collect();
    let _ = self.res.send(sinks);
  }
}

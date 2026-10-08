use anyhow::Result;
use clap::ValueEnum;
use corona_ipc::{IpcCommand, IpcServer};
use gpui_kit::{App, Modifiers};
use serde::{Deserialize, Serialize};

use crate::overlays::switcher::SwitcherState;

pub fn register_commands(server: &mut IpcServer) {
  server.register::<Cycle>();
}

pub struct Cycle;

impl IpcCommand for Cycle {
  const COMMAND: &'static str = "switcher:cycle";

  type Payload = Options;
  type Response = ();

  fn handle(options: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    SwitcherState::cycle(options, cx)
  }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Options {
  pub mode: Mode,
  pub modifier: Modifier,
  pub current_monitor: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum Mode {
  #[default]
  Window,
  Workspace,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum Modifier {
  #[default]
  Super,
  Alt,
  Ctrl,
}

impl Modifier {
  pub(super) fn held(self, modifiers: &Modifiers) -> bool {
    match self {
      Modifier::Super => modifiers.platform,
      Modifier::Alt => modifiers.alt,
      Modifier::Ctrl => modifiers.control,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn modifier_held() {
    let pressed = |m: Modifier| Modifiers {
      platform: m == Modifier::Super,
      alt: m == Modifier::Alt,
      control: m == Modifier::Ctrl,
      ..Default::default()
    };
    for m in Modifier::value_variants() {
      for other in Modifier::value_variants() {
        assert_eq!(m.held(&pressed(*other)), m == other, "{m:?} with {other:?}");
      }
      assert!(!m.held(&Modifiers::default()));
      // shift alone never counts
      let shift = Modifiers {
        shift: true,
        ..Default::default()
      };
      assert!(!m.held(&shift));
    }
  }

  #[test]
  fn options_json() {
    let options: Options =
      serde_json::from_str(r#"{"mode":"Workspace","modifier":"Alt","current_monitor":true}"#)
        .unwrap();
    assert_eq!(options.mode, Mode::Workspace);
    assert_eq!(options.modifier, Modifier::Alt);
    assert!(options.current_monitor);

    let back: Options = serde_json::from_str(&serde_json::to_string(&options).unwrap()).unwrap();
    assert_eq!(back.mode, options.mode);
    assert_eq!(back.modifier, options.modifier);
    assert_eq!(back.current_monitor, options.current_monitor);

    let default = Options::default();
    assert_eq!(default.mode, Mode::Window);
    assert_eq!(default.modifier, Modifier::Super);
    assert!(!default.current_monitor);
  }
}

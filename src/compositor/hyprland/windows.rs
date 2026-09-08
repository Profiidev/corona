use serde::Deserialize;

use crate::{compositor::types, data_cmd};

data_cmd!(
  list_windows,
  "clients",
  Vec<Window>,
  Vec<types::Window>,
  |windows: Vec<Window>| { windows.into_iter().map(|m| m.into()).collect() }
);

impl crate::compositor::hyprland::command::Ipc {
  pub fn active_window(&self) -> anyhow::Result<Option<types::Window>> {
    let cmd = crate::compositor::hyprland::command::Command {
      command: "activewindow".to_string(),
      flags: crate::compositor::hyprland::command::CommandFlags::JSON,
    };
    let res = self.send_cmd(&cmd)?;

    let value: serde_json::Value = serde_json::from_str(&res)?;
    if value.as_object().is_some_and(|window| window.is_empty()) {
      return Ok(None);
    }

    Ok(Some(serde_json::from_value::<Window>(value)?.into()))
  }
}

#[derive(Debug, Deserialize)]
pub struct Window {
  pub address: String,
  pub monitor: u32,
  pub class: String,
  pub title: String,
  pub workspace: WindowWorkspace,
  pub at: (i32, i32),
}
#[derive(Debug, Deserialize)]
pub struct WindowWorkspace {
  pub id: u32,
}

impl From<Window> for types::Window {
  fn from(w: Window) -> Self {
    types::Window {
      address: w.address,
      monitor: w.monitor,
      workspace: w.workspace.id,
      class: w.class,
      title: w.title,
      x: w.at.0,
      y: w.at.1,
    }
  }
}

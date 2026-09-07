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
  pub monitor: u32,
  pub class: String,
  pub title: String,
  pub workspace: WindowWorkspace,
}
#[derive(Debug, Deserialize)]
pub struct WindowWorkspace {
  pub id: u32,
}

impl From<Window> for types::Window {
  fn from(w: Window) -> Self {
    types::Window {
      monitor: w.monitor,
      workspace: w.workspace.id,
      class: w.class,
      title: w.title,
    }
  }
}

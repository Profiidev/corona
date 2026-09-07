use serde::Deserialize;

use crate::{compositor::types, data_cmd};

data_cmd!(
  list_windows,
  "clients",
  Vec<Window>,
  Vec<types::Window>,
  |windows: Vec<Window>| { windows.into_iter().map(|m| m.into()).collect() }
);

data_cmd!(
  active_window,
  "activewindow",
  Window,
  types::Window,
  |window: Window| { window.into() }
);

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

use serde::Deserialize;

use crate::{hypr_data_cmd, types};

impl crate::hyprland::command::Ipc {
  pub fn close_window(&self, address: &str) -> anyhow::Result<()> {
    self.dsp(format!("window.close({{ window = {} }})", window(address)))
  }

  pub fn focus_window(&self, address: &str) -> anyhow::Result<()> {
    self.dsp(format!("focus({{ window = {} }})", window(address)))
  }
}

fn window(address: &str) -> String {
  crate::hyprland::command::lua_string(&format!("address:{address}"))
}

hypr_data_cmd!(
  list_windows,
  "clients",
  Vec<Window>,
  Vec<types::Window>,
  |windows: Vec<Window>| {
    let mut windows: Vec<types::Window> = windows.into_iter().map(|m| m.into()).collect();
    windows.sort_unstable_by(|a, b| a.x.cmp(&b.x).then_with(|| a.y.cmp(&b.y)));
    windows
  }
);

impl crate::hyprland::command::Ipc {
  pub fn active_window(&self) -> anyhow::Result<Option<types::Window>> {
    let cmd = crate::hyprland::command::Command {
      command: "activewindow".to_string(),
      flags: crate::hyprland::command::CommandFlags::JSON,
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
  pub size: (i32, i32),
  pub floating: bool,
  pub pinned: bool,
  pub fullscreen: u8,
  pub hidden: bool,
  #[serde(rename = "focusHistoryID")]
  pub focus_history_id: i32,
}
#[derive(Debug, Deserialize)]
pub struct WindowWorkspace {
  pub address: String,
}

impl From<Window> for types::Window {
  fn from(w: Window) -> Self {
    types::Window {
      address: w.address,
      monitor: w.monitor,
      workspace: w.workspace.address,
      class: w.class,
      title: w.title,
      x: w.at.0,
      y: w.at.1,
      width: w.size.0,
      height: w.size.1,
      floating: w.floating,
      pinned: w.pinned,
      fullscreen: w.fullscreen != 0,
      hidden: w.hidden,
      focus_history_id: w.focus_history_id,
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::hyprland::fake::FakeHyprland;

  #[test]
  fn lists_windows() {
    let hypr = FakeHyprland::start();
    let windows = hypr.ipc().list_windows().unwrap();
    assert_eq!(windows.len(), 2);
    let (firefox, kitty) = (&windows[0], &windows[1]);
    assert_eq!(firefox.address, "0xa");
    assert_eq!(firefox.title, "Fünf ✓");
    assert_eq!(
      (firefox.x, firefox.y, firefox.width, firefox.height),
      (0, 30, 1920, 1050)
    );
    assert!(!firefox.fullscreen);
    // any fullscreen mode counts
    assert!(kitty.fullscreen);
    assert_eq!((kitty.workspace.as_str(), kitty.monitor), ("0x2", 1));
    assert!(kitty.floating);
  }

  #[test]
  fn active_window() {
    let hypr = FakeHyprland::start();
    assert_eq!(
      hypr.ipc().active_window().unwrap().unwrap().class,
      "firefox"
    );
    hypr.answer("j/activewindow", "{}");
    assert_eq!(hypr.ipc().active_window().unwrap(), None);
    hypr.answer("j/activewindow", r#"{"address": "0x1"}"#);
    assert!(hypr.ipc().active_window().is_err());
    hypr.answer("j/activewindow", "[]");
    assert!(hypr.ipc().active_window().is_err());
    hypr.answer("j/activewindow", "");
    assert!(hypr.ipc().active_window().is_err());
    hypr.answer("j/activewindow", "null");
    assert!(hypr.ipc().active_window().is_err());
  }

  #[test]
  fn negative_monitor_deserialization_fails() {
    let raw = r#"{
      "address": "0x1",
      "monitor": -1,
      "class": "test",
      "title": "test",
      "workspace": {"address": "0x1"},
      "at": [0, 0],
      "size": [100, 100],
      "floating": false,
      "pinned": false,
      "fullscreen": 0,
      "hidden": false,
      "focusHistoryID": 0
    }"#;
    assert!(serde_json::from_str::<super::Window>(raw).is_err());
  }

  #[test]
  fn window_dispatches() {
    let hypr = FakeHyprland::start();
    hypr.ipc().focus_window("0xa").unwrap();
    hypr.ipc().close_window("0xb").unwrap();
    assert_eq!(
      hypr.commands(),
      [
        r#"/eval hl.dispatch(hl.dsp.focus({ window = "address:0xa" }))"#,
        r#"/eval hl.dispatch(hl.dsp.window.close({ window = "address:0xb" }))"#,
      ]
    );
  }

  #[test]
  fn focus_window_cannot_inject_lua() {
    let hypr = FakeHyprland::start();
    hypr
      .ipc()
      .focus_window("0xa\" }) hl.exec_cmd(\"x\") --")
      .unwrap();
    hypr.ipc().close_window("0xa\\\" }) --").unwrap();
    assert_eq!(
      hypr.commands(),
      [
        r#"/eval hl.dispatch(hl.dsp.focus({ window = "address:0xa\" }) hl.exec_cmd(\"x\") --" }))"#,
        r#"/eval hl.dispatch(hl.dsp.window.close({ window = "address:0xa\\\" }) --" }))"#,
      ]
    );
  }
}

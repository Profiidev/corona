use anyhow::{Result, bail};
use serde::Deserialize;

use crate::{
  hypr_data_cmd,
  hyprland::command::{Command, CommandFlags, lua_string, lua_value},
};

fn window(address: &str) -> String {
  lua_string(&format!("address:{address}"))
}

impl crate::hyprland::command::Ipc {
  pub fn toggle_floating(&self, address: &str) -> Result<()> {
    self.dsp(format!(
      "window.float({{ action = \"toggle\", window = {} }})",
      window(address)
    ))
  }

  pub fn toggle_fullscreen(&self, address: &str) -> Result<()> {
    self.dsp(format!(
      "window.fullscreen({{ action = \"toggle\", window = {} }})",
      window(address)
    ))
  }

  pub fn toggle_pin(&self, address: &str) -> Result<()> {
    self.dsp(format!(
      "window.pin({{ action = \"toggle\", window = {} }})",
      window(address)
    ))
  }

  pub fn move_to_workspace(&self, address: &str, workspace: &str, follow: bool) -> Result<()> {
    self.dsp(format!(
      "window.move({{ workspace = {}, follow = {follow}, window = {} }})",
      lua_value(workspace),
      window(address)
    ))
  }

  pub fn move_to_monitor(&self, address: &str, monitor: &str) -> Result<()> {
    self.dsp(format!(
      "window.move({{ monitor = {}, window = {} }})",
      lua_string(monitor),
      window(address)
    ))
  }

  /// Floats the window and puts it at `x, y` with size `w × h`, layout pixels
  pub fn place_window(&self, address: &str, (x, y, w, h): (i32, i32, i32, i32)) -> Result<()> {
    let target = window(address);
    self.dsp(format!(
      "window.float({{ action = \"set\", window = {target} }})"
    ))?;
    self.dsp(format!(
      "window.resize({{ x = {w}, y = {h}, window = {target} }})"
    ))?;
    self.dsp(format!(
      "window.move({{ x = {x}, y = {y}, window = {target} }})"
    ))
  }

  pub fn focus_monitor(&self, monitor: &str) -> Result<()> {
    self.dsp(format!("focus({{ monitor = {} }})", lua_string(monitor)))
  }

  pub fn toggle_special_workspace(&self, name: &str) -> Result<()> {
    self.dsp(format!("workspace.toggle_special({})", lua_string(name)))
  }

  /// Switches the main keyboard to layout `index` of its configured ones
  pub fn set_keyboard_layout(&self, index: usize) -> Result<()> {
    let res = self.send_cmd(&Command {
      command: format!("switchxkblayout main {index}"),
      flags: CommandFlags::empty(),
    })?;
    if res.trim() != "ok" {
      bail!("Hyprland could not switch the layout: {res}");
    }
    Ok(())
  }
}

#[derive(Debug, Deserialize)]
pub struct Devices {
  keyboards: Vec<Keyboard>,
}

#[derive(Debug, Deserialize)]
struct Keyboard {
  layout: String,
  main: bool,
}

/// The main keyboard's configured layouts, like `["us", "de"]`
fn layouts(devices: Devices) -> Vec<String> {
  let keyboards = devices.keyboards;
  let main = keyboards.iter().position(|k| k.main).unwrap_or(0);
  keyboards
    .into_iter()
    .nth(main)
    .map(|k| {
      k.layout
        .split(',')
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
    })
    .unwrap_or_default()
}

hypr_data_cmd!(keyboard_layouts, "devices", Devices, Vec<String>, layouts);

#[cfg(test)]
mod tests {
  use crate::hyprland::fake::FakeHyprland;

  #[test]
  fn dispatches() {
    let hypr = FakeHyprland::start();
    let ipc = hypr.ipc();
    ipc.toggle_floating("0xa").unwrap();
    ipc.toggle_fullscreen("0xa").unwrap();
    ipc.toggle_pin("0xa").unwrap();
    ipc.move_to_workspace("0xa", "3", false).unwrap();
    ipc.move_to_workspace("0xa", "special:magic", true).unwrap();
    ipc.move_to_monitor("0xa", "DP-1").unwrap();
    ipc.focus_monitor("HDMI-A-1").unwrap();
    ipc.toggle_special_workspace("corona-desktop").unwrap();
    ipc.place_window("0xa", (10, 20, 300, 400)).unwrap();
    let w = r#"window = "address:0xa""#;
    assert_eq!(
      hypr.commands(),
      [
        format!(r#"/eval hl.dispatch(hl.dsp.window.float({{ action = "toggle", {w} }}))"#),
        format!(r#"/eval hl.dispatch(hl.dsp.window.fullscreen({{ action = "toggle", {w} }}))"#),
        format!(r#"/eval hl.dispatch(hl.dsp.window.pin({{ action = "toggle", {w} }}))"#),
        format!(
          r#"/eval hl.dispatch(hl.dsp.window.move({{ workspace = 3, follow = false, {w} }}))"#
        ),
        format!(
          r#"/eval hl.dispatch(hl.dsp.window.move({{ workspace = "special:magic", follow = true, {w} }}))"#
        ),
        format!(r#"/eval hl.dispatch(hl.dsp.window.move({{ monitor = "DP-1", {w} }}))"#),
        r#"/eval hl.dispatch(hl.dsp.focus({ monitor = "HDMI-A-1" }))"#.to_string(),
        r#"/eval hl.dispatch(hl.dsp.workspace.toggle_special("corona-desktop"))"#.to_string(),
        format!(r#"/eval hl.dispatch(hl.dsp.window.float({{ action = "set", {w} }}))"#),
        format!(r#"/eval hl.dispatch(hl.dsp.window.resize({{ x = 300, y = 400, {w} }}))"#),
        format!(r#"/eval hl.dispatch(hl.dsp.window.move({{ x = 10, y = 20, {w} }}))"#),
      ]
    );
  }

  #[test]
  fn layouts() {
    let devices = serde_json::from_str(
      r#"{ "keyboards": [
        { "layout": "us", "main": false },
        { "layout": "us, de", "main": true }
      ] }"#,
    )
    .unwrap();
    assert_eq!(super::layouts(devices), ["us", "de"]);
  }
}

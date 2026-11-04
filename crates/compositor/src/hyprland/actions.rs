use anyhow::{Result, bail};

use crate::hyprland::command::{Command, CommandFlags, lua_string, lua_value, window};

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

  /// Switches the main keyboard to `next`, `prev` or the layout index of its configured ones
  pub fn set_keyboard_layout(&self, which: &str) -> Result<()> {
    let index = !which.is_empty() && which.bytes().all(|b| b.is_ascii_digit());
    if which != "next" && which != "prev" && !index {
      bail!("Invalid keyboard layout: {which}");
    }
    let res = self.send_cmd(&Command {
      command: format!("switchxkblayout main {which}"),
      flags: CommandFlags::empty(),
    })?;
    if res.trim() != "ok" {
      bail!("Hyprland could not switch the layout: {res}");
    }
    Ok(())
  }
}

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
  fn keyboard_layout_argument_is_checked() {
    let hypr = FakeHyprland::start();
    for which in ["", "1 extra", "next;x", "-1"] {
      assert!(hypr.ipc().set_keyboard_layout(which).is_err(), "{which}");
    }
    assert!(hypr.commands().is_empty());
  }
}

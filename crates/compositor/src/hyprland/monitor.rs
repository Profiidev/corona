use serde::Deserialize;

use crate::{hypr_data_cmd, types};

impl crate::hyprland::command::Ipc {
  /// Changes one setting of monitor `name`; Hyprland keeps the others
  pub fn configure_monitor(&self, name: &str, change: types::MonitorChange) -> anyhow::Result<()> {
    use crate::hyprland::command::lua_string;
    let setting = match change {
      types::MonitorChange::Enable => "disabled = false".to_string(),
      types::MonitorChange::Disable => "disabled = true".to_string(),
      types::MonitorChange::Mirror(source) => format!("mirror = {}", lua_string(&source)),
      types::MonitorChange::StopMirroring => "mirror = \"\"".to_string(),
    };
    let res = self.eval(format!(
      "hl.monitor({{ output = {}, {setting} }})",
      lua_string(name)
    ))?;
    if res.to_lowercase().contains("error") {
      anyhow::bail!("Hyprland monitor change failed: {res}");
    }
    Ok(())
  }

  /// every monitor's power, `on` or `off`
  pub fn dpms(&self, action: &str) -> anyhow::Result<()> {
    let action = crate::hyprland::command::lua_string(action);
    self.dsp(format!("dpms({{ action = {action} }})"))
  }
}

hypr_data_cmd!(
  list_monitors,
  "monitors all",
  Vec<Monitor>,
  Vec<types::Monitor>,
  |monitors: Vec<Monitor>| {
    let mut monitors: Vec<types::Monitor> = monitors.into_iter().map(|m| m.into()).collect();
    monitors.sort_unstable_by(|a, b| a.x.cmp(&b.x).then_with(|| a.y.cmp(&b.y)));
    monitors
  }
);

#[derive(Debug, Deserialize)]
pub struct Monitor {
  pub id: u32,
  pub name: String,
  pub width: u32,
  pub height: u32,
  #[serde(rename = "refreshRate")]
  pub refresh_rate: f32,
  pub x: i32,
  pub y: i32,
  #[serde(rename = "specialWorkspace")]
  pub special_workspace: WorkspaceInfo,
  #[serde(rename = "activeWorkspace")]
  pub active_workspace: WorkspaceInfo,
  pub scale: f32,
  pub focused: bool,
  pub disabled: bool,
  #[serde(rename = "mirrorOf")]
  pub mirror_of: String,
}

#[derive(Debug, Deserialize)]
pub struct WorkspaceInfo {
  pub address: String,
  pub name: String,
}

impl From<Monitor> for types::Monitor {
  fn from(m: Monitor) -> Self {
    types::Monitor {
      id: m.id,
      name: m.name.clone(),
      width: m.width,
      height: m.height,
      refresh_rate: m.refresh_rate,
      x: m.x,
      y: m.y,
      active_scratchpad: (!m.special_workspace.address.is_empty()).then_some(types::Workspace {
        id: m.special_workspace.address,
        name: m.special_workspace.name,
        monitor: m.name.clone(),
        monitor_id: m.id,
      }),
      active_workspace: types::Workspace {
        id: m.active_workspace.address,
        name: m.active_workspace.name,
        monitor: m.name,
        monitor_id: m.id,
      },
      scale: m.scale,
      focused: m.focused,
      disabled: m.disabled,
      mirror_of: m.mirror_of,
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::{hyprland::fake::FakeHyprland, types};

  #[test]
  fn lists_by_position() {
    let hypr = FakeHyprland::start();
    let monitors = hypr.ipc().list_monitors().unwrap();
    assert_eq!(
      monitors.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
      ["eDP-1", "DP-1"]
    );
    let internal = &monitors[0];
    assert_eq!(
      internal.active_scratchpad,
      Some(types::Workspace {
        id: "0x9".into(),
        name: "special:magic".into(),
        monitor: "eDP-1".into(),
        monitor_id: 0,
      })
    );
    assert_eq!(internal.active_workspace.monitor, "eDP-1");
    assert_eq!((internal.scale, internal.focused), (1.25, true));
    // no special workspace shown: empty address
    assert_eq!(monitors[1].active_scratchpad, None);
    assert_eq!(monitors[1].active_workspace.monitor_id, 1);
    assert_eq!(hypr.commands(), ["j/monitors all"]);
  }

  #[test]
  fn sorts_left_to_right_then_top_down() {
    let hypr = FakeHyprland::start();
    let monitor = |name: &str, x: i32, y: i32| {
      format!(
        r#"{{"id": 0, "name": "{name}", "width": 1, "height": 1, "refreshRate": 60, "x": {x}, "y": {y},
          "specialWorkspace": {{"address": "", "name": ""}}, "activeWorkspace": {{"address": "0x1", "name": "1"}},
          "scale": 1, "focused": false, "disabled": false, "mirrorOf": "none"}}"#
      )
    };
    let list = format!(
      "[{},{},{}]",
      monitor("C", 0, 1080),
      monitor("B", -1920, 0),
      monitor("A", 0, 0)
    );
    hypr.answer("j/monitors all", list);
    let names: Vec<_> = hypr
      .ipc()
      .list_monitors()
      .unwrap()
      .into_iter()
      .map(|m| m.name)
      .collect();
    assert_eq!(names, ["B", "A", "C"]);
  }

  #[test]
  fn configures_one_monitor() {
    use types::MonitorChange::*;
    let hypr = FakeHyprland::start();
    let ipc = hypr.ipc();
    for change in [Enable, Disable, Mirror("DP-1".into()), StopMirroring] {
      ipc.configure_monitor("HDMI-A-1", change).unwrap();
    }
    // a name cannot end the Lua string it is put in
    ipc.configure_monitor("x\" })", Disable).unwrap();
    assert_eq!(
      hypr.commands(),
      [
        r#"/eval hl.monitor({ output = "HDMI-A-1", disabled = false })"#,
        r#"/eval hl.monitor({ output = "HDMI-A-1", disabled = true })"#,
        r#"/eval hl.monitor({ output = "HDMI-A-1", mirror = "DP-1" })"#,
        r#"/eval hl.monitor({ output = "HDMI-A-1", mirror = "" })"#,
        r#"/eval hl.monitor({ output = "x\" })", disabled = true })"#,
      ]
    );
    hypr.answer(
      r#"/eval hl.monitor({ output = "Z", disabled = true })"#,
      "error: no such output",
    );
    assert!(ipc.configure_monitor("Z", Disable).is_err());
  }

  #[test]
  fn dpms_dispatch() {
    let hypr = FakeHyprland::start();
    hypr.ipc().dpms("off").unwrap();
    assert_eq!(
      hypr.commands(),
      [r#"/eval hl.dispatch(hl.dsp.dpms({ action = "off" }))"#]
    );
  }
}

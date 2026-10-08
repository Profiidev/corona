use serde::Deserialize;

use crate::{hypr_data_cmd, types};

impl crate::hyprland::command::Ipc {
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
  fn dpms_dispatch() {
    let hypr = FakeHyprland::start();
    hypr.ipc().dpms("off").unwrap();
    assert_eq!(
      hypr.commands(),
      [r#"/eval hl.dispatch(hl.dsp.dpms({ action = "off" }))"#]
    );
  }
}

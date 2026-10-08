use serde::Deserialize;

use crate::{hypr_data_cmd, hypr_dsp, types};

hypr_data_cmd!(
  list_workspaces,
  "workspaces",
  Vec<Workspace>,
  Vec<types::Workspace>,
  |workspaces: Vec<Workspace>| {
    let mut workspaces: Vec<types::Workspace> = workspaces
      .into_iter()
      .filter(|w| w.workspace_type != "special")
      .map(|w| w.into())
      .collect();
    workspaces.sort_unstable_by_key(|w| w.id.clone());
    workspaces
  }
);

hypr_dsp!(
  focus_workspace,
  "focus({{ workspace = {} }})",
  workspace: &str
);

hypr_data_cmd!(
  active_workspace,
  "activeworkspace",
  Workspace,
  types::Workspace,
  |workspace: Workspace| { workspace.into() }
);

#[derive(Debug, Deserialize)]
pub struct Workspace {
  pub address: String,
  #[serde(rename = "type")]
  pub workspace_type: String,
  pub name: String,
  pub monitor: String,
  #[serde(rename = "monitorID")]
  pub monitor_id: u32,
}

impl From<Workspace> for types::Workspace {
  fn from(w: Workspace) -> Self {
    types::Workspace {
      id: w.address,
      name: w.name,
      monitor: w.monitor,
      monitor_id: w.monitor_id,
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::{hyprland::fake::FakeHyprland, types};

  #[test]
  fn lists_without_special_workspaces() {
    let hypr = FakeHyprland::start();
    let workspaces = hypr.ipc().list_workspaces().unwrap();
    assert_eq!(
      workspaces,
      [
        types::Workspace {
          id: "0x1".into(),
          name: "1".into(),
          monitor: "eDP-1".into(),
          monitor_id: 0,
        },
        types::Workspace {
          id: "0x2".into(),
          name: "2".into(),
          monitor: "DP-1".into(),
          monitor_id: 1,
        },
      ]
    );
    assert_eq!(hypr.ipc().active_workspace().unwrap().name, "1");
  }

  #[test]
  fn malformed_answers_are_errors() {
    let hypr = FakeHyprland::start();
    hypr.answer("j/workspaces", "unknown request");
    assert!(hypr.ipc().list_workspaces().is_err());
    hypr.answer("j/activeworkspace", r#"{"name": "1"}"#);
    assert!(hypr.ipc().active_workspace().is_err());
  }

  #[test]
  fn focus_dispatch() {
    let hypr = FakeHyprland::start();
    hypr.ipc().focus_workspace("3").unwrap();
    assert_eq!(
      hypr.commands(),
      ["/eval hl.dispatch(hl.dsp.focus({ workspace = 3 }))"]
    );
  }

  #[test]
  #[ignore = "BUG: the workspace goes into Lua unescaped, a plugin can run any Lua inside Hyprland"]
  fn bug_focus_workspace_cannot_inject_lua() {
    let hypr = FakeHyprland::start();
    hypr
      .ipc()
      .focus_workspace("1 }) hl.exec_cmd(\"rm -rf ~\") --")
      .ok();
    for command in hypr.commands() {
      assert!(!command.contains("hl.exec_cmd"), "sent {command}");
    }
  }
}

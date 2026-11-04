use anyhow::{Result, bail};
use clap::ValueEnum;
use corona_compositor::{
  Compositor, CompositorExt,
  types::{Monitor, MonitorChange},
};
use corona_ipc::{IpcCommand, IpcExt};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

pub(crate) fn register(cx: &mut App) {
  cx.ipc_mut().register::<Configure>();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
pub enum Change {
  Enable,
  Disable,
  Mirror,
  StopMirroring,
}

/// Enables, disables or mirrors a monitor until the compositor reloads its config
pub struct Configure;

impl IpcCommand for Configure {
  const COMMAND: &'static str = "monitor:configure";

  /// The monitor, the change and for mirroring the monitor to show
  type Payload = (String, Change, Option<String>);
  type Response = ();

  fn handle((name, change, source): Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Compositor::refresh_monitors(cx)?;
    let monitors = cx.compositor().list_monitors(cx);
    if !monitors.iter().any(|m| m.name == name) {
      bail!("no monitor {name}");
    }
    match change {
      Change::Disable => {
        let on = |m: &&Monitor| !m.disabled;
        if monitors.iter().filter(on).count() <= 1
          && monitors.iter().filter(on).any(|m| m.name == name)
        {
          bail!("the last monitor that is on stays on");
        }
      }
      Change::Mirror => {
        let source = source.as_deref().unwrap_or_default();
        if source == name {
          bail!("{source} can't show itself");
        }
        match monitors.iter().find(|m| m.name == source) {
          None => bail!("no monitor {source}"),
          Some(m) if m.disabled => bail!("{source} is disabled"),
          _ => {}
        }
      }
      _ => {}
    }
    let change = match change {
      Change::Enable => MonitorChange::Enable,
      Change::Disable => MonitorChange::Disable,
      Change::Mirror => MonitorChange::Mirror(
        source.ok_or_else(|| anyhow::anyhow!("mirroring needs the monitor to show"))?,
      ),
      Change::StopMirroring => MonitorChange::StopMirroring,
    };
    Compositor::configure_monitor(&name, change, cx)
  }
}

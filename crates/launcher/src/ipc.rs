//! The launcher's IPC commands: opening it, dmenu, the calculator, trash, processes and timers

use anyhow::{Context, Result, anyhow, bail};
use corona_ipc::{IpcCommand, IpcExt, Reply};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

use crate::{
  commands::{
    calculator::calculate,
    processes::{by_port, kill, snapshot},
    storage::{empty_trash, trash_count},
    timers::{self, Kind},
  },
  dmenu,
  ui::{self, Open as OpenWith},
};

pub(crate) fn register(cx: &mut App) {
  cx.ipc_mut()
    .register::<Open>()
    .register::<Close>()
    .register::<Toggle>()
    .register::<Dmenu>()
    .register::<ListCommands>()
    .register::<Evaluate>()
    .register::<TrashCount>()
    .register::<EmptyTrash>()
    .register::<KillProcess>()
    .register::<KillPort>()
    .register::<ListProcesses>()
    .register::<StartTimer>()
    .register::<ListTimers>()
    .register::<StopTimer>();
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenRequest {
  /// Id of a command to open straight into
  pub command: Option<String>,
  /// Text the search starts with, or the command's arguments
  pub query: Option<String>,
}

pub struct Open;

impl IpcCommand for Open {
  const COMMAND: &'static str = "launcher:open";

  type Payload = OpenRequest;
  type Response = ();

  fn handle(request: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let query = request.query.unwrap_or_default();
    let open = match request.command {
      Some(id) => OpenWith::Command { id, query },
      None => OpenWith::Root { query },
    };
    ui::open(open, cx)
  }
}

pub struct Close;

impl IpcCommand for Close {
  const COMMAND: &'static str = "launcher:close";

  type Payload = ();
  type Response = ();

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    ui::close(cx);
    Ok(())
  }
}

pub struct Toggle;

impl IpcCommand for Toggle {
  const COMMAND: &'static str = "launcher:toggle";

  type Payload = ();
  type Response = ();

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    ui::toggle(cx)
  }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmenuRequest {
  pub lines: Vec<String>,
  pub prompt: Option<String>,
  pub placeholder: Option<String>,
  pub multi: bool,
}

/// Opens the launcher on the lines and answers with the choice once the user makes one
pub struct Dmenu;

impl IpcCommand for Dmenu {
  const COMMAND: &'static str = "launcher:dmenu";

  type Payload = DmenuRequest;
  /// The chosen lines; an error when cancelled
  type Response = Vec<String>;

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    Err(anyhow!("launcher:dmenu answers asynchronously"))
  }

  fn reply(request: Self::Payload, cx: &mut App) -> Reply<Self::Response> {
    let (view, choice) = dmenu::view(dmenu::Options {
      lines: request.lines,
      prompt: request.prompt,
      placeholder: request.placeholder,
      multi: request.multi,
    });
    let opened = ui::open(OpenWith::View(view), cx);
    Box::pin(async move {
      opened?;
      choice.await.map_err(|_| anyhow!("cancelled"))
    })
  }
}

/// Every command id with its title, for completions
pub struct ListCommands;

impl IpcCommand for ListCommands {
  const COMMAND: &'static str = "launcher:commands";

  type Payload = ();
  type Response = Vec<(String, String)>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let registry = cx.global::<crate::registry::Registry>();
    let mut commands: Vec<(String, String)> = registry
      .all(cx)
      .iter()
      .map(|c| (c.id().to_string(), c.meta(cx).title.to_string()))
      .collect();
    commands.sort();
    commands.dedup_by(|a, b| a.0 == b.0);
    Ok(commands)
  }
}

/// What the launcher's calculator makes of an expression
pub struct Evaluate;

impl IpcCommand for Evaluate {
  const COMMAND: &'static str = "calc:eval";

  type Payload = String;
  type Response = String;

  fn handle(input: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    calculate(&input, cx).context("not a calculation")
  }
}

pub struct TrashCount;

impl IpcCommand for TrashCount {
  const COMMAND: &'static str = "trash:count";

  type Payload = ();
  type Response = usize;

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    trash_count()
  }
}

pub struct EmptyTrash;

impl IpcCommand for EmptyTrash {
  const COMMAND: &'static str = "trash:empty";

  type Payload = ();
  /// How many items went
  type Response = usize;

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    empty_trash()
  }
}

pub struct KillProcess;

impl IpcCommand for KillProcess {
  const COMMAND: &'static str = "process:kill";

  /// The pid and whether to force
  type Payload = (u32, bool);
  type Response = ();

  fn handle((pid, force): Self::Payload, _: &mut App) -> Result<Self::Response> {
    kill(pid, force)
  }
}

/// Process ids with their names
pub struct ListProcesses;

impl IpcCommand for ListProcesses {
  const COMMAND: &'static str = "process:list";

  type Payload = ();
  type Response = Vec<(u32, String)>;

  fn handle(_: Self::Payload, _: &mut App) -> Result<Self::Response> {
    let mut list: Vec<(u32, String)> = snapshot().into_iter().map(|p| (p.pid, p.name)).collect();
    list.sort();
    Ok(list)
  }
}

pub struct KillPort;

impl IpcCommand for KillPort {
  const COMMAND: &'static str = "process:kill_port";

  type Payload = u16;
  /// The pids that got the signal
  type Response = Vec<u32>;

  fn handle(port: Self::Payload, _: &mut App) -> Result<Self::Response> {
    let pids = by_port(port)?;
    for pid in &pids {
      kill(*pid, false)?;
    }
    Ok(pids)
  }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TimerRequest {
  /// None starts a stopwatch
  pub seconds: Option<u64>,
  pub label: Option<String>,
}

pub struct StartTimer;

impl IpcCommand for StartTimer {
  const COMMAND: &'static str = "timer:start";

  type Payload = TimerRequest;
  /// The new timer's id
  type Response = u64;

  fn handle(request: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let now = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)?
      .as_millis() as u64;
    let kind = match request.seconds {
      Some(0) => bail!("expected at least a second"),
      Some(s) => Kind::Timer {
        end: now + s * 1000,
        total: s * 1000,
      },
      None => Kind::Stopwatch { start: now },
    };
    Ok(timers::start(kind, request.label, cx).id)
  }
}

pub struct ListTimers;

impl IpcCommand for ListTimers {
  const COMMAND: &'static str = "timer:list";

  type Payload = ();
  type Response = Vec<timers::Timer>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Ok(timers::list(cx))
  }
}

pub struct StopTimer;

impl IpcCommand for StopTimer {
  const COMMAND: &'static str = "timer:stop";

  type Payload = u64;
  type Response = ();

  fn handle(id: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    if !timers::remove(id, cx) {
      bail!("no timer {id}");
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn requests_json() {
    let open: OpenRequest = serde_json::from_str(r#"{"command":"volume","query":"40"}"#).unwrap();
    assert_eq!(open.command.as_deref(), Some("volume"));
    assert_eq!(open.query.as_deref(), Some("40"));
    let dmenu: DmenuRequest =
      serde_json::from_str(r#"{"lines":["a","b"],"prompt":null,"placeholder":null,"multi":true}"#)
        .unwrap();
    assert_eq!(dmenu.lines, ["a", "b"]);
    assert!(dmenu.multi);
  }
}

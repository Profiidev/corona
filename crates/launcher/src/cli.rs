//! `corona ipc launcher` and `corona launcher`

use std::io::{BufRead, IsTerminal};

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::{
  IpcCommandSend,
  cli::{described, done},
};

use crate::{
  commands::{timers::Kind, web::decode},
  ipc::{
    Close, Dmenu, DmenuRequest, EmptyTrash, Evaluate, ListCommands, ListTimers, Open, OpenRequest,
    StartTimer, StopTimer, TimerRequest, Toggle, TrashCount,
  },
};

#[derive(Subcommand)]
pub enum Commands {
  /// Open the launcher, optionally inside a command or with a query
  Open {
    /// Id of the command to open straight into, like `volume`
    #[arg(long, add = ArgValueCandidates::new(commands))]
    command: Option<String>,
    /// Text to search for, or the command's arguments
    #[arg(long)]
    query: Option<String>,
  },
  Close,
  Toggle,
  /// The trash
  Trash {
    #[command(subcommand)]
    command: TrashCommands,
  },
  /// Timers and stopwatches
  Timer {
    #[command(subcommand)]
    command: TimerCommands,
  },
  /// Evaluate math, units, currency, time zones or dates like the launcher
  Calc {
    #[arg(allow_hyphen_values = true, num_args = 1.., trailing_var_arg = true)]
    expression: Vec<String>,
  },
}

impl Commands {
  pub fn run(self) {
    let what = "control the launcher";
    match self {
      Commands::Open { command, query } => done(
        Open::send(OpenRequest {
          command,
          query,
          prefill: false,
        }),
        what,
      ),
      Commands::Close => done(Close::send(()), what),
      Commands::Toggle => done(Toggle::send(()), what),
      Commands::Trash { command } => command.run(),
      Commands::Timer { command } => command.run(),
      Commands::Calc { expression } => {
        println!("{}", ok(Evaluate::send(expression.join(" ")), "calculate"))
      }
    }
  }
}

#[derive(Subcommand)]
pub enum TrashCommands {
  /// How many items are in the trash
  Count,
  /// Delete everything in the trash
  Empty,
}

impl TrashCommands {
  pub fn run(self) {
    let result = match self {
      TrashCommands::Count => TrashCount::send(()),
      TrashCommands::Empty => EmptyTrash::send(()),
    };
    println!("{}", ok(result, "use the trash"));
  }
}

/// The result, or logs the error and exits 1
fn ok<T>(result: Result<T, impl std::fmt::Display>, what: &str) -> T {
  result.unwrap_or_else(|e| {
    tracing::error!("Failed to {what}: {}", e);
    std::process::exit(1);
  })
}

fn seconds(s: &str) -> Result<u64, String> {
  match crate::args::parse_duration(s) {
    Ok(d) => Ok(d.as_secs()),
    Err(e) => Err(e.to_string()),
  }
}

#[derive(Subcommand)]
pub enum TimerCommands {
  /// Start a timer like 5m or 1h30m, a bare number is minutes
  Start {
    #[arg(value_parser = seconds)]
    duration: u64,
    label: Option<String>,
  },
  /// Start a stopwatch
  Stopwatch,
  /// List running timers
  List,
  /// Stop a timer by id
  Stop {
    #[arg(add = ArgValueCandidates::new(timers))]
    id: u64,
  },
}

impl TimerCommands {
  pub fn run(self) {
    match self {
      TimerCommands::Start { duration, label } => done(
        StartTimer::send(TimerRequest {
          seconds: Some(duration),
          label,
        }),
        "start the timer",
      ),
      TimerCommands::Stopwatch => done(
        StartTimer::send(TimerRequest {
          seconds: None,
          label: None,
        }),
        "start the stopwatch",
      ),
      TimerCommands::List => {
        for t in ok(ListTimers::send(()), "list timers") {
          let kind = match t.kind {
            Kind::Timer { .. } => "timer",
            Kind::Stopwatch { .. } => "stopwatch",
          };
          println!("{}\t{kind}\t{}", t.id, t.label.unwrap_or_default());
        }
      }
      TimerCommands::Stop { id } => done(StopTimer::send(id), "stop the timer"),
    }
  }
}

fn commands() -> Vec<CompletionCandidate> {
  described(ListCommands::send(()).unwrap_or_default())
}

fn timers() -> Vec<CompletionCandidate> {
  described(
    ListTimers::send(())
      .unwrap_or_default()
      .into_iter()
      .map(|t| (t.id.to_string(), t.label.unwrap_or_default())),
  )
}

/// `corona launcher`: tools that talk to the launcher from scripts and links
#[derive(Subcommand)]
pub enum ToolCommands {
  /// Choose one of the lines on stdin; prints it, exits 1 when cancelled. Like `rofi -dmenu`
  Dmenu {
    /// Shown as the breadcrumb
    #[arg(short, long)]
    prompt: Option<String>,
    /// Shown in the empty search field
    #[arg(long)]
    placeholder: Option<String>,
    /// Ctrl+Enter marks lines, Enter prints every marked one
    #[arg(long)]
    multi: bool,
  },
  /// Open a `corona://launcher/<namespace>/<command>?arguments=…` link
  OpenUrl { url: String },
}

impl ToolCommands {
  pub fn run(self) {
    match self {
      ToolCommands::Dmenu {
        prompt,
        placeholder,
        multi,
      } => {
        let lines = read_lines();
        let request = DmenuRequest {
          lines,
          prompt,
          placeholder,
          multi,
        };
        match Dmenu::send(request) {
          Ok(Some(chosen)) => {
            for line in chosen {
              println!("{line}");
            }
          }
          Ok(None) => std::process::exit(1),
          Err(e) => {
            tracing::error!("Failed to run dmenu: {e:#}");
            std::process::exit(1);
          }
        }
      }
      ToolCommands::OpenUrl { url } => {
        let sent = parse_url(&url).and_then(Open::send);
        if let Err(e) = sent {
          tracing::error!("Failed to open {url}: {e:#}");
          std::process::exit(1);
        }
      }
    }
  }
}

fn read_lines() -> Vec<String> {
  let stdin = std::io::stdin();
  if stdin.is_terminal() {
    return Vec::new();
  }
  stdin
    .lock()
    .split(b'\n')
    .map_while(Result::ok)
    .map(|l| String::from_utf8_lossy(&l).into_owned())
    .filter(|l| !l.is_empty())
    .collect()
}

/// `corona://launcher/<ns>/<command>?arguments=…` as the command `<ns>:<command>`
fn parse_url(url: &str) -> Result<OpenRequest> {
  let rest = url
    .strip_prefix("corona://launcher")
    .context("not a corona://launcher link")?;
  let (path, query) = rest.split_once('?').unwrap_or((rest, ""));
  let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
  let command = match parts.as_slice() {
    [] => None,
    [ns, command] => Some(format!("{}:{}", decode(ns), decode(command))),
    _ => bail!("expected corona://launcher/<namespace>/<command>"),
  };
  let mut arguments = None;
  for pair in query.split('&').filter(|p| !p.is_empty()) {
    let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
    if matches!(key, "arguments" | "query") {
      // `+` is a space only in the query
      arguments = Some(decode(&value.replace('+', " ")));
    }
  }
  Ok(OpenRequest {
    command,
    query: arguments,
    // a link must never run a command on its own
    prefill: true,
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn durations() {
    assert_eq!(seconds("5m"), Ok(300));
    assert_eq!(seconds("1h30m"), Ok(5400));
    assert_eq!(seconds("90s"), Ok(90));
    assert_eq!(seconds("10"), Ok(600));
    assert!(seconds("soon").is_err());
    assert!(seconds("5é").is_err());
    assert!(seconds("0").is_err());
    assert!(seconds("99999999999999999999h").is_err());
    assert!(seconds(&format!("{}h", u64::MAX / 60)).is_err());
  }

  #[test]
  fn urls() {
    let r = parse_url("corona://launcher/corona/volume?arguments=40%25").unwrap();
    assert_eq!(r.command.as_deref(), Some("corona:volume"));
    assert_eq!(r.query.as_deref(), Some("40%"));
    let r = parse_url("corona://launcher").unwrap();
    assert_eq!(
      r,
      OpenRequest {
        prefill: true,
        ..Default::default()
      }
    );
    let r = parse_url("corona://launcher/?query=hello+world").unwrap();
    assert_eq!(r.query.as_deref(), Some("hello world"));
    assert!(parse_url("https://example.com").is_err());
    assert!(parse_url("corona://launcher/a/b/c").is_err());
    let r = parse_url("corona://launcher/a/b?arguments=%zz").unwrap();
    assert_eq!(r.query.as_deref(), Some("%zz"));
  }
}

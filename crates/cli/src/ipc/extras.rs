use clap::Subcommand;
use corona_ipc::IpcCommandSend;
use corona_shell::commands::extras::{
  Drive, DriveAction, EmptyTrash, KillPort, KillProcess, ListDrives, ListTimers, StartTimer,
  StopTimer, TimerRequest, TrashCount,
};

use crate::ipc::done;

#[derive(Subcommand)]
pub enum TrashCommands {
  /// How many items are in the trash
  Count,
  /// Delete everything in the trash
  Empty,
}

impl TrashCommands {
  pub fn execute(self) {
    let result = match self {
      TrashCommands::Count => TrashCount::send(()),
      TrashCommands::Empty => EmptyTrash::send(()),
    };
    match result {
      Ok(n) => println!("{n}"),
      Err(e) => tracing::error!("Failed to use the trash: {}", e),
    }
  }
}

#[derive(Subcommand)]
pub enum DriveCommands {
  /// List removable drives
  List,
  /// Mount a drive by device or label
  Mount { drive: String },
  /// Unmount and power off a drive by device or label
  Eject { drive: String },
}

impl DriveCommands {
  pub fn execute(self) {
    match self {
      DriveCommands::List => match ListDrives::send(()) {
        Ok(drives) => {
          for d in drives {
            println!("{}\t{}\t{}", d.device, d.label, d.mount_points.join(","));
          }
        }
        Err(e) => tracing::error!("Failed to list drives: {}", e),
      },
      DriveCommands::Mount { drive } => done(DriveAction::send((Drive::Mount, drive)), "mount"),
      DriveCommands::Eject { drive } => done(DriveAction::send((Drive::Eject, drive)), "eject"),
    }
  }
}

#[derive(Subcommand)]
pub enum ProcessCommands {
  /// Send SIGTERM, or SIGKILL with --force
  Kill {
    pid: u32,
    #[arg(long)]
    force: bool,
  },
  /// Kill what listens on a TCP port
  KillPort { port: u16 },
}

impl ProcessCommands {
  pub fn execute(self) {
    match self {
      ProcessCommands::Kill { pid, force } => done(KillProcess::send((pid, force)), "kill"),
      ProcessCommands::KillPort { port } => match KillPort::send(port) {
        Ok(pids) => {
          for pid in pids {
            println!("{pid}");
          }
        }
        Err(e) => tracing::error!("Failed to kill by port: {}", e),
      },
    }
  }
}

fn seconds(s: &str) -> Result<u64, String> {
  let total = s
    .split_inclusive(['h', 'm', 's'])
    .map(|part| {
      let (n, unit) = part.split_at(part.len() - 1);
      let (n, unit) = match unit.parse::<u64>() {
        Ok(_) => (part, "m"),
        Err(_) => (n, unit),
      };
      let n: u64 = n.parse().map_err(|_| format!("bad duration {s}"))?;
      Ok(
        n * match unit {
          "h" => 3600,
          "m" => 60,
          _ => 1,
        },
      )
    })
    .sum::<Result<u64, String>>()?;
  (total > 0)
    .then_some(total)
    .ok_or_else(|| format!("bad duration {s}"))
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
  Stop { id: u64 },
}

impl TimerCommands {
  pub fn execute(self) {
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
      TimerCommands::List => match ListTimers::send(()) {
        Ok(timers) => {
          for t in timers {
            println!("{}\t{:?}\t{}", t.id, t.kind, t.label.unwrap_or_default());
          }
        }
        Err(e) => tracing::error!("Failed to list timers: {}", e),
      },
      TimerCommands::Stop { id } => done(StopTimer::send(id), "stop the timer"),
    }
  }
}

#[cfg(test)]
mod tests {
  #[test]
  fn durations() {
    assert_eq!(super::seconds("5m"), Ok(300));
    assert_eq!(super::seconds("1h30m"), Ok(5400));
    assert_eq!(super::seconds("90s"), Ok(90));
    assert_eq!(super::seconds("10"), Ok(600));
    assert!(super::seconds("soon").is_err());
  }
}

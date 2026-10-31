//! `corona ipc process`

use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::{
  IpcCommandSend,
  cli::{described, done},
};

use crate::commands::{KillPort, KillProcess, ListProcesses};

#[derive(Subcommand)]
pub enum Commands {
  /// Send SIGTERM, or SIGKILL with --force
  Kill {
    #[arg(add = ArgValueCandidates::new(processes))]
    pid: u32,
    #[arg(long)]
    force: bool,
  },
  /// Kill what listens on a TCP port
  KillPort { port: u16 },
}

impl Commands {
  pub fn run(self) {
    match self {
      Commands::Kill { pid, force } => done(KillProcess::send((pid, force)), "kill"),
      Commands::KillPort { port } => match KillPort::send(port) {
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

fn processes() -> Vec<CompletionCandidate> {
  described(
    ListProcesses::send(())
      .unwrap_or_default()
      .into_iter()
      .map(|(pid, name)| (pid.to_string(), name)),
  )
}

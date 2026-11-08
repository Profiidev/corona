//! `corona ipc process`

use clap::Subcommand;
use clap_complete::{ArgValueCandidates, CompletionCandidate};
use corona_ipc::{
  IpcCommandSend,
  cli::{described, done},
};

use corona_shell::ipc::sysinfo::{KillPort, KillProcess, ListProcesses};

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
      Commands::KillPort { port } => {
        let result = KillPort::send(port);
        if let Ok(pids) = &result {
          for pid in pids {
            println!("{pid}");
          }
        }
        done(result, "kill by port")
      }
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

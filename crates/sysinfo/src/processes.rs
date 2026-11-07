//! Running processes sorted by CPU or memory, killing them and killing by port

use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use rust_i18n::t;
pub use sysinfo::System;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate};

#[derive(Clone, Debug, PartialEq)]
pub struct Process {
  pub pid: u32,
  pub name: String,
  pub cpu: f32,
  pub memory: u64,
}

pub fn sample(system: &mut System) -> Vec<Process> {
  system.refresh_processes_specifics(
    ProcessesToUpdate::All,
    true,
    ProcessRefreshKind::nothing().with_cpu().with_memory(),
  );
  let cores = system.cpus().len().max(1) as f32;
  system
    .processes()
    .values()
    .map(|p| Process {
      pid: p.pid().as_u32(),
      name: p.name().to_string_lossy().to_string(),
      cpu: p.cpu_usage() / cores,
      memory: p.memory(),
    })
    .collect()
}

/// Every process once, without CPU use (that needs two samples)
pub fn snapshot() -> Result<Vec<Process>> {
  let mut system = System::new()?;
  Ok(sample(&mut system))
}

/// SIGTERM, or SIGKILL when forced
pub fn kill(pid: u32, force: bool) -> Result<()> {
  if pid == std::process::id() {
    bail!("refusing to kill the shell");
  }
  let pid = i32::try_from(pid)
    .ok()
    .and_then(rustix::process::Pid::from_raw)
    .context("not a process id")?;
  let signal = match force {
    true => rustix::process::Signal::KILL,
    false => rustix::process::Signal::TERM,
  };
  rustix::process::kill_process(pid, signal)?;
  Ok(())
}

/// SIGTERM to every pid, the ones that got it; fails only when none did
pub fn kill_all(pids: &[u32]) -> Result<Vec<u32>> {
  let mut killed = Vec::new();
  let mut last = None;
  for pid in pids {
    match kill(*pid, false) {
      Ok(()) => killed.push(*pid),
      Err(e) => last = Some(e),
    }
  }
  match (killed.is_empty(), last) {
    (true, Some(e)) => Err(e),
    _ => Ok(killed),
  }
}

/// Socket inodes listening on `port` in `/proc/net/tcp`-style text
fn listening_inodes(table: &str, port: u16) -> HashSet<u64> {
  table
    .lines()
    .skip(1)
    .filter_map(|line| {
      let fields: Vec<&str> = line.split_whitespace().collect();
      let local = fields.get(1)?;
      let state = fields.get(3)?;
      let inode = fields.get(9)?.parse().ok()?;
      let local_port = u16::from_str_radix(local.rsplit(':').next()?, 16).ok()?;
      (*state == "0A" && local_port == port).then_some(inode)
    })
    .collect()
}

/// Signals everything listening on `port`, failing when nothing does
pub fn kill_port(port: u16) -> Result<Vec<u32>> {
  let pids = by_port(port)?;
  if pids.is_empty() {
    bail!(
      "{}",
      t!("app.launcher.processes.nothing_listens", port = port)
    );
  }
  kill_all(&pids)
}

/// Processes with a socket listening on `port`
pub fn by_port(port: u16) -> Result<Vec<u32>> {
  let mut inodes = HashSet::new();
  for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
    if let Ok(text) = std::fs::read_to_string(table) {
      inodes.extend(listening_inodes(&text, port));
    }
  }
  if inodes.is_empty() {
    return Ok(Vec::new());
  }
  let mut pids = Vec::new();
  for entry in std::fs::read_dir("/proc")?.filter_map(Result::ok) {
    let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
      continue;
    };
    let Ok(fds) = std::fs::read_dir(entry.path().join("fd")) else {
      continue;
    };
    let owns = fds.filter_map(Result::ok).any(|fd| {
      std::fs::read_link(fd.path())
        .ok()
        .and_then(|link| {
          let link = link.to_string_lossy().to_string();
          link
            .strip_prefix("socket:[")?
            .strip_suffix(']')?
            .parse::<u64>()
            .ok()
        })
        .is_some_and(|inode| inodes.contains(&inode))
    });
    if owns {
      pids.push(pid);
    }
  }
  if pids.is_empty() {
    bail!(
      "{}",
      t!("app.launcher.processes.not_accessible", port = port)
    );
  }
  Ok(pids)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn kill_rejects_invalid_pids() {
    assert!(kill(u32::MAX, false).is_err());
    assert!(kill(0, false).is_err());
  }

  #[test]
  fn listening_sockets() {
    let table = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 4242 1 0000000000000000 100 0 0 10 0
   1: 0100007F:1F90 0100007F:9C40 01 00000000:00000000 00:00000000 00000000  1000        0 4343 1 0000000000000000 20 4 30 10 -1
   2: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 77 1 0000000000000000 100 0 0 10 0";
    assert_eq!(listening_inodes(table, 8080), HashSet::from([4242]));
    assert_eq!(listening_inodes(table, 22), HashSet::from([77]));
    assert!(listening_inodes(table, 443).is_empty());
  }

  #[test]
  fn samples_this_process() -> Result<()> {
    let mut system = System::new()?;
    let me = std::process::id();
    assert!(sample(&mut system).iter().any(|p| p.pid == me));
    Ok(())
  }
}

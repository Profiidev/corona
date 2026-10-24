//! Running processes sorted by CPU or memory, killing them and killing by port

use std::{
  cell::{Cell, RefCell},
  collections::HashSet,
  rc::Rc,
  time::Duration,
};

use anyhow::{Context, Result, bail};
use gpui_kit::{App, AppContext, Entity, assets::IconName};
use rust_i18n::t;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

use crate::{
  args::{ArgSpec, ArgValue},
  command::{Accessory, Action, Command, Confirm, Icon, Meta, Outcome},
  kit::{ListSource, watch},
  registry, simple,
  view::{Filter, ListView},
};

const REFRESH: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq)]
pub struct Process {
  pub pid: u32,
  pub name: String,
  pub cpu: f32,
  pub memory: u64,
}

fn sample(system: &mut System) -> Vec<Process> {
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

/// SIGTERM, or SIGKILL when forced
pub fn kill(pid: u32, force: bool) -> Result<()> {
  let pid = rustix::process::Pid::from_raw(pid as i32).context("not a process id")?;
  let signal = match force {
    true => rustix::process::Signal::KILL,
    false => rustix::process::Signal::TERM,
  };
  rustix::process::kill_process(pid, signal)?;
  Ok(())
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
  Ok(pids)
}

fn mib(bytes: u64) -> String {
  format!("{:.0} MiB", bytes as f64 / 1024. / 1024.)
}

fn process_row(p: Process) -> Rc<dyn Command> {
  let shown = p.clone();
  simple(
    format!("process:{}", p.pid),
    move |_| Meta {
      title: shown.name.clone().into(),
      subtitle: Some(format!("PID {}", shown.pid).into()),
      icon: Icon::Named(IconName::Cpu),
      keywords: vec![shown.pid.to_string().into()],
      accessories: vec![
        Accessory::Text(format!("{:.1}%", shown.cpu).into()),
        Accessory::Text(mib(shown.memory).into()),
      ],
      ..Default::default()
    },
    move |_| {
      let (pid, name) = (p.pid, p.name.clone());
      vec![
        Action::sync("kill", t!("app.launcher.processes.kill"), move |_, _, _| {
          kill(pid, false)?;
          Ok(Outcome::KeepOpen)
        })
        .destructive()
        .keep_open(),
        Action::sync(
          "force_kill",
          t!("app.launcher.processes.force_kill"),
          move |_, _, _| {
            kill(pid, true)?;
            Ok(Outcome::KeepOpen)
          },
        )
        .destructive()
        .keep_open()
        .confirm(Confirm {
          title: t!("app.launcher.processes.force_kill").into(),
          message: t!("app.launcher.processes.force_message", name = name.clone()).into(),
          button: t!("app.launcher.processes.force_kill").into(),
          skippable: true,
        }),
        Action::sync(
          "copy_pid",
          t!("app.launcher.processes.copy_pid"),
          move |_, _, _| Ok(Outcome::Copy(crate::Clip::Text(pid.to_string()))),
        ),
      ]
    },
  )
}

fn processes_view() -> crate::View {
  let selected = Rc::new(Cell::new(0));
  let filter = Filter {
    label: t!("app.launcher.processes.sort").into(),
    options: vec![
      ("cpu".into(), "CPU".into()),
      ("memory".into(), t!("app.launcher.processes.memory").into()),
      ("name".into(), t!("app.launcher.processes.name").into()),
    ],
    selected: selected.clone(),
  };
  let sampled: Rc<RefCell<Option<Entity<Vec<Process>>>>> = Rc::default();
  let (read, write) = (sampled.clone(), sampled);
  let source = ListSource::new("processes", move |_, cx| {
    let Some(mut list) = read.borrow().as_ref().map(|e| e.read(cx).clone()) else {
      return Vec::new();
    };
    match selected.get() {
      1 => list.sort_by_key(|p| std::cmp::Reverse(p.memory)),
      2 => list.sort_by_key(|p| p.name.to_lowercase()),
      _ => list.sort_by(|a, b| b.cpu.total_cmp(&a.cpu)),
    }
    list.into_iter().map(process_row).collect()
  })
  .observe(move |notify, cx| {
    let entity = cx.new(|_| Vec::new());
    *write.borrow_mut() = Some(entity.clone());
    let weak = entity.downgrade();
    // samples while the view is open, the entity goes with it
    cx.spawn(async move |cx| {
      let mut system = System::new();
      loop {
        let mut owned = std::mem::take(&mut system);
        let (list, owned) = cx
          .background_spawn(async move { (sample(&mut owned), owned) })
          .await;
        system = owned;
        if weak
          .update(cx, |v, cx| {
            *v = list;
            cx.notify();
          })
          .is_err()
        {
          break;
        }
        cx.background_executor().timer(REFRESH).await;
      }
    })
    .detach();
    vec![watch(&entity, &notify, cx)]
  });
  ListView::new(
    t!("app.launcher.processes.title"),
    t!("app.launcher.processes.search"),
    Rc::new(source),
  )
  .filter(filter)
  .into_view()
}

pub fn init(cx: &mut App) {
  registry::register_all(
    [
      crate::kit::opener(
        "processes",
        t!("app.launcher.processes.title"),
        Icon::Tile(IconName::Activity, 0.),
        &["task manager", "top", "kill", "cpu", "memory"],
        |_| processes_view(),
      ),
      simple(
        "processes:kill_port",
        |_| Meta {
          title: t!("app.launcher.processes.kill_port").into(),
          icon: Icon::Tile(IconName::Unplug, 0.),
          keywords: vec!["port".into(), "listen".into(), "kill".into()],
          args: vec![ArgSpec::number(t!("app.launcher.processes.port"))],
          ..Default::default()
        },
        |_| {
          vec![
            Action::sync("kill", t!("app.launcher.processes.kill"), |args, _, _| {
              let Some(ArgValue::Number(port)) = args.get(0) else {
                bail!("{}", t!("app.launcher.processes.port"));
              };
              let pids = by_port(*port as u16)?;
              if pids.is_empty() {
                bail!(
                  "{}",
                  t!(
                    "app.launcher.processes.nothing_listens",
                    port = *port as u16
                  )
                );
              }
              for pid in &pids {
                kill(*pid, false)?;
              }
              Ok(Outcome::hud(t!(
                "app.launcher.processes.killed",
                count = pids.len()
              )))
            })
            .destructive(),
          ]
        },
      ),
    ],
    cx,
  );
}

#[cfg(test)]
mod tests {
  use super::*;

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
  fn samples_this_process() {
    let mut system = System::new();
    let me = std::process::id();
    assert!(sample(&mut system).iter().any(|p| p.pid == me));
  }
}

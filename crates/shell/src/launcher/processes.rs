//! The process list and kill-by-port rows of the launcher

use std::{
  cell::{Cell, RefCell},
  rc::Rc,
  time::Duration,
};

use corona_launcher::{
  args::{ArgSpec, ArgValue},
  command::{Accessory, Action, Command, Confirm, Icon, Meta, Outcome},
  kit::{ListSource, watch},
  registry, simple,
  view::{Filter, ListView},
};
use corona_sysinfo::processes::{Process, System, kill, kill_port, sample};
use gpui_kit::{App, AppContext, Entity, assets::IconName};
use anyhow::bail;
use rust_i18n::t;

const REFRESH: Duration = Duration::from_secs(2);

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
          move |_, _, _| Ok(Outcome::Copy(corona_launcher::Clip::Text(pid.to_string()))),
        ),
      ]
    },
  )
}

fn processes_view() -> corona_launcher::View {
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
      let mut system = match System::new() {
        Ok(s) => Some(s),
        Err(e) => {
          tracing::error!("failed to initialize sysinfo for process monitor: {e}");
          return;
        }
      };
      while let Some(mut owned) = system.take() {
        let (list, returned) = cx
          .background_spawn(async move { (sample(&mut owned), owned) })
          .await;
        system = Some(returned);
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

pub(super) fn register(cx: &mut App) {
  registry::register_all(
    [
      corona_launcher::kit::opener(
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
              let port = match args.get(0) {
                Some(ArgValue::Number(n)) if n.fract() == 0. && (1. ..=65535.).contains(n) => {
                  *n as u16
                }
                _ => bail!("{}", t!("app.launcher.processes.port")),
              };
              let killed = kill_port(port)?;
              Ok(Outcome::hud(t!(
                "app.launcher.processes.killed",
                count = killed.len()
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


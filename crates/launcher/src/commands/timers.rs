//! Timers and stopwatches: kept across restarts, listed while they run, a notification
//! when a timer ends

use std::{rc::Rc, time::Duration};

use anyhow::{Result, bail};
use gpui_kit::{
  App, AppContext, Entity, Global, SharedString, Subscription, Task, assets::IconName,
};
use rust_i18n::t;
use serde::{Deserialize, Serialize};

use crate::{
  args::{ArgSpec, ArgValue, Args},
  command::{Accessory, Action, Clip, Command, CommandSource, Hud, Icon, Meta, Outcome, Section},
  hooks,
  kit::{Notify, watch},
  matching::Query,
  registry, simple, state,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Kind {
  /// Ends at `end`, unix milliseconds, after `total` milliseconds
  Timer { end: u64, total: u64 },
  /// Started at unix milliseconds
  Stopwatch { start: u64 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Timer {
  pub id: u64,
  pub label: Option<String>,
  pub kind: Kind,
}

struct Timers {
  list: Entity<Vec<Timer>>,
  /// Bumped every second while something runs, for the remaining time
  tick: Entity<u64>,
  _ticker: Option<Task<()>>,
}

impl Global for Timers {}

fn now_ms() -> u64 {
  std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map_or(0, |d| d.as_millis() as u64)
}

/// `1h 05m`, `4:59`
pub fn clock(ms: u64) -> String {
  let s = ms / 1000;
  match s >= 3600 {
    true => format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60),
    false => format!("{}:{:02}", s / 60, s % 60),
  }
}

fn save(list: &[Timer]) {
  if let Err(e) = state::save("timers", &list) {
    tracing::warn!("timers: {e:#}");
  }
}

pub fn init(cx: &mut App) {
  let saved: Vec<Timer> = state::load("timers");
  let list = cx.new(|_| saved.clone());
  let tick = cx.new(|_| 0u64);
  cx.set_global(Timers {
    list,
    tick,
    _ticker: None,
  });
  for timer in saved {
    schedule(&timer, cx);
  }
  run_ticker(cx);
  registry::register_source(Rc::new(Running), cx);
  registry::register_all([timer_command(), stopwatch_command()], cx);
}

/// Ticks once a second while anything runs
fn run_ticker(cx: &mut App) {
  let timers = cx.global::<Timers>();
  let running = !timers.list.read(cx).is_empty();
  if !running || timers._ticker.is_some() {
    if !running {
      cx.global_mut::<Timers>()._ticker = None;
    }
    return;
  }
  let (tick, list) = (timers.tick.clone(), timers.list.clone());
  let task = cx.spawn(async move |cx| {
    loop {
      cx.background_executor().timer(Duration::from_secs(1)).await;
      let more = cx.update(|cx| {
        tick.update(cx, |t, cx| {
          *t += 1;
          cx.notify();
        });
        !list.read(cx).is_empty()
      });
      if !more {
        break;
      }
    }
    cx.update(|cx| cx.global_mut::<Timers>()._ticker = None);
  });
  cx.global_mut::<Timers>()._ticker = Some(task);
}

/// Notifies and drops a timer when it ends
fn schedule(timer: &Timer, cx: &mut App) {
  let Kind::Timer { end, .. } = timer.kind else {
    return;
  };
  let (id, label) = (timer.id, timer.label.clone());
  let wait = Duration::from_millis(end.saturating_sub(now_ms()));
  cx.spawn(async move |cx| {
    cx.background_executor().timer(wait).await;
    cx.update(|cx| {
      let gone = remove(id, cx);
      if gone {
        hooks::notify(
          t!("app.launcher.timers.finished").to_string(),
          label.unwrap_or_default(),
          cx,
        );
      }
    });
  })
  .detach();
}

/// Adds a timer or stopwatch; it runs until removed
pub fn start(kind: Kind, label: Option<String>, cx: &mut App) -> Timer {
  let timer = Timer {
    id: now_ms(),
    label,
    kind,
  };
  let list = cx.global::<Timers>().list.clone();
  list.update(cx, |list, cx| {
    list.push(timer.clone());
    save(list);
    cx.notify();
  });
  schedule(&timer, cx);
  run_ticker(cx);
  timer
}

/// Removes a timer; false when it was gone already
pub fn remove(id: u64, cx: &mut App) -> bool {
  let list = cx.global::<Timers>().list.clone();
  list.update(cx, |list, cx| {
    let before = list.len();
    list.retain(|t| t.id != id);
    save(list);
    cx.notify();
    list.len() != before
  })
}

pub fn list(cx: &App) -> Vec<Timer> {
  cx.global::<Timers>().list.read(cx).clone()
}

fn timer_row(timer: Timer) -> Rc<dyn Command> {
  let shown = timer.clone();
  simple(
    format!("timer:{}", timer.id),
    move |_| {
      let now = now_ms();
      let (title, progress, icon) = match shown.kind {
        Kind::Timer { end, total } => (
          clock(end.saturating_sub(now)),
          Some(1. - end.saturating_sub(now) as f32 / total.max(1) as f32),
          IconName::Timer,
        ),
        Kind::Stopwatch { start } => (clock(now.saturating_sub(start)), None, IconName::Watch),
      };
      Meta {
        title: title.into(),
        subtitle: shown.label.clone().map(Into::into),
        icon: Icon::Tile(icon, 20.),
        section: Section::Running,
        accessories: progress.map(Accessory::Progress).into_iter().collect(),
        ..Default::default()
      }
    },
    move |_| {
      let id = timer.id;
      let mut actions = vec![
        Action::sync("stop", t!("app.launcher.timers.stop"), move |_, _, cx| {
          remove(id, cx);
          Ok(Outcome::KeepOpen)
        })
        .destructive()
        .keep_open(),
      ];
      if let Kind::Stopwatch { start } = timer.kind {
        actions.push(Action::sync(
          "copy",
          t!("app.launcher.timers.copy"),
          move |_, _, _| {
            Ok(Outcome::Copy(Clip::Text(clock(
              now_ms().saturating_sub(start),
            ))))
          },
        ));
      }
      actions
    },
  )
}

/// The running timers
struct Running;

impl CommandSource for Running {
  fn id(&self) -> SharedString {
    "timers".into()
  }

  fn commands(&self, _: &Query, cx: &App) -> Vec<Rc<dyn Command>> {
    list(cx).into_iter().map(timer_row).collect()
  }

  fn observe(&self, notify: Notify, cx: &mut App) -> Vec<Subscription> {
    let timers = cx.global::<Timers>();
    let (list, tick) = (timers.list.clone(), timers.tick.clone());
    vec![watch(&list, &notify, cx), watch(&tick, &notify, cx)]
  }
}

fn duration(args: &Args) -> Result<Duration> {
  match args.get(0) {
    Some(ArgValue::Duration(d)) => Ok(*d),
    _ => bail!("{}", t!("app.launcher.timers.duration")),
  }
}

fn timer_command() -> Rc<dyn Command> {
  struct TimerCommand;
  impl Command for TimerCommand {
    fn id(&self) -> SharedString {
      "timer".into()
    }
    fn meta(&self, _: &App) -> Meta {
      Meta {
        title: t!("app.launcher.timers.timer").into(),
        icon: Icon::Tile(IconName::Timer, 20.),
        keywords: vec!["countdown".into(), "alarm".into()],
        args: vec![
          ArgSpec::duration(t!("app.launcher.timers.duration")),
          ArgSpec::text(t!("app.launcher.timers.label")).optional(),
        ],
        ..Default::default()
      }
    }
    fn describe(&self, args: &Args, _: &App) -> Option<SharedString> {
      let d = duration(args).ok()?;
      Some(
        t!(
          "app.launcher.timers.start_for",
          time = clock(d.as_millis() as u64)
        )
        .into(),
      )
    }
    fn actions(&self, _: &App) -> Vec<Action> {
      vec![Action::sync(
        "start",
        t!("app.launcher.timers.start"),
        |args, _, cx| {
          let d = duration(args)?;
          let total = d.as_millis() as u64;
          let label = args.text(1).map(str::to_string);
          start(
            Kind::Timer {
              end: now_ms() + total,
              total,
            },
            label,
            cx,
          );
          Ok(Outcome::Close(Some(
            Hud::new(t!("app.launcher.timers.started", time = clock(total))).icon(IconName::Timer),
          )))
        },
      )]
    }
  }
  Rc::new(TimerCommand)
}

fn stopwatch_command() -> Rc<dyn Command> {
  simple(
    "stopwatch",
    |_| Meta {
      title: t!("app.launcher.timers.stopwatch").into(),
      icon: Icon::Tile(IconName::Watch, 20.),
      keywords: vec!["stopwatch".into(), "time".into()],
      ..Default::default()
    },
    |_| {
      vec![Action::sync(
        "start",
        t!("app.launcher.timers.start"),
        |_, _, cx| {
          start(Kind::Stopwatch { start: now_ms() }, None, cx);
          Ok(Outcome::hud(t!("app.launcher.timers.stopwatch_started")))
        },
      )]
    },
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn clocks() {
    assert_eq!(clock(299_000), "4:59");
    assert_eq!(clock(3_900_000), "1:05:00");
    assert_eq!(clock(0), "0:00");
  }
}

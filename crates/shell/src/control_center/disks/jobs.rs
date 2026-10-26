//! What the disks are doing and did: udisks' jobs, the panel's own copies and benchmarks, and a
//! history since the shell started

use std::{
  rc::Rc,
  sync::{Arc, atomic::Ordering},
  time::{Duration, Instant},
};

use anyhow::Result;
use corona_launcher::Tone;
use corona_udisks::{UdisksExt, is_not_authorized};
use futures::future::LocalBoxFuture;
use gpui_kit::{
  AnyElement, App, AppContext, Context, Entity, Global, IntoElement, ParentElement, Styled,
  assets::IconName,
  component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
  },
  div,
  prelude::FluentBuilder,
};
use rust_i18n::t;
use zbus::zvariant::OwnedObjectPath;

use crate::control_center::disks::{
  DisksPanel,
  fd::Progress,
  model::{job_title, remaining},
  ui::{icon, note, section, tag, usage_bar},
};

/// Finished entries kept
const HISTORY: usize = 50;
/// A failure seen twice, from the call and from its job, within this is one entry
const SAME_FAILURE: Duration = Duration::from_secs(5);

/// Starts an action again
pub type Retry = Rc<dyn Fn(&mut App)>;
/// An action the panel runs, again on retry
pub type Start = Rc<dyn Fn(&mut App) -> LocalBoxFuture<'static, Result<()>>>;
/// Like [`Start`], with a result for the panel
pub type StartWith = Rc<dyn Fn(&mut App) -> LocalBoxFuture<'static, Result<String>>>;
/// What to do with a [`StartWith`]'s result
pub type OnOk = Rc<dyn Fn(String, &mut App)>;

/// A copy or benchmark the panel runs itself
pub struct PanelJob {
  pub id: u64,
  pub title: String,
  /// What `progress.done` counts up to
  pub total: u64,
  pub progress: Arc<Progress>,
  pub started: String,
}

#[derive(Clone)]
pub struct Finished {
  pub title: String,
  /// Why it failed, or what came out
  pub detail: String,
  pub success: bool,
  /// `HH:MM`
  pub at: String,
  pub objects: Vec<OwnedObjectPath>,
  pub retry: Option<Retry>,
  pub when: Instant,
}

#[derive(Clone)]
pub struct DisksLog {
  pub running: Entity<Vec<Rc<PanelJob>>>,
  pub finished: Entity<Vec<Finished>>,
  next_id: Rc<std::cell::Cell<u64>>,
}

impl Global for DisksLog {}

pub fn now() -> String {
  jiff::Zoned::now().strftime("%H:%M").to_string()
}

pub fn init(cx: &mut App) {
  let log = DisksLog {
    running: cx.new(|_| Vec::new()),
    finished: cx.new(|_| Vec::new()),
    next_id: Rc::default(),
  };
  cx.set_global(log.clone());
  let Some(udisks) = cx.try_global::<corona_udisks::Udisks>().cloned() else {
    return;
  };
  cx.observe(&udisks.job_completed, move |completed, cx| {
    let Some(done) = completed.read(cx).clone() else {
      return;
    };
    let title = job_title(&done.operation, &done.objects, cx.udisks().list_blocks(cx));
    log.push(
      Finished {
        title,
        detail: done.message,
        success: done.success,
        at: now(),
        objects: done.objects,
        retry: None,
        when: Instant::now(),
      },
      cx,
    );
  })
  .detach();
}

pub fn log(cx: &App) -> &DisksLog {
  cx.global::<DisksLog>()
}

impl DisksLog {
  /// Newest first; a failure already told by the call or the job only adds its retry
  pub fn push(&self, entry: Finished, cx: &mut App) {
    self.finished.update(cx, |list, cx| {
      let twin = (!entry.success)
        .then(|| {
          list.iter_mut().find(|f| {
            !f.success
              && f.when.elapsed() < SAME_FAILURE
              && f.objects.iter().any(|o| entry.objects.contains(o))
          })
        })
        .flatten();
      match twin {
        Some(twin) => {
          if twin.retry.is_none() {
            twin.retry = entry.retry;
          }
        }
        None => {
          list.insert(0, entry);
          list.truncate(HISTORY);
        }
      }
      cx.notify();
    });
  }

  /// Runs a udisks call; a failure lands in Finished with a retry. `Ok` is told by its job
  pub fn run(cx: &mut App, title: String, objects: Vec<OwnedObjectPath>, start: Start) {
    let start: StartWith = Rc::new(move |cx| {
      let task = start(cx);
      Box::pin(async move { task.await.map(|()| String::new()) })
    });
    Self::run_then(cx, title, objects, start, |_, _| {});
  }

  /// Like [`Self::run`], `ok` gets the result when it worked, on a retry too
  pub fn run_then(
    cx: &mut App,
    title: String,
    objects: Vec<OwnedObjectPath>,
    start: StartWith,
    ok: impl Fn(String, &mut App) + 'static,
  ) {
    Self::attempt(cx, title, objects, start, Rc::new(ok));
  }

  fn attempt(
    cx: &mut App,
    title: String,
    objects: Vec<OwnedObjectPath>,
    start: StartWith,
    ok: OnOk,
  ) {
    let task = start(cx);
    cx.spawn(async move |cx| {
      let e = match task.await {
        Ok(result) => return cx.update(|cx| ok(result, cx)),
        Err(e) => e,
      };
      tracing::warn!("disks: {title}: {e:#}");
      let detail = match is_not_authorized(&e) {
        true => t!("app.disks.jobs.not_authorized").to_string(),
        false => format!("{e:#}"),
      };
      cx.update(|cx| {
        let retry: Retry = {
          let (title, objects, start, ok) =
            (title.clone(), objects.clone(), start.clone(), ok.clone());
          Rc::new(move |cx| {
            Self::attempt(
              cx,
              title.clone(),
              objects.clone(),
              start.clone(),
              ok.clone(),
            )
          })
        };
        log(cx).clone().push(
          Finished {
            title,
            detail,
            success: false,
            at: now(),
            objects,
            retry: Some(retry),
            when: Instant::now(),
          },
          cx,
        );
      });
    })
    .detach();
  }

  /// Runs `work` on a background thread as a panel job counting to `total`; its `Ok` text is
  /// the Finished detail
  pub fn spawn(
    cx: &mut App,
    title: String,
    objects: Vec<OwnedObjectPath>,
    total: u64,
    work: impl FnOnce(Arc<Progress>) -> Result<String> + Send + 'static,
    done: impl FnOnce(&Result<String>, &mut App) + 'static,
  ) {
    let this = log(cx).clone();
    let id = this.next_id.get();
    this.next_id.set(id + 1);
    let progress = Arc::new(Progress::default());
    let job = Rc::new(PanelJob {
      id,
      title: title.clone(),
      total,
      progress: progress.clone(),
      started: now(),
    });
    this.running.update(cx, |running, cx| {
      running.push(job);
      cx.notify();
    });
    let task = cx.background_spawn({
      let progress = progress.clone();
      async move { work(progress) }
    });
    let running = this.running.clone();
    // the bar moves while the thread works
    let ticker = cx.spawn(async move |cx| {
      loop {
        cx.background_executor()
          .timer(Duration::from_millis(250))
          .await;
        running.update(cx, |_, cx| cx.notify());
      }
    });
    cx.spawn(async move |cx| {
      let result = task.await;
      drop(ticker);
      cx.update(|cx| {
        this.running.update(cx, |running, cx| {
          running.retain(|j| j.id != id);
          cx.notify();
        });
        done(&result, cx);
        let (success, detail) = match &result {
          Ok(detail) => (true, detail.clone()),
          Err(e) => (false, format!("{e:#}")),
        };
        this.push(
          Finished {
            title,
            detail,
            success,
            at: now(),
            objects,
            retry: None,
            when: Instant::now(),
          },
          cx,
        );
      });
    })
    .detach();
  }

  /// udisks jobs and panel jobs
  pub fn running_count(&self, cx: &App) -> usize {
    let udisks = cx.try_global::<corona_udisks::Udisks>();
    self.running.read(cx).len() + udisks.map_or(0, |u| u.list_jobs(cx).len())
  }
}

impl DisksPanel {
  pub(super) fn jobs_tab(&self, cx: &Context<'_, Self>) -> Vec<AnyElement> {
    let theme = cx.theme();
    let udisks = cx.udisks();
    let blocks = udisks.list_blocks(cx);
    let now_micros = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .map_or(0, |d| d.as_micros() as u64);
    let log = log(cx);
    let mut running: Vec<AnyElement> = udisks
      .list_jobs(cx)
      .iter()
      .map(|job| {
        let title = job_title(&job.operation, &job.objects, blocks);
        let percent = (job.progress * 100.) as f32;
        let left = match remaining(job, now_micros) {
          Some(secs) if secs >= 60 => t!("app.disks.jobs.minutes_left", count = secs / 60),
          Some(secs) => t!("app.disks.jobs.seconds_left", count = secs),
          None => t!("app.disks.jobs.running"),
        };
        let path = job.path.clone();
        job_row(
          title,
          left.to_string(),
          job.progress_valid.then_some(percent),
          job.cancelable.then(|| {
            Button::new(format!("disks-job-cancel-{}", job.path.as_str()))
              .label(t!("app.disks.jobs.cancel"))
              .small()
              .cursor_pointer()
              .on_click(cx.listener(move |this, _, _, cx| {
                let path = path.clone();
                this.act(
                  t!("app.disks.jobs.cancel").into(),
                  vec![],
                  Rc::new(move |cx| Box::pin(cx.udisks().cancel_job(&path))),
                  cx,
                );
              }))
              .into_any_element()
          }),
          cx,
        )
      })
      .collect();
    running.extend(log.running.read(cx).iter().map(|job| {
      let done = job.progress.done.load(Ordering::Relaxed);
      let percent = crate::control_center::disks::model::percent(done, job.total);
      let progress = job.progress.clone();
      job_row(
        job.title.clone(),
        t!("app.disks.jobs.started", time = job.started.clone()).to_string(),
        Some(percent),
        Some(
          Button::new(format!("disks-panel-job-cancel-{}", job.id))
            .label(t!("app.disks.jobs.cancel"))
            .small()
            .cursor_pointer()
            .on_click(move |_, _, _| progress.cancel.store(true, Ordering::Relaxed))
            .into_any_element(),
        ),
        cx,
      )
    }));
    let count = running.len();
    if running.is_empty() {
      running.push(note(t!("app.disks.jobs.none_running"), cx));
    }

    let finished = log.finished.read(cx);
    let mut done: Vec<AnyElement> = finished
      .iter()
      .enumerate()
      .map(|(i, f)| {
        let (tone, label) = match f.success {
          true => (Tone::Ok, t!("app.disks.jobs.success")),
          false => (Tone::Err, t!("app.disks.jobs.failed")),
        };
        let when = match f.success {
          true => t!("app.disks.jobs.completed_at", time = f.at.clone()),
          false => t!("app.disks.jobs.failed_at", time = f.at.clone()),
        };
        let sub = match f.detail.is_empty() {
          true => when.to_string(),
          false => format!("{when} · {}", f.detail),
        };
        let retry = f.retry.clone().map(|retry| {
          Button::new(gpui_kit::ElementId::Name(
            format!("disks-job-retry-{i}").into(),
          ))
          .label(t!("app.disks.jobs.retry"))
          .small()
          .primary()
          .cursor_pointer()
          .on_click(move |_, _, cx| retry(cx))
          .into_any_element()
        });
        div()
          .flex()
          .items_center()
          .gap_2()
          .p_1()
          .child(icon(if f.success {
            IconName::Check
          } else {
            IconName::TriangleAlert
          }))
          .child(
            div()
              .flex()
              .flex_col()
              .flex_1()
              .min_w_0()
              .child(div().text_sm().truncate().child(f.title.clone()))
              .child(
                div()
                  .text_xs()
                  .text_color(theme.colors.muted_foreground)
                  .truncate()
                  .child(sub),
              ),
          )
          .child(tag(label, tone, cx))
          .children(retry)
          .into_any_element()
      })
      .collect();
    if done.is_empty() {
      done.push(note(t!("app.disks.jobs.none_finished"), cx));
    }
    vec![
      section(
        t!("app.disks.jobs.running_title"),
        t!("app.disks.jobs.count", count = count),
        None,
        running,
        cx,
      )
      .into_any_element(),
      section(
        t!("app.disks.jobs.finished_title"),
        t!("app.disks.jobs.today"),
        None,
        done,
        cx,
      )
      .into_any_element(),
    ]
  }
}

fn job_row(
  title: String,
  sub: String,
  percent: Option<f32>,
  button: Option<AnyElement>,
  cx: &App,
) -> AnyElement {
  let theme = cx.theme();
  div()
    .flex()
    .flex_col()
    .gap_1()
    .p_1()
    .child(
      div()
        .flex()
        .items_center()
        .gap_2()
        .child(icon(IconName::Activity))
        .child(
          div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .child(div().text_sm().truncate().child(title))
            .child(
              div()
                .text_xs()
                .text_color(theme.colors.muted_foreground)
                .truncate()
                .child(sub),
            ),
        )
        .when_some(percent, |d, p| {
          d.child(tag(format!("{p:.0} %"), Tone::Ok, cx))
        })
        .children(button),
    )
    .when_some(percent, |d, p| d.child(usage_bar(p, "", "", cx)))
    .into_any_element()
}

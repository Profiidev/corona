use std::{collections::HashMap, thread, time::Duration};

use anyhow::{Context, Result};
use gpui_kit::{App, Global};
use rustix::event::{PollFd, PollFlags, Timespec, poll};
use wayland_client::{
  Connection, Dispatch, QueueHandle, delegate_noop,
  globals::{GlobalListContents, registry_queue_init},
  protocol::{wl_registry, wl_seat::WlSeat},
};
use wayland_protocols::ext::idle_notify::v1::client::{
  ext_idle_notification_v1::{self, ExtIdleNotificationV1},
  ext_idle_notifier_v1::ExtIdleNotifierV1,
};

/// How often the watcher looks for new timeouts while the compositor is quiet
const COMMAND_POLL: Duration = Duration::from_millis(250);

/// The idle watcher; [`set_timeouts`](Idle::set_timeouts) says what to watch
pub struct Idle {
  timeouts: flume::Sender<Vec<(String, Duration)>>,
}

impl Global for Idle {}

/// Starts watching, `on_change(name, idle)` runs when the time named `name`
/// passes without input (`true`) and when input follows (`false`).
pub fn init(cx: &mut App, on_change: impl Fn(&str, bool, &mut App) + 'static) {
  let (timeouts, timeouts_rx) = flume::unbounded();
  let (events, events_rx) = flume::unbounded();
  thread::spawn(move || {
    if let Err(e) = watch(timeouts_rx, events) {
      tracing::error!("idle watcher stopped: {e:#}");
    }
  });
  cx.spawn(async move |cx| {
    while let Ok((name, idle)) = events_rx.recv_async().await {
      cx.update(|cx| on_change(&name, idle, cx));
    }
  })
  .detach();
  cx.set_global(Idle { timeouts });
}

impl Idle {
  /// Replaces what is watched; a zero duration is left out. Each one active
  /// before reports `false` first.
  pub fn set_timeouts(&self, timeouts: Vec<(String, Duration)>) {
    let _ = self.timeouts.send(timeouts);
  }
}

pub trait IdleExt {
  fn idle(&self) -> &Idle;
}

impl IdleExt for App {
  fn idle(&self) -> &Idle {
    self.global::<Idle>()
  }
}

struct State {
  events: flume::Sender<(String, bool)>,
  /// The watched notifications by name, and whether each is idle now
  notifications: HashMap<String, (ExtIdleNotificationV1, bool)>,
}

fn watch(
  timeouts: flume::Receiver<Vec<(String, Duration)>>,
  events: flume::Sender<(String, bool)>,
) -> Result<()> {
  let conn = Connection::connect_to_env()?;
  let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
  let qh = queue.handle();
  let notifier: ExtIdleNotifierV1 = globals
    .bind(&qh, 1..=1, ())
    .context("the compositor has no ext_idle_notifier_v1")?;
  let seat: WlSeat = globals.bind(&qh, 1..=1, ())?;
  let mut state = State {
    events,
    notifications: HashMap::new(),
  };

  loop {
    while let Ok(next) = timeouts.try_recv() {
      for (name, (notification, idle)) in state.notifications.drain() {
        notification.destroy();
        if idle {
          let _ = state.events.send((name, false));
        }
      }
      for (name, timeout) in next.into_iter().filter(|(_, t)| !t.is_zero()) {
        let millis = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        let notification = notifier.get_idle_notification(millis, &seat, &qh, name.clone());
        state.notifications.insert(name, (notification, false));
      }
    }

    queue.dispatch_pending(&mut state)?;
    queue.flush()?;
    let Some(guard) = queue.prepare_read() else {
      continue;
    };
    let fd = guard.connection_fd();
    let mut fds = [PollFd::new(&fd, PollFlags::IN)];
    let timeout = Timespec::try_from(COMMAND_POLL)?;
    let ready = poll(&mut fds, Some(&timeout))? > 0;
    if ready {
      guard.read()?;
    }
    if timeouts.is_disconnected() {
      return Ok(());
    }
  }
}

impl Dispatch<ExtIdleNotificationV1, String> for State {
  fn event(
    state: &mut Self,
    _: &ExtIdleNotificationV1,
    event: ext_idle_notification_v1::Event,
    name: &String,
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
    let idle = match event {
      ext_idle_notification_v1::Event::Idled => true,
      ext_idle_notification_v1::Event::Resumed => false,
      _ => return,
    };
    if let Some((_, was)) = state.notifications.get_mut(name)
      && *was != idle
    {
      *was = idle;
      let _ = state.events.send((name.clone(), idle));
    }
  }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
  fn event(
    _: &mut Self,
    _: &wl_registry::WlRegistry,
    _: wl_registry::Event,
    _: &GlobalListContents,
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
  }
}

delegate_noop!(State: ignore WlSeat);
delegate_noop!(State: ExtIdleNotifierV1);

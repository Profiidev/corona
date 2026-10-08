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
#[cfg(not(test))]
const COMMAND_POLL: Duration = Duration::from_millis(250);
#[cfg(test)]
const COMMAND_POLL: Duration = Duration::from_millis(10);

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
        // a name given twice: the later timeout wins, the earlier one must not leak
        if let Some((earlier, _)) = state.notifications.insert(name, (notification, false)) {
          earlier.destroy();
        }
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

#[cfg(test)]
mod tests {
  use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
      Arc, Mutex,
      atomic::{AtomicBool, Ordering},
    },
    time::Instant,
  };

  use gpui_kit::{self as gpui, TestAppContext};
  use wayland_protocols::ext::idle_notify::v1::server::{
    ext_idle_notification_v1::ExtIdleNotificationV1 as Notification,
    ext_idle_notifier_v1::{self, ExtIdleNotifierV1 as Notifier},
  };
  use wayland_server::{
    Client, DataInit, Dispatch as ServerDispatch, Display, DisplayHandle, GlobalDispatch,
    ListeningSocket, New, Resource,
    backend::{ClientData, ClientId, DisconnectReason},
    protocol::wl_seat::WlSeat as ServerSeat,
  };

  use super::*;

  struct Watched {
    resource: Notification,
    timeout: u32,
    alive: bool,
  }

  #[derive(Clone, Default)]
  struct Shared {
    notifications: Arc<Mutex<Vec<Watched>>>,
  }

  struct NoData;
  impl ClientData for NoData {
    fn disconnected(&self, _: ClientId, _: DisconnectReason) {}
  }

  impl GlobalDispatch<ServerSeat, ()> for Shared {
    fn bind(
      _: &mut Self,
      _: &DisplayHandle,
      _: &Client,
      seat: New<ServerSeat>,
      _: &(),
      init: &mut DataInit<'_, Self>,
    ) {
      init.init(seat, ());
    }
  }

  impl ServerDispatch<ServerSeat, ()> for Shared {
    fn request(
      _: &mut Self,
      _: &Client,
      _: &ServerSeat,
      _: <ServerSeat as Resource>::Request,
      _: &(),
      _: &DisplayHandle,
      _: &mut DataInit<'_, Self>,
    ) {
    }
  }

  impl GlobalDispatch<Notifier, ()> for Shared {
    fn bind(
      _: &mut Self,
      _: &DisplayHandle,
      _: &Client,
      notifier: New<Notifier>,
      _: &(),
      init: &mut DataInit<'_, Self>,
    ) {
      init.init(notifier, ());
    }
  }

  impl ServerDispatch<Notifier, ()> for Shared {
    fn request(
      state: &mut Self,
      _: &Client,
      _: &Notifier,
      request: ext_idle_notifier_v1::Request,
      _: &(),
      _: &DisplayHandle,
      init: &mut DataInit<'_, Self>,
    ) {
      if let ext_idle_notifier_v1::Request::GetIdleNotification { id, timeout, .. } = request {
        let resource = init.init(id, ());
        state.notifications.lock().unwrap().push(Watched {
          resource,
          timeout,
          alive: true,
        });
      }
    }
  }

  impl ServerDispatch<Notification, ()> for Shared {
    fn request(
      _: &mut Self,
      _: &Client,
      _: &Notification,
      _: <Notification as Resource>::Request,
      _: &(),
      _: &DisplayHandle,
      _: &mut DataInit<'_, Self>,
    ) {
    }

    fn destroyed(state: &mut Self, _: ClientId, resource: &Notification, _: &()) {
      for watched in state.notifications.lock().unwrap().iter_mut() {
        if watched.resource == *resource {
          watched.alive = false;
        }
      }
    }
  }

  /// A compositor on its own socket, `WAYLAND_DISPLAY` points at it.
  /// nextest runs each test in its own process, so setting the environment is safe.
  struct Compositor {
    shared: Shared,
    stop: Arc<AtomicBool>,
    _dir: tempfile::TempDir,
  }

  impl Compositor {
    fn start(with_notifier: bool) -> Self {
      Self::start_with_config(with_notifier, 1)
    }

    fn start_with_config(with_notifier: bool, seat_count: usize) -> Self {
      let dir = tempfile::tempdir().unwrap();
      unsafe {
        std::env::set_var("XDG_RUNTIME_DIR", dir.path());
        std::env::set_var("WAYLAND_DISPLAY", "wayland-test");
        std::env::remove_var("WAYLAND_SOCKET");
      }
      let socket = ListeningSocket::bind("wayland-test").unwrap();
      let shared = Shared::default();
      let stop = Arc::new(AtomicBool::new(false));
      let (mut state, stopped) = (shared.clone(), stop.clone());
      thread::spawn(move || {
        let mut display = Display::<Shared>::new().unwrap();
        let handle = display.handle();
        for _ in 0..seat_count {
          handle.create_global::<Shared, ServerSeat, ()>(7, ());
        }
        if with_notifier {
          handle.create_global::<Shared, Notifier, ()>(1, ());
        }
        while !stopped.load(Ordering::Relaxed) {
          if let Some(stream) = socket.accept().unwrap() {
            display
              .handle()
              .insert_client(stream, Arc::new(NoData))
              .unwrap();
          }
          display.dispatch_clients(&mut state).unwrap();
          display.flush_clients().ok();
          thread::sleep(Duration::from_millis(2));
        }
      });
      Self {
        shared,
        stop,
        _dir: dir,
      }
    }

    /// (timeout, still alive) of every notification asked for, oldest first
    fn watched(&self) -> Vec<(u32, bool)> {
      let list = self.shared.notifications.lock().unwrap();
      list.iter().map(|w| (w.timeout, w.alive)).collect()
    }

    fn live(&self) -> Vec<u32> {
      self
        .watched()
        .into_iter()
        .filter(|w| w.1)
        .map(|w| w.0)
        .collect()
    }

    fn send(&self, timeout: u32, idle: bool) {
      for watched in self.shared.notifications.lock().unwrap().iter() {
        if watched.alive && watched.timeout == timeout {
          match idle {
            true => watched.resource.idled(),
            false => watched.resource.resumed(),
          }
        }
      }
    }
  }

  impl Drop for Compositor {
    fn drop(&mut self) {
      self.stop.store(true, Ordering::Relaxed);
    }
  }

  type Events = Rc<RefCell<Vec<(String, bool)>>>;

  fn start(cx: &mut TestAppContext) -> Events {
    cx.executor().allow_parking();
    let events = Events::default();
    let seen = events.clone();
    cx.update(|cx| {
      init(cx, move |name, idle, _| {
        seen.borrow_mut().push((name.into(), idle))
      })
    });
    events
  }

  fn set(cx: &mut TestAppContext, timeouts: &[(&str, u64)]) {
    let timeouts = timeouts
      .iter()
      .map(|(name, millis)| (name.to_string(), Duration::from_millis(*millis)))
      .collect();
    cx.update(|cx| cx.idle().set_timeouts(timeouts));
  }

  /// the watcher and compositor are real threads: wait in real time
  fn wait(cx: &mut TestAppContext, done: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
      cx.run_until_parked();
      if done() {
        return;
      }
      assert!(Instant::now() < deadline, "timed out");
      thread::sleep(Duration::from_millis(5));
    }
  }

  /// nothing more arrives for a while
  fn settle(cx: &mut TestAppContext) {
    for _ in 0..20 {
      cx.run_until_parked();
      thread::sleep(Duration::from_millis(5));
    }
  }

  #[gpui::test]
  fn watches_the_timeouts(cx: &mut TestAppContext) {
    let compositor = Compositor::start(true);
    let events = start(cx);
    set(cx, &[("lock", 300_000), ("off", 0), ("huge", u64::MAX / 2)]);
    // zero is left out, too long is clamped
    wait(cx, || compositor.watched().len() == 2);
    let mut live = compositor.live();
    live.sort();
    assert_eq!(live, [300_000, u32::MAX]);

    compositor.send(300_000, true);
    wait(cx, || events.borrow().len() == 1);
    // repeated states are not reported twice
    compositor.send(300_000, true);
    compositor.send(300_000, false);
    wait(cx, || events.borrow().len() == 2);
    compositor.send(300_000, false);
    settle(cx);
    assert_eq!(
      *events.borrow(),
      [("lock".to_string(), true), ("lock".to_string(), false)]
    );
  }

  #[gpui::test]
  fn replacing_resumes_what_was_idle(cx: &mut TestAppContext) {
    let compositor = Compositor::start(true);
    let events = start(cx);
    set(cx, &[("lock", 1000), ("dim", 500)]);
    wait(cx, || compositor.live().len() == 2);
    compositor.send(500, true);
    wait(cx, || events.borrow().len() == 1);

    set(cx, &[("lock", 2000)]);
    wait(cx, || compositor.live() == [2000]);
    wait(cx, || events.borrow().len() == 2);
    // only the idle one reports false, the old ones are destroyed
    assert_eq!(events.borrow()[1], ("dim".to_string(), false));
    assert_eq!(compositor.watched().iter().filter(|w| !w.1).count(), 2);

    // an empty list stops watching
    set(cx, &[]);
    wait(cx, || compositor.live().is_empty());
    settle(cx);
    assert_eq!(events.borrow().len(), 2);
  }

  #[gpui::test]
  fn events_of_destroyed_notifications_are_dropped(cx: &mut TestAppContext) {
    let compositor = Compositor::start(true);
    let events = start(cx);
    set(cx, &[("lock", 1000)]);
    wait(cx, || compositor.live().len() == 1);
    set(cx, &[("other", 3000)]);
    wait(cx, || compositor.live() == [3000]);
    // the compositor still knows the old object until it processes the destroy
    for watched in compositor.shared.notifications.lock().unwrap().iter() {
      if watched.timeout == 1000 {
        watched.resource.idled();
      }
    }
    settle(cx);
    assert!(events.borrow().is_empty());
  }

  #[gpui::test]
  fn duplicate_names_are_cleaned_up(cx: &mut TestAppContext) {
    let compositor = Compositor::start(true);
    let _events = start(cx);
    set(cx, &[("lock", 1000), ("lock", 2000)]);
    wait(cx, || compositor.watched().len() == 2);
    set(cx, &[]);
    settle(cx);
    assert!(compositor.live().is_empty(), "{:?}", compositor.live());
  }

  #[gpui::test]
  fn without_a_notifier_nothing_happens(cx: &mut TestAppContext) {
    let compositor = Compositor::start(false);
    let events = start(cx);
    set(cx, &[("lock", 1000)]);
    settle(cx);
    assert!(compositor.watched().is_empty());
    assert!(events.borrow().is_empty());
  }

  #[gpui::test]
  fn without_a_compositor_nothing_happens(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    unsafe {
      std::env::set_var("XDG_RUNTIME_DIR", dir.path());
      std::env::set_var("WAYLAND_DISPLAY", "nothing-here");
    }
    let events = start(cx);
    set(cx, &[("lock", 1000)]);
    settle(cx);
    assert!(events.borrow().is_empty());
  }

  #[test]
  fn the_watcher_ends_with_its_sender() {
    let compositor = Compositor::start(true);
    let (timeouts, timeouts_rx) = flume::unbounded();
    let (events, _events_rx) = flume::unbounded();
    let watcher = thread::spawn(move || watch(timeouts_rx, events));
    timeouts
      .send(vec![("lock".into(), Duration::from_secs(1))])
      .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while compositor.live().is_empty() {
      assert!(Instant::now() < deadline);
      thread::sleep(Duration::from_millis(5));
    }
    drop(timeouts);
    watcher.join().unwrap().unwrap();
  }

  #[test]
  fn the_watcher_reports_a_missing_notifier() {
    let _compositor = Compositor::start(false);
    let (_timeouts, timeouts_rx) = flume::unbounded();
    let (events, _events_rx) = flume::unbounded();
    let error = watch(timeouts_rx, events).unwrap_err();
    assert_eq!(
      error.to_string(),
      "the compositor has no ext_idle_notifier_v1"
    );
  }

  #[test]
  fn the_watcher_reports_a_missing_seat() {
    let _compositor = Compositor::start_with_config(true, 0);
    let (_timeouts, timeouts_rx) = flume::unbounded();
    let (events, _events_rx) = flume::unbounded();
    let error = watch(timeouts_rx, events).unwrap_err();
    assert!(!error.to_string().is_empty());
  }

  #[gpui::test]
  fn multiple_seats_binds_first_seat(cx: &mut TestAppContext) {
    let compositor = Compositor::start_with_config(true, 2);
    let events = start(cx);
    set(cx, &[("lock", 1000)]);
    wait(cx, || compositor.live().len() == 1);
    compositor.send(1000, true);
    wait(cx, || events.borrow().len() == 1);
    assert_eq!(*events.borrow(), [("lock".to_string(), true)]);
  }

  #[gpui::test]
  fn watcher_failure_leaves_idle_handle_as_zombie(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    unsafe {
      std::env::set_var("XDG_RUNTIME_DIR", dir.path());
      std::env::set_var("WAYLAND_DISPLAY", "nothing-running-here");
    }
    let events = start(cx);
    settle(cx);
    // Setting timeouts when the watcher thread exited does not panic
    set(cx, &[("lock", 1000), ("dim", 500)]);
    settle(cx);
    assert!(events.borrow().is_empty());
  }

  #[gpui::test]
  fn events_of_destroyed_notifications_are_dropped_when_name_is_reused(cx: &mut TestAppContext) {
    let compositor = Compositor::start(true);
    let events = start(cx);
    set(cx, &[("lock", 1000)]);
    wait(cx, || compositor.live() == [1000]);

    // Replace timeout with a different duration under the same name "lock"
    set(cx, &[("lock", 3000)]);
    wait(cx, || compositor.live() == [3000]);

    // Emit idled on the old notification
    for watched in compositor.shared.notifications.lock().unwrap().iter() {
      if watched.timeout == 1000 {
        watched.resource.idled();
      }
    }
    settle(cx);
    // The old notification was destroyed, so its events must be dropped even though
    // "lock" exists in state.notifications with the new timeout.
    assert!(events.borrow().is_empty());
  }

  #[test]
  fn abrupt_connection_drop_terminates_watcher() {
    let compositor = Compositor::start(true);
    let (timeouts, timeouts_rx) = flume::unbounded();
    let (events, _events_rx) = flume::unbounded();
    let watcher = thread::spawn(move || watch(timeouts_rx, events));
    timeouts
      .send(vec![("lock".into(), Duration::from_millis(500))])
      .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while compositor.live().is_empty() {
      assert!(Instant::now() < deadline);
      thread::sleep(Duration::from_millis(5));
    }
    drop(compositor);
    let res = watcher.join().unwrap();
    assert!(
      res.is_err(),
      "expected watch to return error when compositor drops abruptly"
    );
  }
}

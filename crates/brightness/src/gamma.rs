//! Color temperature through `zwlr_gamma_control_v1`. The compositor keeps the original tables
//! and puts them back when a control is destroyed or the shell goes away, so they are never read.

use std::{collections::HashMap, io::Write, thread, time::Duration};

use anyhow::{Context, Result};
use rustix::{
  event::{PollFd, PollFlags, Timespec, poll},
  fs::{MemfdFlags, SeekFrom, memfd_create, seek},
};
use wayland_client::{
  Connection, Dispatch, Proxy, QueueHandle,
  globals::{GlobalListContents, registry_queue_init},
  protocol::{wl_output::WlOutput, wl_registry},
};
use wayland_protocols_wlr::gamma_control::v1::client::{
  zwlr_gamma_control_manager_v1::ZwlrGammaControlManagerV1,
  zwlr_gamma_control_v1::{self, ZwlrGammaControlV1},
};

pub(crate) const MIN_KELVIN: u32 = 1000;
/// Daylight, the tables as they are
pub(crate) const IDENTITY_KELVIN: u32 = 6500;

/// How often the thread looks for new temperatures while the compositor is quiet
#[cfg(not(test))]
const COMMAND_POLL: Duration = Duration::from_millis(250);
#[cfg(test)]
const COMMAND_POLL: Duration = Duration::from_millis(10);

/// The white point of `kelvin` as red, green and blue factors; Tanner Helland's fit of the
/// Planckian locus, scaled so 6500 K leaves colors as they are
fn white_point(kelvin: u32) -> [f64; 3] {
  let fit = |kelvin: u32| {
    let t = kelvin as f64 / 100.;
    let red = if t <= 66. {
      255.
    } else {
      329.698727446 * (t - 60.).powf(-0.1332047592)
    };
    let green = if t <= 66. {
      99.4708025861 * t.ln() - 161.1195681661
    } else {
      288.1221695283 * (t - 60.).powf(-0.0755148492)
    };
    let blue = if t >= 66. {
      255.
    } else if t <= 19. {
      0.
    } else {
      138.5177312231 * (t - 10.).ln() - 305.0447927307
    };
    [red, green, blue].map(|c| c.clamp(0., 255.) / 255.)
  };
  let (white, daylight) = (
    fit(kelvin.clamp(MIN_KELVIN, IDENTITY_KELVIN)),
    fit(IDENTITY_KELVIN),
  );
  [0, 1, 2].map(|i| (white[i] / daylight[i]).min(1.))
}

/// The red, green and blue ramps of `size` entries each, one after the other
pub(crate) fn ramp(kelvin: u32, size: usize) -> Vec<u16> {
  let step = 1. / (size.max(2) - 1) as f64;
  white_point(kelvin)
    .iter()
    .flat_map(|factor| {
      (0..size).map(move |i| (i as f64 * step * factor * u16::MAX as f64).round() as u16)
    })
    .collect()
}

struct Output {
  output: WlOutput,
  control: Option<ZwlrGammaControlV1>,
  /// Entries per ramp, once the compositor said
  size: Option<u32>,
  /// Another client holds the tables; tried again the next time the light comes on
  failed: bool,
}

struct State {
  manager: ZwlrGammaControlManagerV1,
  /// By registry name
  outputs: HashMap<u32, Output>,
  /// None: the original tables
  kelvin: Option<u32>,
}

impl State {
  fn apply(&mut self, kelvin: Option<u32>, qh: &QueueHandle<Self>) {
    let kelvin = kelvin.filter(|&k| k < IDENTITY_KELVIN);
    if kelvin.is_none() {
      for output in self.outputs.values_mut() {
        if let Some(control) = output.control.take() {
          control.destroy();
        }
        output.size = None;
        output.failed = false;
      }
    }
    self.kelvin = kelvin;
    let names: Vec<u32> = self.outputs.keys().copied().collect();
    for name in names {
      self.refresh(name, qh);
    }
  }

  /// Brings one output to the current temperature
  fn refresh(&mut self, name: u32, qh: &QueueHandle<Self>) {
    let (Some(kelvin), Some(output)) = (self.kelvin, self.outputs.get_mut(&name)) else {
      return;
    };
    match (&output.control, output.size) {
      (None, _) if !output.failed => {
        output.control = Some(self.manager.get_gamma_control(&output.output, qh, name));
      }
      (Some(control), Some(size)) => {
        if let Err(e) = set_gamma(control, kelvin, size) {
          tracing::warn!("nightlight: setting gamma failed: {e:#}");
        }
      }
      _ => {}
    }
  }
}

fn set_gamma(control: &ZwlrGammaControlV1, kelvin: u32, size: u32) -> Result<()> {
  let bytes: Vec<u8> = ramp(kelvin, size as usize)
    .into_iter()
    .flat_map(u16::to_ne_bytes)
    .collect();
  let fd = memfd_create("corona-gamma", MemfdFlags::CLOEXEC)?;
  let mut file = std::fs::File::from(fd);
  file.write_all(&bytes)?;
  seek(&file, SeekFrom::Start(0))?;
  control.set_gamma(std::os::fd::AsFd::as_fd(&file));
  Ok(())
}

/// Starts the gamma thread: it takes temperatures (None: off) and says once whether the
/// compositor lets the shell change gamma
pub(crate) fn spawn(commands: flume::Receiver<Option<u32>>, available: flume::Sender<bool>) {
  thread::Builder::new()
    .name("nightlight".into())
    .spawn(move || {
      if let Err(e) = watch(commands, &available) {
        tracing::warn!("nightlight unavailable: {e:#}");
        let _ = available.send(false);
      }
    })
    .expect("spawning the nightlight thread");
}

fn watch(commands: flume::Receiver<Option<u32>>, available: &flume::Sender<bool>) -> Result<()> {
  let conn = Connection::connect_to_env()?;
  let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
  let qh = queue.handle();
  let manager: ZwlrGammaControlManagerV1 = globals
    .bind(&qh, 1..=1, ())
    .context("the compositor has no zwlr_gamma_control_manager_v1")?;
  let mut state = State {
    manager,
    outputs: HashMap::new(),
    kelvin: None,
  };
  globals.contents().with_list(|list| {
    for global in list.iter().filter(|g| g.interface == "wl_output") {
      let output = globals
        .registry()
        .bind(global.name, global.version.min(4), &qh, ());
      state.outputs.insert(global.name, new_output(output));
    }
  });
  let _ = available.send(true);

  loop {
    // only the latest temperature matters
    if let Some(kelvin) = commands.try_iter().last() {
      state.apply(kelvin, &qh);
    }
    queue.dispatch_pending(&mut state)?;
    queue.flush()?;
    let Some(guard) = queue.prepare_read() else {
      continue;
    };
    let fd = guard.connection_fd();
    let mut fds = [PollFd::new(&fd, PollFlags::IN)];
    let timeout = Timespec::try_from(COMMAND_POLL)?;
    if poll(&mut fds, Some(&timeout))? > 0 {
      guard.read()?;
    }
    if commands.is_disconnected() {
      return Ok(());
    }
  }
}

fn new_output(output: WlOutput) -> Output {
  Output {
    output,
    control: None,
    size: None,
    failed: false,
  }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
  fn event(
    state: &mut Self,
    registry: &wl_registry::WlRegistry,
    event: wl_registry::Event,
    _: &GlobalListContents,
    _: &Connection,
    qh: &QueueHandle<Self>,
  ) {
    match event {
      wl_registry::Event::Global {
        name,
        interface,
        version,
      } if interface == "wl_output" => {
        let output = registry.bind(name, version.min(4), qh, ());
        state.outputs.insert(name, new_output(output));
        state.refresh(name, qh);
      }
      wl_registry::Event::GlobalRemove { name } => {
        if let Some(output) = state.outputs.remove(&name) {
          if let Some(control) = output.control {
            control.destroy();
          }
          if output.output.version() >= 3 {
            output.output.release();
          }
        }
      }
      _ => {}
    }
  }
}

impl Dispatch<ZwlrGammaControlV1, u32> for State {
  fn event(
    state: &mut Self,
    control: &ZwlrGammaControlV1,
    event: zwlr_gamma_control_v1::Event,
    name: &u32,
    _: &Connection,
    qh: &QueueHandle<Self>,
  ) {
    let Some(output) = state.outputs.get_mut(name) else {
      return;
    };
    match event {
      zwlr_gamma_control_v1::Event::GammaSize { size } => {
        output.size = Some(size);
        state.refresh(*name, qh);
      }
      zwlr_gamma_control_v1::Event::Failed => {
        tracing::warn!("nightlight: an output refused gamma, another program may hold it");
        control.destroy();
        output.control = None;
        output.size = None;
        output.failed = true;
      }
      _ => {}
    }
  }
}

wayland_client::delegate_noop!(State: ignore WlOutput);
wayland_client::delegate_noop!(State: ZwlrGammaControlManagerV1);

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn daylight_is_the_identity() {
    let ramp = ramp(IDENTITY_KELVIN, 256);
    assert_eq!(ramp.len(), 768);
    for channel in ramp.chunks(256) {
      assert_eq!(channel[0], 0);
      assert_eq!(channel[255], u16::MAX);
      assert_eq!(channel[128], (128. / 255. * 65535f64).round() as u16);
    }
  }

  #[test]
  fn warmer_cuts_blue_first() {
    let [r, g, b] = white_point(3000);
    assert_eq!(r, 1.);
    assert!(g < r && b < g, "{r} {g} {b}");
    // at the lowest temperature blue is gone
    assert_eq!(white_point(MIN_KELVIN)[2], 0.);
    // out of range is clamped
    assert_eq!(white_point(500), white_point(MIN_KELVIN));
    assert_eq!(white_point(9000), [1.; 3]);
    // warmer never adds blue or green
    let mut last = white_point(MIN_KELVIN);
    for kelvin in (MIN_KELVIN..=IDENTITY_KELVIN).step_by(100) {
      let now = white_point(kelvin);
      assert!(now[1] >= last[1] && now[2] >= last[2], "{kelvin}");
      last = now;
    }
  }

  #[test]
  fn ramps_rise_and_odd_sizes_work() {
    let ramp = ramp(4000, 1024);
    for channel in ramp.chunks(1024) {
      assert!(channel.windows(2).all(|w| w[0] <= w[1]));
    }
    assert!(super::ramp(4000, 0).is_empty());
    assert_eq!(super::ramp(4000, 1), [0, 0, 0]);
    assert_eq!(
      super::ramp(IDENTITY_KELVIN, 2),
      [0, 65535, 0, 65535, 0, 65535]
    );
  }

  mod compositor {
    use std::{
      io::Read,
      sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
      },
      time::Instant,
    };

    use wayland_protocols_wlr::gamma_control::v1::server::{
      zwlr_gamma_control_manager_v1::{self, ZwlrGammaControlManagerV1 as Manager},
      zwlr_gamma_control_v1::{self as control, ZwlrGammaControlV1 as Control},
    };
    use wayland_server::{
      Client, DataInit, Dispatch as ServerDispatch, Display, DisplayHandle, GlobalDispatch,
      ListeningSocket, New, Resource,
      backend::{ClientData, ClientId, DisconnectReason, GlobalId},
      protocol::wl_output::WlOutput as ServerOutput,
    };

    use corona_config::ConfigProvider;

    use super::super::*;

    /// gamma size of every output; an output named by `BUSY` fails instead
    const SIZE: u32 = 8;
    const BUSY: u32 = 99;

    #[derive(Default)]
    struct Recorded {
      /// (output, latest tables, still alive) per control asked for
      controls: Vec<(u32, Option<Vec<u16>>, bool, Control)>,
    }

    #[derive(Clone, Default)]
    struct Shared(Arc<Mutex<Recorded>>);

    struct NoData;
    impl ClientData for NoData {
      fn disconnected(&self, _: ClientId, _: DisconnectReason) {}
    }

    impl GlobalDispatch<ServerOutput, u32> for Shared {
      fn bind(
        _: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        output: New<ServerOutput>,
        id: &u32,
        init: &mut DataInit<'_, Self>,
      ) {
        init.init(output, *id);
      }
    }

    impl ServerDispatch<ServerOutput, u32> for Shared {
      fn request(
        _: &mut Self,
        _: &Client,
        _: &ServerOutput,
        _: <ServerOutput as Resource>::Request,
        _: &u32,
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
      ) {
      }
    }

    impl GlobalDispatch<Manager, ()> for Shared {
      fn bind(
        _: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        manager: New<Manager>,
        _: &(),
        init: &mut DataInit<'_, Self>,
      ) {
        init.init(manager, ());
      }
    }

    impl ServerDispatch<Manager, ()> for Shared {
      fn request(
        state: &mut Self,
        _: &Client,
        _: &Manager,
        request: zwlr_gamma_control_manager_v1::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
      ) {
        if let zwlr_gamma_control_manager_v1::Request::GetGammaControl { id, output } = request {
          let output = *output.data::<u32>().unwrap();
          let control = init.init(id, output);
          match output {
            BUSY => control.failed(),
            _ => control.gamma_size(SIZE),
          }
          state
            .0
            .lock()
            .unwrap()
            .controls
            .push((output, None, true, control));
        }
      }
    }

    impl ServerDispatch<Control, u32> for Shared {
      fn request(
        state: &mut Self,
        _: &Client,
        resource: &Control,
        request: control::Request,
        _: &u32,
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
      ) {
        if let control::Request::SetGamma { fd } = request {
          let mut bytes = Vec::new();
          std::fs::File::from(fd).read_to_end(&mut bytes).unwrap();
          let tables = bytes
            .chunks(2)
            .map(|b| u16::from_ne_bytes([b[0], b[1]]))
            .collect();
          let mut recorded = state.0.lock().unwrap();
          if let Some(entry) = recorded.controls.iter_mut().find(|c| c.3 == *resource) {
            entry.1 = Some(tables);
          }
        }
      }

      fn destroyed(state: &mut Self, _: ClientId, resource: &Control, _: &u32) {
        let mut recorded = state.0.lock().unwrap();
        if let Some(entry) = recorded.controls.iter_mut().find(|c| c.3 == *resource) {
          entry.2 = false;
        }
      }
    }

    enum Command {
      AddOutput(u32),
      RemoveOutput(u32),
    }

    /// A compositor on its own socket, `WAYLAND_DISPLAY` points at it.
    /// nextest runs each test in its own process, so setting the environment is safe.
    struct Compositor {
      shared: Shared,
      commands: flume::Sender<Command>,
      stop: Arc<AtomicBool>,
      _dir: tempfile::TempDir,
    }

    impl Compositor {
      fn start(with_manager: bool, outputs: &[u32]) -> Self {
        let dir = tempfile::tempdir().unwrap();
        unsafe {
          std::env::set_var("XDG_RUNTIME_DIR", dir.path());
          std::env::set_var("WAYLAND_DISPLAY", "wayland-test");
          std::env::remove_var("WAYLAND_SOCKET");
        }
        let socket = ListeningSocket::bind("wayland-test").unwrap();
        let shared = Shared::default();
        let stop = Arc::new(AtomicBool::new(false));
        let (commands, received) = flume::unbounded();
        let (mut state, stopped, outputs) = (shared.clone(), stop.clone(), outputs.to_vec());
        thread::spawn(move || {
          let mut display = Display::<Shared>::new().unwrap();
          let mut handle = display.handle();
          let mut globals: HashMap<u32, GlobalId> = HashMap::new();
          for id in outputs {
            globals.insert(id, handle.create_global::<Shared, ServerOutput, u32>(4, id));
          }
          if with_manager {
            handle.create_global::<Shared, Manager, ()>(1, ());
          }
          while !stopped.load(Ordering::Relaxed) {
            for command in received.try_iter() {
              match command {
                Command::AddOutput(id) => {
                  globals.insert(id, handle.create_global::<Shared, ServerOutput, u32>(4, id));
                }
                Command::RemoveOutput(id) => {
                  handle.remove_global::<Shared>(globals.remove(&id).unwrap());
                }
              }
            }
            if let Some(stream) = socket.accept().unwrap() {
              handle.insert_client(stream, Arc::new(NoData)).unwrap();
            }
            display.dispatch_clients(&mut state).unwrap();
            display.flush_clients().ok();
            thread::sleep(Duration::from_millis(2));
          }
        });
        Self {
          shared,
          commands,
          stop,
          _dir: dir,
        }
      }

      /// (output, latest tables, alive) of every control, oldest first
      fn controls(&self) -> Vec<(u32, Option<Vec<u16>>, bool)> {
        let recorded = self.shared.0.lock().unwrap();
        recorded
          .controls
          .iter()
          .map(|(o, t, a, _)| (*o, t.clone(), *a))
          .collect()
      }

      /// the latest tables of each live control, by output
      fn live(&self) -> Vec<(u32, Option<Vec<u16>>)> {
        let mut live: Vec<_> = self
          .controls()
          .into_iter()
          .filter(|c| c.2)
          .map(|c| (c.0, c.1))
          .collect();
        live.sort_by_key(|c| c.0);
        live
      }
    }

    impl Drop for Compositor {
      fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
      }
    }

    fn wait(done: impl Fn() -> bool) {
      let deadline = Instant::now() + Duration::from_secs(10);
      while !done() {
        assert!(Instant::now() < deadline, "timed out");
        thread::sleep(Duration::from_millis(5));
      }
    }

    fn start() -> (flume::Sender<Option<u32>>, flume::Receiver<bool>) {
      let (commands, rx) = flume::unbounded();
      let (tx, available) = flume::unbounded();
      spawn(rx, tx);
      (commands, available)
    }

    fn table(kelvin: u32) -> Option<Vec<u16>> {
      Some(ramp(kelvin, SIZE as usize))
    }

    #[test]
    fn sets_every_output_and_restores_when_off() {
      let compositor = Compositor::start(true, &[1, 2]);
      let (commands, available) = start();
      assert_eq!(available.recv_timeout(Duration::from_secs(10)), Ok(true));

      commands.send(Some(3000)).unwrap();
      wait(|| compositor.live() == [(1, table(3000)), (2, table(3000))]);
      // the same controls take a new temperature
      commands.send(Some(4500)).unwrap();
      wait(|| compositor.live() == [(1, table(4500)), (2, table(4500))]);
      assert_eq!(compositor.controls().len(), 2);

      // off and daylight give the tables back
      commands.send(None).unwrap();
      wait(|| compositor.live().is_empty());
      commands.send(Some(3000)).unwrap();
      wait(|| compositor.live().len() == 2);
      commands.send(Some(IDENTITY_KELVIN)).unwrap();
      wait(|| compositor.live().is_empty());
      assert_eq!(compositor.controls().len(), 4);
    }

    #[test]
    fn new_outputs_get_the_light() {
      let compositor = Compositor::start(true, &[1]);
      let (commands, available) = start();
      assert_eq!(available.recv_timeout(Duration::from_secs(10)), Ok(true));
      commands.send(Some(2000)).unwrap();
      wait(|| compositor.live() == [(1, table(2000))]);

      compositor.commands.send(Command::AddOutput(2)).unwrap();
      wait(|| compositor.live() == [(1, table(2000)), (2, table(2000))]);
      // an unplugged output lets go of its control
      compositor.commands.send(Command::RemoveOutput(1)).unwrap();
      wait(|| compositor.live() == [(2, table(2000))]);

      // while off nothing is asked for
      commands.send(None).unwrap();
      wait(|| compositor.live().is_empty());
      compositor.commands.send(Command::AddOutput(3)).unwrap();
      thread::sleep(Duration::from_millis(100));
      assert!(compositor.live().is_empty());
    }

    #[test]
    fn a_held_output_is_skipped() {
      let compositor = Compositor::start(true, &[1, BUSY]);
      let (commands, available) = start();
      assert_eq!(available.recv_timeout(Duration::from_secs(10)), Ok(true));
      commands.send(Some(3000)).unwrap();
      wait(|| compositor.live() == [(1, table(3000))]);
      // not asked again until the light comes on anew
      commands.send(Some(3500)).unwrap();
      wait(|| compositor.live() == [(1, table(3500))]);
      let busy = |c: &Compositor| c.controls().iter().filter(|c| c.0 == BUSY).count();
      assert_eq!(busy(&compositor), 1);
      commands.send(None).unwrap();
      wait(|| compositor.live().is_empty());
      commands.send(Some(3000)).unwrap();
      wait(|| busy(&compositor) == 2);
    }

    #[test]
    fn without_the_protocol_it_is_unavailable() {
      let _compositor = Compositor::start(false, &[1]);
      let (_commands, available) = start();
      assert_eq!(available.recv_timeout(Duration::from_secs(10)), Ok(false));
    }

    #[gpui_kit::test]
    fn the_shell_switches_it(cx: &mut gpui_kit::TestAppContext) {
      use crate::BrightnessExt;

      let compositor = Compositor::start(true, &[1]);
      cx.executor().allow_parking();
      let bus = corona_utils::test_bus::TestBus::new();
      let conn = futures_lite::future::block_on(bus.conn());
      cx.update(|cx| {
        cx.set_global(corona_config::Config::default());
        crate::init(cx, &conn).unwrap();
      });
      let nightlight = |cx: &mut gpui_kit::TestAppContext| {
        cx.run_until_parked();
        cx.read(|cx| *cx.brightness().nightlight(cx))
      };
      let deadline = Instant::now() + Duration::from_secs(10);
      while !nightlight(cx).available {
        assert!(Instant::now() < deadline, "timed out");
        thread::sleep(Duration::from_millis(5));
      }
      assert!(!nightlight(cx).on);
      assert!(compositor.live().is_empty());

      cx.update(|cx| cx.brightness().set_nightlight(true, cx))
        .unwrap();
      assert!(nightlight(cx).on);
      wait(|| compositor.live() == [(1, table(4000))]);
      let set = cx.update(|cx| cx.brightness().set_nightlight_temperature(20000, cx));
      assert_eq!(set.unwrap(), IDENTITY_KELVIN);
      cx.update(|cx| cx.brightness().set_nightlight_temperature(2500, cx))
        .unwrap();
      assert_eq!(nightlight(cx).temperature, 2500);
      wait(|| compositor.live() == [(1, table(2500))]);

      // the settings start the temperature over
      cx.update(|cx| {
        let mut config = cx.config().clone();
        config.nightlight.temperature = 3300;
        cx.set_global(config);
      });
      assert_eq!(nightlight(cx).temperature, 3300);
      wait(|| compositor.live() == [(1, table(3300))]);

      cx.update(|cx| cx.brightness().set_nightlight(false, cx))
        .unwrap();
      assert!(!nightlight(cx).on);
      wait(|| compositor.live().is_empty());
    }

    #[gpui_kit::test]
    fn unavailable_refuses_changes(cx: &mut gpui_kit::TestAppContext) {
      use crate::BrightnessExt;

      unsafe { std::env::set_var("WAYLAND_DISPLAY", "nothing-here") };
      cx.executor().allow_parking();
      let bus = corona_utils::test_bus::TestBus::new();
      let conn = futures_lite::future::block_on(bus.conn());
      cx.update(|cx| {
        cx.set_global(corona_config::Config::default());
        crate::init(cx, &conn).unwrap();
      });
      cx.run_until_parked();
      let error = cx
        .update(|cx| cx.brightness().set_nightlight(true, cx))
        .unwrap_err();
      assert!(error.to_string().contains("unavailable"), "{error}");
      assert!(
        cx.update(|cx| cx.brightness().set_nightlight_temperature(3000, cx))
          .is_err()
      );
    }

    #[test]
    fn without_a_compositor_it_is_unavailable() {
      unsafe { std::env::set_var("WAYLAND_DISPLAY", "nothing-here") };
      let (_commands, available) = start();
      assert_eq!(available.recv_timeout(Duration::from_secs(10)), Ok(false));
    }
  }
}

use std::{
  cell::RefCell,
  ops::Deref,
  rc::Rc,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
  time::Duration,
};

use wayland_client::protocol::wl_output::WlOutput;

use crate::{
  adapter::{gpu::GpuContext, slint::SlintCustomPlatform, wayland::WaylandAdapter},
  api::event::OutputEvent,
  error::CoronaError,
  event::{
    dbus::Dbus,
    event::ShellEvent,
    event_loop::{EventLoop, LoopHandle},
  },
  wayland::Dispatcher,
  widgets::Widgets,
};

pub(crate) type EventListener = Box<dyn Fn(ShellEvent)>;

#[derive(Clone)]
pub struct Corona {
  inner: Rc<CoronaInner>,
}

impl Deref for Corona {
  type Target = CoronaInner;

  fn deref(&self) -> &Self::Target {
    &self.inner
  }
}

pub struct CoronaInner {
  wayland: WaylandAdapter,
  gpu: Rc<GpuContext>,
  platform: Rc<SlintCustomPlatform>,
  event_loop: RefCell<Option<EventLoop>>,
  loop_handle: LoopHandle,
  dbus: Dbus,
  event_listeners: RefCell<Vec<EventListener>>,
  widgets: Widgets,
  outputs: RefCell<Vec<WlOutput>>,
  exit_requested: Arc<AtomicBool>,
  dispatcher: RefCell<Option<Dispatcher>>,
}

impl Corona {
  pub fn init() -> Result<Self, CoronaError> {
    let (mut wayland, registry_state, output_state, seat_state) = WaylandAdapter::init()?;
    let outputs = output_state.outputs().collect::<Vec<_>>();

    let gpu = GpuContext::init(&wayland.display_id())?;
    let event_loop = EventLoop::init(&mut wayland)?;
    let exit_requested = Arc::new(AtomicBool::new(false));
    let platform = SlintCustomPlatform::init(gpu.clone(), &event_loop, exit_requested.clone())?;

    let dbus = Dbus::init(event_loop.event_sender())?;
    crate::event::hyprland::spawn(event_loop.event_sender());

    let loop_handle = event_loop.handle();

    let corona = Corona {
      inner: Rc::new(CoronaInner {
        wayland,
        gpu,
        platform,
        loop_handle,
        dbus,
        event_listeners: RefCell::new(Vec::new()),
        event_loop: RefCell::new(Some(event_loop)),
        widgets: Widgets::new(),
        outputs: RefCell::new(outputs),
        exit_requested,
        dispatcher: RefCell::new(None),
      }),
    };

    *corona.dispatcher.borrow_mut() = Some(Dispatcher {
      corona: corona.clone(),
      registry_state,
      output_state,
      seat_state,
      keyboard: None,
      pointer: None,
    });

    Ok(corona)
  }

  pub fn run(self) -> Result<(), CoronaError> {
    let mut event_loop = self
      .event_loop
      .borrow_mut()
      .take()
      .ok_or(CoronaError::EventLoopTaken)?;
    let mut dispatcher = self
      .dispatcher
      .borrow_mut()
      .take()
      .ok_or(CoronaError::DispatcherTaken)?;

    while !self.exit_requested.load(Ordering::Relaxed) {
      let timeout = if self.widgets.needs_render() {
        Some(Duration::ZERO)
      } else {
        slint::platform::duration_until_next_timer_update()
      };

      event_loop.dispatch(&mut dispatcher, timeout)?;
      slint::platform::update_timers_and_animations();
      self.render_if_dirty()?;
      self.wayland.flush()?;
    }

    drop(dispatcher);
    drop(event_loop);
    self.destroy();

    Ok(())
  }

  fn destroy(self) {
    let inner = match Rc::try_unwrap(self.inner) {
      Ok(inner) => inner,
      Err(_) => {
        tracing::warn!("CoronaInner is still referenced elsewhere, cannot destroy");
        return;
      }
    };

    inner.dbus.destroy();

    drop(inner.widgets);
    drop(inner.platform);

    if Rc::try_unwrap(inner.gpu).is_err() {
      tracing::warn!("GpuContext is still referenced elsewhere, cannot destroy");
    }

    if let Err(e) = inner.wayland.flush() {
      tracing::error!("Failed to flush Wayland connection during shutdown: {}", e);
    }
    drop(inner.wayland);
  }

  pub(crate) fn handle_shell_event(&self, event: ShellEvent) {
    if let ShellEvent::Output(event) = &event {
      self.sync_outputs(event);
    }

    for listener in self.event_listeners.borrow().iter() {
      listener(event.clone())
    }
  }

  fn sync_outputs(&self, event: &OutputEvent) {
    let mut outputs = self.outputs.borrow_mut();
    match event {
      OutputEvent::New(output) | OutputEvent::Update(output) => {
        if !outputs.contains(output) {
          outputs.push(output.clone());
        }
      }
      OutputEvent::Destroy(output) => outputs.retain(|existing| existing != output),
    }
  }

  fn render_if_dirty(&self) -> Result<(), CoronaError> {
    self.widgets.render_if_dirty()
  }

  pub(crate) fn widgets(&self) -> &Widgets {
    &self.widgets
  }

  pub(crate) fn wayland(&self) -> &WaylandAdapter {
    &self.wayland
  }

  pub(crate) fn dbus(&self) -> &Dbus {
    &self.dbus
  }

  pub(crate) fn loop_handle(&self) -> &LoopHandle {
    &self.loop_handle
  }

  pub(crate) fn platform(&self) -> &SlintCustomPlatform {
    &self.platform
  }

  pub(crate) fn add_event_listener(&self, f: EventListener) {
    self.event_listeners.borrow_mut().push(f);
  }

  pub fn outputs(&self) -> Vec<WlOutput> {
    self.outputs.borrow().clone()
  }
}

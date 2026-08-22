use std::{
  cell::{Cell, RefCell},
  ops::Deref,
  rc::Rc,
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

pub use slint;

mod adapter;
pub mod api;
mod error;
mod event;
mod wayland;
pub mod widgets;

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

type EventListener = Box<dyn Fn(ShellEvent)>;

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
  exit_requested: Cell<bool>,
  dispatcher: RefCell<Option<Dispatcher>>,
}

impl Corona {
  pub fn init() -> Result<Self, CoronaError> {
    let (mut wayland, registry_state, output_state, seat_state) = WaylandAdapter::init()?;
    let outputs = output_state.outputs().collect::<Vec<_>>();

    let gpu = GpuContext::init(&wayland)?;
    let event_loop = EventLoop::init(&mut wayland)?;
    let platform = SlintCustomPlatform::init(gpu.clone(), &event_loop)?;
    let dbus = Dbus::init(event_loop.event_sender())?;
    event::hyprland::spawn(event_loop.event_sender());

    let corona = Corona {
      inner: Rc::new(CoronaInner {
        wayland,
        gpu,
        platform,
        loop_handle: event_loop.handle(),
        dbus,
        event_listeners: RefCell::new(Vec::new()),
        event_loop: RefCell::new(Some(event_loop)),
        widgets: Widgets::new(),
        outputs: RefCell::new(outputs),
        exit_requested: Cell::new(false),
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

    while !self.exit_requested.get() {
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

  fn handle_shell_event(&self, event: ShellEvent) {
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

  fn set_exit_requested(&self) {
    self.exit_requested.set(true);
  }
}

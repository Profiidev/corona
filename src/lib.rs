use std::{rc::Rc, time::Duration};

use smithay_client_toolkit::{
  delegate_dispatch2, delegate_registry,
  output::OutputState,
  registry::{ProvidesRegistryState, RegistryState},
  registry_handlers,
  seat::SeatState,
};

use crate::{
  adapter::{gpu::GpuContext, slint::SlintCustomPlatform, wayland::WaylandAdapter},
  error::CoronaError,
  event::{
    dbus::Dbus,
    event::ShellEvent,
    event_loop::{EventLoop, LoopHandle},
  },
  widgets::Widgets,
};

pub use slint;

mod adapter;
pub mod api;
mod error;
mod event;
mod wayland;
pub mod widgets;

pub struct Corona {
  wayland: WaylandAdapter,
  gpu: Rc<GpuContext>,
  platform: Rc<SlintCustomPlatform>,
  event_loop: Option<EventLoop>,
  loop_handle: LoopHandle,
  dbus: Dbus,
  event_listeners: Vec<Box<dyn FnMut(ShellEvent)>>,
  widgets: Widgets,
  exit_requested: bool,
}

impl Corona {
  pub fn init() -> Result<Self, CoronaError> {
    let mut wayland = WaylandAdapter::init()?;
    let gpu = GpuContext::init(&wayland)?;
    let event_loop = EventLoop::init(&mut wayland)?;
    let platform = SlintCustomPlatform::init(gpu.clone(), &event_loop)?;
    let dbus = Dbus::init(event_loop.event_sender())?;

    Ok(Self {
      wayland,
      gpu,
      platform,
      loop_handle: event_loop.handle(),
      dbus,
      event_listeners: Vec::new(),
      event_loop: Some(event_loop),
      widgets: Widgets::new(),
      exit_requested: false,
    })
  }

  pub fn run(mut self) -> Result<(), CoronaError> {
    let mut event_loop = self.event_loop.take().ok_or(CoronaError::EventLoopTaken)?;

    while !self.exit_requested {
      let timeout = if self.widgets.needs_render() {
        Some(Duration::ZERO)
      } else {
        slint::platform::duration_until_next_timer_update()
      };

      event_loop.dispatch(&mut self, timeout)?;
      slint::platform::update_timers_and_animations();
      self.render_if_dirty()?;
      self.wayland.flush()?;
    }

    drop(event_loop);
    self.destroy();

    Ok(())
  }

  fn destroy(self) {
    self.dbus.destroy();

    drop(self.widgets);
    drop(self.platform);

    if Rc::try_unwrap(self.gpu).is_err() {
      tracing::warn!("GpuContext is still referenced elsewhere, cannot destroy");
    }

    if let Err(e) = self.wayland.flush() {
      tracing::error!("Failed to flush Wayland connection during shutdown: {}", e);
    }
    drop(self.wayland);
  }

  fn handle_shell_event(&mut self, event: ShellEvent) {
    for listener in &mut self.event_listeners {
      listener(event.clone())
    }
  }

  fn render_if_dirty(&self) -> Result<(), CoronaError> {
    self.widgets.render_if_dirty()
  }
}

delegate_dispatch2!(Corona);
delegate_registry!(Corona);

impl ProvidesRegistryState for Corona {
  fn registry(&mut self) -> &mut RegistryState {
    self.wayland.registry_state_mut()
  }
  registry_handlers![OutputState, SeatState];
}

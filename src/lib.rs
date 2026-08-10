use std::rc::Rc;

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
  event::{event::ShellEvent, event_loop::EventLoop},
  widgets::Widgets,
};

mod adapter;
mod api;
mod error;
mod event;
mod wayland;
pub mod widgets;

pub struct Corona {
  wayland: WaylandAdapter,
  gpu: Rc<GpuContext>,
  platform: Rc<SlintCustomPlatform>,
  event_loop: Option<EventLoop>,
  widgets: Widgets,
  exit_requested: bool,
}

impl Corona {
  pub fn init() -> Result<Self, CoronaError> {
    let mut wayland = WaylandAdapter::init()?;
    let gpu = GpuContext::init(&wayland)?;
    let platform = SlintCustomPlatform::init(gpu.clone())?;
    let event_loop = EventLoop::init(
      &mut wayland,
      #[cfg(feature = "hot-reload")]
      &platform,
    )?;

    Ok(Self {
      wayland,
      gpu,
      platform,
      event_loop: Some(event_loop),
      widgets: Widgets::new(),
      exit_requested: false,
    })
  }

  pub fn run(mut self) -> Result<(), CoronaError> {
    let mut event_loop = self.event_loop.take().ok_or(CoronaError::EventLoopTaken)?;

    while !self.exit_requested {
      event_loop.dispatch(&mut self)?;
      slint::platform::update_timers_and_animations();
      self.render_if_dirty()?;
      self.wayland.flush()?;
    }

    drop(event_loop);
    self.destroy();

    Ok(())
  }

  fn destroy(self) {
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

  fn handle_shell_event(&mut self, _event: ShellEvent) {
    // TODO
  }

  fn tick_clock(&mut self) {
    // TODO
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

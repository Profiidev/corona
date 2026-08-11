use calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::{
  compositor::CompositorState,
  output::OutputState,
  registry::RegistryState,
  seat::{Capability, SeatState},
  shell::{
    WaylandSurface,
    wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface},
  },
};
use wayland_client::{
  ConnectError, Connection, EventQueue, Proxy, QueueHandle,
  backend::{ObjectId, WaylandError},
  globals::{BindError, GlobalError, registry_queue_init},
  protocol::{
    wl_keyboard::WlKeyboard, wl_output::WlOutput, wl_pointer::WlPointer, wl_seat::WlSeat,
  },
};

use crate::Corona;

pub struct WaylandAdapter {
  conn: Connection,
  event_queue: Option<EventQueue<Corona>>,
  queue_handle: QueueHandle<Corona>,
  compositor: CompositorState,
  layer_shell: LayerShell,
  output_state: OutputState,
  seat_state: SeatState,
  registry_state: RegistryState,
  keyboard: Option<WlKeyboard>,
  pointer: Option<WlPointer>,
}

#[derive(Debug, thiserror::Error)]
pub enum WaylandAdapterError {
  #[error("failed to connect to Wayland compositor: {0}")]
  CompositorConnection(#[source] ConnectError),
  #[error("failed to initialize Wayland globals: {0}")]
  RegistryQueueInit(#[source] GlobalError),
  #[error("wl_compositor not available: {0}")]
  CompositorBindError(#[source] BindError),
  #[error("zwlr_layer_shell_v1 not available: {0}")]
  LayerShellBindError(#[source] BindError),
  #[error("failed to flush Wayland connection: {0}")]
  FlushError(#[source] WaylandError),
}

pub struct LayerSurfaceSpec<'a> {
  pub namespace: String,
  pub layer: Layer,
  pub output: Option<&'a WlOutput>,
  pub anchor: Anchor,
  pub width: u32,
  pub height: u32,
  pub exclusive_zone: i32,
  pub keyboard_interactivity: KeyboardInteractivity,
}

impl WaylandAdapter {
  pub fn init() -> Result<Self, WaylandAdapterError> {
    let conn = Connection::connect_to_env().map_err(WaylandAdapterError::CompositorConnection)?;
    let (globals, event_queue) =
      registry_queue_init::<Corona>(&conn).map_err(WaylandAdapterError::RegistryQueueInit)?;
    let qh = event_queue.handle();

    let compositor =
      CompositorState::bind(&globals, &qh).map_err(WaylandAdapterError::CompositorBindError)?;
    let layer_shell =
      LayerShell::bind(&globals, &qh).map_err(WaylandAdapterError::LayerShellBindError)?;

    let output_state = OutputState::new(&globals, &qh);
    let seat_state = SeatState::new(&globals, &qh);
    let registry_state = RegistryState::new(&globals);

    Ok(Self {
      conn,
      event_queue: Some(event_queue),
      queue_handle: qh,
      compositor,
      layer_shell,
      output_state,
      seat_state,
      registry_state,
      keyboard: None,
      pointer: None,
    })
  }

  pub fn set_capability(
    &mut self,
    seat: &WlSeat,
    capability: Capability,
    available: bool,
    loop_handle: &calloop::LoopHandle<'static, Corona>,
  ) {
    match (capability, available) {
      (Capability::Keyboard, true) if self.keyboard.is_none() => {
        let keyboard = self.seat_state.get_keyboard_with_repeat(
          &self.queue_handle,
          seat,
          None,
          loop_handle.clone(),
          Corona::repeat_callback(),
        );

        match keyboard {
          Ok(keyboard) => self.keyboard = Some(keyboard),
          Err(e) => tracing::warn!("failed to bind keyboard: {e}"),
        }
      }
      (Capability::Pointer, true) if self.pointer.is_none() => {
        match self.seat_state.get_pointer(&self.queue_handle, seat) {
          Ok(pointer) => self.pointer = Some(pointer),
          Err(e) => tracing::warn!("failed to bind pointer: {e}"),
        }
      }
      (Capability::Keyboard, false) => {
        if let Some(keyboard) = self.keyboard.take() {
          keyboard.release();
        }
      }
      (Capability::Pointer, false) => {
        if let Some(pointer) = self.pointer.take() {
          pointer.release();
        }
      }
      _ => {}
    }
  }

  pub fn create_layer_surface(&self, spec: LayerSurfaceSpec) -> LayerSurface {
    let wl_surface = self.compositor.create_surface(&self.queue_handle);
    let layer_surface = self.layer_shell.create_layer_surface(
      &self.queue_handle,
      wl_surface,
      spec.layer,
      Some(spec.namespace),
      spec.output,
    );

    layer_surface.set_anchor(spec.anchor);
    layer_surface.set_size(spec.width, spec.height);
    layer_surface.set_exclusive_zone(spec.exclusive_zone);
    layer_surface.set_keyboard_interactivity(spec.keyboard_interactivity);
    layer_surface.wl_surface().commit();
    layer_surface
  }

  pub fn flush(&self) -> Result<(), WaylandAdapterError> {
    self.conn.flush().map_err(WaylandAdapterError::FlushError)
  }

  pub fn event_source(&mut self) -> Option<WaylandSource<Corona>> {
    Some(WaylandSource::new(
      self.conn.clone(),
      self.event_queue.take()?,
    ))
  }

  pub fn display_id(&self) -> ObjectId {
    self.conn.display().id()
  }

  pub fn output_state(&self) -> &OutputState {
    &self.output_state
  }

  pub fn output_state_mut(&mut self) -> &mut OutputState {
    &mut self.output_state
  }

  pub fn seat_state_mut(&mut self) -> &mut SeatState {
    &mut self.seat_state
  }

  pub fn registry_state_mut(&mut self) -> &mut RegistryState {
    &mut self.registry_state
  }
}

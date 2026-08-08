use smithay_client_toolkit::{
  compositor::CompositorState,
  globals::GlobalData,
  output::{OutputData, OutputState},
  reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::ZwlrLayerShellV1,
  registry::RegistryState,
  seat::{SeatData, SeatState},
  shell::wlr_layer::{LayerShell, LayerShellHandler},
};
use wayland_client::{
  ConnectError, Connection, Dispatch, EventQueue,
  globals::{BindError, GlobalError, GlobalListContents, registry_queue_init},
  protocol::{
    wl_compositor::WlCompositor, wl_output::WlOutput, wl_registry::WlRegistry, wl_seat::WlSeat,
  },
};
use wayland_protocols::xdg::xdg_output::zv1::client::{
  zxdg_output_manager_v1::ZxdgOutputManagerV1, zxdg_output_v1::ZxdgOutputV1,
};

pub struct WaylandAdapter<S>
where
  S: Dispatch<WlRegistry, GlobalListContents>,
  S: Dispatch<WlCompositor, GlobalData>,
  S: Dispatch<ZwlrLayerShellV1, GlobalData> + LayerShellHandler,
  S: Dispatch<WlOutput, OutputData>
    + Dispatch<ZxdgOutputManagerV1, GlobalData>
    + Dispatch<ZxdgOutputV1, OutputData>,
  S: Dispatch<WlSeat, SeatData>,
  S: 'static,
{
  conn: Connection,
  event_queue: EventQueue<S>,
  compositor: CompositorState,
  layer_shell: LayerShell,
  output_state: OutputState,
  seat_state: SeatState,
  registry_state: RegistryState,
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
}

impl<S> WaylandAdapter<S>
where
  S: Dispatch<WlRegistry, GlobalListContents>,
  S: Dispatch<WlCompositor, GlobalData>,
  S: Dispatch<ZwlrLayerShellV1, GlobalData> + LayerShellHandler,
  S: Dispatch<WlOutput, OutputData>
    + Dispatch<ZxdgOutputManagerV1, GlobalData>
    + Dispatch<ZxdgOutputV1, OutputData>,
  S: Dispatch<WlSeat, SeatData>,
  S: 'static,
{
  pub fn init() -> Result<Self, WaylandAdapterError> {
    let conn = Connection::connect_to_env().map_err(WaylandAdapterError::CompositorConnection)?;
    let (globals, event_queue) =
      registry_queue_init::<S>(&conn).map_err(WaylandAdapterError::RegistryQueueInit)?;
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
      event_queue,
      compositor,
      layer_shell,
      output_state,
      seat_state,
      registry_state,
    })
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

use smithay_client_toolkit::{
  compositor::CompositorState,
  globals::GlobalData,
  output::{OutputData, OutputState},
  reexports::protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::ZwlrLayerShellV1,
  seat::{SeatData, SeatState},
  shell::wlr_layer::{LayerShell, LayerShellHandler},
};
use wayland_client::{
  Dispatch, QueueHandle,
  globals::{BindError, GlobalList},
  protocol::{wl_compositor::WlCompositor, wl_output::WlOutput, wl_seat::WlSeat},
};
use wayland_protocols::xdg::xdg_output::zv1::client::{
  zxdg_output_manager_v1::ZxdgOutputManagerV1, zxdg_output_v1::ZxdgOutputV1,
};

pub struct WaylandAdapter {
  compositor: CompositorState,
  layer_shell: LayerShell,
  output_state: OutputState,
  seat_state: SeatState,
}

#[derive(Debug, thiserror::Error)]
pub enum WaylandAdapterError {
  #[error("wl_compositor not available: {0}")]
  CompositorBindError(BindError),
  #[error("zwlr_layer_shell_v1 not available: {0}")]
  LayerShellBindError(BindError),
}

impl WaylandAdapter {
  pub fn init<S>(qh: &QueueHandle<S>, globals: &GlobalList) -> Result<Self, WaylandAdapterError>
  where
    S: Dispatch<WlCompositor, GlobalData>,
    S: Dispatch<ZwlrLayerShellV1, GlobalData> + LayerShellHandler,
    S: Dispatch<WlOutput, OutputData>
      + Dispatch<ZxdgOutputManagerV1, GlobalData>
      + Dispatch<ZxdgOutputV1, OutputData>,
    S: Dispatch<WlSeat, SeatData>,
    S: 'static,
  {
    let compositor =
      CompositorState::bind(globals, qh).map_err(WaylandAdapterError::CompositorBindError)?;
    let layer_shell =
      LayerShell::bind(globals, qh).map_err(WaylandAdapterError::LayerShellBindError)?;
    let output_state = OutputState::new(globals, qh);
    let seat_state = SeatState::new(globals, qh);

    Ok(Self {
      compositor,
      layer_shell,
      output_state,
      seat_state,
    })
  }
}

use anyhow::{Context, Result};
use wayland_client::{
  Connection, Dispatch, EventQueue, QueueHandle, delegate_noop,
  globals::{GlobalListContents, registry_queue_init},
  protocol::wl_registry,
};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1;

use crate::toplevel::ToplevelMapper;
use protocol::{
  hyprland_toplevel_mapping_manager_v1::HyprlandToplevelMappingManagerV1,
  hyprland_toplevel_window_mapping_handle_v1::{self, HyprlandToplevelWindowMappingHandleV1},
};

mod protocol;

/// Toplevel index and window address, from the last mapping requests.
#[derive(Default)]
struct Mapped(Vec<(usize, u64)>);

/// Maps windows via hyprland-toplevel-mapping-v1, on its own queue of the
/// capturer's connection.
pub(crate) struct Hyprland {
  queue: EventQueue<Mapped>,
  manager: HyprlandToplevelMappingManagerV1,
}

impl Hyprland {
  pub(crate) fn new(conn: &Connection) -> Result<Self> {
    let (globals, queue) = registry_queue_init::<Mapped>(conn)?;
    let manager = globals
      .bind(&queue.handle(), 1..=1, ())
      .context("compositor lacks hyprland-toplevel-mapping-v1")?;
    Ok(Self { queue, manager })
  }
}

impl ToplevelMapper for Hyprland {
  fn find(&mut self, toplevels: &[ExtForeignToplevelHandleV1], id: &str) -> Result<usize> {
    let address = parse_address(id)?;
    let qh = self.queue.handle();
    let mut mapped = Mapped::default();
    let handles = toplevels
      .iter()
      .enumerate()
      .map(|(i, toplevel)| self.manager.get_window_for_toplevel(toplevel, &qh, i))
      .collect::<Vec<_>>();
    self.queue.roundtrip(&mut mapped)?;
    for handle in handles {
      handle.destroy();
    }

    mapped
      .0
      .iter()
      .find(|(_, a)| *a == address)
      .map(|(index, _)| *index)
      .with_context(|| format!("no toplevel for window {address:#x}"))
  }
}

/// A Hyprland window address, hex with or without `0x`.
fn parse_address(address: &str) -> Result<u64> {
  Ok(u64::from_str_radix(address.trim_start_matches("0x"), 16)?)
}

fn window_address(hi: u32, lo: u32) -> u64 {
  (u64::from(hi) << 32) | u64::from(lo)
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Mapped {
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

delegate_noop!(Mapped: HyprlandToplevelMappingManagerV1);

impl Dispatch<HyprlandToplevelWindowMappingHandleV1, usize> for Mapped {
  fn event(
    state: &mut Self,
    _: &HyprlandToplevelWindowMappingHandleV1,
    event: hyprland_toplevel_window_mapping_handle_v1::Event,
    index: &usize,
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
    if let hyprland_toplevel_window_mapping_handle_v1::Event::WindowAddress {
      address_hi,
      address,
    } = event
    {
      state.0.push((*index, window_address(address_hi, address)));
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn addresses() {
    assert_eq!(parse_address("0x55d2a4e1b2c0").unwrap(), 0x55d2_a4e1_b2c0);
    assert_eq!(parse_address("55D2a4e1b2c0").unwrap(), 0x55d2_a4e1_b2c0);
    assert_eq!(parse_address("0x0").unwrap(), 0);
    assert_eq!(parse_address("ffffffffffffffff").unwrap(), u64::MAX);
    for bad in ["", "0x", "0xg", "1ffffffffffffffff", " 0x1", "-1", "0X1f"] {
      assert!(parse_address(bad).is_err(), "{bad}");
    }
  }

  #[test]
  fn window_addresses() {
    assert_eq!(window_address(0, 0), 0);
    assert_eq!(window_address(0x55d2, 0xa4e1_b2c0), 0x55d2_a4e1_b2c0);
    assert_eq!(window_address(u32::MAX, u32::MAX), u64::MAX);
  }
}

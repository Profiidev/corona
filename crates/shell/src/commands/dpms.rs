use anyhow::Result;
use clap::ValueEnum;
use corona_compositor::CompositorExt;
use corona_ipc::{IpcCommand, IpcServer};
use gpui_kit::App;
use serde::{Deserialize, Serialize};

pub fn register_commands(server: &mut IpcServer) {
  server.register::<Dpms>();
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ValueEnum)]
pub enum Power {
  /// Turn every monitor on
  On,
  /// Turn every monitor off; input does not turn them on again
  Off,
}

pub struct Dpms;

impl IpcCommand for Dpms {
  const COMMAND: &'static str = "dpms:set";

  type Payload = Power;
  type Response = ();

  fn handle(power: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    cx.compositor().set_dpms(matches!(power, Power::On))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::test_support::{FakeCompositor, setup};
  use gpui_kit::{self as gpui, TestAppContext};

  #[gpui::test]
  fn sets_power(cx: &mut TestAppContext) {
    let fake = setup(FakeCompositor::default(), cx);
    cx.update(|cx| {
      Dpms::handle(Power::Off, cx).unwrap();
      Dpms::handle(Power::On, cx).unwrap();
    });
    assert_eq!(*fake.calls.borrow(), ["dpms false", "dpms true"]);
  }
}

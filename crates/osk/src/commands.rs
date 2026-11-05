//! The on-screen keyboard's IPC commands

use crate::{Osk, OskExt};
use anyhow::Result;
use corona_ipc::{IpcCommand, IpcExt};
use gpui_kit::{App, BorrowAppContext};

use corona_ipc::Switch;

pub(crate) fn register(cx: &mut App) {
  cx.ipc_mut()
    .register::<Show>()
    .register::<Hide>()
    .register::<Toggle>()
    .register::<Tablet>();
}

pub struct Show;

impl IpcCommand for Show {
  const COMMAND: &'static str = "osk:show";

  type Payload = ();
  type Response = ();

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    cx.update_global::<Osk, _>(|osk, cx| osk.show(cx));
    Ok(())
  }
}

pub struct Hide;

impl IpcCommand for Hide {
  const COMMAND: &'static str = "osk:hide";

  type Payload = ();
  type Response = ();

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    cx.update_global::<Osk, _>(|osk, cx| osk.hide(cx));
    Ok(())
  }
}

pub struct Toggle;

impl IpcCommand for Toggle {
  const COMMAND: &'static str = "osk:toggle";

  type Payload = ();
  type Response = ();

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    cx.update_global::<Osk, _>(|osk, cx| osk.toggle(cx));
    Ok(())
  }
}

/// Tablet mode, for a Hyprland `switch:` bind: on shows the keyboard when a
/// text field has focus, off hides it
pub struct Tablet;

impl IpcCommand for Tablet {
  const COMMAND: &'static str = "osk:tablet";

  type Payload = Switch;
  /// on or off afterwards
  type Response = bool;

  fn handle(switch: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let current = *cx.osk().tablet.read(cx);
    if let Some(on) = switch.target(current) {
      cx.update_global::<Osk, _>(|osk, cx| osk.set_tablet(on, cx));
    }
    Ok(*cx.osk().tablet.read(cx))
  }
}

use anyhow::{Context, Result, bail};
use corona_ipc::{IpcCommand, IpcServer};
use corona_power::PowerExt;
use corona_utils::error::ErrorLogExt;
use gpui_kit::App;

pub fn register_commands(server: &mut IpcServer) {
  server
    .register::<SetProfile>()
    .register::<CycleProfile>()
    .register::<ListProfiles>();
}

fn set(name: String, cx: &mut App) {
  let task = cx.power().set_profile(name);
  cx.spawn(async move |_| {
    let _ = task.await.log_err();
  })
  .detach();
}

pub struct SetProfile;

impl IpcCommand for SetProfile {
  const COMMAND: &'static str = "power_profile:set";

  type Payload = String;
  type Response = ();

  fn handle(name: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let profiles = cx.power().profiles(cx).context("no power profiles")?;
    if !profiles.available.contains(&name) {
      bail!(
        "unknown profile {name}, expected one of {}",
        profiles.available.join(", ")
      );
    }
    set(name, cx);
    Ok(())
  }
}

pub struct CycleProfile;

impl IpcCommand for CycleProfile {
  const COMMAND: &'static str = "power_profile:cycle";

  type Payload = ();
  type Response = ();

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    let next = cx
      .power()
      .profiles(cx)
      .and_then(|p| p.next().cloned())
      .context("no power profiles")?;
    set(next, cx);
    Ok(())
  }
}

pub struct ListProfiles;

impl IpcCommand for ListProfiles {
  const COMMAND: &'static str = "power_profile:list";

  type Payload = ();
  type Response = Vec<String>;

  fn handle(_: Self::Payload, cx: &mut App) -> Result<Self::Response> {
    Ok(
      cx.power()
        .profiles(cx)
        .map(|p| p.available.clone())
        .unwrap_or_default(),
    )
  }
}

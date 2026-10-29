//! The launcher's commands that need the shell, registered into `corona_launcher`

mod audio;
mod devices;
mod display;
mod drives;
mod info;
mod ip;
mod monitors;
mod network;
mod notifications;
mod session;
mod system;
mod tools;
mod windows;

use corona_compositor::CompositorExt;
use corona_utils::error::ErrorLogExt;
use gpui_kit::{App, AppContext};

pub fn init(cx: &mut App) {
  if let Err(e) = corona_clipboard::init(cx) {
    tracing::warn!("clipboard history is off: {e:#}");
  }
  corona_launcher::paste::set_paster(
    |_, cx| {
      // the window the launcher hands the keyboard back to
      let terminal = cx
        .compositor()
        .active_window(cx)
        .is_some_and(|w| corona_clipboard::paste::is_terminal(&w.class));
      cx.background_spawn(async move {
        std::thread::sleep(std::time::Duration::from_millis(150));
        corona_clipboard::paste::paste(terminal)
      })
    },
    cx,
  );
  corona_launcher::hooks::set_notify(
    |summary, body, cx| {
      let send = corona_notifications::NotificationsExt::notifications(cx).send(summary, body);
      cx.spawn(async move |_| {
        let _ = send.await.log_err();
      })
      .detach();
    },
    cx,
  );
  corona_launcher::commands::init(cx);
  corona_clipboard::launcher::init(cx);
  session::init(cx);
  system::init(cx);
  audio::init(cx);
  display::init(cx);
  tools::init(cx);
  windows::init(cx);
  windows::init_actions(cx);
  info::init(cx);
  devices::init(cx);
  network::init(cx);
  drives::init(cx);
  notifications::init(cx);
  monitors::init(cx);
  ip::init(cx);
}

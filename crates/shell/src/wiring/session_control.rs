//! Lets the lock screen read and run the session actions through logind

use std::rc::Rc;

use corona_components::auth_screen::SessionControl;
use corona_power::PowerExt;
use gpui_kit::App;

pub(super) fn init(cx: &mut App) {
  cx.set_global(SessionControl {
    capabilities: Rc::new(|cx| {
      let capabilities = cx.power().session_capabilities();
      cx.background_executor().spawn(capabilities)
    }),
    run: Rc::new(|action, cx| {
      let run = cx.power().session_action(action);
      cx.background_executor().spawn(run)
    }),
  });
}

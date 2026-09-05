use anyhow::Result;
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, FocusHandle, Global, Size, Window,
  WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, component::*, div, point,
  prelude::*, px,
};

/// A session lock screen, on `ext-session-lock-v1`.
///
/// **This does not authenticate anything** — Escape unlocks it. It exists to exercise the
/// protocol end to end. Wire PAM into [`Lock::unlock`] (on a background thread, it blocks
/// for seconds on a wrong password) before binding this to anything.
pub struct Lock {
  focus: FocusHandle,
}

#[derive(Default)]
struct LockState {
  windows: Vec<AnyWindowHandle>,
}

impl Global for LockState {}

impl Lock {
  /// Locks the session, then covers every display. Both steps are the compositor's to
  /// grant, so nothing is drawn until it confirms the lock.
  pub fn lock(cx: &mut App) {
    if cx
      .try_global::<LockState>()
      .is_some_and(|s| !s.windows.is_empty())
    {
      return;
    }

    let locked = cx.lock_session();

    // The windows go up now, not after `locked` resolves: the compositor withholds that
    // confirmation until a lock frame has been presented on every output, so waiting for
    // it first just stalls until the compositor's timeout fires.
    let windows = cx
      .displays()
      .into_iter()
      .map(|display| Self::open(display.id(), cx))
      .collect::<Result<Vec<_>>>();

    match windows {
      Ok(windows) => cx.set_global(LockState { windows }),
      Err(e) => {
        eprintln!("failed to cover the displays: {e:#}");
        cx.unlock_session();
        return;
      }
    }

    cx.spawn(async move |cx| {
      // Refused — another client holds the session, or the compositor said no. The
      // surfaces are already up, so take them down again.
      if let Err(e) = locked.await.map_err(anyhow::Error::from).and_then(|r| r) {
        eprintln!("session lock refused: {e:#}");
        let _ = cx.update(Self::unlock);
      }
    })
    .detach();
  }

  /// Releases the lock. The compositor keeps the session locked until this runs — a
  /// crash here leaves the machine locked with no way back in but another TTY.
  pub fn unlock(cx: &mut App) {
    cx.unlock_session();

    // Only after unlocking: the surfaces stay on screen until the compositor lets go.
    for window in std::mem::take(&mut cx.global_mut::<LockState>().windows) {
      let _ = window.update(cx, |_, window, _| window.remove_window());
    }
  }

  fn open(display: gpui_kit::DisplayId, cx: &mut App) -> Result<AnyWindowHandle> {
    let window = cx.open_window(
      WindowOptions {
        kind: WindowKind::SessionLock,
        // The compositor sizes a lock surface to its output and sends that in the first
        // configure; this is only a placeholder until then.
        display_id: Some(display),
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          origin: point(px(0.), px(0.)),
          size: Size::new(px(640.), px(480.)),
        })),
        window_background: WindowBackgroundAppearance::Opaque,
        app_id: Some("corona_lock".to_string()),
        titlebar: None,
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|cx| Lock {
          focus: cx.focus_handle(),
        });
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
        cx.new(|cx| Root::new(view, window, cx).bordered(false))
      },
    )?;

    Ok(window.into())
  }
}

impl Render for Lock {
  fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    div()
      .track_focus(&self.focus)
      .on_key_down(cx.listener(|_, event: &gpui_kit::KeyDownEvent, _, cx| {
        if event.keystroke.key == "escape" {
          Lock::unlock(cx);
        }
      }))
      .size_full()
      .flex()
      .items_center()
      .justify_center()
      .bg(cx.theme().background)
      .text_color(cx.theme().foreground)
      .child("Locked — press Escape")
  }
}

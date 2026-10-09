use std::{
  cell::Cell,
  rc::Rc,
  time::{Duration, Instant},
};

use anyhow::Result;
use corona_auth::AuthExt;
use corona_capture::{
  Frame, RgbaImageExt, capture_all,
  image::imageops::{self, FilterType},
};
use corona_compositor::CompositorExt;
use corona_config::{APP_NAME, ConfigProvider};
use corona_utils::display::{display_id_for, display_uuid};
use futures::{
  channel::oneshot,
  future::{Either, FutureExt, Shared, select},
};
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, DisplayId, Global, Size, Styled, Task,
  WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind, WindowOptions,
  base::Root,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point, px, transparent_black,
};

use corona_components::animation::animation_duration;

use crate::lock::{
  user,
  view::{Background, Lock, Unlock, ZOOM_SPEED},
};

const UNLOCK_NAMESPACE: &str = "corona_unlock";
const BLUR_SCALE: u32 = 4;
const CAPTURE_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Default)]
pub struct LockState {
  windows: Vec<AnyWindowHandle>,
  /// The lock in progress, kept so a second caller can wait for it too.
  locking: Option<Shared<Task<()>>>,
  /// The display each lock window covers, and what it showed
  screens: Vec<(DisplayId, Option<Background>)>,
  /// Click-through overlays under the lock, waiting to play the unlock animation.
  /// Opened up front so they already cover the screens when the lock goes
  overlays: Vec<AnyWindowHandle>,
  /// When the unlock animation started, shared by the overlays
  unlock_start: Rc<Cell<Option<Instant>>>,
  /// When the current animation started, shared so every display plays it in sync
  animation_start: Option<Instant>,
  /// One scan for all displays, the reader takes one claim at a time.
  /// Dropping it on unlock lets go of the reader
  fingerprint: Option<Task<()>>,
}

impl Global for LockState {}

impl LockState {
  /// Resolves once the compositor confirms the lock, or gave up on it.
  pub fn lock(cx: &mut App) -> Task<()> {
    if let Some(state) = cx.try_global::<Self>() {
      let locking = state.locking.clone();
      return cx.spawn(async move |_| {
        if let Some(locking) = locking {
          locking.await;
        }
      });
    }
    cx.set_global(LockState::default());

    let names: Vec<String> = cx
      .compositor()
      .list_monitors(cx)
      .iter()
      .filter(|m| !m.disabled)
      .map(|m| m.name.clone())
      .collect();

    let sigma = cx.config().lockscreen.blur;
    let locking = cx
      .spawn(async move |cx| {
        let capture = cx
          .background_executor()
          .spawn(async move { capture_all(names) });
        let timeout = cx.background_executor().timer(CAPTURE_TIMEOUT);
        let frames = match select(capture, timeout).await {
          Either::Left((frames, _)) => cx
            .background_executor()
            .spawn(async move {
              let frames = frames?;
              std::thread::scope(|s| {
                frames
                  .iter()
                  .map(|(name, frame)| {
                    s.spawn(move || Ok((name.clone(), background(frame, sigma)?)))
                  })
                  .collect::<Vec<_>>()
                  .into_iter()
                  .map(|h| h.join().expect("lock background panicked"))
                  .collect::<Result<Vec<_>>>()
              })
            })
            .await
            .inspect_err(|e| tracing::warn!("lock: no screen capture for the background: {e:#}"))
            .unwrap_or_default(),
          Either::Right(_) => {
            tracing::warn!("lock: screen capture timed out, locking without it");
            Vec::new()
          }
        };
        let Some(locked) = cx.update(|cx| Self::engage(frames, cx)) else {
          return;
        };
        match locked.await.map_err(anyhow::Error::from).and_then(|r| r) {
          Ok(()) => cx.update(|cx| {
            Self::open_overlays(cx);
            Self::scan_fingerprints(cx);
          }),
          Err(e) => {
            tracing::error!("session lock refused: {e:#}");
            cx.update(Self::unlock);
          }
        }
      })
      .shared();
    cx.global_mut::<Self>().locking = Some(locking.clone());
    cx.spawn(async move |_| locking.await)
  }

  fn engage(
    frames: Vec<(String, Background)>,
    cx: &mut App,
  ) -> Option<oneshot::Receiver<Result<()>>> {
    if !cx.has_global::<Self>() {
      return None;
    }

    let screens: Vec<_> = cx
      .displays()
      .into_iter()
      .map(|display| {
        let uuid = display.uuid().ok();
        let background = frames
          .iter()
          .find(|(name, _)| Some(display_uuid(name)) == uuid)
          .map(|(_, image)| image.clone());
        (display.id(), background)
      })
      .collect();

    cx.global_mut::<Self>().screens = screens.clone();
    let main = (cx.config().lockscreen.monitor.as_deref()).and_then(|m| display_id_for(m, cx));
    let locked = cx.lock_session();

    let windows = open_screens(screens, main, |display, background, login| {
      Self::open(display, background, login, cx)
    });

    match windows {
      Ok(windows) => cx.global_mut::<Self>().windows = windows,
      Err(e) => {
        tracing::error!("failed to cover the displays: {e:#}");
        Self::unlock(cx);
        return None;
      }
    }

    Some(locked)
  }

  /// Starts the clock on the first display to draw
  pub fn animation_start(cx: &mut App) -> Instant {
    *cx
      .global_mut::<Self>()
      .animation_start
      .get_or_insert_with(Instant::now)
  }

  /// Opened once the lock covers the screens, so they first draw hidden, already
  /// showing the lock. The compositor sends no frame callbacks under the lock, so
  /// they could not redraw for it later
  fn open_overlays(cx: &mut App) {
    let Some(state) = cx.try_global::<Self>() else {
      return;
    };
    let screens = state.screens.clone();
    let start = state.unlock_start.clone();
    for (display, background) in screens {
      match Self::open_unlock(display, background, start.clone(), cx) {
        Ok(overlay) => cx.global_mut::<Self>().overlays.push(overlay),
        Err(e) => tracing::warn!("unlock: no animation overlay: {e:#}"),
      }
    }
  }

  /// Unlocks on a matching finger, until the reader or fprintd gives up
  fn scan_fingerprints(cx: &mut App) {
    if !cx.has_global::<Self>() {
      return;
    }
    let scan = cx.auth().clone();
    let user = user(cx).name.to_string();
    let task = cx.spawn(async move |cx| {
      loop {
        match scan.fingerprint(user.clone()).await {
          Ok(true) => return cx.update(Self::unlock_animated),
          // the next finger
          Ok(false) => {}
          // no reader, no prints enrolled or fprintd missing; the password still works
          Err(e) => return tracing::info!("lock: no fingerprint unlock: {e:#}"),
        }
      }
    });
    cx.global_mut::<Self>().fingerprint = Some(task);
  }

  /// Unlocks right away, the overlays then play the lock animation backwards
  pub fn unlock_animated(cx: &mut App) {
    let Some(state) = cx.try_global::<Self>() else {
      return;
    };
    let duration = animation_duration(ZOOM_SPEED, cx);
    let overlays = state.overlays.clone();
    if !overlays.is_empty() && !duration.is_zero() {
      state.unlock_start.set(Some(Instant::now()));
      cx.refresh_windows();
      cx.spawn(async move |cx| {
        cx.background_executor().timer(duration).await;
        cx.update(|cx| {
          for overlay in overlays {
            let _ = overlay.update(cx, |_, window, _| window.remove_window());
          }
        });
      })
      .detach();
    }
    // deferred, so it also closes the window whose key press got us here
    cx.defer(Self::unlock);
  }

  pub fn unlock(cx: &mut App) {
    if !cx.has_global::<Self>() {
      return;
    }
    cx.unlock_session();

    let state = cx.remove_global::<LockState>();
    let mut windows = state.windows;
    // a playing animation closes its overlays itself
    if state.unlock_start.get().is_none() {
      windows.extend(state.overlays);
    }
    for window in windows {
      let _ = window.update(cx, |_, window, _| window.remove_window());
    }
  }

  fn open(
    display: gpui_kit::DisplayId,
    background: Option<Background>,
    login: Option<AnyWindowHandle>,
    cx: &mut App,
  ) -> Result<AnyWindowHandle> {
    let window = cx.open_window(
      WindowOptions {
        kind: WindowKind::SessionLock,
        display_id: Some(display),
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          origin: point(px(0.), px(0.)),
          size: Size::new(px(640.), px(480.)),
        })),
        window_background: WindowBackgroundAppearance::Opaque,
        window_decorations: Some(WindowDecorations::Client),
        inactive_frame_interval: None,
        app_id: Some(APP_NAME.to_string()),
        titlebar: None,
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|cx| Lock::new(background, login, window, cx));
        let focus = view.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        cx.new(|cx| Root::new(view, window, cx))
      },
    )?;

    Ok(window.into())
  }

  fn open_unlock(
    display: DisplayId,
    background: Option<Background>,
    start: Rc<Cell<Option<Instant>>>,
    cx: &mut App,
  ) -> Result<AnyWindowHandle> {
    let window = cx.open_window(
      WindowOptions {
        kind: WindowKind::LayerShell(LayerShellOptions {
          anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
          exclusive_zone: Some(px(-1.)),
          exclusive_edge: None,
          margin: None,
          layer: Layer::Overlay,
          namespace: UNLOCK_NAMESPACE.to_string(),
          keyboard_interactivity: KeyboardInteractivity::None,
        }),
        display_id: Some(display),
        window_bounds: Some(WindowBounds::Windowed(Bounds {
          origin: point(px(0.), px(0.)),
          size: Size::new(px(0.), px(0.)),
        })),
        window_background: WindowBackgroundAppearance::Transparent,
        window_decorations: Some(WindowDecorations::Client),
        inactive_frame_interval: None,
        app_id: Some(APP_NAME.to_string()),
        titlebar: None,
        ..Default::default()
      },
      |window, cx| {
        window.set_input_region(Some(&[]));
        let view = cx.new(|_| Unlock::new(background, start));
        cx.new(|cx| Root::new(view, window, cx).bg(transparent_black()))
      },
    )?;

    Ok(window.into())
  }
}

/// Opens a window per screen, `main`'s first so the others get its window to
/// forward their keys to. No `main`, or one not connected: each shows a login
fn open_screens<T, W: Copy>(
  mut screens: Vec<(DisplayId, T)>,
  main: Option<DisplayId>,
  mut open: impl FnMut(DisplayId, T, Option<W>) -> Result<W>,
) -> Result<Vec<W>> {
  screens.sort_by_key(|(display, _)| main.is_some_and(|m| m != *display));
  let mut login = None;
  screens
    .into_iter()
    .map(|(display, screen)| {
      let window = open(display, screen, login)?;
      if main == Some(display) {
        login = Some(window);
      }
      Ok(window)
    })
    .collect()
}

/// `frame` sharp, and blurred by `sigma` for behind the lock
fn background(frame: &Frame, sigma: f32) -> Result<Background> {
  let image = frame.read_all()?;
  let sharp = image.to_gpui();
  if sigma <= 0. {
    return Ok(Background {
      blurred: sharp.clone(),
      sharp,
    });
  }
  let small = imageops::resize(
    &image,
    (image.width() / BLUR_SCALE).max(1),
    (image.height() / BLUR_SCALE).max(1),
    FilterType::Triangle,
  );
  Ok(Background {
    sharp,
    blurred: imageops::fast_blur(&small, sigma).to_gpui(),
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::test_support::{FakeCompositor, setup};
  use gpui_kit::{self as gpui, TestAppContext};

  fn no_capture() {
    // capture fails fast without a compositor
    unsafe { std::env::set_var("WAYLAND_DISPLAY", "/nonexistent/corona-test") };
  }

  #[gpui::test]
  fn refused_lock_unlocks(cx: &mut TestAppContext) {
    no_capture();
    setup(FakeCompositor::default(), cx);
    let task = cx.update(LockState::lock);
    assert!(cx.update(|cx| cx.has_global::<LockState>()));
    // a second caller waits for the same lock
    let second = cx.update(LockState::lock);
    cx.run_until_parked();
    // the test platform refuses session locks
    assert!(!cx.update(|cx| cx.has_global::<LockState>()));
    assert!(cx.update(|cx| cx.windows().is_empty()));
    assert!(task.now_or_never().is_some());
    assert!(second.now_or_never().is_some());
  }

  /// Which display each window opened for and where it forwards keys, `0`
  /// for its own login
  fn opened(displays: &[u64], main: Option<u64>) -> Vec<(u64, u64)> {
    let screens = displays.iter().map(|&d| (DisplayId::new(d), ())).collect();
    open_screens(screens, main.map(DisplayId::new), |display, (), login| {
      Ok((u64::from(display), login.map_or(0, |(d, _)| d)))
    })
    .unwrap()
  }

  #[test]
  fn login_on_main_display() {
    // the main display opens first, the others forward to it
    assert_eq!(opened(&[1, 2, 3], Some(2)), [(2, 0), (1, 2), (3, 2)]);
    assert_eq!(opened(&[1, 2], Some(1)), [(1, 0), (2, 1)]);
    // unset or disconnected: a login everywhere
    assert_eq!(opened(&[1, 2], None), [(1, 0), (2, 0)]);
    assert_eq!(opened(&[1, 2], Some(9)), [(1, 0), (2, 0)]);
    assert_eq!(opened(&[], Some(1)), []);
  }

  #[test]
  fn open_screens_stops_on_failure() {
    let screens = vec![(DisplayId::new(1), ()), (DisplayId::new(2), ())];
    let mut calls = 0;
    let res = open_screens(screens, None, |_, (), _: Option<()>| {
      calls += 1;
      anyhow::bail!("no window")
    });
    assert!(res.is_err());
    assert_eq!(calls, 1);
  }

  #[gpui::test]
  fn unlock_without_lock_is_noop(cx: &mut TestAppContext) {
    setup(FakeCompositor::default(), cx);
    cx.update(LockState::unlock);
    cx.update(LockState::unlock_animated);
    cx.run_until_parked();
    assert!(!cx.update(|cx| cx.has_global::<LockState>()));
  }
}

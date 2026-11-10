use std::{
  cell::Cell,
  path::PathBuf,
  rc::Rc,
  time::{Duration, Instant},
};

use anyhow::{Result, anyhow};
use corona_auth::AuthExt;
use corona_capture::{
  Frame, RgbaImageExt, capture_all,
  image::imageops::{self, FilterType},
};
use corona_components::auth_screen::Fingerprint;
use corona_compositor::CompositorExt;
use corona_config::{APP_NAME, ConfigProvider};
use corona_surface::overlay::{layer_options, root};
use corona_utils::display::{display_id_for, display_uuid};
use futures::{
  channel::oneshot,
  future::{Either, FutureExt, Shared, select},
};
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, DisplayEvent, DisplayId, Entity, Global, Size,
  Subscription, Task, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowKind,
  WindowOptions,
  base::Root,
  layer_shell::{Anchor, KeyboardInteractivity, Layer, LayerShellOptions},
  point, px,
};

use corona_components::animation::animation_duration;

use crate::overlay::lock::{
  user,
  view::{Background, Lock, Unlock, ZOOM_SPEED},
};

const UNLOCK_NAMESPACE: &str = "corona_unlock";
const BLUR_SCALE: u32 = 4;
const CAPTURE_TIMEOUT: Duration = Duration::from_millis(500);
/// Suspend drops the reader mid scan and it comes back a moment after resume
const FINGERPRINT_RETRY: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct LockState {
  windows: Vec<(DisplayId, AnyWindowHandle)>,
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
  /// What the login shows of the scan
  reader: Option<Entity<Fingerprint>>,
  /// The configured main display and its window, what displays plugged in later forward to
  main: Option<(DisplayId, AnyWindowHandle)>,
  /// Covers displays plugged in while locked, gone with the state
  displays: Option<Subscription>,
}

impl Global for LockState {}

/// Exists while locked. The compositor keeps the session locked when we die, this
/// tells the next start to cover it again
fn marker() -> Option<PathBuf> {
  let dir = std::env::var_os("XDG_RUNTIME_DIR")?;
  Some(PathBuf::from(dir).join("corona-locked"))
}

impl LockState {
  /// Resolves once the compositor confirms the lock, or gave up on it.
  pub fn lock(cx: &mut App) -> Task<()> {
    Self::start(cx, true)
  }

  /// Covers a session a previous run left locked, if it did. No capture: the
  /// compositor only shows its own lock screen by now
  pub fn recover(cx: &mut App) {
    if marker().is_some_and(|m| m.exists()) {
      tracing::warn!("lock: session was left locked, locking again");
      if let Err(e) = cx.compositor().allow_session_lock_restore() {
        tracing::warn!("lock: cannot allow the lock restore: {e:#}");
      }
      Self::start(cx, false).detach();
    }
  }

  fn start(cx: &mut App, capture: bool) -> Task<()> {
    if let Some(state) = cx.try_global::<Self>() {
      let locking = state.locking.clone();
      return cx.spawn(async move |_| {
        if let Some(locking) = locking {
          locking.await;
        }
      });
    }
    cx.set_global(LockState::default());
    cx.global_mut::<Self>().reader = Some(cx.new(|_| Fingerprint::Off));

    let names: Vec<String> = match capture {
      true => cx
        .compositor()
        .list_monitors(cx)
        .iter()
        .filter(|m| !m.disabled)
        .map(|m| m.name.clone())
        .collect(),
      false => Vec::new(),
    };

    let sigma = cx.config().lockscreen.blur;
    let locking = cx
      .spawn(async move |cx| {
        // capture and blur share one deadline, the lock must not wait on either
        let work = cx.background_executor().spawn(async move {
          if names.is_empty() {
            return Ok(Vec::new());
          }
          let frames = capture_all(names)?;
          std::thread::scope(|s| {
            frames
              .iter()
              .map(|(name, frame)| s.spawn(move || Ok((name.clone(), background(frame, sigma)?))))
              .collect::<Vec<_>>()
              .into_iter()
              .map(|h| h.join().map_err(|_| anyhow!("background panicked"))?)
              .collect::<Result<Vec<_>>>()
          })
        });
        let timeout = cx.background_executor().timer(CAPTURE_TIMEOUT);
        let frames = match select(work, timeout).await {
          Either::Left((frames, _)) => frames
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
            if let Some(marker) = marker()
              && let Err(e) = std::fs::write(marker, "")
            {
              tracing::warn!("lock: cannot mark the session as locked: {e}");
            }
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

    let compositor = cx.compositor();
    let screens: Vec<_> = cx
      .displays()
      .into_iter()
      // only those known disabled: every output that shows needs a lock surface
      .filter(|d| !d.uuid().is_ok_and(|uuid| compositor.is_disabled(uuid, cx)))
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

    let mut windows = Vec::new();
    let mut main_window = None;
    let opened = open_screens(
      screens,
      main,
      &mut Vec::new(),
      |display, background, login| {
        let window = Self::open(display, background, login, cx)?;
        windows.push((display, window));
        if main == Some(display) {
          main_window = Some((display, window));
        }
        Ok(window)
      },
    );
    // kept even on failure, so the unlock closes what did open
    let state = cx.global_mut::<Self>();
    state.windows = windows;
    state.main = main_window;

    if let Err(e) = opened {
      tracing::error!("failed to cover the displays: {e:#}");
      Self::unlock(cx);
      return None;
    }

    // a failed unlock above removed the state, so only a live lock watches
    let subscription = cx.observe_displays(|event, cx| match event {
      DisplayEvent::Added(display) => Self::cover(display, cx),
      DisplayEvent::Removed(display) => Self::uncover(display, cx),
      _ => {}
    });
    cx.global_mut::<Self>().displays = Some(subscription);
    Some(locked)
  }

  /// A display plugged in while locked gets a lock window like the others. A failure
  /// leaves that display bare, the session stays locked
  fn cover(display: DisplayId, cx: &mut App) {
    let Some(state) = cx.try_global::<Self>() else {
      return;
    };
    let compositor = cx.compositor();
    let disabled = cx
      .displays()
      .iter()
      .find(|d| d.id() == display)
      .is_some_and(|d| d.uuid().is_ok_and(|uuid| compositor.is_disabled(uuid, cx)));
    if disabled || !is_uncovered(&state.screens, display) {
      return;
    }
    let login = state.main.map(|(_, window)| window);
    match Self::open(display, None, login, cx) {
      Ok(window) => {
        let state = cx.global_mut::<Self>();
        state.screens.push((display, None));
        state.windows.push((display, window));
      }
      Err(e) => tracing::error!("lock: no window on the new display: {e:#}"),
    }
  }

  /// A display unplugged while locked loses its window. The others forward their keys
  /// to the main display's window, so without it they each get their own login
  fn uncover(display: DisplayId, cx: &mut App) {
    let Some(state) = cx.try_global::<Self>() else {
      return;
    };
    let main_gone = state.main.is_some_and(|(d, _)| d == display);
    let (gone, mut windows): (Vec<_>, Vec<_>) = state
      .windows
      .iter()
      .copied()
      .partition(|(d, _)| *d == display || main_gone);
    let mut screens = state.screens.clone();
    screens.retain(|(d, _)| *d != display);
    if gone.is_empty() && screens.len() == state.screens.len() {
      return;
    }
    let mut main = state.main;
    for (_, window) in gone {
      let _ = window.update(cx, |_, window, _| window.remove_window());
    }
    if main_gone {
      main = None;
      windows.clear();
      let reopened = open_screens(
        screens.clone(),
        None,
        &mut Vec::new(),
        |display, background, login| {
          let window = Self::open(display, background, login, cx)?;
          windows.push((display, window));
          Ok(window)
        },
      );
      if let Err(e) = reopened {
        tracing::error!("lock: cannot cover the displays left: {e:#}");
      }
    }
    let state = cx.global_mut::<Self>();
    state.screens = screens;
    state.windows = windows;
    state.main = main;
  }

  /// Starts the clock on the first display to draw
  pub fn animation_start(cx: &mut App) -> Instant {
    // a window of a failed lock can still draw once the state is gone
    if !cx.has_global::<Self>() {
      return Instant::now();
    }
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

  /// Unlocks on a matching finger. Errors retry: the screen usually locks
  /// right before suspend, which disconnects the reader mid scan
  fn scan_fingerprints(cx: &mut App) {
    if !cx.has_global::<Self>() {
      return;
    }
    let scan = cx.auth().clone();
    let user = user(cx).name.to_string();
    let task = cx.spawn(async move |cx| {
      let mut logged = false;
      loop {
        let (listening, listens) = oneshot::channel();
        let shown = async {
          if listens.await.is_ok() {
            // a failed finger stays shown until typing clears it
            cx.update(|cx| {
              Self::set_reader(cx, |r| {
                if *r == Fingerprint::Off {
                  *r = Fingerprint::Ready;
                }
              })
            });
          }
        };
        let (result, ()) =
          futures::join!(scan.fingerprint_listening(user.clone(), listening), shown);
        match result {
          Ok(true) => return cx.update(Self::unlock_animated),
          // the next finger
          Ok(false) => {
            logged = false;
            cx.update(|cx| Self::set_reader(cx, |r| *r = Fingerprint::Failed));
          }
          // idle; the claim is released, claim again
          Err(e) if e.is::<corona_auth::ScanTimeout>() => {}
          // no reader, no prints enrolled or fprintd missing; the password still works
          Err(e) => {
            cx.update(|cx| Self::set_reader(cx, |r| *r = Fingerprint::Off));
            if !std::mem::replace(&mut logged, true) {
              tracing::info!("lock: no fingerprint unlock: {e:#}");
            }
            cx.background_executor().timer(FINGERPRINT_RETRY).await;
          }
        }
      }
    });
    cx.global_mut::<Self>().fingerprint = Some(task);
  }

  fn set_reader(cx: &mut App, update: impl FnOnce(&mut Fingerprint)) {
    let Some(reader) = cx.try_global::<Self>().and_then(|s| s.reader.clone()) else {
      return;
    };
    reader.update(cx, |state, cx| {
      update(state);
      cx.notify();
    });
  }

  pub fn reader(cx: &App) -> Option<Entity<Fingerprint>> {
    cx.try_global::<Self>()?.reader.clone()
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
    if let Some(marker) = marker() {
      let _ = std::fs::remove_file(marker);
    }

    let state = cx.remove_global::<LockState>();
    let mut windows: Vec<_> = state.windows.into_iter().map(|(_, w)| w).collect();
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
        inactive_frame_interval: None,
        ..layer_options(
          LayerShellOptions {
            anchor: Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            exclusive_zone: Some(px(-1.)),
            exclusive_edge: None,
            margin: None,
            layer: Layer::Overlay,
            namespace: UNLOCK_NAMESPACE.to_string(),
            keyboard_interactivity: KeyboardInteractivity::None,
          },
          Some(display),
          Size::default(),
        )
      },
      |window, cx| {
        window.set_input_region(Some(&[]));
        let view = cx.new(|_| Unlock::new(background, start));
        root(view, window, cx)
      },
    )?;

    Ok(window.into())
  }
}

fn is_uncovered<T>(screens: &[(DisplayId, T)], display: DisplayId) -> bool {
  !screens.iter().any(|(d, _)| *d == display)
}

/// Opens a window per screen, `main`'s first so the others get its window to
/// forward their keys to. No `main`, or one not connected: each shows a login.
/// Every window opened is in `windows`, also when a later one fails
fn open_screens<T, W: Copy>(
  mut screens: Vec<(DisplayId, T)>,
  main: Option<DisplayId>,
  windows: &mut Vec<W>,
  mut open: impl FnMut(DisplayId, T, Option<W>) -> Result<W>,
) -> Result<()> {
  screens.sort_by_key(|(display, _)| main.is_some_and(|m| m != *display));
  let mut login = None;
  for (display, screen) in screens {
    let window = open(display, screen, login)?;
    windows.push(window);
    if main == Some(display) {
      login = Some(window);
    }
  }
  Ok(())
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
  use corona_surface::testing::{FakeCompositor, setup};
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
    let mut windows = Vec::new();
    open_screens(
      screens,
      main.map(DisplayId::new),
      &mut windows,
      |display, (), login| Ok((u64::from(display), login.map_or(0, |(d, _)| d))),
    )
    .unwrap();
    windows
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
    let mut windows = Vec::new();
    let mut calls = 0;
    let res = open_screens(screens, None, &mut windows, |_, (), _: Option<()>| {
      calls += 1;
      match calls {
        1 => Ok(()),
        _ => anyhow::bail!("no window"),
      }
    });
    assert!(res.is_err());
    assert_eq!(calls, 2);
    // the one that opened is still there to close
    assert_eq!(windows.len(), 1);
  }

  #[test]
  fn covered_displays_are_not_covered_twice() {
    let screens = [(DisplayId::new(1), ())];
    assert!(!is_uncovered(&screens, DisplayId::new(1)));
    assert!(is_uncovered(&screens, DisplayId::new(2)));
  }

  #[gpui::test]
  fn recover_only_with_marker(cx: &mut TestAppContext) {
    no_capture();
    setup(FakeCompositor::default(), cx);
    let marker = marker().unwrap();
    let _ = std::fs::remove_file(&marker);
    cx.update(LockState::recover);
    assert!(!cx.update(|cx| cx.has_global::<LockState>()));
    std::fs::write(&marker, "").unwrap();
    cx.update(LockState::recover);
    assert!(cx.update(|cx| cx.has_global::<LockState>()));
    cx.run_until_parked();
    // the test platform refuses the lock, which clears the marker again
    assert!(!cx.update(|cx| cx.has_global::<LockState>()));
    assert!(!marker.exists());
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

use std::{sync::Arc, time::Duration};

use anyhow::Result;
use corona_capture::{
  Frame, RgbaImageExt, capture_all,
  image::imageops::{self, FilterType},
};
use corona_compositor::CompositorExt;
use corona_config::{APP_NAME, ConfigProvider};
use corona_utils::display::display_uuid;
use futures::{
  channel::oneshot,
  future::{Either, FutureExt, Shared, select},
};
use gpui_kit::{
  AnyWindowHandle, App, AppContext, Bounds, Global, RenderImage, Size, Task,
  WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, base::Root, point, px,
};

use crate::lock::view::Lock;

const BLUR_SCALE: u32 = 4;
const CAPTURE_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Default)]
pub struct LockState {
  windows: Vec<AnyWindowHandle>,
  /// The lock in progress, kept so a second caller can wait for it too.
  locking: Option<Shared<Task<()>>>,
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
              frames?
                .into_iter()
                .map(|(name, frame)| Ok((name, blur(&frame, sigma)?)))
                .collect::<Result<Vec<_>>>()
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
        if let Err(e) = locked.await.map_err(anyhow::Error::from).and_then(|r| r) {
          tracing::error!("session lock refused: {e:#}");
          cx.update(Self::unlock);
        }
      })
      .shared();
    cx.global_mut::<Self>().locking = Some(locking.clone());
    cx.spawn(async move |_| locking.await)
  }

  fn engage(
    frames: Vec<(String, Arc<RenderImage>)>,
    cx: &mut App,
  ) -> Option<oneshot::Receiver<Result<()>>> {
    if !cx.has_global::<Self>() {
      return None;
    }

    let locked = cx.lock_session();

    let windows = cx
      .displays()
      .into_iter()
      .map(|display| {
        let uuid = display.uuid().ok();
        let background = frames
          .iter()
          .find(|(name, _)| Some(display_uuid(name)) == uuid)
          .map(|(_, image)| image.clone());
        Self::open(display.id(), background, cx)
      })
      .collect::<Result<Vec<_>>>();

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

  pub fn unlock(cx: &mut App) {
    if !cx.has_global::<Self>() {
      return;
    }
    cx.unlock_session();

    for window in cx.remove_global::<LockState>().windows {
      let _ = window.update(cx, |_, window, _| window.remove_window());
    }
  }

  fn open(
    display: gpui_kit::DisplayId,
    background: Option<Arc<RenderImage>>,
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
        app_id: Some(APP_NAME.to_string()),
        titlebar: None,
        ..Default::default()
      },
      |window, cx| {
        let view = cx.new(|cx| Lock::new(background, cx));
        let focus = view.read(cx).focus.clone();
        window.focus(&focus, cx);
        cx.new(|cx| Root::new(view, window, cx))
      },
    )?;

    Ok(window.into())
  }
}

fn blur(frame: &Frame, sigma: f32) -> Result<Arc<RenderImage>> {
  let image = frame.read_all()?;
  if sigma <= 0. {
    return Ok(image.to_gpui());
  }
  let small = imageops::resize(
    &image,
    (image.width() / BLUR_SCALE).max(1),
    (image.height() / BLUR_SCALE).max(1),
    FilterType::Triangle,
  );
  Ok(imageops::fast_blur(&small, sigma).to_gpui())
}

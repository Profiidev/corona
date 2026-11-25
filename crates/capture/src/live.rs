use std::{
  os::fd::{AsFd, OwnedFd},
  pin::Pin,
  sync::Arc,
  task::{Context, Poll},
  thread,
  time::{Duration, Instant},
};

use anyhow::Result;
use futures::{
  SinkExt, Stream,
  channel::mpsc::{Receiver, Sender, channel},
};
use gpui_kit::{Dmabuf, DmabufPlane};
use rustix::event::{EventfdFlags, eventfd};

use wayland_client::protocol::wl_buffer::WlBuffer;
use wayland_protocols::ext::image_capture_source::v1::client::ext_image_capture_source_v1::ExtImageCaptureSourceV1;

use crate::{Capturer, Frame};

/// Captures the compositor's window `id` continuously on its own thread,
/// at most `max_fps` frames a second. The compositor only completes a capture
/// once the window changed, so idle windows send fewer frames. Drop the
/// receiver to stop, which also wakes a thread waiting on an idle window.
///
/// Frames rotate through three buffers written in place: the one on screen,
/// the one waiting in the channel and the one being captured. A stalled
/// consumer slows the capture down instead of queueing frames or overwriting
/// the one on screen.
pub fn capture_window(id: String, max_fps: u32) -> Result<Frames> {
  spawn(
    move |c: &mut Capturer| c.window_source(&id),
    max_fps,
    Capturer::alloc,
    |frame: &Arc<Frame>| shown(frame),
  )
}

/// Captures the output named `output` continuously like [`capture_window`].
/// Each frame can be read with [`Frame::read`] until the next one arrives.
pub fn capture_output(output: String, max_fps: u32) -> Result<OutputFrames> {
  spawn(
    move |c: &mut Capturer| c.output_source(&output),
    max_fps,
    Capturer::alloc_readable,
    |frame: &Arc<Frame>| Ok(frame.clone()),
  )
}

fn spawn<T: Send + 'static>(
  source: impl FnOnce(&mut Capturer) -> Result<ExtImageCaptureSourceV1> + Send + 'static,
  max_fps: u32,
  alloc: fn(&mut Capturer) -> Result<(Frame, WlBuffer)>,
  send: impl Fn(&Arc<Frame>) -> Result<T> + Send + 'static,
) -> Result<LiveFrames<T>> {
  // One slot, the sender's own: a send waits until the last frame was taken.
  let (tx, rx) = channel(0);
  let wake = Arc::new(eventfd(0, EventfdFlags::CLOEXEC)?);
  let stop = wake.clone();
  thread::Builder::new()
    .name("live-capture".into())
    .spawn(move || {
      if let Err(e) = run(source, max_fps.max(1), alloc, send, tx, &stop) {
        tracing::warn!("live capture stopped: {e:#}");
      }
    })?;
  Ok(LiveFrames { rx, wake })
}

/// The frames of [`capture_window`]. Dropping it stops the capture.
pub type Frames = LiveFrames<Arc<Dmabuf>>;
/// The frames of [`capture_output`]. Dropping it stops the capture.
pub type OutputFrames = LiveFrames<Arc<Frame>>;

pub struct LiveFrames<T> {
  rx: Receiver<T>,
  wake: Arc<OwnedFd>,
}

impl<T> Stream for LiveFrames<T> {
  type Item = T;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    Pin::new(&mut self.rx).poll_next(cx)
  }
}

impl<T> Drop for LiveFrames<T> {
  fn drop(&mut self) {
    let _ = rustix::io::write(&*self.wake, &1u64.to_ne_bytes());
  }
}

/// The surface to show for a window frame
fn shown(frame: &Frame) -> Result<Arc<Dmabuf>> {
  Ok(match frame.surface.modifier {
    Some(_) => frame.surface.clone(),
    // The renderer uploads shm once per `Arc`, a new one shows new content.
    None => Arc::new(Dmabuf {
      width: frame.surface.width,
      height: frame.surface.height,
      planes: frame
        .surface
        .planes
        .iter()
        .map(|p| {
          Ok(DmabufPlane {
            fd: p.fd.try_clone()?,
            offset: p.offset,
            stride: p.stride,
          })
        })
        .collect::<std::io::Result<_>>()?,
      modifier: None,
      opaque: frame.surface.opaque,
    }),
  })
}

fn run<T>(
  source: impl FnOnce(&mut Capturer) -> Result<ExtImageCaptureSourceV1>,
  max_fps: u32,
  alloc: fn(&mut Capturer) -> Result<(Frame, WlBuffer)>,
  send: impl Fn(&Arc<Frame>) -> Result<T>,
  mut tx: Sender<T>,
  stop: &OwnedFd,
) -> Result<()> {
  let interval = Duration::from_secs(1) / max_fps;
  let mut capturer = Capturer::new()?;
  let source = source(&mut capturer)?;
  let session = match capturer.start_session(&source) {
    Ok(session) => session,
    Err(e) => {
      source.destroy();
      return Err(e);
    }
  };

  let mut buffers: Vec<(Arc<Frame>, WlBuffer)> = Vec::new();
  let mut size = (0, 0);
  let mut next = 0;
  let result = loop {
    // A resized source changes the constraints, the buffers must follow.
    if capturer.size() != size {
      for (_, buffer) in buffers.drain(..) {
        buffer.destroy();
      }
      size = capturer.size();
      match (
        alloc(&mut capturer),
        alloc(&mut capturer),
        alloc(&mut capturer),
      ) {
        (Ok(a), Ok(b), Ok(c)) => {
          buffers.extend([a, b, c].map(|(frame, buffer)| (Arc::new(frame), buffer)))
        }
        (Err(e), _, _) | (_, Err(e), _) | (_, _, Err(e)) => break Err(e),
      }
    }

    let Some((frame, buffer)) = buffers.get(next) else {
      // the compositor finished the session without a buffer size
      break Err(anyhow::anyhow!("the compositor gave no buffer size"));
    };
    let started = Instant::now();
    match capturer.copy(&session, buffer, Some(stop.as_fd())) {
      Ok(true) => {}
      // the receiver was dropped
      Ok(false) => break Ok(()),
      Err(e) => {
        if capturer.size() != size {
          continue;
        }
        break Err(e);
      }
    }

    let item = match send(frame) {
      Ok(item) => item,
      Err(e) => break Err(e),
    };
    if futures::executor::block_on(tx.send(item)).is_err() {
      break Ok(());
    }
    next = (next + 1) % buffers.len();
    thread::sleep(interval.saturating_sub(started.elapsed()));
  };

  for (_, buffer) in buffers {
    buffer.destroy();
  }
  session.destroy();
  source.destroy();
  result
}

#[cfg(test)]
mod tests {
  use super::*;
  use futures::StreamExt;

  #[test]
  fn capture_window_ends_when_no_compositor() {
    unsafe {
      std::env::set_var("WAYLAND_DISPLAY", "non-existent-wayland-display");
    }
    let rx = capture_window("0x1234".into(), 0);
    assert!(rx.is_ok());
    let mut rx = rx.unwrap();
    let frame = futures::executor::block_on(rx.next());
    assert!(frame.is_none());
  }
}

#[cfg(test)]
mod output_tests {
  use super::*;
  use futures::StreamExt;

  #[test]
  fn capture_output_ends_when_no_compositor() {
    unsafe {
      std::env::set_var("WAYLAND_DISPLAY", "non-existent-wayland-display");
    }
    let mut rx = capture_output("DP-1".into(), 0).unwrap();
    assert!(futures::executor::block_on(rx.next()).is_none());
  }
}

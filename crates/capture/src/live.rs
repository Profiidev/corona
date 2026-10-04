use std::{
  sync::Arc,
  thread,
  time::{Duration, Instant},
};

use anyhow::Result;
use futures::{
  SinkExt,
  channel::mpsc::{Receiver, Sender, channel},
};
use gpui_kit::{Dmabuf, DmabufPlane};

use wayland_client::protocol::wl_buffer::WlBuffer;

use crate::{Capturer, Frame};

/// Captures the Hyprland window at `address` continuously on its own thread,
/// at most `max_fps` frames a second. The compositor only completes a capture
/// once the window changed, so idle windows send fewer frames. Drop the
/// receiver to stop.
///
/// Frames alternate between two buffers written in place. The thread waits
/// until the previous frame was taken before capturing into the other one, so
/// a stalled consumer slows the capture down instead of queueing frames or
/// overwriting the one on screen.
pub fn capture_window(address: u64, max_fps: u32) -> Result<Receiver<Arc<Dmabuf>>> {
  // One slot, the sender's own: a send waits until the last frame was taken.
  let (tx, rx) = channel(0);
  thread::Builder::new()
    .name("live-capture".into())
    .spawn(move || {
      if let Err(e) = run(address, max_fps.max(1), tx) {
        tracing::warn!("live capture stopped: {e:#}");
      }
    })?;
  Ok(rx)
}

fn run(address: u64, max_fps: u32, mut tx: Sender<Arc<Dmabuf>>) -> Result<()> {
  let interval = Duration::from_secs(1) / max_fps;
  let mut capturer = Capturer::new()?;
  let source = capturer.window_source(address)?;
  let session = capturer.start_session(&source)?;

  let mut buffers: Vec<(Frame, WlBuffer)> = Vec::new();
  let mut size = (0, 0);
  let mut next = 0;
  let result = loop {
    // A resized window changes the constraints, the buffers must follow.
    if capturer.size() != size {
      for (_, buffer) in buffers.drain(..) {
        buffer.destroy();
      }
      size = capturer.size();
      match (capturer.alloc(), capturer.alloc()) {
        (Ok(a), Ok(b)) => buffers.extend([a, b]),
        (Err(e), _) | (_, Err(e)) => break Err(e),
      }
    }

    let started = Instant::now();
    let (frame, buffer) = &buffers[next];
    if let Err(e) = capturer.copy(&session, buffer) {
      if capturer.size() != size {
        continue;
      }
      break Err(e);
    }

    let surface = match frame.surface.modifier {
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
    };
    if futures::executor::block_on(tx.send(surface)).is_err() {
      break Ok(());
    }
    next ^= 1;
    thread::sleep(interval.saturating_sub(started.elapsed()));
  };

  for (_, buffer) in buffers {
    buffer.destroy();
  }
  session.destroy();
  source.destroy();
  result
}

use std::os::fd::AsFd;

use anyhow::{Context, Result, anyhow, bail};
use image::RgbaImage;
use rustix::fs::{MemfdFlags, ftruncate, memfd_create};
use wayland_client::{
  Connection, Dispatch, EventQueue, QueueHandle, WEnum, delegate_noop,
  globals::{GlobalListContents, registry_queue_init},
  protocol::{
    wl_buffer::WlBuffer,
    wl_output::WlOutput,
    wl_registry,
    wl_shm::{self, WlShm},
    wl_shm_pool::WlShmPool,
  },
};
use wayland_protocols::ext::{
  image_capture_source::v1::client::{
    ext_image_capture_source_v1::ExtImageCaptureSourceV1,
    ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
  },
  image_copy_capture::v1::client::{
    ext_image_copy_capture_frame_v1::{self, ExtImageCopyCaptureFrameV1},
    ext_image_copy_capture_manager_v1::{ExtImageCopyCaptureManagerV1, Options},
    ext_image_copy_capture_session_v1::{self, ExtImageCopyCaptureSessionV1},
  },
};

#[derive(Default)]
struct State {
  outputs: Vec<(WlOutput, Option<String>)>,
  size: (u32, u32),
  format: Option<wl_shm::Format>,
  session_done: bool,
  frame: Option<Result<(), String>>,
}

pub struct Capturer {
  queue: EventQueue<State>,
  qh: QueueHandle<State>,
  state: State,
  shm: WlShm,
  copy: ExtImageCopyCaptureManagerV1,
  sources: ExtOutputImageCaptureSourceManagerV1,
}

impl Capturer {
  pub fn new() -> Result<Self> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
    let qh = queue.handle();
    let mut state = State::default();

    let shm: WlShm = globals.bind(&qh, 1..=1, ())?;
    let copy: ExtImageCopyCaptureManagerV1 = globals
      .bind(&qh, 1..=1, ())
      .context("compositor lacks ext-image-copy-capture-v1")?;
    let sources: ExtOutputImageCaptureSourceManagerV1 = globals.bind(&qh, 1..=1, ())?;

    for global in globals.contents().clone_list() {
      if global.interface == "wl_output" {
        let wl_output =
          globals
            .registry()
            .bind::<WlOutput, _, _>(global.name, global.version.min(4), &qh, ());
        state.outputs.push((wl_output, None));
      }
    }
    queue.roundtrip(&mut state)?;

    Ok(Self {
      queue,
      qh,
      state,
      shm,
      copy,
      sources,
    })
  }

  pub fn capture(&mut self, output: &str) -> Result<RgbaImage> {
    let (queue, qh, state) = (&mut self.queue, &self.qh, &mut self.state);
    state.size = (0, 0);
    state.format = None;
    state.session_done = false;
    state.frame = None;

    let wl_output = state
      .outputs
      .iter()
      .find(|(_, n)| n.as_deref() == Some(output))
      .ok_or_else(|| anyhow!("no output {output}"))?;
    let source = self.sources.create_source(&wl_output.0, qh, ());

    let session = self.copy.create_session(&source, Options::empty(), qh, ());
    while !state.session_done && state.frame.is_none() {
      queue.blocking_dispatch(state)?;
    }
    if let Some(Err(e)) = state.frame.take() {
      session.destroy();
      source.destroy();
      bail!("capture failed: {e}");
    }
    let format = state.format.context("no shm format offered")?;
    let (w, h) = state.size;
    let stride = w * 4;
    let len = (stride * h) as usize;

    let fd = memfd_create("corona-shot", MemfdFlags::CLOEXEC)?;
    ftruncate(&fd, len as u64)?;
    let pool = self.shm.create_pool(fd.as_fd(), len as i32, qh, ());
    let buffer = pool.create_buffer(0, w as i32, h as i32, stride as i32, format, qh, ());

    let frame = session.create_frame(qh, ());
    frame.attach_buffer(&buffer);
    frame.damage_buffer(0, 0, w as i32, h as i32);
    frame.capture();
    while state.frame.is_none() {
      queue.blocking_dispatch(state)?;
    }
    let result = state.frame.take().unwrap();

    frame.destroy();
    buffer.destroy();
    pool.destroy();
    session.destroy();
    source.destroy();
    result.map_err(|e| anyhow!("capture failed: {e}"))?;

    // Safety: No other processes have access to this memfd, and we don't use it after this point.
    let map = unsafe { memmap2::Mmap::map(&fd)? };
    // Argb/Xrgb8888 are little-endian BGRA in memory.
    let opaque = matches!(format, wl_shm::Format::Xrgb8888);
    let mut rgba = map[..len].to_vec();
    for px in rgba.as_chunks_mut::<4>().0 {
      px.swap(0, 2);
      if opaque {
        px[3] = 255;
      }
    }

    RgbaImage::from_raw(w, h, rgba).context("capture buffer smaller than its size")
  }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for State {
  fn event(
    _: &mut Self,
    _: &wl_registry::WlRegistry,
    _: wl_registry::Event,
    _: &GlobalListContents,
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
  }
}

impl Dispatch<WlOutput, ()> for State {
  fn event(
    state: &mut Self,
    output: &WlOutput,
    event: wayland_client::protocol::wl_output::Event,
    _: &(),
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
    if let wayland_client::protocol::wl_output::Event::Name { name } = event
      && let Some(entry) = state.outputs.iter_mut().find(|(o, _)| o == output)
    {
      entry.1 = Some(name);
    }
  }
}

impl Dispatch<ExtImageCopyCaptureSessionV1, ()> for State {
  fn event(
    state: &mut Self,
    _: &ExtImageCopyCaptureSessionV1,
    event: ext_image_copy_capture_session_v1::Event,
    _: &(),
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
    use ext_image_copy_capture_session_v1::Event;
    match event {
      Event::BufferSize { width, height } => state.size = (width, height),
      Event::ShmFormat {
        format: WEnum::Value(f @ (wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888)),
      } => {
        state.format.get_or_insert(f);
      }
      Event::Done => state.session_done = true,
      Event::Stopped => state.frame = Some(Err("session stopped".into())),
      _ => {}
    }
  }
}

impl Dispatch<ExtImageCopyCaptureFrameV1, ()> for State {
  fn event(
    state: &mut Self,
    _: &ExtImageCopyCaptureFrameV1,
    event: ext_image_copy_capture_frame_v1::Event,
    _: &(),
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
    use ext_image_copy_capture_frame_v1::Event;
    match event {
      Event::Ready => state.frame = Some(Ok(())),
      Event::Failed { reason } => state.frame = Some(Err(format!("{reason:?}"))),
      _ => {}
    }
  }
}

delegate_noop!(State: ignore WlShm);
delegate_noop!(State: WlShmPool);
delegate_noop!(State: ignore WlBuffer);
delegate_noop!(State: ExtImageCaptureSourceV1);
delegate_noop!(State: ExtImageCopyCaptureManagerV1);
delegate_noop!(State: ExtOutputImageCaptureSourceManagerV1);

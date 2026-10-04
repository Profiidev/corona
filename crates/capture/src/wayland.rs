use std::{
  fs::{self, File, OpenOptions},
  os::{
    fd::{AsFd, OwnedFd},
    unix::fs::FileExt,
  },
  path::PathBuf,
  sync::Arc,
};

use anyhow::{Context, Result, anyhow, bail};
use gbm::{BufferObjectFlags, Format, Modifier};
use gpui_kit::{Dmabuf, DmabufPlane};
use gpui_wgpu::wgpu;

use crate::view::FrameView;
use image::RgbaImage;
use rustix::fs::{MemfdFlags, ftruncate, major, memfd_create, minor, stat};
use wayland_client::{
  Connection, Dispatch, EventQueue, QueueHandle, WEnum, delegate_noop, event_created_child,
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
  foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1,
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
  },
  image_capture_source::v1::client::{
    ext_foreign_toplevel_image_capture_source_manager_v1::ExtForeignToplevelImageCaptureSourceManagerV1,
    ext_image_capture_source_v1::ExtImageCaptureSourceV1,
    ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
  },
  image_copy_capture::v1::client::{
    ext_image_copy_capture_frame_v1::{self, ExtImageCopyCaptureFrameV1},
    ext_image_copy_capture_manager_v1::{ExtImageCopyCaptureManagerV1, Options},
    ext_image_copy_capture_session_v1::{self, ExtImageCopyCaptureSessionV1},
  },
};
use wayland_protocols::wp::linux_dmabuf::zv1::client::{
  zwp_linux_buffer_params_v1::{self, ZwpLinuxBufferParamsV1},
  zwp_linux_dmabuf_v1::ZwpLinuxDmabufV1,
};

#[derive(Default)]
struct State {
  outputs: Vec<(WlOutput, Option<String>)>,
  size: (u32, u32),
  format: Option<wl_shm::Format>,
  dmabuf_device: Option<u64>,
  dmabuf_formats: Vec<(u32, Vec<u64>)>,
  toplevels: Vec<ExtForeignToplevelHandleV1>,
  session_done: bool,
  frame: Option<Result<(), String>>,
}

pub struct Frame {
  pub surface: Arc<Dmabuf>,
  backing: Backing,
}

enum Backing {
  /// The dmabuf imported into [`crate::gpu`], read back by a GPU copy since
  /// it is usually tiled.
  Gpu(wgpu::Texture),
  /// A memfd, mmapped directly.
  Shm,
}

pub struct Capturer {
  queue: EventQueue<State>,
  qh: QueueHandle<State>,
  state: State,
  shm: WlShm,
  dmabuf: Option<ZwpLinuxDmabufV1>,
  gbm: Option<(u64, gbm::Device<File>)>,
  copy: ExtImageCopyCaptureManagerV1,
  sources: ExtOutputImageCaptureSourceManagerV1,
  toplevel_sources: Option<ExtForeignToplevelImageCaptureSourceManagerV1>,
  _toplevel_list: Option<ExtForeignToplevelListV1>,
}

/// Formats sampled as BGRA/BGRX words, see [`Dmabuf`].
const FORMATS: [(Format, wl_shm::Format); 2] = [
  (Format::Argb8888, wl_shm::Format::Argb8888),
  (Format::Xrgb8888, wl_shm::Format::Xrgb8888),
];

/// The render node of the DRM device `dev`, which may be a primary node.
fn render_node(dev: u64) -> Result<PathBuf> {
  let device = |dev: u64| {
    fs::canonicalize(format!(
      "/sys/dev/char/{}:{}/device",
      major(dev),
      minor(dev)
    ))
  };
  let target = device(dev)?;
  for entry in fs::read_dir("/dev/dri")? {
    let path = entry?.path();
    if path
      .file_name()
      .is_some_and(|n| n.to_string_lossy().starts_with("renderD"))
      && device(stat(&path)?.st_rdev)? == target
    {
      return Ok(path);
    }
  }
  bail!("no render node for device {}:{}", major(dev), minor(dev))
}

impl Capturer {
  pub fn new() -> Result<Self> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
    let qh = queue.handle();
    let mut state = State::default();

    let shm: WlShm = globals.bind(&qh, 1..=1, ())?;
    let dmabuf: Option<ZwpLinuxDmabufV1> = globals.bind(&qh, 3..=4, ()).ok();
    let copy: ExtImageCopyCaptureManagerV1 = globals
      .bind(&qh, 1..=1, ())
      .context("compositor lacks ext-image-copy-capture-v1")?;
    let sources: ExtOutputImageCaptureSourceManagerV1 = globals.bind(&qh, 1..=1, ())?;
    let toplevel_sources = globals.bind(&qh, 1..=1, ()).ok();
    let toplevel_list = globals.bind(&qh, 1..=1, ()).ok();

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
      dmabuf,
      gbm: None,
      copy,
      sources,
      toplevel_sources,
      _toplevel_list: toplevel_list,
    })
  }

  pub fn capture(&mut self, output: &str) -> Result<Frame> {
    let wl_output = self
      .state
      .outputs
      .iter()
      .find(|(_, n)| n.as_deref() == Some(output))
      .ok_or_else(|| anyhow!("no output {output}"))?;
    let source = self.sources.create_source(&wl_output.0, &self.qh, ());
    let session = match self.start_session(&source) {
      Ok(session) => session,
      Err(e) => {
        source.destroy();
        return Err(e);
      }
    };

    let frame = match self.alloc_dmabuf() {
      Ok(Some(frame)) => self.capture_into(&session, frame).and_then(|mut frame| {
        frame.backing = Backing::Gpu(crate::gpu::import(&frame.surface)?);
        Ok(frame)
      }),
      Ok(None) => Err(anyhow!(
        "no usable dmabuf format offered: device {:?}, formats {:x?}",
        self.state.dmabuf_device,
        self.state.dmabuf_formats
      )),
      Err(e) => Err(e),
    }
    .or_else(|e| {
      tracing::warn!("dmabuf capture unavailable, using shm: {e:#}");
      self
        .alloc_shm()
        .and_then(|frame| self.capture_into(&session, frame))
    });

    session.destroy();
    source.destroy();
    frame
  }

  /// A capture source for the first toplevel the compositor listed.
  pub(crate) fn first_toplevel_source(&mut self) -> Result<ExtImageCaptureSourceV1> {
    let manager = self
      .toplevel_sources
      .as_ref()
      .context("compositor lacks ext-foreign-toplevel-image-capture-source-v1")?;
    let toplevel = self
      .state
      .toplevels
      .first()
      .context("no toplevel to capture")?;
    Ok(manager.create_source(toplevel, &self.qh, ()))
  }

  /// Opens a session and waits for its buffer constraints.
  pub(crate) fn start_session(
    &mut self,
    source: &ExtImageCaptureSourceV1,
  ) -> Result<ExtImageCopyCaptureSessionV1> {
    let (queue, qh, state) = (&mut self.queue, &self.qh, &mut self.state);
    state.size = (0, 0);
    state.format = None;
    state.dmabuf_device = None;
    state.dmabuf_formats.clear();
    state.session_done = false;
    state.frame = None;

    let session = self.copy.create_session(source, Options::empty(), qh, ());
    while !state.session_done && state.frame.is_none() {
      queue.blocking_dispatch(state)?;
    }
    if let Some(Err(e)) = state.frame.take() {
      session.destroy();
      bail!("capture failed: {e}");
    }
    Ok(session)
  }

  /// The buffer size the session currently asks for.
  pub(crate) fn size(&self) -> (u32, u32) {
    self.state.size
  }

  /// A buffer for the current session, a dmabuf when possible.
  pub(crate) fn alloc(&mut self) -> Result<(Frame, WlBuffer)> {
    match self.alloc_dmabuf() {
      Ok(Some(buffer)) => return Ok(buffer),
      Ok(None) => tracing::warn!("no usable dmabuf format offered, using shm"),
      Err(e) => tracing::warn!("dmabuf unavailable, using shm: {e:#}"),
    }
    self.alloc_shm()
  }

  /// Captures the next frame of `session` into `buffer`.
  pub(crate) fn copy(
    &mut self,
    session: &ExtImageCopyCaptureSessionV1,
    buffer: &WlBuffer,
  ) -> Result<()> {
    let (queue, qh, state) = (&mut self.queue, &self.qh, &mut self.state);
    let (w, h) = state.size;
    state.frame = None;

    let copy = session.create_frame(qh, ());
    copy.attach_buffer(buffer);
    copy.damage_buffer(0, 0, w as i32, h as i32);
    copy.capture();
    while state.frame.is_none() {
      queue.blocking_dispatch(state)?;
    }
    let result = state.frame.take().unwrap();
    copy.destroy();
    result.map_err(|e| anyhow!("capture failed: {e}"))
  }

  /// A dmabuf on the device the compositor asked for, with any modifier it
  /// offers, `None` when it offers no format this can read.
  fn alloc_dmabuf(&mut self) -> Result<Option<(Frame, WlBuffer)>> {
    let (Some(dmabuf), Some(dev)) = (&self.dmabuf, self.state.dmabuf_device) else {
      return Ok(None);
    };
    // Only modifiers the GPU can import, which also rules out implicit ones.
    let importable = crate::gpu::importable_modifiers();
    let Some((format, modifiers)) = FORMATS.into_iter().find_map(|(f, _)| {
      let (_, mods) = self
        .state
        .dmabuf_formats
        .iter()
        .find(|(code, _)| *code == f as u32)?;
      let mods = mods
        .iter()
        .filter(|m| importable.contains(m))
        .map(|m| Modifier::from(*m))
        .collect::<Vec<_>>();
      (!mods.is_empty()).then_some((f, mods))
    }) else {
      return Ok(None);
    };

    if self.gbm.as_ref().is_none_or(|(d, _)| *d != dev) {
      let node = OpenOptions::new()
        .read(true)
        .write(true)
        .open(render_node(dev)?)?;
      self.gbm = Some((dev, gbm::Device::new(node)?));
    }
    let gbm = &self.gbm.as_ref().unwrap().1;

    let (w, h) = self.state.size;
    let alloc = |flags| {
      gbm.create_buffer_object_with_modifiers2::<()>(w, h, format, modifiers.iter().copied(), flags)
    };
    // NVIDIA refuses linear with RENDERING, the compositor renders into it anyway.
    let bo = alloc(BufferObjectFlags::RENDERING).or_else(|_| alloc(BufferObjectFlags::empty()))?;
    let modifier = u64::from(bo.modifier());

    let params = dmabuf.create_params(&self.qh, ());
    let planes = (0..bo.plane_count() as i32)
      .map(|i| {
        let plane = DmabufPlane {
          fd: bo.fd_for_plane(i)?,
          offset: bo.offset(i),
          stride: bo.stride_for_plane(i),
        };
        params.add(
          plane.fd.as_fd(),
          i as u32,
          plane.offset,
          plane.stride,
          (modifier >> 32) as u32,
          modifier as u32,
        );
        Ok(plane)
      })
      .collect::<Result<Vec<_>>>()?;
    let buffer = params.create_immed(
      w as i32,
      h as i32,
      format as u32,
      zwp_linux_buffer_params_v1::Flags::empty(),
      &self.qh,
      (),
    );
    params.destroy();

    // The planes' fds keep the memory alive, the bo isn't needed anymore.
    let surface = Arc::new(Dmabuf {
      width: w,
      height: h,
      planes,
      modifier: Some(modifier),
      opaque: format == Format::Xrgb8888,
    });
    Ok(Some((
      Frame {
        surface,
        // Replaced by the import once captured.
        backing: Backing::Shm,
      },
      buffer,
    )))
  }

  fn alloc_shm(&mut self) -> Result<(Frame, WlBuffer)> {
    let format = self.state.format.context("no shm format offered")?;
    let (w, h) = self.state.size;
    let stride = w * 4;
    let len = (stride * h) as usize;

    let fd: OwnedFd = memfd_create("corona-shot", MemfdFlags::CLOEXEC)?;
    ftruncate(&fd, len as u64)?;
    let pool = self.shm.create_pool(fd.as_fd(), len as i32, &self.qh, ());
    let buffer = pool.create_buffer(0, w as i32, h as i32, stride as i32, format, &self.qh, ());
    pool.destroy();

    let surface = Arc::new(Dmabuf {
      width: w,
      height: h,
      planes: vec![DmabufPlane {
        fd,
        offset: 0,
        stride,
      }],
      modifier: None,
      opaque: format == wl_shm::Format::Xrgb8888,
    });
    Ok((
      Frame {
        surface,
        backing: Backing::Shm,
      },
      buffer,
    ))
  }

  fn capture_into(
    &mut self,
    session: &ExtImageCopyCaptureSessionV1,
    (frame, buffer): (Frame, WlBuffer),
  ) -> Result<Frame> {
    let result = self.copy(session, &buffer);
    buffer.destroy();
    result.map(|()| frame)
  }
}

impl Frame {
  pub fn width(&self) -> u32 {
    self.surface.width
  }

  pub fn height(&self) -> u32 {
    self.surface.height
  }

  /// An element drawing the frame, see [`FrameView`].
  pub fn view(&self) -> FrameView {
    FrameView::new(self.surface.clone())
  }

  pub fn read_all(&self) -> Result<RgbaImage> {
    self.read(0, 0, self.width(), self.height())
  }

  /// Copies the pixels in the given rect, clamped to the frame, into RAM.
  pub fn read(&self, x: u32, y: u32, w: u32, h: u32) -> Result<RgbaImage> {
    let x = x.min(self.width());
    let y = y.min(self.height());
    let w = w.min(self.width() - x);
    let h = h.min(self.height() - y);
    let opaque = self.surface.opaque;
    if w == 0 || h == 0 {
      return Ok(RgbaImage::new(w, h));
    }

    match &self.backing {
      Backing::Gpu(texture) => {
        let (data, stride) = crate::gpu::read(texture, x, y, w, h)?;
        to_rgba(&data, 0, stride, w, h, opaque)
      }
      Backing::Shm => {
        // Reads just the asked rows of the memfd.
        let plane = &self.surface.planes[0];
        let file = File::from(plane.fd.try_clone()?);
        let row = w as usize * 4;
        let mut data = vec![0; row * h as usize];
        for (i, line) in data.chunks_exact_mut(row).enumerate() {
          let at = plane.offset + (y + i as u32) * plane.stride + x * 4;
          file.read_exact_at(line, at.into())?;
        }
        to_rgba(&data, 0, row as u32, w, h, opaque)
      }
    }
  }
}

/// Rows of little-endian BGRA/BGRX words to RGBA.
fn to_rgba(
  data: &[u8],
  start: usize,
  stride: u32,
  w: u32,
  h: u32,
  opaque: bool,
) -> Result<RgbaImage> {
  let mut rgba = Vec::with_capacity((w * h * 4) as usize);
  for row in 0..h as usize {
    let at = start + row * stride as usize;
    let line = data
      .get(at..at + w as usize * 4)
      .context("capture buffer smaller than its size")?;
    for px in line.as_chunks::<4>().0 {
      rgba.extend_from_slice(&[px[2], px[1], px[0], if opaque { 255 } else { px[3] }]);
    }
  }
  RgbaImage::from_raw(w, h, rgba).context("capture buffer smaller than its size")
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
      Event::DmabufDevice { device } => {
        state.dmabuf_device = device.try_into().ok().map(u64::from_ne_bytes);
      }
      Event::DmabufFormat { format, modifiers } => {
        let modifiers = modifiers
          .as_chunks::<8>()
          .0
          .iter()
          .map(|m| u64::from_ne_bytes(*m))
          .collect();
        state.dmabuf_formats.push((format, modifiers));
      }
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
delegate_noop!(State: ignore ZwpLinuxDmabufV1);
delegate_noop!(State: ignore ExtForeignToplevelHandleV1);
delegate_noop!(State: ExtForeignToplevelImageCaptureSourceManagerV1);

impl Dispatch<ExtForeignToplevelListV1, ()> for State {
  fn event(
    state: &mut Self,
    _: &ExtForeignToplevelListV1,
    event: ext_foreign_toplevel_list_v1::Event,
    _: &(),
    _: &Connection,
    _: &QueueHandle<Self>,
  ) {
    if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = event {
      state.toplevels.push(toplevel);
    }
  }

  event_created_child!(State, ExtForeignToplevelListV1, [
    ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ()),
  ]);
}
delegate_noop!(State: ZwpLinuxBufferParamsV1);

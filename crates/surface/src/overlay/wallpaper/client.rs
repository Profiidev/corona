use std::{
  collections::HashMap,
  os::fd::AsFd,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
};

use anyhow::{Context as _, Result};
use gpui_kit::{Dmabuf, RenderImage};
use rustix::{
  event::{PollFd, PollFlags, poll},
  io::Errno,
  time::Timespec,
};
use tracing::error;
use wayland_client::{
  Connection, Dispatch, QueueHandle, WEnum, delegate_noop,
  globals::{GlobalListContents, registry_queue_init},
  protocol::{
    wl_buffer::WlBuffer,
    wl_compositor::WlCompositor,
    wl_output::{self, Transform, WlOutput},
    wl_registry::{self, WlRegistry},
    wl_shm::{self, WlShm},
    wl_shm_pool::WlShmPool,
    wl_surface::WlSurface,
  },
};
use wayland_protocols::wp::viewporter::client::{
  wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};
use wayland_protocols_wlr::layer_shell::v1::client::{
  zwlr_layer_shell_v1::{Layer, ZwlrLayerShellV1},
  zwlr_layer_surface_v1::{self, Anchor, KeyboardInteractivity, ZwlrLayerSurfaceV1},
};

use super::scaled;

const NAMESPACE: &str = "corona_wallpaper";
/// How often the thread looks whether it should stop
const STOP_CHECK: Timespec = Timespec {
  tv_sec: 1,
  tv_nsec: 0,
};

/// The wallpaper surfaces stay up until this is dropped
pub struct Surfaces(Arc<AtomicBool>);

impl Drop for Surfaces {
  fn drop(&mut self) {
    self.0.store(true, Ordering::Relaxed);
  }
}

pub fn spawn(image: Arc<RenderImage>) -> Result<Surfaces> {
  let stop = Arc::new(AtomicBool::new(false));
  let thread_stop = stop.clone();
  std::thread::Builder::new()
    .name("wallpaper".into())
    .spawn(move || {
      if let Err(e) = run(image, &thread_stop) {
        error!("wallpaper stopped: {e:#}");
      }
    })?;
  Ok(Surfaces(stop))
}

struct Output {
  wl: WlOutput,
  /// Pixels of the current mode, in the panel's orientation
  mode: (u32, u32),
  /// The output is rotated a quarter turn
  transposed: bool,
  surface: Option<Surface>,
}

struct Surface {
  wl: WlSurface,
  layer: ZwlrLayerSurfaceV1,
  viewport: WpViewport,
  /// Logical size from the last configure, none before
  size: Option<(i32, i32)>,
  buffer: Option<WlBuffer>,
}

struct State {
  image: Arc<RenderImage>,
  compositor: WlCompositor,
  shm: WlShm,
  viewporter: WpViewporter,
  layer_shell: ZwlrLayerShellV1,
  outputs: HashMap<u32, Output>,
  /// The image scaled to an output's pixels, shared by outputs of that size
  scaled: HashMap<(u32, u32), Arc<Dmabuf>>,
}

fn run(image: Arc<RenderImage>, stop: &AtomicBool) -> Result<()> {
  let conn = Connection::connect_to_env()?;
  let (globals, mut queue) = registry_queue_init::<State>(&conn)?;
  let qh = queue.handle();
  let mut state = State {
    image,
    compositor: globals.bind(&qh, 1..=6, ())?,
    shm: globals.bind(&qh, 1..=1, ())?,
    viewporter: globals
      .bind(&qh, 1..=1, ())
      .context("compositor lacks wp_viewporter")?,
    layer_shell: globals
      .bind(&qh, 1..=4, ())
      .context("compositor lacks wlr-layer-shell")?,
    outputs: HashMap::new(),
    scaled: HashMap::new(),
  };
  for global in globals.contents().clone_list() {
    if global.interface == "wl_output" {
      state.add_output(globals.registry(), global.name, global.version, &qh);
    }
  }

  while !stop.load(Ordering::Relaxed) {
    queue.dispatch_pending(&mut state)?;
    queue.flush()?;
    let Some(guard) = queue.prepare_read() else {
      continue;
    };
    let fd = guard.connection_fd();
    match poll(&mut [PollFd::new(&fd, PollFlags::IN)], Some(&STOP_CHECK)) {
      Ok(0) | Err(Errno::INTR) => {}
      r => {
        r?;
        guard.read()?;
      }
    }
  }
  Ok(())
}

impl State {
  fn add_output(&mut self, registry: &WlRegistry, name: u32, version: u32, qh: &QueueHandle<Self>) {
    let wl = registry.bind::<WlOutput, _, _>(name, version.min(4), qh, name);
    self.outputs.insert(
      name,
      Output {
        wl,
        mode: (0, 0),
        transposed: false,
        surface: None,
      },
    );
  }

  fn create_surface(&mut self, name: u32, qh: &QueueHandle<Self>) {
    let Some(output) = self.outputs.get_mut(&name) else {
      return;
    };
    let wl = self.compositor.create_surface(qh, ());
    let viewport = self.viewporter.get_viewport(&wl, qh, ());
    let layer = self.layer_shell.get_layer_surface(
      &wl,
      Some(&output.wl),
      Layer::Background,
      NAMESPACE.to_string(),
      qh,
      name,
    );
    layer.set_size(0, 0);
    layer.set_anchor(Anchor::all());
    layer.set_exclusive_zone(-1);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    wl.commit();
    output.surface = Some(Surface {
      wl,
      layer,
      viewport,
      size: None,
      buffer: None,
    });
  }

  /// Shows the image at the output's pixels, stretched over its logical size
  fn draw(&mut self, name: u32, qh: &QueueHandle<Self>) {
    let Some(output) = self.outputs.get(&name) else {
      return;
    };
    let (
      Some(Surface {
        size: Some(size), ..
      }),
      mode,
    ) = (&output.surface, output.mode)
    else {
      return;
    };
    let size = *size;
    if mode.0 == 0 || mode.1 == 0 {
      return;
    }
    let frame = match self.scaled.get(&mode) {
      Some(frame) => frame.clone(),
      None => match scaled(&self.image, mode) {
        Ok(frame) => self.scaled.entry(mode).or_insert(frame).clone(),
        Err(e) => return error!("failed to scale wallpaper: {e:#}"),
      },
    };
    let (width, height) = (frame.width as i32, frame.height as i32);
    let pool = self
      .shm
      .create_pool(frame.planes[0].fd.as_fd(), width * height * 4, qh, ());
    let buffer = pool.create_buffer(
      0,
      width,
      height,
      width * 4,
      wl_shm::Format::Xrgb8888,
      qh,
      (),
    );
    pool.destroy();

    let surface = self
      .outputs
      .get_mut(&name)
      .and_then(|o| o.surface.as_mut())
      .expect("checked above");
    surface.viewport.set_destination(size.0, size.1);
    surface.wl.attach(Some(&buffer), 0, 0);
    surface.wl.damage_buffer(0, 0, width, height);
    surface.wl.commit();
    if let Some(old) = surface.buffer.replace(buffer) {
      old.destroy();
    }

    let modes: Vec<_> = self
      .outputs
      .values()
      .map(|o| match o.transposed {
        true => (o.mode.1, o.mode.0),
        false => o.mode,
      })
      .collect();
    self.scaled.retain(|mode, _| modes.contains(mode));
  }
}

impl Dispatch<WlRegistry, GlobalListContents> for State {
  fn event(
    state: &mut Self,
    registry: &WlRegistry,
    event: wl_registry::Event,
    _: &GlobalListContents,
    _: &Connection,
    qh: &QueueHandle<Self>,
  ) {
    match event {
      wl_registry::Event::Global {
        name,
        interface,
        version,
      } if interface == "wl_output" => state.add_output(registry, name, version, qh),
      wl_registry::Event::GlobalRemove { name } => {
        if let Some(Output {
          surface: Some(surface),
          wl,
          ..
        }) = state.outputs.remove(&name)
        {
          surface.layer.destroy();
          surface.viewport.destroy();
          surface.wl.destroy();
          wl.release();
        }
      }
      _ => {}
    }
  }
}

impl Dispatch<WlOutput, u32> for State {
  fn event(
    state: &mut Self,
    _: &WlOutput,
    event: wl_output::Event,
    name: &u32,
    _: &Connection,
    qh: &QueueHandle<Self>,
  ) {
    let Some(output) = state.outputs.get_mut(name) else {
      return;
    };
    match event {
      wl_output::Event::Geometry { transform, .. } => {
        output.transposed = matches!(
          transform,
          WEnum::Value(
            Transform::_90 | Transform::_270 | Transform::Flipped90 | Transform::Flipped270
          )
        );
      }
      wl_output::Event::Mode {
        flags: WEnum::Value(flags),
        width,
        height,
        ..
      } if flags.contains(wl_output::Mode::Current) => {
        output.mode = (width.max(0) as u32, height.max(0) as u32);
      }
      wl_output::Event::Done => {
        if output.surface.is_none() {
          state.create_surface(*name, qh);
        } else {
          state.draw(*name, qh);
        }
      }
      _ => {}
    }
  }
}

impl Dispatch<ZwlrLayerSurfaceV1, u32> for State {
  fn event(
    state: &mut Self,
    layer: &ZwlrLayerSurfaceV1,
    event: zwlr_layer_surface_v1::Event,
    name: &u32,
    _: &Connection,
    qh: &QueueHandle<Self>,
  ) {
    match event {
      zwlr_layer_surface_v1::Event::Configure {
        serial,
        width,
        height,
      } => {
        layer.ack_configure(serial);
        if let Some(surface) = state.outputs.get_mut(name).and_then(|o| o.surface.as_mut()) {
          surface.size = Some((width as i32, height as i32));
        }
        state.draw(*name, qh);
      }
      zwlr_layer_surface_v1::Event::Closed => {
        if let Some(output) = state.outputs.get_mut(name) {
          output.surface = None;
        }
        layer.destroy();
      }
      _ => {}
    }
  }
}

delegate_noop!(State: WlCompositor);
delegate_noop!(State: ignore WlShm);
delegate_noop!(State: WlShmPool);
delegate_noop!(State: ignore WlBuffer);
delegate_noop!(State: ignore WlSurface);
delegate_noop!(State: WpViewporter);
delegate_noop!(State: WpViewport);
delegate_noop!(State: ZwlrLayerShellV1);

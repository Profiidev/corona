use std::sync::{OnceLock, mpsc};

use anyhow::{Context, Result, anyhow};
use gpui_kit::Dmabuf;
use gpui_wgpu::{dmabuf, wgpu};

struct Gpu {
  device: wgpu::Device,
  queue: wgpu::Queue,
}

fn gpu() -> Result<&'static Gpu> {
  static GPU: OnceLock<Result<Gpu, String>> = OnceLock::new();
  GPU
    .get_or_init(|| open().map_err(|e| format!("{e:#}")))
    .as_ref()
    .map_err(|e| anyhow!("{e}"))
}

fn open() -> Result<Gpu> {
  let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
    backends: wgpu::Backends::VULKAN,
    ..wgpu::InstanceDescriptor::new_without_display_handle()
  });
  // ponytail: the high performance adapter, not necessarily the compositor's
  // GPU on multi GPU systems. Match the DRM device id if that ever matters.
  let adapter =
    futures::executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
      power_preference: wgpu::PowerPreference::HighPerformance,
      ..Default::default()
    }))?;
  let (device, queue) = futures::executor::block_on(dmabuf::request_device(
    &adapter,
    &wgpu::DeviceDescriptor {
      label: Some("corona_capture"),
      ..Default::default()
    },
  ))?;
  Ok(Gpu { device, queue })
}

/// The modifiers [`import`] takes, empty when the GPU can't import dmabufs.
pub fn importable_modifiers() -> &'static [u64] {
  static MODIFIERS: OnceLock<Vec<u64>> = OnceLock::new();
  MODIFIERS.get_or_init(|| match gpu() {
    Ok(gpu) => dmabuf::importable_modifiers(&gpu.device),
    Err(e) => {
      tracing::warn!("no GPU for dmabuf capture: {e:#}");
      Vec::new()
    }
  })
}

/// Fails when the dmabuf can't be sampled, so it is also a check the renderer
/// will be able to show it.
pub fn import(surface: &Dmabuf) -> Result<wgpu::Texture> {
  dmabuf::import(&gpu()?.device, surface)
}

/// Copies a rect of `texture` to RAM as BGRA rows, returns them with their
/// stride.
pub fn read(texture: &wgpu::Texture, x: u32, y: u32, w: u32, h: u32) -> Result<(Vec<u8>, u32)> {
  let Gpu { device, queue } = gpu()?;
  let stride = (w * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
  let buffer = device.create_buffer(&wgpu::BufferDescriptor {
    label: Some("capture_readback"),
    size: u64::from(stride) * u64::from(h),
    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
    mapped_at_creation: false,
  });

  let mut encoder = device.create_command_encoder(&Default::default());
  encoder.copy_texture_to_buffer(
    wgpu::TexelCopyTextureInfo {
      texture,
      mip_level: 0,
      origin: wgpu::Origin3d { x, y, z: 0 },
      aspect: wgpu::TextureAspect::All,
    },
    wgpu::TexelCopyBufferInfo {
      buffer: &buffer,
      layout: wgpu::TexelCopyBufferLayout {
        offset: 0,
        bytes_per_row: Some(stride),
        rows_per_image: Some(h),
      },
    },
    wgpu::Extent3d {
      width: w,
      height: h,
      depth_or_array_layers: 1,
    },
  );
  queue.submit([encoder.finish()]);

  let (tx, rx) = mpsc::channel();
  buffer
    .slice(..)
    .map_async(wgpu::MapMode::Read, move |r| drop(tx.send(r)));
  device.poll(wgpu::PollType::wait_indefinitely())?;
  rx.recv().context("readback dropped")??;
  let data = buffer.slice(..).get_mapped_range().to_vec();
  Ok((data, stride))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn importable_modifiers_does_not_panic() {
    let mods1 = importable_modifiers();
    let mods2 = importable_modifiers();
    assert_eq!(mods1, mods2);
  }

  #[test]
  fn import_handles_empty_surface() {
    let surface = Dmabuf {
      width: 0,
      height: 0,
      planes: vec![],
      modifier: None,
      opaque: false,
    };
    let _ = import(&surface);
  }
}

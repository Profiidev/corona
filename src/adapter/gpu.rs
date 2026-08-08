use std::{ffi::c_void, ptr::NonNull, rc::Rc};

use raw_window_handle::{
  RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use wayland_client::backend::ObjectId;
use wgpu::{
  Adapter, Backends, CreateSurfaceError, Device, DeviceDescriptor, Instance, InstanceDescriptor,
  Queue, RequestAdapterError, RequestAdapterOptions, RequestDeviceError,
};

pub struct GpuContext {
  pub instance: Instance,
  adapter: Adapter,
  pub device: Device,
  pub queue: Queue,
  display_handle: RawDisplayHandle,
}

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
  #[error("no Vulkan adapter available: {0}")]
  NoVulkanAdapter(#[source] RequestAdapterError),
  #[error("failed to request wgpu device: {0}")]
  RequestDevice(#[source] RequestDeviceError),
  #[error("failed to create wgpu surface")]
  CreateSurface(#[source] CreateSurfaceError),
  #[error("surface unsupported by this adapter")]
  SurfaceUnsupported,
  #[error("invalid display handle")]
  InvalidDisplayHandle,
  #[error("invalid surface handle")]
  InvalidSurfaceHandle,
}

impl GpuContext {
  pub fn init(display_id: ObjectId) -> Result<Rc<Self>, GpuError> {
    let instance = Instance::new(InstanceDescriptor {
      backends: Backends::VULKAN,
      ..InstanceDescriptor::new_without_display_handle()
    });

    let adapter = spin_on::spin_on(instance.request_adapter(&RequestAdapterOptions::default()))
      .map_err(GpuError::NoVulkanAdapter)?;
    let info = adapter.get_info();

    tracing::info!(
      "wgpu adapter: {} ({:?}, {:?})",
      info.name,
      info.backend,
      info.device_type
    );

    let (device, queue) = spin_on::spin_on(adapter.request_device(&DeviceDescriptor::default()))
      .map_err(GpuError::RequestDevice)?;

    Ok(Rc::new(Self {
      instance,
      adapter,
      device,
      queue,
      display_handle: wayland_display_handle(&display_id)?,
    }))
  }

  pub fn create_surface(
    &self,
    surface_id: &ObjectId,
    width: u32,
    height: u32,
  ) -> Result<wgpu::Surface<'static>, GpuError> {
    let target = wgpu::SurfaceTargetUnsafe::RawHandle {
      raw_display_handle: Some(self.display_handle),
      raw_window_handle: wayland_surface_handle(surface_id)?,
    };
    // # Safety
    // the wl_display and the wl_surface both outlive the wgpu::Surface
    // (wl_display: the wayland connection is never dropped until the process exits; wl_surface: the layer surface is managed together with the wgpu::Surface, and is not destroyed until the wgpu::Surface is dropped).
    let surface =
      unsafe { self.instance.create_surface_unsafe(target) }.map_err(GpuError::CreateSurface)?;

    self.configure_surface(&surface, width, height)?;

    Ok(surface)
  }

  pub fn configure_surface(
    &self,
    surface: &wgpu::Surface<'_>,
    width: u32,
    height: u32,
  ) -> Result<(), GpuError> {
    let config = surface
      .get_default_config(&self.adapter, width.max(1), height.max(1))
      .ok_or(GpuError::SurfaceUnsupported)?;
    surface.configure(&self.device, &config);

    Ok(())
  }
}

fn wayland_display_handle(display_id: &ObjectId) -> Result<RawDisplayHandle, GpuError> {
  let ptr =
    NonNull::new(display_id.as_ptr().cast::<c_void>()).ok_or(GpuError::InvalidDisplayHandle)?;
  Ok(RawDisplayHandle::Wayland(WaylandDisplayHandle::new(ptr)))
}

fn wayland_surface_handle(surface_id: &ObjectId) -> Result<RawWindowHandle, GpuError> {
  let ptr =
    NonNull::new(surface_id.as_ptr().cast::<c_void>()).ok_or(GpuError::InvalidSurfaceHandle)?;
  Ok(RawWindowHandle::Wayland(WaylandWindowHandle::new(ptr)))
}

//! wgpu (Vulkan backend) context: one shared `Instance`/`Adapter`/`Device`/`Queue` process-wide,
//! one presentable `Surface` per layer surface. femtovg draws into that surface's current texture
//! via `FemtoVGWGPURenderer` — see `window.rs` for the per-frame acquire/render/present sequence.
//!
//! Replaces the EGL/glutin/OpenGL path: femtovg itself is OpenGL-only, but slint's
//! `i-slint-renderer-femtovg` crate also ships a wgpu-backed renderer (`unstable-wgpu-29`
//! feature) that renders to a `wgpu::Texture` we own — wgpu's Vulkan backend gets us Vulkan.

use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;

use anyhow::{Context, Result};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle};
use wayland_client::backend::ObjectId;

pub struct GpuContext {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    display_id: ObjectId,
}

impl GpuContext {
    pub fn new(display_id: ObjectId) -> Result<Rc<Self>> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });

        let adapter = spin_on::spin_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .context("no Vulkan adapter available")?;
        let info = adapter.get_info();
        tracing::info!("wgpu adapter: {} ({:?}, {:?})", info.name, info.backend, info.device_type);

        let (device, queue) = spin_on::spin_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .context("failed to request wgpu device")?;

        Ok(Rc::new(Self { instance, adapter, device, queue, display_id }))
    }

    /// Creates and configures a presentable surface for one layer surface's `wl_surface`.
    pub fn create_surface(&self, surface_id: &ObjectId, width: u32, height: u32) -> Result<wgpu::Surface<'static>> {
        let target = wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(wayland_display_handle(&self.display_id)?),
            raw_window_handle: wayland_surface_handle(surface_id)?,
        };
        // Safety: the wl_display connection and the wl_surface both outlive every `CoronaWindow`
        // (the layer surface owns the wgpu::Surface; the connection outlives the whole process).
        let surface = unsafe { self.instance.create_surface_unsafe(target) }.context("failed to create wgpu surface")?;
        self.configure_surface(&surface, width, height)?;
        Ok(surface)
    }

    pub fn configure_surface(&self, surface: &wgpu::Surface<'_>, width: u32, height: u32) -> Result<()> {
        let config = surface
            .get_default_config(&self.adapter, width.max(1), height.max(1))
            .context("surface unsupported by this adapter")?;
        surface.configure(&self.device, &config);
        Ok(())
    }
}

fn wayland_display_handle(display_id: &ObjectId) -> Result<RawDisplayHandle> {
    let ptr = NonNull::new(display_id.as_ptr().cast::<c_void>()).context("wl_display pointer was null")?;
    Ok(RawDisplayHandle::Wayland(WaylandDisplayHandle::new(ptr)))
}

fn wayland_surface_handle(surface_id: &ObjectId) -> Result<RawWindowHandle> {
    let ptr = NonNull::new(surface_id.as_ptr().cast::<c_void>()).context("wl_surface pointer was null")?;
    Ok(RawWindowHandle::Wayland(WaylandWindowHandle::new(ptr)))
}

//! Wayland-side shared state for corona: registry/output bookkeeping plus our one layer surface
//! (the bar). Multi-surface (notifications, OSD, widgets) will grow this — for now, single
//! surface, single output picked by the compositor.

use std::rc::Rc;

use anyhow::{Context, Result};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::delegate_compositor;
use smithay_client_toolkit::delegate_layer;
use smithay_client_toolkit::delegate_output;
use smithay_client_toolkit::delegate_registry;
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::registry_handlers;
use smithay_client_toolkit::shell::WaylandSurface as _;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use wayland_client::protocol::{wl_output, wl_surface};
use wayland_client::{Connection, Proxy as _, QueueHandle};

use crate::egl::{RenderContextFactory, RenderContextManager};
use crate::platform::CoronaPlatform;
use crate::window::CoronaWindow;

slint::include_modules!();

pub struct AppState {
    registry_state: RegistryState,
    output_state: OutputState,
    // Kept alive for future use (anchor/layer changes, despawn) — not read yet.
    #[allow(dead_code)]
    layer_surface: LayerSurface,
    render_factory: Rc<RenderContextFactory>,
    platform: Rc<CoronaPlatform>,
    /// Set once the first `configure` gives us a real size; the whole point of the two-phase
    /// setup is that we can't create an EGL surface before we know its size.
    window: Option<Rc<CoronaWindow>>,
    bar: Option<Bar>,
    pub exit: bool,
}

impl AppState {
    pub fn new(
        conn: &Connection,
        qh: &QueueHandle<Self>,
        globals: &wayland_client::globals::GlobalList,
    ) -> Result<Self> {
        let compositor =
            CompositorState::bind(globals, qh).context("wl_compositor not available")?;
        let layer_shell = LayerShell::bind(globals, qh).context(
            "zwlr_layer_shell_v1 not available (compositor isn't wlr-layer-shell capable)",
        )?;

        let wl_surface = compositor.create_surface(qh);
        let layer_surface = layer_shell.create_layer_surface(
            qh,
            wl_surface,
            Layer::Top,
            Some("corona-bar"),
            None, // let the compositor pick the output
        );
        layer_surface.set_anchor(Anchor::TOP | Anchor::LEFT | Anchor::RIGHT);
        layer_surface.set_size(0, 32); // width 0 = "give me the full anchored width"
        layer_surface.set_exclusive_zone(32);
        layer_surface.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer_surface.wl_surface().commit();

        let render_manager = RenderContextManager::new(&conn.display().id())?;
        let render_factory = RenderContextFactory::new(render_manager);

        Ok(Self {
            registry_state: RegistryState::new(globals),
            output_state: OutputState::new(globals, qh),
            layer_surface,
            render_factory,
            platform: CoronaPlatform::new(),
            window: None,
            bar: None,
            exit: false,
        })
    }

    /// Re-render the bar if its window was marked dirty since the last call.
    pub fn render_if_dirty(&self) -> Result<()> {
        if let Some(window) = &self.window {
            window.render_if_dirty()?;
        }
        Ok(())
    }
}

impl CompositorHandler for AppState {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    // ponytail: no frame-callback throttling yet: we just render whenever something is dirty on
    // the next event-loop tick. Add wl_surface.frame() damage tracking if idle CPU use matters.
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for AppState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for AppState {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let (width, height) = configure.new_size;
        // A dimension of 0 means "you choose"; we always request a concrete size, but fall back
        // just in case a compositor still hands us zero on the very first configure.
        let width = width.max(1);
        let height = height.max(32);
        let physical_size = slint::PhysicalSize::new(width, height);

        match &self.window {
            None => {
                // First configure: create the EGL context/surface now that we know the real size,
                // wire it into a WindowAdapter, register it with the Slint platform, then set the
                // platform (must happen before the Bar component is constructed) and build the UI.
                let result: Result<()> = (|| {
                    let context = self.render_factory.create_context(
                        &layer.wl_surface().id(),
                        width,
                        height,
                    )?;
                    let window = CoronaWindow::new(context, physical_size)?;
                    self.platform.add_window(Rc::clone(&window));
                    slint::platform::set_platform(Box::new(PlatformWrapper(Rc::clone(
                        &self.platform,
                    ))))
                    .map_err(|e| anyhow::anyhow!("failed to set Slint platform: {e:?}"))?;
                    self.window = Some(window);
                    self.bar = Some(Bar::new()?);
                    Ok(())
                })();
                if let Err(e) = result {
                    tracing::error!("failed to initialize bar surface: {e:#}");
                    self.exit = true;
                }
            }
            Some(window) => {
                window.set_physical_size(physical_size);
            }
        }
    }
}

/// `slint::platform::set_platform` wants `Platform`, not `Rc<dyn Platform>` — this just forwards.
struct PlatformWrapper(Rc<CoronaPlatform>);
impl slint::platform::Platform for PlatformWrapper {
    fn create_window_adapter(
        &self,
    ) -> Result<Rc<dyn slint::platform::WindowAdapter>, slint::PlatformError> {
        self.0.create_window_adapter()
    }
}

delegate_compositor!(AppState);
delegate_output!(AppState);
delegate_layer!(AppState);
delegate_registry!(AppState);

impl ProvidesRegistryState for AppState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

//! Reactive egui CPU rasterization with softbuffer's Win32 GDI presenter.
//! Unchanged expose events reuse the retained bitmap; minimized windows run logic only.
use anyhow::{Result, anyhow, ensure};
use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use std::{
    num::NonZeroU32,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

pub trait Application {
    fn logic(&mut self, ctx: &egui::Context);
    fn ui(&mut self, ctx: &egui::Context);
    fn close_requested(&self) -> bool;
}
type Factory = Box<dyn FnOnce(&egui::Context) -> Box<dyn Application>>;
#[derive(Debug)]
struct Wake(Instant);
#[derive(Default)]
struct Counts {
    logic: u64,
    layout: u64,
    raster: u64,
    present: u64,
}
struct Root {
    window: Arc<Window>,
    surface: softbuffer::Surface<Arc<Window>, Arc<Window>>,
    ctx: egui::Context,
    input: egui_winit::State,
    info: egui::ViewportInfo,
    app: Box<dyn Application>,
    renderer: EguiSoftwareRender,
    pixels: Vec<[u8; 4]>,
    bitmap: Vec<u32>,
    bitmap_size: [u32; 2],
    dirty: bool,
    cleanup_done: bool,
    occluded: bool,
    counts: Counts,
}
impl Root {
    fn drawable(&self) -> bool {
        let size = self.window.inner_size();
        !self.occluded
            && self.window.is_minimized() != Some(true)
            && self.window.is_visible() != Some(false)
            && size.width > 0
            && size.height > 0
    }
    fn logic(&mut self, event_loop: &ActiveEventLoop) {
        self.counts.logic += 1;
        self.app.logic(&self.ctx);
        if self.app.close_requested() {
            event_loop.exit();
        }
    }
    fn present(&mut self) -> Result<()> {
        let [w, h] = self.bitmap_size;
        if w == 0 || h == 0 {
            return Ok(());
        }
        self.surface
            .resize(NonZeroU32::new(w).unwrap(), NonZeroU32::new(h).unwrap())
            .map_err(|e| anyhow!("GDI bitmap resize: {e}"))?;
        let mut buffer = self
            .surface
            .buffer_mut()
            .map_err(|e| anyhow!("GDI buffer: {e}"))?;
        buffer.copy_from_slice(&self.bitmap);
        self.window.pre_present_notify();
        buffer.present().map_err(|e| anyhow!("GDI present: {e}"))?;
        self.counts.present += 1;
        Ok(())
    }
    fn draw(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        if !self.drawable() {
            return Ok(());
        }
        if !self.dirty {
            return self.present();
        }
        self.logic(event_loop);
        if event_loop.exiting() {
            return Ok(());
        }
        let size = self.window.inner_size();
        ensure!(
            u64::from(size.width) * u64::from(size.height) <= 16_777_216,
            "UI bitmap exceeds the 16-megapixel allocation limit"
        );
        egui_winit::update_viewport_info(&mut self.info, &self.ctx, &self.window, false);
        let mut raw = self.input.take_egui_input(&self.window);
        raw.viewports
            .insert(egui::ViewportId::ROOT, self.info.clone());
        let mut output = self.ctx.run(raw, |ctx| self.app.ui(ctx));
        self.counts.layout += 1;
        let primitives = self
            .ctx
            .tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point);
        ensure!(
            primitives
                .iter()
                .all(|p| matches!(p.primitive, egui::epaint::Primitive::Mesh(_))),
            "CPU host does not support GPU paint callbacks"
        );
        self.pixels
            .resize(size.width as usize * size.height as usize, [0; 4]);
        self.renderer.render(
            &mut BufferMutRef::new(&mut self.pixels, size.width as usize, size.height as usize),
            &primitives,
            &output.textures_delta,
            output.pixels_per_point,
        );
        self.counts.raster += 1;
        self.bitmap.clear();
        self.bitmap
            .extend(self.pixels.iter().copied().map(gdi_pixel));
        self.bitmap_size = [size.width, size.height];
        self.present()?;
        if !self.cleanup_done {
            second_screen::update::cleanup_after_update(
                &std::env::args().skip(1).collect::<Vec<_>>(),
            );
            self.cleanup_done = true;
        }
        self.dirty = false;
        self.input
            .handle_platform_output(&self.window, output.platform_output);
        if let Some(viewport) = output.viewport_output.remove(&egui::ViewportId::ROOT) {
            let close = viewport.commands.contains(&egui::ViewportCommand::Close)
                && !viewport
                    .commands
                    .contains(&egui::ViewportCommand::CancelClose);
            let mut actions = Vec::new();
            egui_winit::process_viewport_commands(
                &self.ctx,
                &mut self.info,
                viewport.commands,
                &self.window,
                &mut actions,
            );
            if close {
                event_loop.exit();
            }
        }
        self.info.events.clear();
        Ok(())
    }
}
struct Host {
    ctx: egui::Context,
    factory: Option<Factory>,
    root: Option<Root>,
    next: Deadline,
    error: Option<anyhow::Error>,
}
impl Host {
    fn create(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let icon =
            image::load_from_memory(include_bytes!("../assets/SecondScreen.png"))?.into_rgba8();
        let icon =
            winit::window::Icon::from_rgba(icon.as_raw().clone(), icon.width(), icon.height())?;
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("SecondScreen")
                    .with_window_icon(Some(icon.clone()))
                    .with_visible(false)
                    .with_inner_size(winit::dpi::LogicalSize::new(820.0, 760.0))
                    .with_min_inner_size(winit::dpi::LogicalSize::new(640.0, 540.0)),
            )?,
        );
        // winit's generic icon config assigns ICON_SMALL only on Windows.
        // Explicitly assign ICON_BIG for taskbar and Alt+Tab identity as well.
        #[cfg(windows)]
        {
            use winit::platform::windows::WindowExtWindows;
            window.set_taskbar_icon(Some(icon));
        }
        let input = egui_winit::State::new(
            self.ctx.clone(),
            egui::ViewportId::ROOT,
            window.as_ref(),
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(16384),
        );
        let context =
            softbuffer::Context::new(window.clone()).map_err(|e| anyhow!("GDI context: {e}"))?;
        let surface = softbuffer::Surface::new(&context, window.clone())
            .map_err(|e| anyhow!("GDI surface: {e}"))?;
        let app = self
            .factory
            .take()
            .ok_or_else(|| anyhow!("Window already initialized"))?(&self.ctx);
        self.root = Some(Root {
            window: window.clone(),
            surface,
            ctx: self.ctx.clone(),
            input,
            info: Default::default(),
            app,
            renderer: EguiSoftwareRender::new(ColorFieldOrder::Rgba),
            pixels: Vec::new(),
            bitmap: Vec::new(),
            bitmap_size: [0, 0],
            dirty: true,
            cleanup_done: false,
            occluded: false,
            counts: Counts::default(),
        });
        window.set_visible(true);
        self.next.schedule(Instant::now());
        Ok(())
    }
    fn fail(&mut self, error: anyhow::Error, event_loop: &ActiveEventLoop) {
        self.error = Some(error);
        event_loop.exit();
    }
}
impl ApplicationHandler<Wake> for Host {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.root.is_none()
            && let Err(error) = self.create(event_loop)
        {
            self.fail(error, event_loop);
        }
    }
    fn user_event(&mut self, _: &ActiveEventLoop, event: Wake) {
        self.next.schedule(event.0);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(root) = &mut self.root else {
            return;
        };
        if id != root.window.id() {
            return;
        }
        if matches!(event, WindowEvent::CloseRequested) {
            event_loop.exit();
            return;
        }
        if matches!(event, WindowEvent::RedrawRequested) {
            if let Err(error) = root.draw(event_loop) {
                self.fail(error, event_loop);
            }
            return;
        }
        if let WindowEvent::Occluded(value) = event {
            root.occluded = value;
        }
        let response = root.input.on_window_event(&root.window, &event);
        if response.repaint
            || matches!(
                event,
                WindowEvent::Resized(_)
                    | WindowEvent::ScaleFactorChanged { .. }
                    | WindowEvent::Occluded(false)
            )
        {
            root.dirty = true;
            self.next.schedule(Instant::now());
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.next.take_due(Instant::now())
            && let Some(root) = &mut self.root
        {
            root.dirty = true;
            if root.drawable() {
                root.window.request_redraw();
            } else {
                root.logic(event_loop);
            }
        }
        event_loop.set_control_flow(
            self.next
                .0
                .map_or(ControlFlow::Wait, ControlFlow::WaitUntil),
        );
    }
    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.ctx.set_request_repaint_callback(|_| {});
        // Optional counters do not log synchronously in the rendering path.
        if let Some(root) = &self.root
            && let Some(path) = std::env::var_os("SECOND_SCREEN_RENDER_METRICS")
        {
            let c = &root.counts;
            let report = serde_json::json!({"backend":"egui_software_backend 0.0.2 -> CPU RGBA -> softbuffer Win32 GDI",
                "logic":c.logic,"layout":c.layout,"raster":c.raster,"present":c.present});
            let _ = std::fs::write(path, report.to_string());
        }
    }
}
pub fn run(factory: Factory) -> Result<()> {
    let event_loop = EventLoop::<Wake>::with_user_event().build()?;
    let proxy = event_loop.create_proxy();
    let ctx = egui::Context::default();
    ctx.set_request_repaint_callback(move |info| {
        if info.delay != Duration::MAX
            && let Some(when) = Instant::now().checked_add(info.delay)
        {
            let _ = proxy.send_event(Wake(when));
        }
    });
    let mut host = Host {
        ctx,
        factory: Some(factory),
        root: None,
        next: Deadline::default(),
        error: None,
    };
    event_loop.run_app(&mut host)?;
    if let Some(error) = host.error {
        return Err(error);
    }
    Ok(())
}
fn gdi_pixel([r, g, b, _]: [u8; 4]) -> u32 {
    u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
}
#[derive(Default)]
struct Deadline(Option<Instant>);
impl Deadline {
    fn schedule(&mut self, when: Instant) {
        self.0 = Some(self.0.map_or(when, |old| old.min(when)));
    }
    fn take_due(&mut self, now: Instant) -> bool {
        if self.0.is_some_and(|when| when <= now) {
            self.0 = None;
            true
        } else {
            false
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn finite_deadlines_expire_without_idle_polling() {
        let now = Instant::now();
        let mut deadline = Deadline::default();
        deadline.schedule(now + Duration::from_secs(10));
        deadline.schedule(now);
        assert!(deadline.take_due(now));
        assert!(deadline.0.is_none());
        assert!(!deadline.take_due(now + Duration::from_secs(60)));
    }
    #[test]
    fn native_bitmap_channel_order_is_rgb() {
        assert_eq!(gdi_pixel([1, 2, 3, 255]), 0x00010203);
    }
}

//! Bounded offscreen rendering of real UI components. No window, app workers or I/O beyond PNGs.
use egui_software_backend::{BufferMutRef, ColorFieldOrder, EguiSoftwareRender};
use second_screen::{
    settings::Settings,
    ui::{self, HostView, UpdateView},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("Supply an isolated output directory")?;
    std::fs::create_dir_all(&output)?;
    let started = Instant::now();
    for (name, width, height, scenario) in [
        ("connect", 820, 760, 0),
        ("pairing", 820, 760, 1),
        ("updates", 820, 760, 2),
        ("connect-narrow", 640, 640, 0),
        ("extend-setup", 820, 760, 3),
        ("driver-missing", 820, 760, 4),
    ] {
        let ctx = egui::Context::default();
        ui::apply_theme(&ctx, Default::default());
        let mut renderer = EguiSoftwareRender::new(ColorFieldOrder::Rgba);
        let mut pixels = vec![[0u8; 4]; width * height];
        let mut settings = Settings {
            bind_ip: "192.168.1.20".into(),
            ..Default::default()
        };
        let mut trusted = false;
        if scenario >= 3 {
            settings.stream.source = second_screen::capture::SourceKind::VirtualDisplay;
        }
        let mut tab = if scenario == 2 { 1 } else { 0 };
        let mut host = HostView {
            address: "http://192.168.1.20:7420".into(),
            code: "7K3M9R2AX4".into(),
            ..Default::default()
        };
        if scenario == 4 {
            host.approved = true;
            host.peer = Some("192.168.1.42".into());
            host.error = Some("SecondScreen virtual-display driver is not installed. See the driver build guide; this app never installs it.".into());
        }
        let update = UpdateView {
            message: "No release check yet. The repository may not be published.".into(),
            busy: false,
            available: None,
            progress: None,
            installer_error: None,
        };
        // Four finite frames settle fonts/layout. This never enters a native event loop.
        for frame in 0..4 {
            if started.elapsed() > Duration::from_secs(20) {
                return Err("Offscreen rendering deadline exceeded".into());
            }
            let result = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width as f32, height as f32),
                    )),
                    time: Some(frame as f64 / 60.0),
                    ..Default::default()
                },
                |ctx| {
                    ui::header(ctx, &mut tab, settings.language);
                    egui::CentralPanel::default()
                        .frame(egui::Frame::new().fill(ui::BACKGROUND).inner_margin(24.0))
                        .show(ctx, |surface| {
                            egui::ScrollArea::vertical().show(surface, |surface| {
                                if scenario == 2 {
                                    ui::preferences(surface, &mut settings);
                                    ui::updates(
                                        surface,
                                        &mut settings.startup_updates,
                                        &update,
                                        settings.language,
                                    );
                                } else {
                                    ui::connect(
                                        surface,
                                        &mut settings,
                                        &mut trusted,
                                        if scenario == 1 || scenario == 4 {
                                            Some(&host)
                                        } else {
                                            None
                                        },
                                        false,
                                        "",
                                    );
                                }
                            });
                        });
                },
            );
            let primitives = ctx.tessellate(result.shapes, result.pixels_per_point);
            renderer.render(
                &mut BufferMutRef::new(&mut pixels, width, height),
                &primitives,
                &result.textures_delta,
                result.pixels_per_point,
            );
        }
        let rgba: Vec<u8> = pixels.into_iter().flatten().collect();
        image::save_buffer(
            output.join(format!("{name}.png")),
            &rgba,
            width as u32,
            height as u32,
            image::ColorType::Rgba8,
        )?;
    }
    std::fs::write(
        output.join("capture.txt"),
        "Real SecondScreen ui components; isolated fixtures; 1x scale; CPU fallback rasterizer; no window or production OpenGL presentation. Four frames per image. Native parity, idle/hidden behavior and interactions are unverified.\n",
    )?;
    println!(
        "Saved six offscreen component captures to {} in {:?}",
        output.display(),
        started.elapsed()
    );
    Ok(())
}

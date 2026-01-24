#![cfg_attr(windows, windows_subsystem = "windows")]

mod reactive;

use eframe::egui;
use second_screen::{
    VERSION,
    capture::SourceKind,
    server::{Host, Shared},
    settings::{self, Settings},
    update::{self, Available},
};
use std::{
    net::{Ipv4Addr, SocketAddrV4},
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

enum HostCommand {
    Start(Settings),
    Stop,
    Save(Settings),
    Quit,
}
enum HostEvent {
    Started(String, Arc<Shared>),
    Stopped,
    Error(String),
    Saved,
}
enum UpdateCommand {
    Check,
    Install(Available),
}
#[derive(Default)]
struct UpdateState {
    message: String,
    busy: bool,
    available: Option<Available>,
    progress: Option<f32>,
    restart: bool,
}

struct App {
    settings: Settings,
    host_tx: crossbeam_channel::Sender<HostCommand>,
    host_rx: crossbeam_channel::Receiver<HostEvent>,
    host_worker: Option<thread::JoinHandle<()>>,
    update_tx: crossbeam_channel::Sender<UpdateCommand>,
    updates: Arc<Mutex<UpdateState>>,
    shared: Option<Arc<Shared>>,
    address: String,
    message: String,
    busy: bool,
    trusted_network: bool,
    tab: u8,
    installer_error: Option<String>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(12.0, 12.0);
        style.spacing.button_padding = egui::vec2(16.0, 10.0);
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = egui::Color32::from_rgb(11, 20, 30);
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(30, 95, 83);
        cc.egui_ctx.set_style(style);
        let (settings, message) = match settings::load(&settings::path()) {
            Ok(s) => (s, "Sharing is off. Nothing is being captured.".into()),
            Err(e) => (
                Settings {
                    startup_updates: false,
                    ..Default::default()
                },
                format!("{e}. Defaults loaded; startup update check skipped."),
            ),
        };
        let (host_tx, commands) = crossbeam_channel::bounded(8);
        let (events, host_rx) = crossbeam_channel::bounded(16);
        let ctx = cc.egui_ctx.clone();
        let host_worker = thread::spawn(move || {
            let mut host = None;
            while let Ok(command) = commands.recv() {
                let event = match command {
                    HostCommand::Start(settings) => {
                        let result = settings
                            .bind_ip
                            .parse::<Ipv4Addr>()
                            .map_err(|_| "Enter a valid IPv4 address".to_string())
                            .and_then(|ip| {
                                if settings.port == 0 {
                                    return Err("Choose a port from 1 to 65535".into());
                                }
                                let repaint = ctx.clone();
                                Host::start(
                                    SocketAddrV4::new(ip, settings.port),
                                    settings.stream.clone(),
                                    Arc::new(move || repaint.request_repaint()),
                                )
                            });
                        match result {
                            Ok(new_host) => {
                                let event = HostEvent::Started(
                                    format!("http://{}", new_host.address),
                                    new_host.shared.clone(),
                                );
                                host = Some(new_host);
                                event
                            }
                            Err(e) => HostEvent::Error(e),
                        }
                    }
                    HostCommand::Stop => {
                        host = None;
                        HostEvent::Stopped
                    }
                    HostCommand::Save(s) => match settings::save(&settings::path(), &s) {
                        Ok(()) => HostEvent::Saved,
                        Err(e) => HostEvent::Error(e),
                    },
                    HostCommand::Quit => break,
                };
                if events.send(event).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
            drop(host);
        });
        let updates = Arc::new(Mutex::new(UpdateState {
            message: "No update check yet. Repository and releases may not be published.".into(),
            ..Default::default()
        }));
        let update_state = updates.clone();
        let update_ctx = cc.egui_ctx.clone();
        let (update_tx, commands) = crossbeam_channel::bounded(2);
        thread::spawn(move || {
            while let Ok(command) = commands.recv() {
                {
                    let mut s = update_state.lock().unwrap();
                    s.busy = true;
                    s.progress = None;
                }
                update_ctx.request_repaint();
                match command {
                    UpdateCommand::Check => {
                        let result = update::check();
                        let mut s = update_state.lock().unwrap();
                        match result {
                            Ok(Some(available)) => {
                                s.message = format!("Version {} is available.", available.version);
                                s.available = Some(available);
                            }
                            Ok(None) => {
                                s.message = "You have the latest published stable version.".into();
                                s.available = None;
                            }
                            Err(e) => {
                                s.message = e;
                                s.available = None;
                            }
                        }
                    }
                    UpdateCommand::Install(available) => {
                        update_state.lock().unwrap().message =
                            "Downloading and verifying the update…".into();
                        let result =
                            std::env::current_exe()
                                .map_err(|e| e.to_string())
                                .and_then(|exe| {
                                    let staged =
                                        update::download(&available, &exe, |done, total| {
                                            update_state.lock().unwrap().progress =
                                                Some(done as f32 / total as f32);
                                            update_ctx.request_repaint();
                                        })?;
                                    if let Err(e) =
                                        update::launch_helper(&staged, &available.digest)
                                    {
                                        let _ = std::fs::remove_file(staged);
                                        return Err(e);
                                    }
                                    Ok(())
                                });
                        let mut s = update_state.lock().unwrap();
                        match result {
                            Ok(()) => {
                                s.restart = true;
                                s.message = "Restarting to install the verified update…".into();
                            }
                            Err(e) => s.message = e,
                        }
                    }
                }
                update_state.lock().unwrap().busy = false;
                update_ctx.request_repaint();
            }
        });
        if settings.startup_updates {
            updates.lock().unwrap().busy = true;
            let _ = update_tx.try_send(UpdateCommand::Check);
        }
        let installer_error = settings::path()
            .parent()
            .and_then(|p| std::fs::read_to_string(p.join("update-error.txt")).ok())
            .map(|s| s.chars().take(2048).collect());
        Self {
            settings,
            host_tx,
            host_rx,
            host_worker: Some(host_worker),
            update_tx,
            updates,
            shared: None,
            address: String::new(),
            message,
            busy: false,
            trusted_network: false,
            tab: 0,
            installer_error,
        }
    }
    fn send_host(&mut self, command: HostCommand) {
        match self.host_tx.try_send(command) {
            Ok(()) => self.busy = true,
            Err(_) => self.message = "Host worker is busy; try again shortly.".into(),
        }
    }
    fn host_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Your tablet. A familiar view.");
        ui.label("Share your primary monitor with one paired browser on your local network.");
        ui.colored_label(
            egui::Color32::from_rgb(106, 229, 189),
            "MIRRORING ONLY  ·  READ ONLY",
        );
        ui.separator();
        if let Some(shared) = self.shared.clone() {
            let (code, peer, frame, error, stopped) = {
                let s = shared.state.lock().unwrap();
                (
                    s.pairing.code.clone(),
                    s.pairing.session.as_ref().map(|p| (p.ip, p.approved)),
                    s.frame
                        .as_ref()
                        .map(|f| (f.width, f.height, f.sequence, f.jpeg.len())),
                    s.error.clone(),
                    s.stopped,
                )
            };
            ui.heading(if stopped {
                "Listener stopped"
            } else {
                "Host is listening"
            });
            ui.label("Open this exact address on your tablet (same network):");
            ui.horizontal(|ui| {
                ui.monospace(&self.address);
                if ui.button("Copy address").clicked() {
                    ui.ctx().copy_text(self.address.clone());
                }
            });
            if self.settings.bind_ip == "127.0.0.1" {
                ui.label("Loopback address: only browsers on this computer can connect.");
            }
            if let Some((ip, approved)) = peer {
                ui.label(format!(
                    "Viewer: {ip} — {}",
                    if approved {
                        "Approved"
                    } else {
                        "Waiting for your approval"
                    }
                ));
                if !approved && ui.button("Approve viewer and start capture").clicked() {
                    shared.approve();
                }
                if ui.button("Disconnect viewer / rotate code").clicked() {
                    shared.disconnect();
                }
            } else {
                ui.label("Pairing code (expires after five minutes):");
                ui.label(
                    egui::RichText::new(code)
                        .monospace()
                        .size(30.0)
                        .color(egui::Color32::from_rgb(106, 229, 189)),
                );
                ui.label("Enter this code on the tablet, then approve its request here.");
                if ui.button("New pairing code").clicked() {
                    shared.disconnect();
                }
            }
            if let Some((w, h, sequence, bytes)) = frame {
                ui.label(format!(
                    "Latest frame: {w} × {h}  ·  #{sequence}  ·  {} KiB",
                    bytes / 1024
                ));
            }
            if let Some(error) = error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
                ui.label("Stop sharing and start again to retry capture.");
            }
            if ui
                .add_enabled(!self.busy, egui::Button::new("Stop sharing"))
                .clicked()
            {
                self.send_host(HostCommand::Stop);
            }
        } else {
            ui.add_enabled_ui(!self.busy, |ui| {
                egui::Grid::new("stream-settings").spacing([20.0, 12.0]).show(ui, |ui| {
                    ui.label("Source");
                    egui::ComboBox::from_id_salt("source").selected_text(if self.settings.stream.source == SourceKind::PrimaryMonitor { "Primary monitor" } else { "Synthetic test pattern" }).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.settings.stream.source, SourceKind::PrimaryMonitor, "Primary monitor");
                        ui.selectable_value(&mut self.settings.stream.source, SourceKind::Synthetic, "Synthetic test pattern");
                    }); ui.end_row();
                    ui.label("Listen IPv4"); ui.text_edit_singleline(&mut self.settings.bind_ip); ui.end_row();
                    ui.label("Port"); ui.add(egui::DragValue::new(&mut self.settings.port).range(1..=65535)); ui.end_row();
                    ui.label("Maximum width"); ui.add(egui::Slider::new(&mut self.settings.stream.max_width, 320..=1920).suffix(" px")); ui.end_row();
                    ui.label("Maximum height"); ui.add(egui::Slider::new(&mut self.settings.stream.max_height, 240..=1080).suffix(" px")); ui.end_row();
                    ui.label("JPEG quality"); ui.add(egui::Slider::new(&mut self.settings.stream.quality, 30..=90)); ui.end_row();
                    ui.label("Frame rate limit"); ui.add(egui::Slider::new(&mut self.settings.stream.fps, 5..=20).suffix(" fps")); ui.end_row();
                });
                ui.label("Enter your computer's private LAN IPv4 to reach it from a tablet. The default 127.0.0.1 is local only. Display aspect ratio is preserved; settings apply on the next start.");
                ui.colored_label(egui::Color32::from_rgb(223, 194, 132), "Unencrypted HTTP: frames and credentials can be read on the network.");
                ui.checkbox(&mut self.trusted_network, "I am using a trusted private network and understand this limitation");
                ui.horizontal(|ui| {
                    if ui.add_enabled(self.trusted_network, egui::Button::new("Start sharing")).clicked() { self.send_host(HostCommand::Start(self.settings.clone())); }
                    if ui.button("Save settings").clicked() { self.send_host(HostCommand::Save(self.settings.clone())); }
                });
            });
        }
        ui.separator();
        ui.label(&self.message);
    }
    fn update_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Updates & settings");
        ui.label(format!(
            "Installed version: {VERSION}  ·  Windows x64 portable"
        ));
        if ui
            .checkbox(
                &mut self.settings.startup_updates,
                "Check for updates on startup",
            )
            .changed()
        {
            self.send_host(HostCommand::Save(self.settings.clone()));
        }
        let (message, busy, available, progress) = {
            let s = self.updates.lock().unwrap();
            (s.message.clone(), s.busy, s.available.clone(), s.progress)
        };
        ui.label(message);
        if let Some(progress) = progress {
            ui.add(egui::ProgressBar::new(progress).show_percentage());
        }
        if ui
            .add_enabled(!busy, egui::Button::new("Check for updates"))
            .clicked()
            && self.update_tx.try_send(UpdateCommand::Check).is_ok()
        {
            self.updates.lock().unwrap().busy = true;
        }
        if let Some(available) = available {
            ui.label("Installation will disconnect the tablet and restart SecondScreen. Settings are preserved. A backup of the previous executable stays beside the application.");
            if ui
                .add_enabled(!busy, egui::Button::new("Download and install"))
                .clicked()
                && self
                    .update_tx
                    .try_send(UpdateCommand::Install(available))
                    .is_ok()
            {
                self.updates.lock().unwrap().busy = true;
            }
        }
        if let Some(error) = &self.installer_error {
            ui.colored_label(
                egui::Color32::LIGHT_RED,
                format!("Last installer error: {error}"),
            );
        }
        ui.separator();
        ui.label(&self.message);
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        while let Ok(event) = self.host_rx.try_recv() {
            self.busy = false;
            match event {
                HostEvent::Started(address, shared) => {
                    self.address = address;
                    self.shared = Some(shared);
                    self.message =
                        "Waiting for an explicitly paired and approved viewer. No capture yet."
                            .into();
                }
                HostEvent::Stopped => {
                    self.shared = None;
                    self.message = "Sharing stopped. Tokens revoked and capture released.".into();
                }
                HostEvent::Error(e) => self.message = e,
                HostEvent::Saved => self.message = "Settings saved.".into(),
            }
        }
        let (available, restart) = {
            let s = self.updates.lock().unwrap();
            (
                s.available.as_ref().map(|a| a.version.to_string()),
                s.restart,
            )
        };
        if restart {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("SecondScreen").size(25.0).strong());
                ui.label(
                    egui::RichText::new("LOCAL PREVIEW")
                        .small()
                        .color(egui::Color32::from_rgb(106, 229, 189)),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.selectable_value(&mut self.tab, 2, "About");
                    ui.selectable_value(&mut self.tab, 1, "Updates");
                    ui.selectable_value(&mut self.tab, 0, "Connect");
                });
            });
            ui.add_space(8.0);
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if let Some(version) = available && ui.button(format!("Update {version} available — open Updates")).clicked() { self.tab = 1; }
                match self.tab {
                    0 => self.host_ui(ui),
                    1 => self.update_ui(ui),
                    _ => {
                        ui.heading("Room for another perspective.");
                        ui.label("SecondScreen is an independent, open-source local desktop mirror. No account, cloud relay, driver or remote input.");
                        ui.label("Extended desktop is not implemented. A real virtual monitor requires an IddCx/UMDF display driver with signing, deployment and hardware validation. This build does not create or configure Windows displays.");
                        ui.label("The capture path and tablet compatibility still require manual validation. GDI may not capture protected content, the secure desktop or the cursor.");
                        ui.label(format!("Version {VERSION} · MIT License"));
                        ui.hyperlink_to("Project repository (planned; may be unavailable)", "https://github.com/siarheisinelnikau/second-screen");
                    }
                }
            });
        });
        // Only a streaming host needs periodic small status refreshes. Idle UI is event driven.
        if self.shared.is_some() {
            ctx.request_repaint_after(Duration::from_secs(1));
        }
    }
}
impl Drop for App {
    fn drop(&mut self) {
        let _ = self.host_tx.send(HostCommand::Quit);
        if let Some(worker) = self.host_worker.take() {
            let _ = worker.join();
        }
    }
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "--apply-update") {
        if let Err(error) = update::apply_helper(&args[1..]) {
            update::record_helper_error(&error);
        }
        return Ok(());
    }
    let event_loop =
        winit::event_loop::EventLoop::<eframe::UserEvent>::with_user_event().build()?;
    let native = eframe::create_native(
        "SecondScreen",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([820.0, 760.0])
                .with_min_inner_size([640.0, 540.0]),
            ..Default::default()
        },
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
        &event_loop,
    );
    event_loop.run_app(&mut reactive::Reactive(native))?;
    Ok(())
}

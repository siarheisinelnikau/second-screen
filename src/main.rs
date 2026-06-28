#![cfg_attr(windows, windows_subsystem = "windows")]

mod host;

use second_screen::{
    VERSION,
    server::{Host, Shared},
    settings::{self, Settings},
    ui::{self, ConnectAction, HostView, UpdateView},
    update::{self, Available},
};
use std::{
    net::{Ipv4Addr, SocketAddrV4},
    sync::{Arc, Mutex},
    thread,
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
    Install(Box<Available>),
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
    fn new(ctx: &egui::Context) -> Self {
        ui::apply_theme(ctx);
        let (settings, message) = match settings::load(&settings::path()) {
            Ok(s) => (s, String::new()),
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
        let host_ctx = ctx.clone();
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
                                let repaint = host_ctx.clone();
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
                host_ctx.request_repaint();
            }
            drop(host);
        });
        let updates = Arc::new(Mutex::new(UpdateState {
            message: "No update check yet. Repository and releases may not be published.".into(),
            ..Default::default()
        }));
        let update_state = updates.clone();
        let update_ctx = ctx.clone();
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
                        let result = (|| {
                            let mut prepared = update::download(&available, |done, total| {
                                update_state.lock().unwrap().progress =
                                    Some(done as f32 / total as f32);
                                update_ctx.request_repaint();
                            })?;
                            update::launch_helper(&mut prepared)
                        })();
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
    fn host_ui(&mut self, surface: &mut egui::Ui) {
        let host = self.shared.as_ref().map(|shared| {
            let s = shared.state.lock().unwrap();
            HostView {
                address: self.address.clone(),
                code: s.pairing.code.clone(),
                peer: s.pairing.session.as_ref().map(|p| p.ip.to_string()),
                approved: s.pairing.session.as_ref().is_some_and(|p| p.approved),
                has_frame: s.frame.is_some(),
                stopped: s.stopped,
                error: s.error.clone(),
            }
        });
        let actions = ui::connect(
            surface,
            &mut self.settings,
            &mut self.trusted_network,
            host.as_ref(),
            self.busy,
            &self.message,
        );
        for action in actions {
            match action {
                ConnectAction::Start => self.send_host(HostCommand::Start(self.settings.clone())),
                ConnectAction::Stop => self.send_host(HostCommand::Stop),
                ConnectAction::Save => self.send_host(HostCommand::Save(self.settings.clone())),
                ConnectAction::Approve => {
                    if let Some(shared) = &self.shared {
                        shared.approve();
                        self.message.clear();
                    }
                }
                ConnectAction::Disconnect => {
                    if let Some(shared) = &self.shared {
                        shared.disconnect();
                        self.message.clear();
                    }
                }
                ConnectAction::CopyAddress => surface.ctx().copy_text(self.address.clone()),
            }
        }
    }
    fn update_ui(&mut self, surface: &mut egui::Ui) {
        let view = {
            let s = self.updates.lock().unwrap();
            UpdateView {
                message: s.message.clone(),
                busy: s.busy,
                available: s.available.as_ref().map(|a| a.version.to_string()),
                progress: s.progress,
                installer_error: self.installer_error.clone(),
            }
        };
        let actions = ui::updates(surface, &mut self.settings.startup_updates, &view);
        if actions.save {
            self.send_host(HostCommand::Save(self.settings.clone()));
        }
        if actions.check && self.update_tx.try_send(UpdateCommand::Check).is_ok() {
            self.updates.lock().unwrap().busy = true;
        }
        if actions.install {
            let available = self.updates.lock().unwrap().available.clone();
            if let Some(available) = available
                && self
                    .update_tx
                    .try_send(UpdateCommand::Install(Box::new(available)))
                    .is_ok()
            {
                self.updates.lock().unwrap().busy = true;
            }
        }
    }
}
impl host::Application for App {
    fn close_requested(&self) -> bool {
        self.updates.lock().unwrap().restart
    }
    fn logic(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.host_rx.try_recv() {
            self.busy = false;
            match event {
                HostEvent::Started(address, shared) => {
                    self.address = address;
                    self.shared = Some(shared);
                    self.message.clear();
                }
                HostEvent::Stopped => {
                    self.shared = None;
                    self.message = "Sharing stopped. Tokens revoked and capture released.".into();
                }
                HostEvent::Error(e) => self.message = e,
                HostEvent::Saved => self.message = "Settings saved.".into(),
            }
        }
        let restart = self.updates.lock().unwrap().restart;
        if restart {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
    fn ui(&mut self, ctx: &egui::Context) {
        let available = self
            .updates
            .lock()
            .unwrap()
            .available
            .as_ref()
            .map(|a| a.version.to_string());
        ui::header(ctx, &mut self.tab);
        egui::CentralPanel::default().frame(egui::Frame::new().fill(ui::BACKGROUND).inner_margin(24.0)).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                if let Some(version) = available && ui.button(format!("Update {version} available — open Updates")).clicked() { self.tab = 1; }
                match self.tab {
                    0 => self.host_ui(ui),
                    1 => self.update_ui(ui),
                    _ => {
                        ui.heading("Room for another perspective.");
                        ui.label("SecondScreen is an independent, open-source local desktop viewer. No account, cloud relay or remote input.");
                        ui.label("Primary monitor mode mirrors an existing screen. The experimental virtual-display backend requires a separately built, signed and installed SecondScreen IddCx/UMDF driver. It creates a monitor only after explicit pairing/approval and removes it on disconnect. End-to-end driver validation is pending; Windows desktop arrangement is never changed automatically.");
                        ui.label("The capture path and tablet compatibility still require manual validation. GDI may not capture protected content, the secure desktop or the cursor.");
                        ui.label(format!("Version {VERSION} · MIT License"));
                        ui.hyperlink_to("Project repository (planned; may be unavailable)", "https://github.com/siarheisinelnikau/second-screen");
                    }
                }
            });
        });
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

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|s| s == "--apply-update") {
        if let Err(error) = update::apply_helper() {
            update::record_helper_error(&error);
        }
        return Ok(());
    }
    host::run(Box::new(|ctx| Box::new(App::new(ctx))))
}

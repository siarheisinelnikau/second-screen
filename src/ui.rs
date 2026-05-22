//! Shared native UI components. Views emit actions; no filesystem, network or device I/O.
use crate::{VERSION, capture::SourceKind, settings::Settings};
use eframe::egui::{self, Color32, RichText};

pub const ACCENT: Color32 = Color32::from_rgb(106, 229, 189);
pub const MUTED: Color32 = Color32::from_rgb(168, 188, 204);
pub const WARNING: Color32 = Color32::from_rgb(229, 198, 142);
pub const ERROR: Color32 = Color32::from_rgb(255, 170, 174);
pub const BACKGROUND: Color32 = Color32::from_rgb(11, 20, 30);

pub fn apply_theme(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.override_text_color = Some(Color32::from_rgb(230, 240, 247));
    style.visuals.window_fill = Color32::from_rgb(18, 34, 49);
    style.visuals.extreme_bg_color = Color32::from_rgb(8, 16, 24);
    style.visuals.selection.bg_fill = Color32::from_rgb(30, 95, 83);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(35, 87, 77);
    style.spacing.item_spacing = egui::vec2(12.0, 10.0);
    style.spacing.button_padding = egui::vec2(16.0, 10.0);
    style.spacing.interact_size.y = 36.0;
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(25.0));
    ctx.set_style(style);
}

pub fn header(ctx: &egui::Context, tab: &mut u8) {
    egui::TopBottomPanel::top("header")
        .frame(egui::Frame::new().fill(BACKGROUND).inner_margin(20.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("SecondScreen").size(25.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.selectable_value(tab, 2, "About");
                    ui.selectable_value(tab, 1, "Updates");
                    ui.selectable_value(tab, 0, "Connect");
                });
            });
        });
}

#[derive(Clone, Copy)]
pub enum ConnectAction {
    Start,
    Stop,
    Save,
    Approve,
    Disconnect,
    CopyAddress,
}
#[derive(Default)]
pub struct HostView {
    pub address: String,
    pub code: String,
    pub peer: Option<String>,
    pub approved: bool,
    pub has_frame: bool,
    pub stopped: bool,
    pub error: Option<String>,
}
fn primary(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).strong().color(BACKGROUND)).fill(ACCENT),
    )
}
fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(MUTED));
}

pub fn connect(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    trusted: &mut bool,
    host: Option<&HostView>,
    busy: bool,
    message: &str,
) -> Vec<ConnectAction> {
    let mut actions = Vec::new();
    ui.heading("Connect your tablet");
    note(
        ui,
        "A second view of your work, on the same trusted network.",
    );
    ui.add_space(8.0);
    if let Some(host) = host {
        let state = if host.stopped {
            "Sharing stopped"
        } else if host.error.is_some() {
            "Sharing needs attention"
        } else if host.approved && host.has_frame {
            "Sharing with your tablet"
        } else if host.approved {
            "Waiting for desktop output"
        } else if host.peer.is_some() {
            "Your tablet is ready for approval"
        } else {
            "Ready to pair"
        };
        ui.label(
            RichText::new(state)
                .size(21.0)
                .strong()
                .color(if host.error.is_some() { ERROR } else { ACCENT }),
        );
        ui.label(settings.stream.source.mode_label());
        ui.add_space(4.0);
        ui.label("1. Open this address on your tablet");
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(&host.address).monospace().size(19.0));
            if ui.button("Copy address").clicked() {
                actions.push(ConnectAction::CopyAddress);
            }
        });
        if settings.bind_ip == "127.0.0.1" {
            ui.colored_label(WARNING, "Local-only address. To connect a tablet, stop sharing and enter your computer's private network address.");
        }
        ui.separator();
        if let Some(peer) = &host.peer {
            ui.label(format!("Tablet address: {peer}"));
            if !host.approved {
                ui.label("2. Check the tablet address, then approve it");
                let label = if settings.stream.source == SourceKind::VirtualDisplay {
                    "Approve & create virtual display"
                } else {
                    "Approve & start sharing"
                };
                if primary(ui, label, !busy).clicked() {
                    actions.push(ConnectAction::Approve);
                }
            } else if !host.has_frame && host.error.is_none() {
                note(
                    ui,
                    if settings.stream.source == SourceKind::VirtualDisplay {
                        "In Windows Display settings, choose Extend for the new virtual display."
                    } else {
                        "The first desktop frame will appear on your tablet shortly."
                    },
                );
            }
            if ui.button("Disconnect tablet").clicked() {
                actions.push(ConnectAction::Disconnect);
            }
        } else {
            ui.label("2. Enter this pairing code");
            ui.label(
                RichText::new(&host.code)
                    .monospace()
                    .size(32.0)
                    .color(ACCENT),
            );
            note(
                ui,
                "The code expires in five minutes. Approve the tablet here after entering it.",
            );
            if ui.button("New pairing code").clicked() {
                actions.push(ConnectAction::Disconnect);
            }
        }
        if let Some(error) = &host.error {
            ui.colored_label(ERROR, error);
            note(
                ui,
                "Stop sharing, resolve the issue and reconnect your tablet.",
            );
        }
        ui.separator();
        if ui
            .add_enabled(!busy, egui::Button::new("Stop sharing"))
            .clicked()
        {
            actions.push(ConnectAction::Stop);
        }
    } else {
        ui.add_enabled_ui(!busy, |ui| {
            ui.label(RichText::new("1. Choose a display").strong());
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut settings.stream.source, SourceKind::PrimaryMonitor, "Mirror monitor");
                #[cfg(all(windows, feature = "virtual-display"))]
                ui.selectable_value(&mut settings.stream.source, SourceKind::VirtualDisplay, "Extend desktop · experimental");
                ui.selectable_value(&mut settings.stream.source, SourceKind::Synthetic, "Test connection");
            });
            match settings.stream.source {
                SourceKind::PrimaryMonitor => note(ui, "Show your primary monitor on the tablet. No virtual display is created."),
                SourceKind::Synthetic => note(ui, "Send a moving test pattern to check the connection without sharing your screen."),
                SourceKind::VirtualDisplay => {
                    ui.colored_label(WARNING, "Requires the separate SecondScreen display driver. Device validation is pending.");
                    note(ui, "Approval creates one virtual monitor. Choose Extend in Windows yourself; the app never installs a driver or rearranges other displays.");
                }
            }
            ui.separator();
            ui.label(RichText::new("2. Set up the connection").strong());
            ui.horizontal(|ui| {
                ui.label("Computer address");
                ui.add(egui::TextEdit::singleline(&mut settings.bind_ip).desired_width(190.0).hint_text("192.168.1.20"));
            });
            note(ui, "Use your computer's private IPv4 address. 127.0.0.1 connects on this computer only.");
            ui.horizontal_wrapped(|ui| {
                ui.label("Picture");
                for (label, width, height, quality, fps) in [("Balanced",1280,720,75,10),("Sharper",1920,1080,85,15),("Smoother",1280,720,65,20)] {
                    let selected = (settings.stream.max_width, settings.stream.max_height, settings.stream.quality, settings.stream.fps) == (width,height,quality,fps);
                    if ui.selectable_label(selected, label).clicked() { settings.stream.max_width = width; settings.stream.max_height = height; settings.stream.quality = quality; settings.stream.fps = fps; }
                }
            });
            egui::CollapsingHeader::new(format!("Picture & network details · {} × {}", settings.stream.max_width, settings.stream.max_height)).id_salt("stream-details-header").show(ui, |ui| {
                egui::Grid::new("stream-details").spacing([20.0,12.0]).show(ui, |ui| {
                    ui.label("Width"); ui.add(egui::Slider::new(&mut settings.stream.max_width,320..=1920).suffix(" px")); ui.end_row();
                    ui.label("Height"); ui.add(egui::Slider::new(&mut settings.stream.max_height,240..=1080).suffix(" px")); ui.end_row();
                    ui.label("JPEG quality"); ui.add(egui::Slider::new(&mut settings.stream.quality,30..=90)); ui.end_row();
                    ui.label("Frame rate limit"); ui.add(egui::Slider::new(&mut settings.stream.fps,5..=20).suffix(" FPS")); ui.end_row();
                    ui.label("Port"); ui.add(egui::DragValue::new(&mut settings.port).range(1..=65535)); ui.end_row();
                });
                note(ui, "Mirror mode preserves aspect ratio. Extend mode uses the selected dimensions as its virtual display mode.");
            });
            ui.separator();
            ui.colored_label(WARNING, "HTTP is unencrypted. People on the network may be able to read your screen and pairing credentials.");
            ui.checkbox(trusted, "I understand and trust this network");
            ui.horizontal(|ui| {
                if primary(ui, if busy { "Starting…" } else { "Start sharing" }, *trusted && !busy).clicked() { actions.push(ConnectAction::Start); }
                if ui.button("Save settings").clicked() { actions.push(ConnectAction::Save); }
            });
        });
    }
    if !message.is_empty() {
        ui.add_space(8.0);
        note(ui, message);
    }
    actions
}

pub struct UpdateView {
    pub message: String,
    pub busy: bool,
    pub available: Option<String>,
    pub progress: Option<f32>,
    pub installer_error: Option<String>,
}
#[derive(Default)]
pub struct UpdateActions {
    pub save: bool,
    pub check: bool,
    pub install: bool,
}
pub fn updates(ui: &mut egui::Ui, startup: &mut bool, view: &UpdateView) -> UpdateActions {
    let mut actions = UpdateActions::default();
    ui.heading("Updates");
    note(
        ui,
        &format!("SecondScreen {VERSION} · Windows x64 portable"),
    );
    ui.add_space(12.0);
    actions.save = ui
        .checkbox(startup, "Check for updates on startup")
        .changed();
    note(
        ui,
        "Checks run in the background. Downloads and installation always need your action.",
    );
    ui.separator();
    ui.label(&view.message);
    if let Some(progress) = view.progress {
        ui.add(egui::ProgressBar::new(progress).show_percentage());
    }
    if let Some(version) = &view.available {
        ui.label(
            RichText::new(format!("Version {version} is ready"))
                .size(21.0)
                .color(ACCENT),
        );
        note(
            ui,
            "Installing disconnects the tablet and restarts the app. Your settings are preserved. Driver packages are managed separately.",
        );
        actions.install = primary(ui, "Download and install", !view.busy).clicked();
    }
    actions.check = ui
        .add_enabled(
            !view.busy,
            egui::Button::new(if view.busy {
                "Working…"
            } else {
                "Check for updates"
            }),
        )
        .clicked();
    if let Some(error) = &view.installer_error {
        ui.colored_label(ERROR, format!("Last installer error: {error}"));
    }
    actions
}

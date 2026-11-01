//! Shared native UI components. Views emit actions; no filesystem, network or device I/O.
use crate::{
    VERSION,
    capture::SourceKind,
    i18n::{self, Language, tr},
    settings::{Settings, Theme},
};
use egui::{Color32, RichText};

pub const BACKGROUND: Color32 = Color32::from_rgb(11, 20, 30);
fn accent(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(106, 229, 189)
    } else {
        Color32::from_rgb(7, 105, 79)
    }
}
fn muted(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(168, 188, 204)
    } else {
        Color32::from_rgb(66, 90, 96)
    }
}
fn warning(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(229, 198, 142)
    } else {
        Color32::from_rgb(128, 83, 12)
    }
}
fn error_color(ui: &egui::Ui) -> Color32 {
    if ui.visuals().dark_mode {
        Color32::from_rgb(255, 170, 174)
    } else {
        Color32::from_rgb(172, 34, 55)
    }
}

pub fn apply_theme(ctx: &egui::Context, theme: Theme) {
    ctx.set_theme(match theme {
        Theme::Dark => egui::Theme::Dark,
        Theme::Light => egui::Theme::Light,
    });
    let mut style = (*ctx.style()).clone();
    style.visuals = match theme {
        Theme::Dark => egui::Visuals::dark(),
        Theme::Light => egui::Visuals::light(),
    };
    match theme {
        Theme::Dark => {
            style.visuals.panel_fill = BACKGROUND;
            style.visuals.override_text_color = Some(Color32::from_rgb(230, 240, 247));
            style.visuals.window_fill = Color32::from_rgb(18, 34, 49);
            style.visuals.extreme_bg_color = Color32::from_rgb(8, 16, 24);
            style.visuals.selection.bg_fill = Color32::from_rgb(30, 95, 83);
            style.visuals.widgets.active.bg_fill = Color32::from_rgb(35, 87, 77);
        }
        Theme::Light => {
            style.visuals.panel_fill = Color32::from_rgb(243, 248, 246);
            style.visuals.override_text_color = Some(Color32::from_rgb(24, 48, 53));
            style.visuals.window_fill = Color32::WHITE;
            style.visuals.extreme_bg_color = Color32::from_rgb(229, 239, 235);
            style.visuals.selection.bg_fill = Color32::from_rgb(178, 224, 207);
            style.visuals.selection.stroke.color = Color32::from_rgb(18, 65, 49);
            style.visuals.widgets.active.bg_fill = Color32::from_rgb(182, 219, 205);
        }
    }
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

pub fn preferences(ui: &mut egui::Ui, settings: &mut Settings) -> bool {
    let language = settings.language;
    ui.heading(tr(language, "Settings"));
    let before = (settings.language, settings.theme);
    ui.horizontal_wrapped(|ui| {
        ui.label(tr(language, "Language"));
        egui::ComboBox::from_id_salt("interface-language")
            .selected_text(settings.language.native_name())
            .show_ui(ui, |ui| {
                for language in Language::ALL {
                    ui.selectable_value(&mut settings.language, language, language.native_name());
                }
            });
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(tr(language, "Appearance"));
        ui.selectable_value(&mut settings.theme, Theme::Light, tr(language, "Light"));
        ui.selectable_value(&mut settings.theme, Theme::Dark, tr(language, "Dark"));
    });
    note(ui, tr(language, "Changes are saved automatically."));
    ui.separator();
    before != (settings.language, settings.theme)
}

pub fn header(ctx: &egui::Context, tab: &mut u8, language: Language) {
    egui::TopBottomPanel::top("header")
        .frame(
            egui::Frame::new()
                .fill(ctx.style().visuals.panel_fill)
                .inner_margin(20.0),
        )
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("SecondScreen").size(25.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.selectable_value(tab, 2, tr(language, "About"));
                    ui.selectable_value(tab, 1, tr(language, "Settings"));
                    ui.selectable_value(tab, 0, tr(language, "Connect"));
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
        egui::Button::new(
            RichText::new(label)
                .strong()
                .color(if ui.visuals().dark_mode {
                    BACKGROUND
                } else {
                    Color32::WHITE
                }),
        )
        .fill(accent(ui)),
    )
}
fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).color(muted(ui)));
}

pub fn connect(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    trusted: &mut bool,
    host: Option<&HostView>,
    busy: bool,
    message: &str,
) -> Vec<ConnectAction> {
    let language = settings.language;
    let mut actions = Vec::new();
    ui.heading(tr(language, "Connect your tablet"));
    note(
        ui,
        tr(
            language,
            "A second view of your work, on the same trusted network.",
        ),
    );
    ui.add_space(8.0);
    if let Some(host) = host {
        let state = if host.stopped {
            tr(language, "Sharing stopped")
        } else if host.error.is_some() {
            tr(language, "Sharing needs attention")
        } else if host.approved && host.has_frame {
            tr(language, "Sharing with your tablet")
        } else if host.approved {
            tr(language, "Waiting for desktop output")
        } else if host.peer.is_some() {
            tr(language, "Your tablet is ready for approval")
        } else {
            tr(language, "Ready to pair")
        };
        ui.label(
            RichText::new(state)
                .size(21.0)
                .strong()
                .color(if host.error.is_some() {
                    error_color(ui)
                } else {
                    accent(ui)
                }),
        );
        ui.label(tr(language, settings.stream.source.mode_label()));
        ui.add_space(4.0);
        ui.label(tr(language, "1. Open this address on your tablet"));
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(&host.address).monospace().size(19.0));
            if ui.button(tr(language, "Copy address")).clicked() {
                actions.push(ConnectAction::CopyAddress);
            }
        });
        if settings.bind_ip == "127.0.0.1" {
            ui.colored_label(warning(ui), tr(language, "Local-only address. To connect a tablet, stop sharing and enter your computer's private network address."));
        }
        ui.separator();
        if let Some(peer) = &host.peer {
            ui.label(i18n::format(
                language,
                "Tablet address: {peer}",
                &[("peer", peer)],
            ));
            if !host.approved {
                ui.label(tr(language, "2. Check the tablet address, then approve it"));
                let label = if settings.stream.source == SourceKind::VirtualDisplay {
                    tr(language, "Approve & create virtual display")
                } else {
                    tr(language, "Approve & start sharing")
                };
                if primary(ui, label, !busy).clicked() {
                    actions.push(ConnectAction::Approve);
                }
            } else if !host.has_frame && host.error.is_none() {
                note(
                    ui,
                    if settings.stream.source == SourceKind::VirtualDisplay {
                        tr(
                            language,
                            "In Windows Display settings, choose Extend for the new virtual display.",
                        )
                    } else {
                        tr(
                            language,
                            "The first desktop frame will appear on your tablet shortly.",
                        )
                    },
                );
            }
            if ui.button(tr(language, "Disconnect tablet")).clicked() {
                actions.push(ConnectAction::Disconnect);
            }
        } else {
            ui.label(tr(language, "2. Enter this pairing code"));
            ui.label(
                RichText::new(&host.code)
                    .monospace()
                    .size(32.0)
                    .color(accent(ui)),
            );
            note(
                ui,
                tr(
                    language,
                    "The code expires in five minutes. Approve the tablet here after entering it.",
                ),
            );
            if ui.button(tr(language, "New pairing code")).clicked() {
                actions.push(ConnectAction::Disconnect);
            }
        }
        if let Some(error) = &host.error {
            ui.colored_label(
                error_color(ui),
                i18n::format(
                    language,
                    "Sharing error: {detail}",
                    &[("detail", tr(language, error))],
                ),
            );
            note(
                ui,
                tr(
                    language,
                    "Stop sharing, resolve the issue and reconnect your tablet.",
                ),
            );
        }
        ui.separator();
        if ui
            .add_enabled(!busy, egui::Button::new(tr(language, "Stop sharing")))
            .clicked()
        {
            actions.push(ConnectAction::Stop);
        }
    } else {
        ui.add_enabled_ui(!busy, |ui| {
            ui.label(RichText::new(tr(language, "1. Choose a display")).strong());
            ui.horizontal_wrapped(|ui| {
                ui.selectable_value(&mut settings.stream.source, SourceKind::PrimaryMonitor, tr(language, "Mirror monitor"));
                #[cfg(all(windows, feature = "virtual-display"))]
                ui.selectable_value(&mut settings.stream.source, SourceKind::VirtualDisplay, tr(language, "Extend desktop · experimental"));
                ui.selectable_value(&mut settings.stream.source, SourceKind::Synthetic, tr(language, "Test connection"));
            });
            match settings.stream.source {
                SourceKind::PrimaryMonitor => note(ui, tr(language, "Show your primary monitor on the tablet. No virtual display is created.")),
                SourceKind::Synthetic => note(ui, tr(language, "Send a moving test pattern to check the connection without sharing your screen.")),
                SourceKind::VirtualDisplay => {
                    ui.colored_label(warning(ui), tr(language, "Requires the separate SecondScreen display driver. Device validation is pending."));
                    note(ui, tr(language, "Approval creates one virtual monitor. Choose Extend in Windows yourself; the app never installs a driver or rearranges other displays."));
                }
            }
            ui.separator();
            ui.label(RichText::new(tr(language, "2. Set up the connection")).strong());
            ui.horizontal(|ui| {
                ui.label(tr(language, "Computer address"));
                ui.add(egui::TextEdit::singleline(&mut settings.bind_ip).desired_width(190.0).hint_text("192.168.1.20"));
            });
            note(ui, tr(language, "Use your computer's private IPv4 address. 127.0.0.1 connects on this computer only."));
            ui.horizontal_wrapped(|ui| {
                ui.label(tr(language, "Picture"));
                for (label, width, height, quality, fps) in [(tr(language, "Balanced"),1280,720,75,10),(tr(language, "Sharper"),1920,1080,85,15),(tr(language, "Smoother"),1280,720,65,20)] {
                    let selected = (settings.stream.max_width, settings.stream.max_height, settings.stream.quality, settings.stream.fps) == (width,height,quality,fps);
                    if ui.selectable_label(selected, label).clicked() { settings.stream.max_width = width; settings.stream.max_height = height; settings.stream.quality = quality; settings.stream.fps = fps; }
                }
            });
            egui::CollapsingHeader::new(i18n::format(language, "Picture & network details · {width} × {height}", &[("width", &settings.stream.max_width.to_string()), ("height", &settings.stream.max_height.to_string())])).id_salt("stream-details-header").show(ui, |ui| {
                egui::Grid::new("stream-details").spacing([20.0,12.0]).show(ui, |ui| {
                    ui.label(tr(language, "Width")); ui.add(egui::Slider::new(&mut settings.stream.max_width,320..=1920).suffix(" px")); ui.end_row();
                    ui.label(tr(language, "Height")); ui.add(egui::Slider::new(&mut settings.stream.max_height,240..=1080).suffix(" px")); ui.end_row();
                    ui.label(tr(language, "JPEG quality")); ui.add(egui::Slider::new(&mut settings.stream.quality,30..=90)); ui.end_row();
                    ui.label(tr(language, "Frame rate limit")); ui.add(egui::Slider::new(&mut settings.stream.fps,5..=20).suffix(" FPS")); ui.end_row();
                    ui.label(tr(language, "Port")); ui.add(egui::DragValue::new(&mut settings.port).range(1..=65535)); ui.end_row();
                });
                note(ui, tr(language, "Mirror mode preserves aspect ratio. Extend mode uses the selected dimensions as its virtual display mode."));
            });
            ui.separator();
            ui.colored_label(warning(ui), tr(language, "HTTP is unencrypted. People on the network may be able to read your screen and pairing credentials."));
            ui.checkbox(trusted, tr(language, "I understand and trust this network"));
            ui.horizontal(|ui| {
                if primary(ui, if busy { tr(language, "Starting…") } else { tr(language, "Start sharing") }, *trusted && !busy).clicked() { actions.push(ConnectAction::Start); }
                if ui.button(tr(language, "Save settings")).clicked() { actions.push(ConnectAction::Save); }
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
pub fn updates(
    ui: &mut egui::Ui,
    startup: &mut bool,
    view: &UpdateView,
    language: Language,
) -> UpdateActions {
    let mut actions = UpdateActions::default();
    ui.heading(tr(language, "Updates"));
    note(
        ui,
        &i18n::format(
            language,
            "SecondScreen {version} · Windows x64 portable",
            &[("version", VERSION)],
        ),
    );
    ui.add_space(12.0);
    actions.save = ui
        .checkbox(startup, tr(language, "Check for updates on startup"))
        .changed();
    note(
        ui,
        tr(
            language,
            "Checks run in the background. Downloads and installation always need your action.",
        ),
    );
    ui.separator();
    ui.label(&view.message);
    if let Some(progress) = view.progress {
        ui.add(egui::ProgressBar::new(progress).show_percentage());
    }
    if let Some(version) = &view.available {
        ui.label(
            RichText::new(i18n::format(
                language,
                "Version {version} is ready",
                &[("version", version)],
            ))
            .size(21.0)
            .color(accent(ui)),
        );
        note(
            ui,
            tr(
                language,
                "Installing disconnects the tablet and restarts the app. Your settings are preserved. Driver packages are managed separately.",
            ),
        );
        actions.install = primary(ui, tr(language, "Download and install"), !view.busy).clicked();
    }
    actions.check = ui
        .add_enabled(
            !view.busy,
            egui::Button::new(if view.busy {
                tr(language, "Working…")
            } else {
                tr(language, "Check for updates")
            }),
        )
        .clicked();
    if let Some(error) = &view.installer_error {
        ui.colored_label(
            error_color(ui),
            i18n::format(
                language,
                "Last installer error: {detail}",
                &[("detail", tr(language, error))],
            ),
        );
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_appearance_survives_opposite_system_theme_and_mode_changes() {
        let ctx = egui::Context::default();
        for requested in [Theme::Light, Theme::Dark, Theme::Light] {
            apply_theme(&ctx, requested);
            for system in [egui::Theme::Dark, egui::Theme::Light] {
                let _ = ctx.run(
                    egui::RawInput {
                        system_theme: Some(system),
                        ..Default::default()
                    },
                    |ctx| {
                        assert_eq!(ctx.style().visuals.dark_mode, requested == Theme::Dark);
                        assert_eq!(
                            ctx.style().visuals.panel_fill,
                            match requested {
                                Theme::Dark => BACKGROUND,
                                Theme::Light => Color32::from_rgb(243, 248, 246),
                            }
                        );
                    },
                );
            }
        }
    }
}

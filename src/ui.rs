//! The launcher's window shell: menu bar, toolbar, instance tree, properties
//! panel, console, and status bar — styled after the old Eclipse/SWT look
//! (flat gradient header bars, a 22px icon toolbar, a white console inset).
//!
//! The mod installer is wired up for real: installs run on a background tokio
//! runtime and report progress back to the console through a channel, so the
//! immediate-mode UI never blocks on network I/O. Everything else (launching,
//! auth, instance persistence) is still sample data over `todo!()` stubs.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Response, Sense, Shape, Stroke, Vec2,
};

use crate::config::LauncherConfig;
use crate::curseforge::CurseForgeClient;
use crate::instance::Instance;
use crate::java::JavaRuntime;
use crate::modrinth::ModrinthClient;
use crate::mods::{self, ModLoader, ModReference, Provider};

/// Palette lifted from the mockup, so the app and the design stay in step.
mod theme {
    use eframe::egui::Color32;

    pub const WIN_BG: Color32 = Color32::from_rgb(0xEC, 0xEC, 0xEC);
    pub const PANEL_BG: Color32 = Color32::from_rgb(0xF6, 0xF6, 0xF3);
    pub const BORDER: Color32 = Color32::from_rgb(0xAC, 0xA8, 0x99);
    pub const HEADER_TOP: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
    pub const HEADER_BOT: Color32 = Color32::from_rgb(0xD8, 0xD4, 0xC4);
    pub const TOOLBAR_BOT: Color32 = Color32::from_rgb(0xE8, 0xE5, 0xD8);
    pub const SELECT_BG: Color32 = Color32::from_rgb(0x31, 0x6A, 0xC5);
    pub const SELECT_TEXT: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
    pub const TEXT: Color32 = Color32::from_rgb(0x1E, 0x1E, 0x1E);
    pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x5A, 0x5A, 0x52);
    pub const TEXT_DISABLED: Color32 = Color32::from_rgb(0xA0, 0x9D, 0x93);
    pub const CONSOLE_BG: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
    pub const CONSOLE_TEXT: Color32 = Color32::from_rgb(0x33, 0x33, 0x33);
    pub const ACTIVE_BG: Color32 = Color32::from_rgb(0xDC, 0xE7, 0xFF);
    pub const GREEN: Color32 = Color32::from_rgb(0x2E, 0x7D, 0x32);
    pub const RED: Color32 = Color32::from_rgb(0xB0, 0x00, 0x20);
}

const HEADER_H: f32 = 19.0;
const ROW_H: f32 = 18.0;

/// Messages sent from a background install task back to the UI thread.
enum InstallEvent {
    Log(String),
    Finished(Result<Vec<PathBuf>, String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Icon {
    New,
    Play,
    Stop,
    Refresh,
    Package,
    Gear,
    Folder,
}

pub struct LauncherApp {
    instances: Vec<Instance>,
    selected: usize,
    console: Vec<String>,

    /// Mod-install dialog state.
    mods_open: bool,
    provider: Provider,
    project_id_input: String,
    queue: Vec<ModReference>,

    /// Background install plumbing.
    installing: bool,
    events: Option<Receiver<InstallEvent>>,
    runtime: tokio::runtime::Runtime,

    curseforge_key: Option<String>,
}

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([760.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Worst Minecraft Launcher",
        options,
        Box::new(|cc| {
            apply_toolbox_style(&cc.egui_ctx);
            Ok(Box::new(LauncherApp::new()))
        }),
    )
}

// ---------------------------------------------------------------- painting --

/// A vertical two-stop gradient, as used by the header and toolbar strips.
fn gradient_rect(painter: &egui::Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 3, 2);
    painter.add(mesh);
}

fn draw_icon(p: &egui::Painter, r: Rect, icon: Icon, color: Color32) {
    let stroke = Stroke::new(1.4, color);
    let c = r.center();
    match icon {
        Icon::New => {
            p.line_segment(
                [Pos2::new(c.x, r.top()), Pos2::new(c.x, r.bottom())],
                stroke,
            );
            p.line_segment(
                [Pos2::new(r.left(), c.y), Pos2::new(r.right(), c.y)],
                stroke,
            );
        }
        Icon::Play => {
            p.add(Shape::convex_polygon(
                vec![
                    Pos2::new(r.left() + 1.5, r.top()),
                    Pos2::new(r.right() - 0.5, c.y),
                    Pos2::new(r.left() + 1.5, r.bottom()),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Stop => {
            p.rect_filled(
                Rect::from_center_size(c, Vec2::splat(r.width() * 0.72)),
                0.0,
                color,
            );
        }
        Icon::Refresh => {
            let radius = r.width() * 0.40;
            let points: Vec<Pos2> = (0..=20)
                .map(|i| {
                    let t = 0.7 + (i as f32 / 20.0) * std::f32::consts::TAU * 0.78;
                    Pos2::new(c.x + radius * t.cos(), c.y + radius * t.sin())
                })
                .collect();
            p.add(Shape::line(points, stroke));
            // Arrowhead at the open end of the arc.
            let tip = Pos2::new(c.x + radius * 0.7f32.cos(), c.y + radius * 0.7f32.sin());
            p.add(Shape::convex_polygon(
                vec![
                    tip + Vec2::new(2.5, 1.0),
                    tip + Vec2::new(-2.0, 2.0),
                    tip + Vec2::new(0.5, -2.5),
                ],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Package => {
            let body = Rect::from_min_max(
                Pos2::new(r.left(), r.top() + 2.0),
                Pos2::new(r.right(), r.bottom()),
            );
            p.rect_stroke(body, 0.0, stroke);
            p.line_segment(
                [Pos2::new(c.x, body.top()), Pos2::new(c.x, body.bottom())],
                stroke,
            );
        }
        Icon::Gear => {
            p.circle_stroke(c, r.width() * 0.26, stroke);
            for i in 0..6 {
                let t = std::f32::consts::TAU * (i as f32 / 6.0);
                let (sin, cos) = t.sin_cos();
                p.line_segment(
                    [
                        Pos2::new(c.x + cos * r.width() * 0.36, c.y + sin * r.width() * 0.36),
                        Pos2::new(c.x + cos * r.width() * 0.52, c.y + sin * r.width() * 0.52),
                    ],
                    stroke,
                );
            }
        }
        Icon::Folder => {
            let body = Rect::from_min_max(
                Pos2::new(r.left(), r.top() + 3.0),
                Pos2::new(r.right(), r.bottom()),
            );
            p.rect_stroke(body, 0.0, stroke);
            p.add(Shape::line(
                vec![
                    Pos2::new(r.left() + 0.5, r.top() + 3.0),
                    Pos2::new(r.left() + 3.0, r.top()),
                    Pos2::new(r.left() + 6.5, r.top()),
                    Pos2::new(r.left() + 6.5, r.top() + 3.0),
                ],
                stroke,
            ));
        }
    }
}

/// Full-width gradient strip with a bold caption — the panel titles.
fn panel_header(ui: &mut egui::Ui, text: &str) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, HEADER_H), Sense::hover());
    let painter = ui.painter();
    gradient_rect(painter, rect, theme::HEADER_TOP, theme::HEADER_BOT);
    painter.line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, theme::BORDER),
    );
    painter.text(
        rect.left_center() + Vec2::new(7.0, 0.0),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(11.0),
        theme::TEXT,
    );
}

/// A 22px square toolbar button drawn from vector shapes, no widget chrome.
fn icon_button(ui: &mut egui::Ui, icon: Icon, enabled: bool, highlighted: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::splat(22.0),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );

    let hovered = enabled && response.hovered();
    if highlighted || hovered {
        ui.painter().rect_filled(rect, 2.0, theme::ACTIVE_BG);
        ui.painter()
            .rect_stroke(rect, 2.0, Stroke::new(1.0, theme::BORDER));
    }

    let color = if !enabled {
        theme::TEXT_DISABLED
    } else if icon == Icon::Play {
        theme::GREEN
    } else {
        theme::TEXT
    };
    draw_icon(
        ui.painter(),
        Rect::from_center_size(rect.center(), Vec2::splat(13.0)),
        icon,
        color,
    );
    response
}

/// One row in the instance tree: folder icon plus name, full-width selection.
fn instance_row(ui: &mut egui::Ui, name: &str, selected: bool) -> Response {
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, ROW_H), Sense::click());

    if selected {
        ui.painter().rect_filled(rect, 0.0, theme::SELECT_BG);
    }
    let fg = if selected {
        theme::SELECT_TEXT
    } else {
        theme::TEXT
    };
    draw_icon(
        ui.painter(),
        Rect::from_center_size(rect.left_center() + Vec2::new(14.0, 0.0), Vec2::splat(11.0)),
        Icon::Folder,
        fg,
    );
    ui.painter().text(
        rect.left_center() + Vec2::new(26.0, 0.0),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(11.0),
        fg,
    );
    response
}

fn property_row(ui: &mut egui::Ui, label: &str, value: &str, value_color: Color32) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(78.0, 14.0), Sense::hover());
        ui.painter().text(
            rect.left_center(),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(11.0),
            theme::TEXT_MUTED,
        );
        ui.colored_label(value_color, value);
    });
}

// -------------------------------------------------------------------- state --

/// `HH:MM:SS` in UTC. Good enough for a console log, and avoids pulling in a
/// date/time crate just to prefix lines.
fn timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!(
        "{:02}:{:02}:{:02}",
        (secs / 3600) % 24,
        (secs / 60) % 60,
        secs % 60
    )
}

fn slug(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

/// Sample instances, until `instance::list_instances()` is implemented.
/// Their directories are real paths under `./instances`, so installing mods
/// into one actually writes files there.
fn sample_instances(base: &std::path::Path) -> Vec<Instance> {
    let make = |name: &str, version: &str, java: &str, loader: Option<ModLoader>| Instance {
        name: name.to_string(),
        version_id: version.to_string(),
        dir: base.join(slug(name)),
        account_uuid: "00000000-0000-0000-0000-000000000000".to_string(),
        java: JavaRuntime {
            path: PathBuf::from("(managed)"),
            version: java.to_string(),
        },
        mod_loader: loader,
    };

    vec![
        make("Vanilla 1.21.4", "1.21.4", "Temurin 21.0.2", None),
        make(
            "Fabric 1.21.4",
            "1.21.4",
            "Temurin 21.0.2",
            Some(ModLoader::Fabric),
        ),
        make(
            "Fabric 1.20.1",
            "1.20.1",
            "Temurin 17.0.9",
            Some(ModLoader::Fabric),
        ),
    ]
}

impl LauncherApp {
    fn new() -> Self {
        let base = std::env::current_dir()
            .unwrap_or_default()
            .join("instances");
        let curseforge_key = LauncherConfig::curseforge_api_key_from_env();

        let mut console = vec![
            format!("[{}] Worst Minecraft Launcher ready.", timestamp()),
            format!("[{}] Instances directory: {}", timestamp(), base.display()),
        ];
        console.push(match &curseforge_key {
            Some(_) => format!(
                "[{}] CurseForge API key loaded from WML_CURSEFORGE_API_KEY.",
                timestamp()
            ),
            None => format!(
                "[{}] No CurseForge API key (set WML_CURSEFORGE_API_KEY); Modrinth still works.",
                timestamp()
            ),
        });

        Self {
            instances: sample_instances(&base),
            selected: 0,
            console,
            mods_open: false,
            provider: Provider::Modrinth,
            project_id_input: String::new(),
            queue: Vec::new(),
            installing: false,
            events: None,
            runtime: tokio::runtime::Runtime::new().expect("failed to start tokio runtime"),
            curseforge_key,
        }
    }

    fn log(&mut self, message: impl Into<String>) {
        let line = format!("[{}] {}", timestamp(), message.into());
        log::info!("{line}");
        self.console.push(line);
        // Keep the console from growing without bound over a long session.
        if self.console.len() > 500 {
            self.console.drain(..self.console.len() - 500);
        }
    }

    /// Kicks off `mods::install` on the tokio runtime, streaming progress back
    /// over a channel. Returns immediately; the UI stays responsive.
    fn start_install(&mut self, ctx: &egui::Context) {
        let instance = self.instances[self.selected].clone();
        let roots = self.queue.clone();
        let key = self.curseforge_key.clone();
        let ctx = ctx.clone();
        let (tx, rx): (Sender<InstallEvent>, Receiver<InstallEvent>) = channel();

        self.events = Some(rx);
        self.installing = true;
        self.log(format!(
            "Installing {} mod(s) into {}",
            roots.len(),
            instance.dir.join("mods").display()
        ));

        self.runtime.spawn(async move {
            let modrinth = ModrinthClient::new();
            let curseforge = key.map(|k| CurseForgeClient::new(Some(k)));

            let _ = tx.send(InstallEvent::Log(
                "Resolving mods and required dependencies...".to_string(),
            ));
            let result = mods::install(&instance, &roots, &modrinth, curseforge.as_ref()).await;
            let _ = tx.send(InstallEvent::Finished(result.map_err(|e| e.to_string())));
            ctx.request_repaint();
        });
    }

    /// Drains anything the background task has sent since the last frame.
    fn pump_events(&mut self) {
        let Some(rx) = &self.events else { return };

        let mut drained = Vec::new();
        while let Ok(event) = rx.try_recv() {
            drained.push(event);
        }

        for event in drained {
            match event {
                InstallEvent::Log(message) => self.log(message),
                InstallEvent::Finished(Ok(paths)) => {
                    self.installing = false;
                    self.events = None;
                    self.log(format!("Installed {} file(s):", paths.len()));
                    for path in paths {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| path.display().to_string());
                        self.log(format!("  {name}"));
                    }
                    self.queue.clear();
                }
                InstallEvent::Finished(Err(message)) => {
                    self.installing = false;
                    self.events = None;
                    log::error!("install failed: {message}");
                    self.log(format!("Install failed: {message}"));
                }
            }
        }
    }

    fn mods_window(&mut self, ctx: &egui::Context) {
        let mut open = self.mods_open;
        let mut install_clicked = false;

        egui::Window::new("Install mods")
            .open(&mut open)
            .resizable(true)
            .default_width(420.0)
            .show(ctx, |ui| {
                let instance = &self.instances[self.selected];
                ui.label(format!(
                    "Target: {} ({}, {})",
                    instance.name,
                    instance.version_id,
                    match instance.mod_loader {
                        Some(loader) => loader.to_string(),
                        None => "vanilla — mods not supported".to_string(),
                    }
                ));
                ui.separator();

                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.provider, Provider::Modrinth, "Modrinth");
                    ui.radio_value(&mut self.provider, Provider::CurseForge, "CurseForge");
                });

                if self.provider == Provider::CurseForge && self.curseforge_key.is_none() {
                    ui.colored_label(
                        theme::RED,
                        "No API key set — CurseForge installs will fail.",
                    );
                }

                ui.horizontal(|ui| {
                    let hint = match self.provider {
                        Provider::Modrinth => "project slug or id (e.g. fabric-api)",
                        Provider::CurseForge => "numeric mod id (e.g. 306612)",
                    };
                    ui.label("Mod:");
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.project_id_input).hint_text(hint),
                    );
                    let submitted =
                        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    if (ui.button("Add").clicked() || submitted)
                        && !self.project_id_input.trim().is_empty()
                    {
                        let id = self.project_id_input.trim().to_string();
                        let reference = match self.provider {
                            Provider::Modrinth => ModReference::modrinth(id),
                            Provider::CurseForge => ModReference {
                                provider: Provider::CurseForge,
                                project_id: id,
                            },
                        };
                        if !self.queue.contains(&reference) {
                            self.queue.push(reference);
                        }
                        self.project_id_input.clear();
                    }
                });

                ui.separator();
                if self.queue.is_empty() {
                    ui.weak("Nothing queued. Required dependencies are resolved automatically.");
                } else {
                    let mut remove = None;
                    for (i, reference) in self.queue.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.small_button("x").clicked() {
                                remove = Some(i);
                            }
                            ui.label(format!("{} — {}", reference.provider, reference.project_id));
                        });
                    }
                    if let Some(i) = remove {
                        self.queue.remove(i);
                    }
                }

                ui.separator();
                ui.horizontal(|ui| {
                    let can_install = !self.queue.is_empty() && !self.installing;
                    if ui
                        .add_enabled(can_install, egui::Button::new("Install"))
                        .clicked()
                    {
                        install_clicked = true;
                    }
                    if self.installing {
                        ui.spinner();
                        ui.label("Installing...");
                    }
                });
            });

        self.mods_open = open;
        if install_clicked {
            self.start_install(ctx);
        }
    }
}

fn apply_toolbox_style(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.window_fill = theme::WIN_BG;
    visuals.panel_fill = theme::WIN_BG;
    visuals.override_text_color = Some(theme::TEXT);
    visuals.widgets.noninteractive.bg_fill = theme::PANEL_BG;
    visuals.widgets.inactive.bg_fill = theme::TOOLBAR_BOT;
    visuals.widgets.inactive.weak_bg_fill = theme::TOOLBAR_BOT;
    visuals.widgets.hovered.bg_fill = theme::ACTIVE_BG;
    visuals.widgets.hovered.weak_bg_fill = theme::ACTIVE_BG;
    visuals.widgets.active.bg_fill = theme::SELECT_BG;
    visuals.selection.bg_fill = theme::SELECT_BG;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        widget.rounding = egui::Rounding::ZERO;
        widget.bg_stroke = Stroke::new(1.0, theme::BORDER);
    }
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = Vec2::new(5.0, 3.0);
    style.spacing.button_padding = Vec2::new(6.0, 2.0);
    style.spacing.menu_margin = egui::Margin::same(3.0);
    style.text_styles = [
        (
            egui::TextStyle::Body,
            FontId::new(11.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Button,
            FontId::new(11.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Small,
            FontId::new(10.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Heading,
            FontId::new(12.0, egui::FontFamily::Proportional),
        ),
        (
            egui::TextStyle::Monospace,
            FontId::new(10.5, egui::FontFamily::Monospace),
        ),
    ]
    .into();
    ctx.set_style(style);
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump_events();
        if self.installing {
            // Keep repainting so the spinner animates and log lines appear
            // while the background task is running.
            ctx.request_repaint_after(Duration::from_millis(150));
        }

        // -- menu bar ---------------------------------------------------------
        egui::TopBottomPanel::top("menu_bar")
            .exact_height(21.0)
            .frame(
                egui::Frame::none()
                    .fill(theme::WIN_BG)
                    .inner_margin(egui::Margin::symmetric(6.0, 2.0)),
            )
            .show(ctx, |ui| {
                egui::menu::bar(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        if ui.button("Exit").clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    });
                    ui.menu_button("Edit", |_ui| {});
                    ui.menu_button("Instance", |ui| {
                        if ui.button("Install mods...").clicked() {
                            self.mods_open = true;
                            ui.close_menu();
                        }
                    });
                    ui.menu_button("Help", |_ui| {});
                });
            });

        // -- icon toolbar -----------------------------------------------------
        egui::TopBottomPanel::top("toolbar")
            .exact_height(28.0)
            .frame(egui::Frame::none().inner_margin(egui::Margin::symmetric(5.0, 3.0)))
            .show(ctx, |ui| {
                let strip = ui.max_rect();
                gradient_rect(ui.painter(), strip, theme::HEADER_TOP, theme::TOOLBAR_BOT);
                ui.painter().line_segment(
                    [strip.left_bottom(), strip.right_bottom()],
                    Stroke::new(1.0, theme::BORDER),
                );

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    if icon_button(ui, Icon::New, true, false).clicked() {
                        self.log("Creating instances is not implemented yet.");
                    }
                    if icon_button(ui, Icon::Play, true, true).clicked() {
                        self.log("Launch is not implemented yet.");
                    }
                    icon_button(ui, Icon::Stop, false, false);

                    let sep = ui.available_rect_before_wrap();
                    ui.add_space(6.0);
                    ui.painter().line_segment(
                        [
                            Pos2::new(sep.left() + 3.0, sep.center().y - 8.0),
                            Pos2::new(sep.left() + 3.0, sep.center().y + 8.0),
                        ],
                        Stroke::new(1.0, theme::BORDER),
                    );

                    if icon_button(ui, Icon::Refresh, true, false).clicked() {
                        self.log("Refreshed instance list.");
                    }
                    if icon_button(ui, Icon::Package, true, false).clicked() {
                        self.mods_open = true;
                    }
                    let _ = icon_button(ui, Icon::Gear, true, false);
                });
            });

        // -- status bar -------------------------------------------------------
        egui::TopBottomPanel::bottom("status_bar")
            .exact_height(18.0)
            .frame(
                egui::Frame::none()
                    .fill(theme::WIN_BG)
                    .inner_margin(egui::Margin::symmetric(8.0, 2.0)),
            )
            .show(ctx, |ui| {
                let top = ui.max_rect();
                ui.painter().line_segment(
                    [top.left_top(), top.right_top()],
                    Stroke::new(1.0, theme::BORDER),
                );
                ui.horizontal(|ui| {
                    ui.colored_label(
                        theme::TEXT_MUTED,
                        if self.installing {
                            "Installing..."
                        } else {
                            "Ready."
                        },
                    );
                    ui.colored_label(theme::BORDER, "|");
                    ui.colored_label(
                        theme::TEXT_MUTED,
                        format!("{} instances", self.instances.len()),
                    );
                    ui.colored_label(theme::BORDER, "|");
                    ui.colored_label(
                        theme::TEXT_MUTED,
                        if self.curseforge_key.is_some() {
                            "CurseForge: key set"
                        } else {
                            "CurseForge: no key"
                        },
                    );
                });
            });

        // -- instance tree ----------------------------------------------------
        egui::SidePanel::left("instances")
            .resizable(true)
            .default_width(168.0)
            .frame(egui::Frame::none().fill(theme::PANEL_BG))
            .show(ctx, |ui| {
                let panel = ui.max_rect();
                ui.painter().line_segment(
                    [panel.right_top(), panel.right_bottom()],
                    Stroke::new(1.0, theme::BORDER),
                );

                panel_header(ui, "Instances");
                ui.add_space(3.0);
                ui.spacing_mut().item_spacing.y = 0.0;
                for i in 0..self.instances.len() {
                    let name = self.instances[i].name.clone();
                    if instance_row(ui, &name, i == self.selected).clicked() {
                        self.selected = i;
                    }
                }
            });

        // -- properties + console --------------------------------------------
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(theme::WIN_BG))
            .show(ctx, |ui| {
                let selected = &self.instances[self.selected];
                let (name, version, java, loader) = (
                    selected.name.clone(),
                    selected.version_id.clone(),
                    selected.java.version.clone(),
                    selected.mod_loader,
                );

                panel_header(ui, &format!("Properties — {name}"));
                let props = egui::Frame::none()
                    .fill(theme::PANEL_BG)
                    .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                    .show(ui, |ui| {
                        // Fill the panel's full width, not just the text width.
                        ui.set_min_width(ui.available_width());
                        ui.spacing_mut().item_spacing.y = 5.0;
                        property_row(ui, "Account:", "NotSteve_47", theme::TEXT);
                        property_row(ui, "Version:", &version, theme::TEXT);
                        property_row(ui, "Java:", &format!("{java} (managed)"), theme::TEXT);
                        property_row(
                            ui,
                            "Loader:",
                            &match loader {
                                Some(loader) => loader.to_string(),
                                None => "vanilla (no mods)".to_string(),
                            },
                            theme::TEXT,
                        );
                        property_row(
                            ui,
                            "Status:",
                            if self.installing {
                                "Installing..."
                            } else {
                                "Ready."
                            },
                            theme::GREEN,
                        );
                    });
                ui.painter().line_segment(
                    [
                        props.response.rect.left_bottom(),
                        props.response.rect.right_bottom(),
                    ],
                    Stroke::new(1.0, theme::BORDER),
                );

                panel_header(ui, "Console");
                let remaining = ui.available_size();
                egui::Frame::none()
                    .fill(theme::CONSOLE_BG)
                    .inner_margin(egui::Margin::symmetric(10.0, 6.0))
                    .show(ui, |ui| {
                        ui.set_min_size(remaining);
                        egui::ScrollArea::vertical()
                            .stick_to_bottom(true)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                for line in &self.console {
                                    ui.label(
                                        egui::RichText::new(line)
                                            .monospace()
                                            .color(theme::CONSOLE_TEXT),
                                    );
                                }
                            });
                    });
            });

        self.mods_window(ctx);
    }
}

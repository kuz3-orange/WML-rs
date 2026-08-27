//! The launcher's window shell: menu bar, toolbar, instance tree, properties
//! panel, console, and status bar — styled after the old Eclipse/SWT look.
//!
//! The mod installer is wired up for real: installs run on a background tokio
//! runtime and report progress back to the console through a channel, so the
//! immediate-mode UI never blocks on network I/O. Everything else (launching,
//! auth, instance persistence) is still sample data over `todo!()` stubs.

use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::Duration;

use eframe::egui::{self, Color32, Rounding, Stroke};

use crate::config::LauncherConfig;
use crate::curseforge::CurseForgeClient;
use crate::instance::Instance;
use crate::java::JavaRuntime;
use crate::modrinth::ModrinthClient;
use crate::mods::{self, ModLoader, ModReference, Provider};

/// Messages sent from a background install task back to the UI thread.
enum InstallEvent {
    Log(String),
    Finished(Result<Vec<PathBuf>, String>),
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
            curseforge_key: curseforge_key.clone(),
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
                "Resolving mods and required dependencies…".to_string(),
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
                        Color32::from_rgb(0xB0, 0x00, 0x20),
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
                            if ui.small_button("✕").clicked() {
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
                        ui.label("Installing…");
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
    visuals.window_fill = Color32::from_rgb(0xEC, 0xEC, 0xEC);
    visuals.panel_fill = Color32::from_rgb(0xEC, 0xEC, 0xEC);
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(0xF6, 0xF6, 0xF3);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(0xE8, 0xE5, 0xD8);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(0xDC, 0xE7, 0xFF);
    visuals.widgets.active.bg_fill = Color32::from_rgb(0x31, 0x6A, 0xC5);
    visuals.selection.bg_fill = Color32::from_rgb(0x31, 0x6A, 0xC5);
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    for widget in [
        &mut visuals.widgets.noninteractive,
        &mut visuals.widgets.inactive,
        &mut visuals.widgets.hovered,
        &mut visuals.widgets.active,
    ] {
        widget.rounding = Rounding::ZERO;
        widget.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0xAC, 0xA8, 0x99));
    }
    ctx.set_visuals(visuals);

    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(5.0, 3.0);
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

        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Exit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |_ui| {});
                ui.menu_button("Instance", |ui| {
                    if ui.button("Install mods…").clicked() {
                        self.mods_open = true;
                        ui.close_menu();
                    }
                });
                ui.menu_button("Help", |_ui| {});
            });
        });

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(2.0);
                let _ = ui.button("New");
                if ui.button("Launch").clicked() {
                    self.log("Launch is not implemented yet.");
                }
                ui.add_enabled(false, egui::Button::new("Stop"));
                ui.separator();
                if ui.button("Mods").clicked() {
                    self.mods_open = true;
                }
                let _ = ui.button("Settings");
            });
            ui.add_space(1.0);
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(if self.installing {
                    "Installing…"
                } else {
                    "Ready."
                });
                ui.separator();
                ui.label(format!("{} instances", self.instances.len()));
                ui.separator();
                ui.label(if self.curseforge_key.is_some() {
                    "CurseForge: key set"
                } else {
                    "CurseForge: no key"
                });
            });
        });

        egui::SidePanel::left("instances")
            .resizable(true)
            .default_width(170.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Instances").strong());
                ui.separator();
                for (i, instance) in self.instances.iter().enumerate() {
                    ui.selectable_value(&mut self.selected, i, &instance.name);
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let selected = &self.instances[self.selected];
            let (name, version, java, loader) = (
                selected.name.clone(),
                selected.version_id.clone(),
                selected.java.version.clone(),
                selected.mod_loader,
            );

            ui.label(egui::RichText::new(format!("Properties — {name}")).strong());
            ui.separator();
            egui::Grid::new("properties").num_columns(2).show(ui, |ui| {
                ui.label("Account:");
                ui.label("NotSteve_47");
                ui.end_row();
                ui.label("Version:");
                ui.label(&version);
                ui.end_row();
                ui.label("Java:");
                ui.label(format!("{java} (managed)"));
                ui.end_row();
                ui.label("Loader:");
                match loader {
                    Some(loader) => ui.label(loader.to_string()),
                    None => ui.label("vanilla (no mods)"),
                };
                ui.end_row();
            });

            ui.add_space(8.0);
            ui.label(egui::RichText::new("Console").strong());
            ui.separator();
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.style_mut().override_font_id = Some(egui::FontId::monospace(11.0));
                    for line in &self.console {
                        ui.label(line);
                    }
                });
        });

        self.mods_window(ctx);
    }
}

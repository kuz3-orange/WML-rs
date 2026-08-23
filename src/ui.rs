//! The launcher's window shell: menu bar, toolbar, instance tree, properties
//! panel, console, and status bar — styled after the old Eclipse/SWT look.
//! Static/non-functional for now; nothing here calls into the backend modules yet.

use eframe::egui::{self, Color32, Rounding, Stroke};

struct Instance {
    name: &'static str,
    account: &'static str,
    version: &'static str,
    java: &'static str,
    last_played: &'static str,
    playtime: &'static str,
}

const INSTANCES: &[Instance] = &[
    Instance {
        name: "Vanilla 1.21.4",
        account: "NotSteve_47",
        version: "1.21.4 (release)",
        java: "Temurin 21.0.2 (managed)",
        last_played: "2 days ago",
        playtime: "41h 12m total",
    },
    Instance {
        name: "Fabric 1.20.1 — Modpack Experiment",
        account: "NotSteve_47",
        version: "1.20.1",
        java: "Temurin 17.0.9 (managed)",
        last_played: "3 weeks ago",
        playtime: "6h 03m total",
    },
    Instance {
        name: "Snapshot 24w51a",
        account: "TestAlt_12",
        version: "24w51a",
        java: "Temurin 21.0.2 (managed)",
        last_played: "2 months ago",
        playtime: "0h 40m total",
    },
];

#[derive(Default)]
pub struct LauncherApp {
    selected: usize,
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
            Ok(Box::new(LauncherApp::default()))
        }),
    )
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
        egui::TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("Exit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });
                ui.menu_button("Edit", |_ui| {});
                ui.menu_button("Instance", |_ui| {});
                ui.menu_button("Help", |_ui| {});
            });
        });

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(2.0);
                let _ = ui.button("New");
                let _ = ui.add_enabled(true, egui::Button::new("Launch"));
                ui.add_enabled(false, egui::Button::new("Stop"));
                ui.separator();
                let _ = ui.button("Refresh");
                let _ = ui.button("Settings");
            });
            ui.add_space(1.0);
        });

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Ready.");
                ui.separator();
                ui.label(format!("{} instances", INSTANCES.len()));
            });
        });

        egui::SidePanel::left("instances")
            .resizable(true)
            .default_width(170.0)
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("Instances").strong());
                ui.separator();
                for (i, inst) in INSTANCES.iter().enumerate() {
                    ui.selectable_value(&mut self.selected, i, inst.name);
                }
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let selected = &INSTANCES[self.selected];

            ui.label(egui::RichText::new(format!("Properties — {}", selected.name)).strong());
            ui.separator();
            egui::Grid::new("properties").num_columns(2).show(ui, |ui| {
                ui.label("Account:");
                ui.label(selected.account);
                ui.end_row();
                ui.label("Version:");
                ui.label(selected.version);
                ui.end_row();
                ui.label("Java:");
                ui.label(selected.java);
                ui.end_row();
                ui.label("Last played:");
                ui.label(format!("{} — {}", selected.last_played, selected.playtime));
                ui.end_row();
                ui.label("Status:");
                ui.colored_label(Color32::from_rgb(0x2E, 0x7D, 0x32), "Ready.");
                ui.end_row();
            });

            ui.add_space(8.0);
            ui.label(egui::RichText::new("Console").strong());
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.style_mut().override_font_id = Some(egui::FontId::monospace(11.0));
                ui.label("[11:04:02] Verifying assets... done");
                ui.label(format!("[11:04:03] Managed Java OK: {}", selected.java));
                ui.label("[11:04:04] Waiting for you to click Launch.");
            });
        });
    }
}

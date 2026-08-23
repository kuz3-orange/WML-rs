// Debug builds keep the default "console" subsystem (a cmd window on
// Windows shows our timestamped log output); release builds switch to
// "windows" to suppress it. No-op on non-Windows targets.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code, unused_variables)]

mod auth;
mod config;
mod curseforge;
mod instance;
mod java;
mod launch;
mod modrinth;
mod mods;
mod ui;

fn init_logging() {
    let default_level = if cfg!(debug_assertions) {
        "debug"
    } else {
        "warn"
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(default_level))
        .format_timestamp_millis()
        .init();
}

fn main() -> eframe::Result<()> {
    init_logging();
    log::info!(
        "Worst Minecraft Launcher v{} starting ({})",
        env!("CARGO_PKG_VERSION"),
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
    ui::run()
}

//! Launcher settings: install directories, JVM/memory args, and user preferences.

use std::path::PathBuf;

pub struct LauncherConfig {
    pub game_dir: PathBuf,
    pub java_path: Option<PathBuf>,
    pub jvm_args: Vec<String>,
    pub max_memory_mb: u32,
    /// CurseForge requires a per-application API key (console.curseforge.com)
    /// — there's no way around this. Read from `WML_CURSEFORGE_API_KEY` if
    /// set, otherwise falls back to whatever `load()` reads from disk.
    pub curseforge_api_key: Option<String>,
}

impl LauncherConfig {
    pub fn load() -> Self {
        todo!("read config from disk, falling back to defaults, then apply curseforge_api_key_from_env() as an override")
    }

    /// `WML_CURSEFORGE_API_KEY`, if set — intended to override whatever
    /// `load()` eventually reads from disk for this field.
    pub fn curseforge_api_key_from_env() -> Option<String> {
        std::env::var("WML_CURSEFORGE_API_KEY").ok()
    }

    pub fn save(&self) {
        todo!("persist config to disk")
    }
}

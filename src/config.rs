//! Launcher settings: install directories, JVM/memory args, and user preferences.

use std::path::PathBuf;

pub struct LauncherConfig {
    pub game_dir: PathBuf,
    pub java_path: Option<PathBuf>,
    pub jvm_args: Vec<String>,
    pub max_memory_mb: u32,
}

impl LauncherConfig {
    pub fn load() -> Self {
        todo!("read config from disk, falling back to defaults")
    }

    pub fn save(&self) {
        todo!("persist config to disk")
    }
}

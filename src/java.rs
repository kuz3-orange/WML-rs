//! Java runtime discovery and management.

use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct JavaRuntime {
    pub path: PathBuf,
    pub version: String,
}

pub fn discover_installed() -> Vec<JavaRuntime> {
    todo!("scan common install locations / PATH for usable JREs")
}

pub async fn download_runtime(major_version: u32) -> Result<JavaRuntime, JavaError> {
    todo!("fetch a matching JRE (e.g. via Mojang's runtime manifest) if none found")
}

pub enum JavaError {
    NoCompatibleRuntime,
    DownloadFailed,
}

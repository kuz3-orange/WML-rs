//! Version manifest, and asset/library metadata from Mojang's APIs.

pub struct VersionInfo {
    pub id: String,
    pub release_type: String,
    pub url: String,
}

pub async fn fetch_version_manifest() -> Result<Vec<VersionInfo>, MojangError> {
    todo!("GET the version_manifest_v2.json and parse it")
}

pub async fn fetch_version_details(version: &VersionInfo) -> Result<VersionDetails, MojangError> {
    todo!("GET the per-version json: libraries, assets index, main class, etc.")
}

pub struct VersionDetails {
    pub main_class: String,
    pub libraries: Vec<String>,
    pub asset_index_url: String,
}

pub enum MojangError {
    NetworkError,
    ParseError,
}

//! Client for the CurseForge API. Unlike Modrinth, this requires a
//! per-application API key issued by Overwolf/CurseForge
//! (<https://console.curseforge.com>) — there's no way to use this client
//! without the caller supplying one (e.g. via `WML_CURSEFORGE_API_KEY`).

use serde::Deserialize;

const DEFAULT_BASE_URL: &str = "https://api.curseforge.com/v1";

/// CurseForge's `relationType` value for a required dependency.
const RELATION_REQUIRED_DEPENDENCY: u8 = 3;

pub struct CurseForgeClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

impl CurseForgeClient {
    pub fn new(api_key: Option<String>) -> Self {
        Self::with_base_url(DEFAULT_BASE_URL, api_key)
    }

    pub fn with_base_url(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into(),
            api_key,
        }
    }

    /// All files for `mod_id` compatible with the given loader and game version.
    /// `mod_loader_type` is CurseForge's numeric loader id (0=Any, 1=Forge, 4=Fabric,
    /// 5=Quilt, 6=NeoForge — see `crate::mods::ModLoader::curseforge_id`).
    pub async fn get_files(
        &self,
        mod_id: u32,
        mod_loader_type: u8,
        game_version: &str,
    ) -> Result<Vec<File>, CurseForgeError> {
        let api_key = self
            .api_key
            .as_deref()
            .ok_or(CurseForgeError::MissingApiKey)?;
        let url = format!("{}/mods/{}/files", self.base_url, mod_id);
        log::debug!("curseforge: fetching files for mod {mod_id} (loader={mod_loader_type}, game_version={game_version})");

        let response = self
            .http
            .get(&url)
            .header("x-api-key", api_key)
            .query(&[
                ("gameVersion", game_version.to_string()),
                ("modLoaderType", mod_loader_type.to_string()),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            log::warn!("curseforge: {url} returned {}", response.status());
            return Err(CurseForgeError::Status(response.status()));
        }

        let wrapper: FilesResponse = response.json().await?;
        log::debug!(
            "curseforge: mod {mod_id} has {} matching file(s)",
            wrapper.data.len()
        );
        Ok(wrapper.data)
    }

    /// The most recently dated file for `mod_id` matching the loader and game version.
    pub async fn best_file(
        &self,
        mod_id: u32,
        mod_loader_type: u8,
        game_version: &str,
    ) -> Result<File, CurseForgeError> {
        let mut files = self
            .get_files(mod_id, mod_loader_type, game_version)
            .await?;
        files.sort_by(|a, b| b.file_date.cmp(&a.file_date));
        files
            .into_iter()
            .next()
            .ok_or(CurseForgeError::NoCompatibleFile(mod_id))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CurseForgeError {
    #[error("no CurseForge API key configured (set WML_CURSEFORGE_API_KEY)")]
    MissingApiKey,
    #[error("curseforge request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("curseforge returned status {0}")]
    Status(reqwest::StatusCode),
    #[error("no file for curseforge mod {0} matches the requested loader/game version")]
    NoCompatibleFile(u32),
}

#[derive(Debug, Clone, Deserialize)]
struct FilesResponse {
    data: Vec<File>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct File {
    pub id: u32,
    #[serde(rename = "modId")]
    pub mod_id: u32,
    #[serde(rename = "fileName")]
    pub file_name: String,
    #[serde(rename = "fileDate")]
    pub file_date: String,
    #[serde(rename = "downloadUrl")]
    pub download_url: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<FileDependency>,
    #[serde(default)]
    pub hashes: Vec<FileHash>,
}

impl File {
    /// CurseForge's SHA-1 hash entry for this file, if it published one.
    pub fn sha1(&self) -> Option<&str> {
        const ALGO_SHA1: u8 = 1;
        self.hashes
            .iter()
            .find(|h| h.algo == ALGO_SHA1)
            .map(|h| h.value.as_str())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileDependency {
    #[serde(rename = "modId")]
    pub mod_id: u32,
    #[serde(rename = "relationType")]
    pub relation_type: u8,
}

impl FileDependency {
    pub fn is_required(&self) -> bool {
        self.relation_type == RELATION_REQUIRED_DEPENDENCY
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FileHash {
    pub value: String,
    pub algo: u8,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn file_json(id: u32, file_date: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "modId": 6789,
            "fileName": format!("file-{id}.jar"),
            "fileDate": file_date,
            "downloadUrl": format!("https://example.com/file-{id}.jar"),
            "dependencies": [{"modId": 111, "relationType": 3}, {"modId": 222, "relationType": 2}],
            "hashes": [{"value": "abc123", "algo": 1}]
        })
    }

    #[tokio::test]
    async fn missing_api_key_errors_without_a_network_call() {
        let client = CurseForgeClient::with_base_url("https://example.invalid", None);
        let err = client.get_files(6789, 4, "1.21.4").await.unwrap_err();
        assert!(matches!(err, CurseForgeError::MissingApiKey));
    }

    #[tokio::test]
    async fn best_file_picks_the_most_recently_dated_and_sends_the_api_key() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/mods/6789/files"))
            .and(header("x-api-key", "test-key"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [
                    file_json(1, "2024-01-01T00:00:00.000Z"),
                    file_json(2, "2024-06-01T00:00:00.000Z"),
                ]
            })))
            .mount(&server)
            .await;

        let client = CurseForgeClient::with_base_url(server.uri(), Some("test-key".to_string()));
        let file = client.best_file(6789, 4, "1.21.4").await.unwrap();

        assert_eq!(file.id, 2);
        assert_eq!(file.sha1(), Some("abc123"));
        assert!(file
            .dependencies
            .iter()
            .any(|d| d.mod_id == 111 && d.is_required()));
        assert!(file
            .dependencies
            .iter()
            .any(|d| d.mod_id == 222 && !d.is_required()));
    }

    /// Hits the real CurseForge API. Ignored by default since it needs network
    /// access and a key: run with
    /// `WML_CURSEFORGE_API_KEY=... cargo test -- --ignored live_`
    #[tokio::test]
    #[ignore = "requires network access and WML_CURSEFORGE_API_KEY"]
    async fn live_fetches_files_for_a_known_mod() {
        let key = std::env::var("WML_CURSEFORGE_API_KEY")
            .expect("set WML_CURSEFORGE_API_KEY to run the live test");
        let client = CurseForgeClient::new(Some(key));

        // 306612 = Fabric API, which has Fabric builds for many game versions.
        let file = client.best_file(306612, 4, "1.21.4").await.unwrap();

        assert_eq!(file.mod_id, 306612);
        assert!(file.file_name.ends_with(".jar"), "got {}", file.file_name);
        assert!(file.download_url.is_some(), "expected a download URL");
    }
}

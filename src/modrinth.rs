//! Client for the public Modrinth API (labrinth). No API key required.
//! <https://docs.modrinth.com/api/>

use serde::Deserialize;

const DEFAULT_BASE_URL: &str = "https://api.modrinth.com/v2";

pub struct ModrinthClient {
    http: reqwest::Client,
    base_url: String,
}

impl Default for ModrinthClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ModrinthClient {
    pub fn new() -> Self {
        Self::with_base_url(DEFAULT_BASE_URL)
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: base_url.into(),
        }
    }

    /// All versions of `project_id` compatible with the given loader and game version.
    pub async fn get_versions(
        &self,
        project_id: &str,
        loader: &str,
        game_version: &str,
    ) -> Result<Vec<Version>, ModrinthError> {
        let url = format!("{}/project/{}/version", self.base_url, project_id);
        log::debug!("modrinth: fetching versions for {project_id} (loader={loader}, game_version={game_version})");

        let response = self
            .http
            .get(&url)
            .query(&[
                ("loaders", format!("[\"{loader}\"]")),
                ("game_versions", format!("[\"{game_version}\"]")),
            ])
            .send()
            .await?;

        if !response.status().is_success() {
            log::warn!("modrinth: {url} returned {}", response.status());
            return Err(ModrinthError::Status(response.status()));
        }

        let versions: Vec<Version> = response.json().await?;
        log::debug!(
            "modrinth: {project_id} has {} matching version(s)",
            versions.len()
        );
        Ok(versions)
    }

    /// The most recently published version of `project_id` matching the loader and game version.
    pub async fn best_version(
        &self,
        project_id: &str,
        loader: &str,
        game_version: &str,
    ) -> Result<Version, ModrinthError> {
        let mut versions = self.get_versions(project_id, loader, game_version).await?;
        versions.sort_by(|a, b| b.date_published.cmp(&a.date_published));
        versions
            .into_iter()
            .next()
            .ok_or_else(|| ModrinthError::NoCompatibleVersion(project_id.to_string()))
    }

    /// Fetch a single version by id — used to resolve a dependency that only names a `version_id`.
    pub async fn get_version(&self, version_id: &str) -> Result<Version, ModrinthError> {
        let url = format!("{}/version/{}", self.base_url, version_id);
        let response = self.http.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(ModrinthError::Status(response.status()));
        }
        Ok(response.json().await?)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ModrinthError {
    #[error("modrinth request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("modrinth returned status {0}")]
    Status(reqwest::StatusCode),
    #[error("no version of {0} matches the requested loader/game version")]
    NoCompatibleVersion(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct Version {
    pub id: String,
    pub project_id: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub date_published: String,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
    pub files: Vec<VersionFile>,
}

impl Version {
    /// The file to download for this version — the one marked `primary`, or the first listed.
    pub fn primary_file(&self) -> Option<&VersionFile> {
        self.files
            .iter()
            .find(|f| f.primary)
            .or_else(|| self.files.first())
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct VersionFile {
    pub url: String,
    pub filename: String,
    #[serde(default)]
    pub primary: bool,
    #[serde(default)]
    pub hashes: FileHashes,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FileHashes {
    pub sha1: Option<String>,
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Dependency {
    pub version_id: Option<String>,
    pub project_id: Option<String>,
    pub dependency_type: DependencyType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyType {
    Required,
    Optional,
    Incompatible,
    Embedded,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn version_json(id: &str, date_published: &str) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "project_id": "p1",
            "version_number": "1.0",
            "game_versions": ["1.21.4"],
            "loaders": ["fabric"],
            "date_published": date_published,
            "dependencies": [],
            "files": [{
                "url": format!("https://example.com/{id}.jar"),
                "filename": format!("{id}.jar"),
                "primary": true,
                "hashes": {"sha1": "abc123"}
            }]
        })
    }

    #[tokio::test]
    async fn best_version_picks_the_most_recently_published() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/project/p1/version"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                version_json("older", "2024-01-01T00:00:00Z"),
                version_json("newer", "2024-06-01T00:00:00Z"),
            ])))
            .mount(&server)
            .await;

        let client = ModrinthClient::with_base_url(server.uri());
        let version = client.best_version("p1", "fabric", "1.21.4").await.unwrap();

        assert_eq!(version.id, "newer");
        assert_eq!(version.primary_file().unwrap().filename, "newer.jar");
    }

    #[tokio::test]
    async fn no_matching_versions_is_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/project/p1/version"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let client = ModrinthClient::with_base_url(server.uri());
        let err = client
            .best_version("p1", "fabric", "1.21.4")
            .await
            .unwrap_err();

        assert!(matches!(err, ModrinthError::NoCompatibleVersion(id) if id == "p1"));
    }

    /// Hits the real Modrinth API (no key needed). Ignored by default since it
    /// needs network access: `cargo test -- --ignored live_`
    #[tokio::test]
    #[ignore = "requires network access"]
    async fn live_fetches_fabric_api_versions() {
        let client = ModrinthClient::new();
        let version = client
            .best_version("fabric-api", "fabric", "1.21.4")
            .await
            .unwrap();

        assert!(version.loaders.contains(&"fabric".to_string()));
        assert!(version.game_versions.contains(&"1.21.4".to_string()));
        let file = version.primary_file().expect("expected a primary file");
        assert!(file.filename.ends_with(".jar"), "got {}", file.filename);
    }
}

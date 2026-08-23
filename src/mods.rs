//! Resolves mods (and their required dependencies) from Modrinth and
//! CurseForge, and installs them into an instance's `mods/` directory.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};

use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;

use crate::curseforge::{CurseForgeClient, CurseForgeError};
use crate::instance::Instance;
use crate::modrinth::{DependencyType, ModrinthClient, ModrinthError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModLoader {
    Fabric,
    Forge,
    Quilt,
    NeoForge,
}

impl ModLoader {
    /// Modrinth's lowercase loader id, as used in its `loaders` filter.
    pub fn modrinth_id(self) -> &'static str {
        match self {
            ModLoader::Fabric => "fabric",
            ModLoader::Forge => "forge",
            ModLoader::Quilt => "quilt",
            ModLoader::NeoForge => "neoforge",
        }
    }

    /// CurseForge's numeric `modLoaderType` id.
    pub fn curseforge_id(self) -> u8 {
        match self {
            ModLoader::Forge => 1,
            ModLoader::Fabric => 4,
            ModLoader::Quilt => 5,
            ModLoader::NeoForge => 6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    Modrinth,
    CurseForge,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModReference {
    pub provider: Provider,
    pub project_id: String,
}

impl ModReference {
    pub fn modrinth(project_id: impl Into<String>) -> Self {
        Self {
            provider: Provider::Modrinth,
            project_id: project_id.into(),
        }
    }

    pub fn curseforge(mod_id: u32) -> Self {
        Self {
            provider: Provider::CurseForge,
            project_id: mod_id.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedMod {
    pub reference: ModReference,
    pub version_id: String,
    pub filename: String,
    pub download_url: String,
    pub sha1: Option<String>,
    pub dependencies: Vec<ModReference>,
}

#[derive(Debug, thiserror::Error)]
pub enum ModError {
    #[error(transparent)]
    Modrinth(#[from] ModrinthError),
    #[error(transparent)]
    CurseForge(#[from] CurseForgeError),
    #[error("invalid curseforge mod id: {0}")]
    InvalidCurseForgeId(String),
    #[error("{0:?} was requested but no CurseForge API client is configured")]
    CurseForgeNotConfigured(ModReference),
    #[error("this instance has no mod loader — mods can't be installed on a vanilla instance")]
    VanillaInstance,
    #[error("http request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("downloaded file {0} does not match its published hash")]
    HashMismatch(String),
}

/// Resolves `roots` and the full closure of their *required* dependencies.
/// Optional, incompatible, and embedded dependencies are not followed.
/// Diamond dependencies (two mods requiring the same dependency) resolve once.
pub async fn resolve(
    roots: &[ModReference],
    game_version: &str,
    loader: ModLoader,
    modrinth: &ModrinthClient,
    curseforge: Option<&CurseForgeClient>,
) -> Result<Vec<ResolvedMod>, ModError> {
    let mut resolved: HashMap<ModReference, ResolvedMod> = HashMap::new();
    let mut queue: VecDeque<ModReference> = roots.iter().cloned().collect();

    while let Some(reference) = queue.pop_front() {
        if resolved.contains_key(&reference) {
            continue;
        }

        let resolved_mod = match reference.provider {
            Provider::Modrinth => {
                resolve_modrinth(modrinth, &reference, loader, game_version).await?
            }
            Provider::CurseForge => {
                let client = curseforge
                    .ok_or_else(|| ModError::CurseForgeNotConfigured(reference.clone()))?;
                resolve_curseforge(client, &reference, loader, game_version).await?
            }
        };

        log::info!(
            "resolved {:?}:{} -> {} ({} required dependency(s))",
            resolved_mod.reference.provider,
            resolved_mod.reference.project_id,
            resolved_mod.filename,
            resolved_mod.dependencies.len(),
        );

        for dep in &resolved_mod.dependencies {
            if !resolved.contains_key(dep) {
                queue.push_back(dep.clone());
            }
        }
        resolved.insert(reference, resolved_mod);
    }

    Ok(resolved.into_values().collect())
}

async fn resolve_modrinth(
    client: &ModrinthClient,
    reference: &ModReference,
    loader: ModLoader,
    game_version: &str,
) -> Result<ResolvedMod, ModError> {
    let version = client
        .best_version(&reference.project_id, loader.modrinth_id(), game_version)
        .await?;

    let file = version
        .primary_file()
        .ok_or_else(|| ModrinthError::NoCompatibleVersion(reference.project_id.clone()))?;

    let mut dependencies = Vec::new();
    for dep in &version.dependencies {
        if dep.dependency_type != DependencyType::Required {
            continue;
        }
        if let Some(project_id) = &dep.project_id {
            dependencies.push(ModReference::modrinth(project_id.clone()));
        } else if let Some(version_id) = &dep.version_id {
            // Only a version_id was given — resolve its project so this
            // dependency goes through the same loader/game-version matching
            // as everything else, rather than pinning that exact version.
            let dep_version = client.get_version(version_id).await?;
            dependencies.push(ModReference::modrinth(dep_version.project_id));
        }
    }

    Ok(ResolvedMod {
        reference: reference.clone(),
        version_id: version.id.clone(),
        filename: file.filename.clone(),
        download_url: file.url.clone(),
        sha1: file.hashes.sha1.clone(),
        dependencies,
    })
}

async fn resolve_curseforge(
    client: &CurseForgeClient,
    reference: &ModReference,
    loader: ModLoader,
    game_version: &str,
) -> Result<ResolvedMod, ModError> {
    let mod_id: u32 = reference
        .project_id
        .parse()
        .map_err(|_| ModError::InvalidCurseForgeId(reference.project_id.clone()))?;

    let file = client
        .best_file(mod_id, loader.curseforge_id(), game_version)
        .await?;
    let download_url = file
        .download_url
        .clone()
        .ok_or(CurseForgeError::NoCompatibleFile(mod_id))?;

    let dependencies = file
        .dependencies
        .iter()
        .filter(|d| d.is_required())
        .map(|d| ModReference::curseforge(d.mod_id))
        .collect();

    Ok(ResolvedMod {
        reference: reference.clone(),
        version_id: file.id.to_string(),
        filename: file.file_name.clone(),
        download_url,
        sha1: file.sha1().map(str::to_string),
        dependencies,
    })
}

/// Resolves `roots` and downloads the full set into `instance`'s `mods/`
/// directory, verifying each file's SHA-1 when the provider published one.
/// A file already present with a matching hash is not re-downloaded.
pub async fn install(
    instance: &Instance,
    roots: &[ModReference],
    modrinth: &ModrinthClient,
    curseforge: Option<&CurseForgeClient>,
) -> Result<Vec<PathBuf>, ModError> {
    let loader = instance.mod_loader.ok_or(ModError::VanillaInstance)?;
    let resolved = resolve(roots, &instance.version_id, loader, modrinth, curseforge).await?;

    let mods_dir = instance.dir.join("mods");
    tokio::fs::create_dir_all(&mods_dir).await?;

    let http = reqwest::Client::new();
    let mut installed = Vec::with_capacity(resolved.len());

    for m in &resolved {
        let dest = mods_dir.join(&m.filename);

        if dest.exists() {
            match &m.sha1 {
                Some(expected) if file_sha1(&dest).await? == *expected => {
                    log::info!("{} already installed, skipping download", m.filename);
                    installed.push(dest);
                    continue;
                }
                Some(_) => {} // stale/corrupt — fall through and re-download
                None => {
                    installed.push(dest);
                    continue;
                }
            }
        }

        log::info!("downloading {} from {}", m.filename, m.download_url);
        download_file(&http, &m.download_url, &dest).await?;

        if let Some(expected) = &m.sha1 {
            let actual = file_sha1(&dest).await?;
            if actual != *expected {
                log::error!(
                    "hash mismatch for {}: expected {expected}, got {actual}",
                    m.filename
                );
                return Err(ModError::HashMismatch(m.filename.clone()));
            }
        }
        installed.push(dest);
    }

    Ok(installed)
}

async fn download_file(http: &reqwest::Client, url: &str, dest: &Path) -> Result<(), ModError> {
    let bytes = http
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let mut file = tokio::fs::File::create(dest).await?;
    file.write_all(&bytes).await?;
    Ok(())
}

async fn file_sha1(path: &Path) -> Result<String, ModError> {
    let bytes = tokio::fs::read(path).await?;
    let mut hasher = Sha1::new();
    hasher.update(&bytes);
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::java::JavaRuntime;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn modrinth_version_json(
        id: &str,
        project_id: &str,
        dependencies: serde_json::Value,
    ) -> serde_json::Value {
        serde_json::json!({
            "id": id,
            "project_id": project_id,
            "version_number": "1.0",
            "game_versions": ["1.21.4"],
            "loaders": ["fabric"],
            "date_published": "2024-06-01T00:00:00Z",
            "dependencies": dependencies,
            "files": [{
                "url": format!("{id}-url"),
                "filename": format!("{id}.jar"),
                "primary": true,
                "hashes": {"sha1": null}
            }]
        })
    }

    fn test_instance(dir: PathBuf, loader: Option<ModLoader>) -> Instance {
        Instance {
            name: "Test Instance".to_string(),
            version_id: "1.21.4".to_string(),
            dir,
            account_uuid: "00000000-0000-0000-0000-000000000000".to_string(),
            java: JavaRuntime {
                path: PathBuf::from("/usr/bin/java"),
                version: "21".to_string(),
            },
            mod_loader: loader,
        }
    }

    #[tokio::test]
    async fn resolve_follows_required_deps_and_dedupes_diamonds() {
        let server = MockServer::start().await;

        // root requires A and B; A also requires B (diamond) — B should
        // appear exactly once in the resolved set.
        Mock::given(method("GET"))
            .and(path("/project/root/version"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                modrinth_version_json(
                    "root-v",
                    "root",
                    serde_json::json!([
                        {"project_id": "a", "version_id": null, "dependency_type": "required"},
                        {"project_id": "b", "version_id": null, "dependency_type": "required"},
                        {"project_id": "c", "version_id": null, "dependency_type": "optional"},
                    ])
                )
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/project/a/version"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                modrinth_version_json(
                    "a-v",
                    "a",
                    serde_json::json!([
                        {"project_id": "b", "version_id": null, "dependency_type": "required"},
                    ])
                )
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/project/b/version"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
                modrinth_version_json("b-v", "b", serde_json::json!([]))
            ])))
            .mount(&server)
            .await;

        let modrinth = ModrinthClient::with_base_url(server.uri());
        let resolved = resolve(
            &[ModReference::modrinth("root")],
            "1.21.4",
            ModLoader::Fabric,
            &modrinth,
            None,
        )
        .await
        .unwrap();

        let ids: Vec<&str> = resolved
            .iter()
            .map(|m| m.reference.project_id.as_str())
            .collect();
        assert_eq!(
            resolved.len(),
            3,
            "expected root, a, b (deduped, c is optional): {ids:?}"
        );
        assert!(ids.contains(&"root"));
        assert!(ids.contains(&"a"));
        assert!(ids.contains(&"b"));
        assert!(
            !ids.contains(&"c"),
            "optional dependency should not be followed"
        );
    }

    #[tokio::test]
    async fn resolve_errors_when_curseforge_is_needed_but_not_configured() {
        let modrinth = ModrinthClient::with_base_url("https://example.invalid");
        let err = resolve(
            &[ModReference::curseforge(6789)],
            "1.21.4",
            ModLoader::Fabric,
            &modrinth,
            None,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, ModError::CurseForgeNotConfigured(_)));
    }

    #[tokio::test]
    async fn install_rejects_a_vanilla_instance_without_any_network_call() {
        let dir = tempfile::tempdir().unwrap();
        let instance = test_instance(dir.path().to_path_buf(), None);
        let modrinth = ModrinthClient::with_base_url("https://example.invalid");

        let err = install(
            &instance,
            &[ModReference::modrinth("root")],
            &modrinth,
            None,
        )
        .await
        .unwrap_err();

        assert!(matches!(err, ModError::VanillaInstance));
    }

    #[tokio::test]
    async fn install_downloads_files_and_verifies_hash() {
        let server = MockServer::start().await;
        let payload = b"pretend this is a jar file";
        let expected_sha1 = {
            let mut hasher = Sha1::new();
            hasher.update(payload);
            hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };

        Mock::given(method("GET"))
            .and(path("/project/root/version"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!([{
                    "id": "root-v",
                    "project_id": "root",
                    "version_number": "1.0",
                    "game_versions": ["1.21.4"],
                    "loaders": ["fabric"],
                    "date_published": "2024-06-01T00:00:00Z",
                    "dependencies": [],
                    "files": [{
                        "url": format!("{}/download/root.jar", server.uri()),
                        "filename": "root.jar",
                        "primary": true,
                        "hashes": {"sha1": expected_sha1}
                    }]
                }])),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/download/root.jar"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(payload.to_vec()))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().unwrap();
        let instance = test_instance(dir.path().to_path_buf(), Some(ModLoader::Fabric));
        let modrinth = ModrinthClient::with_base_url(server.uri());

        let installed = install(
            &instance,
            &[ModReference::modrinth("root")],
            &modrinth,
            None,
        )
        .await
        .unwrap();

        assert_eq!(installed.len(), 1);
        let contents = tokio::fs::read(&installed[0]).await.unwrap();
        assert_eq!(contents, payload);
    }
}

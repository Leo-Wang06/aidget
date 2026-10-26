//! Native distribution operations. The UI never supplies trusted manifests or URLs.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeSet, VecDeque},
    fs,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{
    archive,
    http::{FetchError, Http},
    model::*,
    snapshot::{self, validate_commit},
};
use crate::{
    common::{
        error::{Error, Result},
        utils::atomic_write,
    },
    settings::{self, GithubToken},
    widgets::{
        model::WidgetMeta,
        package::{validate_package_name, validate_path, WidgetPlatform},
        store::Store,
    },
};
const MAX_JSON: usize = 8 * 1024 * 1024;
static REGISTRIES: Mutex<VecDeque<CachedRegistry>> = Mutex::new(VecDeque::new());
struct CachedRegistry {
    endpoint: String,
    commit: String,
    registry: Arc<Registry>,
}
/// Trusted endpoint roots. Production always uses the official public sources.
#[derive(Clone)]
pub struct Endpoints {
    pub github: String,
    pub raw: String,
    pub npm: String,
    pub downloads: String,
}
impl Default for Endpoints {
    fn default() -> Self {
        Self {
            github: "https://api.github.com/repos/aidget-ai/widgets".into(),
            raw: "https://raw.githubusercontent.com/aidget-ai/widgets".into(),
            npm: "https://registry.npmjs.org".into(),
            downloads: "https://api.npmjs.org".into(),
        }
    }
}
/// A short-lived native client; construct and use on a blocking worker thread.
pub struct Community {
    http: Http,
    endpoints: Endpoints,
    github_token: Option<GithubToken>,
}
#[derive(Deserialize)]
struct Branch {
    commit: Commit,
}
#[derive(Deserialize)]
struct Commit {
    sha: String,
}
#[derive(Serialize, Deserialize)]
struct CountCache {
    at: u64,
    count: u64,
}
impl Community {
    /// Official community and npm endpoints.
    pub fn new() -> Result<Self> {
        Self::with_endpoints(Endpoints::default())
    }
    /// Official endpoints with the token configured in `settings.toml`.
    pub fn for_store(root: &std::path::Path) -> Result<Self> {
        let token = settings::load(root)?.github_token;
        Self::with_endpoints_and_token(Endpoints::default(), token)
    }
    /// Explicit endpoint injection for bounded local HTTP integration tests.
    pub fn with_endpoints(endpoints: Endpoints) -> Result<Self> {
        Self::with_endpoints_and_token(endpoints, None)
    }
    /// Explicit endpoint and credential injection for bounded HTTP integration tests.
    pub fn with_endpoints_and_token(
        endpoints: Endpoints,
        github_token: Option<GithubToken>,
    ) -> Result<Self> {
        Ok(Self {
            http: Http::new()?,
            endpoints,
            github_token,
        })
    }
    fn json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        Ok(serde_json::from_slice(
            &self.http.get(url, MAX_JSON).map_err(|e| {
                if e.missing {
                    Error::NotFound(e.message)
                } else {
                    e.into_error()
                }
            })?,
        )?)
    }
    fn branch(&self, branch: &str) -> Result<String> {
        let value: Branch =
            self.github_json(&format!("{}/branches/{branch}", self.endpoints.github))?;
        validate_commit(&value.commit.sha)?;
        Ok(value.commit.sha)
    }
    fn github_json<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        Ok(serde_json::from_slice(
            &self
                .http
                .get_github(
                    url,
                    MAX_JSON,
                    self.github_token.as_ref().map(GithubToken::as_str),
                )
                .map_err(|e| {
                    if e.missing {
                        Error::NotFound(e.message)
                    } else {
                        e.into_error()
                    }
                })?,
        )?)
    }
    /// Verify that the configured GitHub credential can read the reviewed branch.
    pub fn verify_github_access(&self) -> Result<()> {
        self.branch("main")?;
        Ok(())
    }
    fn registry(&self, commit: &str) -> Result<Arc<Registry>> {
        validate_commit(commit)?;
        // Share immutable snapshots across command clients and serialize cache misses
        // so concurrent image requests only download and validate each snapshot once.
        let mut cache = REGISTRIES
            .lock()
            .map_err(|e| Error::Other(format!("registry cache lock poisoned: {e}")))?;
        if let Some(cached) = cache
            .iter()
            .find(|item| item.endpoint == self.endpoints.raw && item.commit == commit)
        {
            return Ok(Arc::clone(&cached.registry));
        }
        let registry: Registry =
            self.json(&format!("{}/{commit}/registry.json", self.endpoints.raw))?;
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for entry in &registry.widgets {
            entry.package.validate()?;
            entry.config.validate()?;
            if !entry.package.is_community()
                || entry.config.screenshots.is_empty()
                || !ids.insert(&entry.config.id)
                || !names.insert(&entry.package.name)
            {
                return Err(Error::InvalidParams(
                    "invalid or duplicate reviewed community entry".into(),
                ));
            }
            snapshot::validate_files(&entry.files)?;
        }
        let registry = Arc::new(registry);
        if cache.len() == 16 {
            cache.pop_front();
        }
        cache.push_back(CachedRegistry {
            endpoint: self.endpoints.raw.clone(),
            commit: commit.into(),
            registry: Arc::clone(&registry),
        });
        Ok(registry)
    }
    fn npm_metadata(&self, name: &str, version: &str) -> std::result::Result<Value, FetchError> {
        let bytes = self.http.get(
            &format!("{}/{}/{version}", self.endpoints.npm, encoded_package(name)),
            MAX_JSON,
        )?;
        serde_json::from_slice(&bytes).map_err(|e| FetchError {
            unavailable: false,
            missing: false,
            message: format!("invalid npm metadata: {e}"),
        })
    }
    /// Discover a reviewed main snapshot that is immediately installable from Git.
    pub fn list(&self, store: &Store) -> Result<CommunitySnapshot> {
        let commit = self.branch("main")?;
        let registry = self.registry(&commit)?;
        let ids = store.widget_ids()?;
        let mut names = BTreeSet::new();
        for valid in store.list()?.widgets {
            if let Some(package) = store.load_package(&valid.id)? {
                names.insert(package.name);
            }
        }
        let mut widgets = vec![];
        for entry in registry.widgets.iter().cloned() {
            widgets.push(CommunityEntry {
                installed: names.contains(&entry.package.name),
                id_conflict: ids.contains(&entry.config.id),
                supported: entry.config.platforms.contains(&WidgetPlatform::current()),
                entry,
            });
        }
        Ok(CommunitySnapshot { commit, widgets })
    }
    /// Serve reviewed raster media without rendering package HTML in the main window.
    pub fn media(&self, commit: &str, name: &str, path: &str) -> Result<String> {
        let registry = self.registry(commit)?;
        let entry = registry
            .widgets
            .iter()
            .find(|e| e.package.name == name)
            .ok_or_else(|| Error::NotFound(name.into()))?;
        if !entry
            .config
            .logo
            .iter()
            .chain(entry.config.screenshots.iter())
            .any(|p| p == path)
        {
            return Err(Error::InvalidParams("not a declared media file".into()));
        }
        validate_path(path)?;
        let file = entry.files.iter().find(|f| f.path == path).ok_or_else(|| {
            Error::InvalidParams("media missing from registry digest list".into())
        })?;
        let bytes = self
            .http
            .get(
                &snapshot::file_url(&self.endpoints.raw, commit, &entry.config.id, path)?,
                5 * 1024 * 1024,
            )
            .map_err(FetchError::into_error)?;
        if bytes.len() as u64 != file.size
            || crate::common::utils::sha256_hex(&bytes) != file.sha256
        {
            return Err(Error::InvalidParams(
                "media differs from reviewed snapshot".into(),
            ));
        }
        let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            "image/jpeg"
        } else if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
            "image/webp"
        } else {
            return Err(Error::InvalidParams(
                "media must be PNG, JPEG or WebP".into(),
            ));
        };
        Ok(format!("data:{mime};base64,{}", STANDARD.encode(bytes)))
    }
    /// Install the snapshot selected by the main window; never trust caller metadata.
    pub fn install_reviewed(&self, store: &Store, commit: &str, name: &str) -> Result<WidgetMeta> {
        let registry = self.registry(commit)?;
        let entry = registry
            .widgets
            .iter()
            .find(|e| e.package.name == name)
            .ok_or_else(|| Error::NotFound(name.into()))?;
        self.install_version(store, name, &entry.package.version, Some((commit, entry)))
    }
    /// Install a full npm name with an optional exact version. Community defaults use main.
    pub fn install(&self, store: &Store, spec: &str) -> Result<WidgetMeta> {
        let (name, version) = split_spec(spec)?;
        if name.starts_with("@aidget-community/") {
            let main = self.branch("main")?;
            let registry = self.registry(&main)?;
            if let Some(entry) = registry.widgets.iter().find(|e| {
                e.package.name == name && version.as_ref().is_none_or(|v| v == &e.package.version)
            }) {
                return self.install_version(
                    store,
                    &name,
                    &entry.package.version,
                    Some((&main, entry)),
                );
            }
            return Err(Error::NotFound(format!(
                "{spec} does not match the current reviewed community snapshot"
            )));
        }
        let version = match version {
            Some(version) => version,
            None => {
                let value = self
                    .npm_metadata(&name, "latest")
                    .map_err(FetchError::into_error)?;
                value["version"]
                    .as_str()
                    .ok_or_else(|| Error::InvalidParams("npm latest has no version".into()))?
                    .to_owned()
            }
        };
        semver::Version::parse(&version).map_err(|e| Error::InvalidParams(e.to_string()))?;
        self.install_version(store, &name, &version, None)
    }
    fn npm_tarball(
        &self,
        name: &str,
        version: &str,
    ) -> std::result::Result<(Vec<u8>, String), FetchError> {
        let value = self.npm_metadata(name, version)?;
        validate_npm_identity(&value, name, version).map_err(|e| FetchError {
            unavailable: false,
            missing: false,
            message: e.to_string(),
        })?;
        let Some(url) = value["dist"]["tarball"].as_str() else {
            return Err(FetchError {
                unavailable: false,
                missing: false,
                message: "npm metadata has no tarball".into(),
            });
        };
        let parsed = reqwest::Url::parse(url).map_err(|e| FetchError {
            unavailable: false,
            missing: false,
            message: e.to_string(),
        })?;
        if parsed.scheme() != "https"
            && !(self.endpoints.npm.starts_with("http://127.0.0.1:")
                && url.starts_with(&self.endpoints.npm))
        {
            return Err(FetchError {
                unavailable: false,
                missing: false,
                message: "npm tarball must use HTTPS".into(),
            });
        }
        let integrity = value["dist"]["integrity"]
            .as_str()
            .ok_or_else(|| FetchError {
                unavailable: false,
                missing: false,
                message: "npm metadata has no integrity".into(),
            })?
            .to_owned();
        Ok((self.http.get(url, archive::MAX_ARCHIVE)?, integrity))
    }
    fn install_version(
        &self,
        store: &Store,
        name: &str,
        version: &str,
        expected: Option<(&str, &RegistryEntry)>,
    ) -> Result<WidgetMeta> {
        let staged = tempfile::tempdir_in(store.state_dir())?;
        let (config, package) = match self.npm_tarball(name, version) {
            Ok((bytes, integrity)) => {
                archive::verify_integrity(&bytes, &integrity)?;
                archive::extract(&bytes, staged.path(), expected.map(|(_, entry)| entry))?
            }
            // Only transport failures permit switching channels, and only when a
            // reviewed manifest pins the same name/version to an exact Git commit.
            Err(error) if error.unavailable && expected.is_some() => {
                let (commit, entry) = expected.unwrap();
                snapshot::download(
                    &self.http,
                    &self.endpoints.raw,
                    commit,
                    entry,
                    staged.path(),
                )?
            }
            Err(error) => return Err(error.into_error()),
        };
        if package.name != name || package.version != version {
            return Err(Error::InvalidParams(
                "tarball identity differs from requested package".into(),
            ));
        }
        if !config.platforms.contains(&WidgetPlatform::current()) {
            return Err(Error::InvalidParams(
                "widget does not support this operating system".into(),
            ));
        }
        let id = config.id.clone();
        crate::widgets::install::commit(store, staged.path(), config, &package, &id)
    }
    /// Fetch optional npm weekly counts, cached for one hour; failures never become zero.
    pub fn downloads(&self, store: &Store, names: &[String]) -> Result<Vec<DownloadCount>> {
        if names.len() > 100 {
            return Err(Error::InvalidParams(
                "too many download-count requests".into(),
            ));
        }
        let cache_dir = store.state_dir().join("community/downloads");
        fs::create_dir_all(&cache_dir)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| Error::Other(e.to_string()))?
            .as_secs();
        let mut counts = vec![];
        for name in names {
            validate_package_name(name)?;
            let path = cache_dir.join(format!(
                "{}.json",
                crate::common::utils::sha256_hex(name.as_bytes())
            ));
            let cached = match fs::read(&path) {
                Ok(bytes) => Some(serde_json::from_slice::<CountCache>(&bytes)?),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(e.into()),
            };
            if let Some(cache) = cached.filter(|c| now.saturating_sub(c.at) < 3600) {
                counts.push(DownloadCount {
                    package: name.clone(),
                    weekly: Some(cache.count),
                    error: None,
                });
                continue;
            }
            let result = self.json::<Value>(&format!(
                "{}/downloads/point/last-week/{}",
                self.endpoints.downloads,
                encoded_package(name)
            ));
            match result.and_then(|v| {
                v["downloads"]
                    .as_u64()
                    .ok_or_else(|| Error::InvalidParams("invalid npm downloads response".into()))
            }) {
                Ok(count) => {
                    atomic_write(&path, &serde_json::to_vec(&CountCache { at: now, count })?)?;
                    counts.push(DownloadCount {
                        package: name.clone(),
                        weekly: Some(count),
                        error: None,
                    });
                }
                Err(error) => counts.push(DownloadCount {
                    package: name.clone(),
                    weekly: None,
                    error: Some(error.to_string()),
                }),
            }
        }
        Ok(counts)
    }
}
/// Encode an entire npm name, retaining the scope/name distinction in one path segment.
pub fn encoded_package(name: &str) -> String {
    percent_encoding::utf8_percent_encode(name, percent_encoding::NON_ALPHANUMERIC).to_string()
}
fn validate_npm_identity(value: &Value, name: &str, version: &str) -> Result<()> {
    if value["name"] != name || value["version"] != version {
        return Err(Error::InvalidParams(
            "npm metadata identity mismatch".into(),
        ));
    }
    Ok(())
}
/// Parse only an exact version; ranges and tags must not change a confirmed installation.
pub fn split_spec(spec: &str) -> Result<(String, Option<String>)> {
    let (name, version) = match spec.rsplit_once('@') {
        Some((name, version)) if !name.is_empty() => (name, Some(version)),
        _ => (spec, None),
    };
    validate_package_name(name)?;
    if let Some(version) = version {
        semver::Version::parse(version)
            .map_err(|e| Error::InvalidParams(format!("expected exact package version: {e}")))?;
    }
    Ok((name.into(), version.map(str::to_string)))
}

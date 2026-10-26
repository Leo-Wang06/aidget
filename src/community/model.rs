//! Wire formats shared by the repository, native installer and community panel.
use crate::widgets::package::{PackageManifest, WidgetConfig};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One immutable file in the npm package, hashed before packing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct FileDigest {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}
/// A reviewed package and the exact content displayed before installation.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct RegistryEntry {
    pub package: PackageManifest,
    pub config: WidgetConfig,
    pub files: Vec<FileDigest>,
}
/// Deterministically generated discovery document on main.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct Registry {
    pub widgets: Vec<RegistryEntry>,
}
/// User-facing installation availability, separate from package metadata.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CommunityEntry {
    pub entry: RegistryEntry,
    pub installed: bool,
    pub id_conflict: bool,
    pub supported: bool,
}
/// A page is tied to one reviewed main revision until explicitly refreshed.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CommunitySnapshot {
    pub commit: String,
    pub widgets: Vec<CommunityEntry>,
}
/// Optional npm telemetry. An unavailable count is distinct from zero.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DownloadCount {
    pub package: String,
    pub weekly: Option<u64>,
    pub error: Option<String>,
}

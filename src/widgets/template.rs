//! Built-in packages use the same portable files as community packages.
use super::{
    model::WidgetMeta,
    package::{PackageManifest, WidgetConfig},
    store::{Store, BOOTSTRAP_MARKER},
};
use crate::common::{
    error::{Error, Result},
    utils::atomic_write,
};
use include_dir::{include_dir, Dir};
use serde::Serialize;
use std::{fs, path::Path};
static BUILTIN: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/widgets");

/// Display metadata for a bundled widget package.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateSummary {
    pub id: String,
    pub title: String,
    pub description: String,
    pub version: String,
}

/// Read every bundled manifest, reporting invalid bundled content explicitly.
pub fn list() -> Result<Vec<TemplateSummary>> {
    BUILTIN
        .dirs()
        .map(|dir| {
            let id = dir
                .path()
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| Error::InvalidParams("invalid built-in directory".into()))?;
            let (config, package) = metadata(id)?;
            Ok(TemplateSummary {
                id: config.id,
                title: config.title,
                description: package.description,
                version: package.version,
            })
        })
        .collect()
}
/// Read a built-in package's canonical portable metadata.
pub fn metadata(id: &str) -> Result<(WidgetConfig, PackageManifest)> {
    super::model::validate_id(id)?;
    let dir = BUILTIN
        .get_dir(id)
        .ok_or_else(|| Error::NotFound(id.into()))?;
    let read = |name| {
        dir.get_file(Path::new(id).join(name))
            .ok_or_else(|| Error::NotFound(format!("{id}/{name}")))
    };
    let config: WidgetConfig = serde_json::from_slice(read("config.json")?.contents())?;
    let package: PackageManifest = serde_json::from_slice(read("package.json")?.contents())?;
    config.validate()?;
    package.validate()?;
    Ok((config, package))
}
/// Install a complete bundled package using the same commit path as downloads.
pub fn install(store: &Store, template_id: &str, target_id: &str) -> Result<WidgetMeta> {
    let (config, package) = metadata(template_id)?;
    let dir = BUILTIN
        .get_dir(template_id)
        .ok_or_else(|| Error::NotFound(template_id.into()))?;
    let staged = tempfile::tempdir_in(store.state_dir())?;
    copy_tree(dir, staged.path())?;
    super::install::commit(store, staged.path(), config, &package, target_id)
}
fn copy_tree(dir: &Dir<'_>, target: &Path) -> Result<()> {
    for entry in dir.entries() {
        match entry {
            include_dir::DirEntry::Dir(child) => {
                let relative = child
                    .path()
                    .strip_prefix(dir.path())
                    .map_err(|e| Error::Other(e.to_string()))?;
                let destination = target.join(relative);
                fs::create_dir_all(&destination)?;
                copy_tree(child, &destination)?;
            }
            include_dir::DirEntry::File(file) => {
                let relative = file
                    .path()
                    .strip_prefix(dir.path())
                    .map_err(|e| Error::Other(e.to_string()))?;
                atomic_write(&target.join(relative), file.contents())?;
            }
        }
    }
    Ok(())
}
/// Author guidance bundled with this template.
pub fn agents(id: &str) -> Option<String> {
    BUILTIN
        .get_file(Path::new(id).join("agents.md"))
        .and_then(|f| f.contents_utf8())
        .map(str::to_string)
}
/// Install the welcome package only when bootstrapping an empty data directory.
pub fn bootstrap_welcome(store: &Store) -> Result<()> {
    let marker = store.root().join(BOOTSTRAP_MARKER);
    if marker.exists() {
        return Ok(());
    }
    if store.widget_ids()?.is_empty() {
        install(store, "welcome", "welcome")?;
    }
    atomic_write(&marker, b"")
}

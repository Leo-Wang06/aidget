//! Install reviewed files directly from an immutable Git commit without build tools.
use super::{
    archive,
    http::{FetchError, Http},
    model::{FileDigest, RegistryEntry},
};
use crate::{
    common::{
        error::{Error, Result},
        utils::{atomic_write, sha256_hex},
    },
    widgets::package::{validate_path, PackageManifest, WidgetConfig},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// Reject unsafe, ambiguous or oversized manifests before downloading any content.
pub fn validate_files(files: &[FileDigest]) -> Result<()> {
    if files.is_empty() || files.len() > 1000 {
        return Err(Error::InvalidParams(
            "snapshot must contain 1–1000 files".into(),
        ));
    }
    let mut total = 0u64;
    let mut paths = BTreeSet::new();
    for file in files {
        validate_path(&file.path)?;
        total = total
            .checked_add(file.size)
            .ok_or_else(|| Error::InvalidParams("snapshot size overflow".into()))?;
        if total > 100 * 1024 * 1024
            || !paths.insert(file.path.to_lowercase())
            || file.sha256.len() != 64
            || !file.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(Error::InvalidParams(
                "unsafe or oversized snapshot file manifest".into(),
            ));
        }
    }
    Ok(())
}

/// Only full Git object IDs can select reviewed content.
pub(super) fn validate_commit(commit: &str) -> Result<()> {
    if commit.len() != 40 || !commit.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Error::InvalidParams("expected exact Git commit SHA".into()));
    }
    Ok(())
}

/// Encode each path segment so filenames cannot change the URL's query or fragment.
pub(super) fn file_url(raw: &str, commit: &str, id: &str, path: &str) -> Result<String> {
    validate_commit(commit)?;
    validate_path(id)?;
    validate_path(path)?;
    let mut url = reqwest::Url::parse(raw).map_err(|e| Error::InvalidParams(e.to_string()))?;
    url.path_segments_mut()
        .map_err(|_| Error::InvalidParams("invalid snapshot endpoint".into()))?
        .pop_if_empty()
        .extend([commit, "packages", id])
        .extend(path.split('/'));
    Ok(url.into())
}

/// Download exactly the reviewed manifest into a disposable staging directory.
pub(super) fn download(
    http: &Http,
    raw: &str,
    commit: &str,
    entry: &RegistryEntry,
    directory: &Path,
) -> Result<(WidgetConfig, PackageManifest)> {
    validate_files(&entry.files)?;
    let mut files = BTreeMap::new();
    for file in &entry.files {
        let url = file_url(raw, commit, &entry.config.id, &file.path)?;
        let bytes = http
            .get(&url, file.size as usize)
            .map_err(FetchError::into_error)?;
        if bytes.len() as u64 != file.size || sha256_hex(&bytes) != file.sha256 {
            return Err(Error::InvalidParams(format!(
                "{} differs from the reviewed Git snapshot",
                file.path
            )));
        }
        atomic_write(&directory.join(&file.path), &bytes)?;
        files.insert(file.path.clone(), file.clone());
    }
    archive::validate_contents(directory, &files, Some(entry))
}

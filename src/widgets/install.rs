//! Commit a validated directory without exposing partial widget files to the watcher.
use super::{
    model::{WidgetFrame, WidgetMeta, MAX_WIDGETS},
    package::{PackageManifest, WidgetConfig},
    store::{Store, CONFIG_FILE, PACKAGE_FILE},
};
use crate::common::{
    error::{Error, Result},
    utils::atomic_write,
};
use std::fs;
use std::path::Path;

/// Reserve a unique widget and update its board entry under the shared write lock.
pub fn commit(
    store: &Store,
    staged: &Path,
    config: WidgetConfig,
    package: &PackageManifest,
    target_id: &str,
) -> Result<WidgetMeta> {
    let _lock = store.write_lock()?;
    config.validate()?;
    package.validate()?;
    let existing = store.widget_ids()?;
    if existing.len() >= MAX_WIDGETS {
        return Err(Error::InvalidParams(format!(
            "widget limit reached ({MAX_WIDGETS})"
        )));
    }
    let target = store.widget_dir(target_id)?;
    if target.exists() {
        return Err(Error::InvalidParams(format!(
            "widget `{target_id}` already exists"
        )));
    }
    for id in &existing {
        if store
            .load_package(id)?
            .is_some_and(|p| p.name == package.name)
        {
            return Err(Error::InvalidParams(format!(
                "package `{}` is already installed",
                package.name
            )));
        }
    }
    let mut meta = WidgetMeta::new(target_id, &config.title);
    meta.config = config;
    meta.id = target_id.into();
    atomic_write(
        &staged.join(CONFIG_FILE),
        &serde_json::to_vec_pretty(&meta)?,
    )?;
    if !staged.join(PACKAGE_FILE).is_file() {
        return Err(Error::InvalidParams("missing package.json".into()));
    }
    let mut board = store.load_board()?;
    let mut frame = WidgetFrame::default_for(existing.len());
    frame.w = meta.size.width;
    frame.h = meta.size.height;
    board.widgets.insert(target_id.into(), frame);
    board.revision += 1;
    super::storage::clear(store, target_id)?;
    // The native window is only created after both complete files and a frame exist.
    fs::rename(staged, &target)?;
    if let Err(error) = store.save_board(&board) {
        fs::rename(&target, staged).map_err(|rollback| {
            Error::Other(format!("{error}; installation rollback failed: {rollback}"))
        })?;
        return Err(error);
    }
    Ok(meta)
}

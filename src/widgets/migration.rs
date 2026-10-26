//! Explicit startup upgrade; regular reads never accept the legacy format.
use super::{
    model::{BrokenWidget, WidgetDragMode, WidgetMeta, WidgetPermission},
    package::{WidgetConfig, WidgetPlatform},
    store::{Store, CONFIG_FILE},
};
use crate::common::{error::Result, utils::atomic_write};
use serde::Deserialize;
use serde_json::Value;
use std::{fs, path::Path};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Legacy {
    id: String,
    title: String,
    version: String,
    allow_scripts: bool,
    #[serde(default)]
    drag_mode: WidgetDragMode,
    #[serde(default)]
    permissions: Vec<WidgetPermission>,
    source: Option<Value>,
    revision: u64,
    updated_at: String,
}
/// Upgrade each directory independently. Failures remain visible as broken widgets.
pub fn run(store: &Store) -> Result<Vec<BrokenWidget>> {
    let _lock = store.write_lock()?;
    let mut broken = vec![];
    for root in [store.widgets_dir(), store.trash_dir()] {
        for entry in fs::read_dir(root)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let path = entry.path();
            if let Err(error) = migrate(store, &path) {
                broken.push(BrokenWidget {
                    id: path.file_name().unwrap().to_string_lossy().into(),
                    error: error.to_string(),
                });
            }
        }
    }
    Ok(broken)
}
fn migrate(store: &Store, directory: &Path) -> Result<()> {
    let path = directory.join(CONFIG_FILE);
    let bytes = fs::read(&path)?;
    let value: Value = serde_json::from_slice(&bytes)?;
    if value.get("allowScripts").is_none() {
        let meta: WidgetMeta = serde_json::from_value(value)?;
        return meta.config.validate();
    }
    let old: Legacy = serde_json::from_value(value)?;
    let _old_version = &old.version;
    let mut config = WidgetConfig::new(&old.id, &old.title);
    config.drag_mode = old.drag_mode;
    config.permissions = old.permissions;
    if old.allow_scripts {
        config.permissions.push(WidgetPermission::Script);
    }
    config.permissions.sort();
    config.permissions.dedup();
    config.platforms = vec![WidgetPlatform::current()];
    if let Some(frame) = store.load_board()?.widgets.get(&old.id) {
        config.size.width = frame.w;
        config.size.height = frame.h;
    }
    config.validate()?;
    let meta = WidgetMeta {
        config,
        revision: old.revision,
        updated_at: old.updated_at,
    };
    let category = directory
        .parent()
        .and_then(Path::file_name)
        .unwrap()
        .to_string_lossy();
    let backup = store
        .root()
        .join("backups/widget-metadata")
        .join(category.as_ref())
        .join(directory.file_name().unwrap());
    fs::create_dir_all(&backup)?;
    // Never replace an earlier backup when resuming an interrupted migration.
    let mut temporary = tempfile::NamedTempFile::new_in(&backup)?;
    {
        use std::io::Write;
        temporary.write_all(&bytes)?;
        temporary.as_file().sync_all()?;
    }
    let backup_file = backup.join(CONFIG_FILE);
    if let Err(error) = temporary.persist_noclobber(&backup_file) {
        if error.error.kind() != std::io::ErrorKind::AlreadyExists {
            return Err(error.error.into());
        }
        if fs::read(&backup_file)? != bytes {
            return Err(crate::Error::InvalidParams(
                "legacy config differs from its existing migration backup".into(),
            ));
        }
    }
    if old
        .source
        .as_ref()
        .is_some_and(|s| s["kind"] == "builtin" && s["id"] == "welcome")
        && !directory.join("package.json").exists()
    {
        let package = include_bytes!("../../widgets/welcome/package.json");
        atomic_write(&directory.join("package.json"), package)?;
    }
    atomic_write(&path, &serde_json::to_vec_pretty(&meta)?)?;
    Ok(())
}

/// Surface invalid trash configurations without blocking healthy trash entries.
pub fn trash_errors(store: &Store) -> Result<Vec<BrokenWidget>> {
    let mut errors = vec![];
    for entry in fs::read_dir(store.trash_dir())? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let result = fs::read(entry.path().join(CONFIG_FILE))
            .map_err(crate::Error::from)
            .and_then(|b| {
                let meta: WidgetMeta = serde_json::from_slice(&b)?;
                meta.config.validate()?;
                Ok(meta)
            });
        if let Err(error) = result {
            errors.push(BrokenWidget {
                id: format!("trash/{}", entry.file_name().to_string_lossy()),
                error: error.to_string(),
            });
        }
    }
    Ok(errors)
}

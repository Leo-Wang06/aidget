use std::fs;
use std::path::{Path, PathBuf};

use aidget::agents::skill::{ensure_skill, set_skill, skill_state, skill_tree_hash, SKILL_CONTENT};
use aidget::widgets::store::Store;
use serde_json::json;
use tempfile::tempdir;

fn source(root: &Path, content: &str) -> PathBuf {
    let source = root.join("store/skills/aidget");
    fs::create_dir_all(&source).unwrap();
    fs::write(source.join("SKILL.md"), content).unwrap();
    source
}

#[test]
fn embedded_skill_is_materialized_under_the_store_root() {
    let temp = tempdir().unwrap();
    let store = Store::new(temp.path());
    let directory = ensure_skill(&store).unwrap();
    assert_eq!(directory, temp.path().join("skills/aidget"));
    assert_eq!(
        fs::read_to_string(directory.join("SKILL.md")).unwrap(),
        SKILL_CONTENT
    );

    fs::write(directory.join("SKILL.md"), "stale").unwrap();
    ensure_skill(&store).unwrap();
    assert_eq!(
        fs::read_to_string(directory.join("SKILL.md")).unwrap(),
        SKILL_CONTENT
    );
}

#[test]
fn install_uses_the_shared_skill_source() {
    let temp = tempdir().unwrap();
    let source = source(temp.path(), "skill");
    let path = temp.path().join("agent/skills/aidget");

    set_skill(&path, &source, true).unwrap();

    assert!(skill_state(&path, &source).unwrap().installed);
    assert_eq!(fs::read_to_string(path.join("SKILL.md")).unwrap(), "skill");
    #[cfg(unix)]
    assert!(fs::symlink_metadata(&path)
        .unwrap()
        .file_type()
        .is_symlink());
    set_skill(&path, &source, true).unwrap();
}

#[test]
fn linked_skill_reads_source_changes_immediately() {
    let temp = tempdir().unwrap();
    let source = source(temp.path(), "first");
    let path = temp.path().join("agent/skills/aidget");
    set_skill(&path, &source, true).unwrap();

    fs::write(source.join("SKILL.md"), "second").unwrap();

    assert_eq!(fs::read_to_string(path.join("SKILL.md")).unwrap(), "second");
}

#[test]
fn install_replaces_the_whole_legacy_directory_without_a_backup() {
    let temp = tempdir().unwrap();
    let source = source(temp.path(), "current");
    let path = temp.path().join("agent/skills/aidget");
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("SKILL.md"), "old").unwrap();
    fs::write(path.join("notes.txt"), "remove me").unwrap();

    assert!(skill_state(&path, &source).unwrap().conflict.is_some());
    set_skill(&path, &source, true).unwrap();

    assert!(skill_state(&path, &source).unwrap().installed);
    assert_eq!(
        fs::read_to_string(path.join("SKILL.md")).unwrap(),
        "current"
    );
    assert!(!path.join("notes.txt").exists());
    assert!(!temp.path().join("state/agent-backups").exists());
}

#[test]
fn remove_only_unlinks_the_agent_and_keeps_the_source() {
    let temp = tempdir().unwrap();
    let source = source(temp.path(), "skill");
    let path = temp.path().join("agent/skills/aidget");
    set_skill(&path, &source, true).unwrap();

    set_skill(&path, &source, false).unwrap();

    assert!(!path.exists());
    assert_eq!(
        fs::read_to_string(source.join("SKILL.md")).unwrap(),
        "skill"
    );
}

#[test]
fn an_ordinary_directory_is_not_removed_as_a_managed_skill() {
    let temp = tempdir().unwrap();
    let source = source(temp.path(), "skill");
    let path = temp.path().join("agent/skills/aidget");
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("SKILL.md"), "mine").unwrap();

    assert!(set_skill(&path, &source, false).is_err());
    assert_eq!(fs::read_to_string(path.join("SKILL.md")).unwrap(), "mine");
}

#[test]
fn a_current_marked_copy_counts_as_installed_until_it_becomes_stale() {
    let temp = tempdir().unwrap();
    let source = source(temp.path(), "first");
    let path = temp.path().join("agent/skills/aidget");
    fs::create_dir_all(&path).unwrap();
    fs::copy(source.join("SKILL.md"), path.join("SKILL.md")).unwrap();
    let copy_hash = skill_tree_hash(&path).unwrap();
    fs::write(
        path.join(".aidget-skill"),
        serde_json::to_vec(&json!({
            "source": fs::canonicalize(&source).unwrap(),
            "treeHash": copy_hash,
        }))
        .unwrap(),
    )
    .unwrap();

    assert!(skill_state(&path, &source).unwrap().installed);
    fs::write(source.join("SKILL.md"), "second").unwrap();
    assert!(skill_state(&path, &source).unwrap().conflict.is_some());

    set_skill(&path, &source, true).unwrap();
    assert!(skill_state(&path, &source).unwrap().installed);
    assert_eq!(fs::read_to_string(path.join("SKILL.md")).unwrap(), "second");
}

#[cfg(unix)]
#[test]
fn a_wrong_link_is_replaced_and_its_target_is_not_modified() {
    let temp = tempdir().unwrap();
    let source = source(temp.path(), "right");
    let other = temp.path().join("other");
    fs::create_dir_all(&other).unwrap();
    fs::write(other.join("SKILL.md"), "other").unwrap();
    let path = temp.path().join("agent/skills/aidget");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&other, &path).unwrap();

    assert!(skill_state(&path, &source).unwrap().conflict.is_some());
    set_skill(&path, &source, true).unwrap();

    assert_eq!(fs::read_to_string(path.join("SKILL.md")).unwrap(), "right");
    assert_eq!(fs::read_to_string(other.join("SKILL.md")).unwrap(), "other");
}

#[cfg(windows)]
#[test]
fn a_junction_is_a_managed_link_and_removal_keeps_the_source() {
    let temp = tempdir().unwrap();
    let source = source(temp.path(), "skill");
    let path = temp.path().join("agent/skills/aidget");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    junction::create(&source, &path).unwrap();

    assert!(skill_state(&path, &source).unwrap().installed);
    set_skill(&path, &source, false).unwrap();

    assert!(!path.exists());
    assert_eq!(
        fs::read_to_string(source.join("SKILL.md")).unwrap(),
        "skill"
    );
}

#[test]
fn failed_replacement_leaves_the_old_directory_intact() {
    let temp = tempdir().unwrap();
    let missing_source = temp.path().join("missing");
    let path = temp.path().join("agent/skills/aidget");
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("SKILL.md"), "old").unwrap();

    assert!(set_skill(&path, &missing_source, true).is_err());

    assert_eq!(fs::read_to_string(path.join("SKILL.md")).unwrap(), "old");
}

#[test]
fn the_shared_source_is_never_replaced_by_itself() {
    let temp = tempdir().unwrap();
    let source = source(temp.path(), "skill");

    assert!(set_skill(&source, &source, true).is_err());

    assert_eq!(
        fs::read_to_string(source.join("SKILL.md")).unwrap(),
        "skill"
    );
}

#[test]
fn skill_tree_hash_changes_with_nested_content() {
    let temp = tempdir().unwrap();
    let source = source(temp.path(), "skill");
    let first = skill_tree_hash(&source).unwrap();
    fs::create_dir_all(source.join("assets")).unwrap();
    fs::write(source.join("assets/data.txt"), "data").unwrap();

    let second = skill_tree_hash(&source).unwrap();

    assert_ne!(first, second);
}

use aidget::agents::config::{AgentResourceState, McpFormat, McpTarget};
use aidget::agents::integration::{
    apply_change, migrate_existing_skills, select_agents, skill_path, AgentIntegration,
    IntegrationResource, IntegrationState,
};
use aidget::agents::registry::AgentSpec;
use aidget::agents::skill::{ensure_skill, skill_state, SKILL_CONTENT};
use aidget::widgets::store::Store;
use std::fs;
use tempfile::tempdir;

#[test]
fn integration_state_uses_camel_case_and_keeps_agent_capabilities() {
    let state = IntegrationState {
        mcp_command: "/aidget".to_string(),
        mcp_args: vec!["mcp".to_string()],
        skill_directory: "/tmp/skills/aidget".to_string(),
        skill_content: "-".to_string(),
        agents: vec![AgentIntegration {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            icon: "codex-color".to_string(),
            mcp: Some(AgentResourceState {
                path: "/tmp/config.toml".to_string(),
                installed: true,
                conflict: None,
            }),
            skills: None,
        }],
    };
    let value = serde_json::to_value(state).unwrap();
    assert_eq!(value["mcpCommand"], "/aidget");
    assert_eq!(value["skillDirectory"], "/tmp/skills/aidget");
    assert_eq!(value["agents"][0]["id"], "codex");
    assert_eq!(value["agents"][0]["mcp"]["installed"], true);
    assert!(value["agents"][0].get("skills").is_none());
}

#[test]
fn selected_agents_must_be_detected() {
    let agents = vec![AgentSpec {
        id: "codex",
        name: "Codex",
        icon: "codex-color",
        commands: vec![],
        detect: vec![],
        mcp: Some(McpTarget {
            path: "/tmp/config.toml".into(),
            format: McpFormat::Codex,
        }),
        skills: None,
    }];
    assert!(select_agents(&agents, &["codex".to_string()]).is_ok());
    assert!(select_agents(&agents, &["missing".to_string()]).is_err());
}

#[test]
fn skill_install_path_is_the_aidget_folder_under_the_agent_root() {
    assert_eq!(
        skill_path(std::path::Path::new("/tmp/codex/skills")),
        std::path::PathBuf::from("/tmp/codex/skills/aidget")
    );
}

#[test]
fn batch_change_installs_mcp_and_skill_for_a_detected_agent() {
    let temp = tempdir().unwrap();
    let skill_directory = ensure_skill(&Store::new(temp.path())).unwrap();
    let mcp_path = temp.path().join("config.json");
    let skills = temp.path().join("agent/skills");
    let agent = AgentSpec {
        id: "fixture",
        name: "Fixture",
        icon: "generic",
        commands: vec![],
        detect: vec![],
        mcp: Some(McpTarget {
            path: mcp_path.clone(),
            format: McpFormat::Claude,
        }),
        skills: Some(skills.clone()),
    };
    let agents = vec![&agent];
    assert!(apply_change(
        &agents,
        "/aidget",
        &skill_directory,
        temp.path(),
        IntegrationResource::Mcp,
        true,
    )
    .is_empty());
    assert!(apply_change(
        &agents,
        "/aidget",
        &skill_directory,
        temp.path(),
        IntegrationResource::Skills,
        true,
    )
    .is_empty());
    assert!(mcp_path.exists());
    assert!(skills.join("aidget/SKILL.md").exists());
    assert!(
        skill_state(&skills.join("aidget"), &skill_directory)
            .unwrap()
            .installed
    );
}

#[test]
fn packaged_skill_keeps_its_original_front_matter() {
    let mut front_matter = SKILL_CONTENT.lines();
    assert_eq!(front_matter.next(), Some("---"));
    assert_eq!(front_matter.next(), Some("name: aidget"));
    assert!(SKILL_CONTENT.contains("# aidget"));
}

#[test]
fn migration_replaces_an_existing_copy_once_across_shared_agent_roots() {
    let temp = tempdir().unwrap();
    let source = ensure_skill(&Store::new(temp.path().join("aidget"))).unwrap();
    let skills = temp.path().join("shared/skills");
    let first = AgentSpec {
        id: "first",
        name: "First",
        icon: "generic",
        commands: vec![],
        detect: vec![],
        mcp: None,
        skills: Some(skills.clone()),
    };
    let second = AgentSpec {
        id: "second",
        name: "Second",
        icon: "generic",
        commands: vec![],
        detect: vec![],
        mcp: None,
        skills: Some(skills.clone()),
    };
    let path = skills.join("aidget");
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("SKILL.md"), "old").unwrap();
    fs::write(path.join("extra.txt"), "remove").unwrap();

    let problems = migrate_existing_skills(&[&first, &second], &source);
    assert!(problems.is_empty());
    assert!(skill_state(&path, &source).unwrap().installed);
    assert_eq!(
        fs::read_to_string(path.join("SKILL.md")).unwrap(),
        SKILL_CONTENT
    );
    assert!(!path.join("extra.txt").exists());
    assert!(migrate_existing_skills(&[&first, &second], &source).is_empty());
}

#[cfg(unix)]
#[test]
fn migration_preserves_links_to_other_aidget_instances() {
    let temp = tempdir().unwrap();
    let source = ensure_skill(&Store::new(temp.path().join("aidget-current"))).unwrap();
    let other_source = ensure_skill(&Store::new(temp.path().join("aidget-other"))).unwrap();
    let skills = temp.path().join("agent/skills");
    let path = skills.join("aidget");
    let agent = AgentSpec {
        id: "fixture",
        name: "Fixture",
        icon: "generic",
        commands: vec![],
        detect: vec![],
        mcp: None,
        skills: Some(skills.clone()),
    };
    fs::create_dir_all(&skills).unwrap();
    std::os::unix::fs::symlink(&other_source, &path).unwrap();

    assert!(migrate_existing_skills(&[&agent], &source).is_empty());

    assert_eq!(
        fs::canonicalize(&path).unwrap(),
        fs::canonicalize(&other_source).unwrap()
    );
}

#[test]
fn migration_preserves_copies_managed_by_other_aidget_instances() {
    let temp = tempdir().unwrap();
    let source = ensure_skill(&Store::new(temp.path().join("aidget-current"))).unwrap();
    let other_source = ensure_skill(&Store::new(temp.path().join("aidget-other"))).unwrap();
    let skills = temp.path().join("agent/skills");
    let path = skills.join("aidget");
    let agent = AgentSpec {
        id: "fixture",
        name: "Fixture",
        icon: "generic",
        commands: vec![],
        detect: vec![],
        mcp: None,
        skills: Some(skills.clone()),
    };
    fs::create_dir_all(&path).unwrap();
    fs::copy(other_source.join("SKILL.md"), path.join("SKILL.md")).unwrap();
    fs::write(
        path.join(".aidget-skill"),
        serde_json::json!({
            "source": fs::canonicalize(&other_source).unwrap(),
            "treeHash": "other",
        })
        .to_string(),
    )
    .unwrap();

    assert!(migrate_existing_skills(&[&agent], &source).is_empty());

    assert_eq!(
        fs::read_to_string(path.join("SKILL.md")).unwrap(),
        SKILL_CONTENT
    );
    assert!(!skill_state(&path, &source).unwrap().installed);
}

#[test]
fn migration_refreshes_a_stale_copy_from_the_current_instance() {
    let temp = tempdir().unwrap();
    let source = ensure_skill(&Store::new(temp.path().join("aidget-current"))).unwrap();
    let skills = temp.path().join("agent/skills");
    let path = skills.join("aidget");
    let agent = AgentSpec {
        id: "fixture",
        name: "Fixture",
        icon: "generic",
        commands: vec![],
        detect: vec![],
        mcp: None,
        skills: Some(skills.clone()),
    };
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("SKILL.md"), "stale").unwrap();
    fs::write(
        path.join(".aidget-skill"),
        serde_json::json!({
            "source": fs::canonicalize(&source).unwrap(),
            "treeHash": "stale",
        })
        .to_string(),
    )
    .unwrap();

    assert!(migrate_existing_skills(&[&agent], &source).is_empty());

    assert!(skill_state(&path, &source).unwrap().installed);
}

#[test]
fn migration_failure_keeps_the_old_copy_and_reports_the_agent() {
    let temp = tempdir().unwrap();
    let missing_source = temp.path().join("missing");
    let skills = temp.path().join("skills");
    let agent = AgentSpec {
        id: "fixture",
        name: "Fixture",
        icon: "generic",
        commands: vec![],
        detect: vec![],
        mcp: None,
        skills: Some(skills.clone()),
    };
    let path = skills.join("aidget");
    fs::create_dir_all(&path).unwrap();
    fs::write(path.join("SKILL.md"), "old").unwrap();

    let problems = migrate_existing_skills(&[&agent], &missing_source);

    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].agent_id, "fixture");
    assert_eq!(fs::read_to_string(path.join("SKILL.md")).unwrap(), "old");
}

#[test]
fn migration_does_not_install_a_skill_the_agent_never_had() {
    let temp = tempdir().unwrap();
    let source = ensure_skill(&Store::new(temp.path().join("aidget"))).unwrap();
    let skills = temp.path().join("agent/skills");
    let agent = AgentSpec {
        id: "fixture",
        name: "Fixture",
        icon: "generic",
        commands: vec![],
        detect: vec![],
        mcp: None,
        skills: Some(skills.clone()),
    };

    assert!(migrate_existing_skills(&[&agent], &source).is_empty());

    assert!(!skills.join("aidget").exists());
}

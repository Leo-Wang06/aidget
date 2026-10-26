use std::fs;

use aidget::agents::config::{mcp_state, set_mcp, McpFormat, McpTarget};
use tempfile::tempdir;

#[test]
fn json_mcp_entry_round_trips_without_touching_neighbors() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.json");
    fs::write(
        &path,
        "{\n  // keep\n  \"theme\": \"dark\",\n  \"mcpServers\": {\"other\": {\"command\": \"x\"}}\n}\n",
    )
    .unwrap();
    let target = McpTarget {
        path: path.clone(),
        format: McpFormat::Claude,
    };
    set_mcp(&target, "/a", true, temp.path(), "codex").unwrap();
    assert!(mcp_state(&target, "/a").unwrap().installed);
    assert!(fs::read_to_string(&path).unwrap().contains("// keep"));
    set_mcp(&target, "/a", false, temp.path(), "codex").unwrap();
    assert!(!mcp_state(&target, "/a").unwrap().installed);
    assert!(fs::read_to_string(&path).unwrap().contains("\"other\""));
}

#[test]
fn conflicting_mcp_entry_is_rejected() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.json");
    fs::write(
        &path,
        "{\"mcpServers\":{\"aidget\":{\"command\":\"/old\",\"args\":[\"mcp\"]}}}",
    )
    .unwrap();
    let target = McpTarget {
        path,
        format: McpFormat::Claude,
    };
    assert_eq!(
        mcp_state(&target, "/new").unwrap().conflict.as_deref(),
        Some("a different `aidget` entry already exists")
    );
    assert!(set_mcp(&target, "/new", true, temp.path(), "claude").is_err());
}

#[test]
fn opencode_and_cline_shapes_are_read_back() {
    let temp = tempdir().unwrap();
    let opencode = temp.path().join("opencode.json");
    let target = McpTarget {
        path: opencode.clone(),
        format: McpFormat::OpenCode,
    };
    set_mcp(&target, "/a", true, temp.path(), "opencode").unwrap();
    assert!(mcp_state(&target, "/a").unwrap().installed);

    let cline = temp.path().join("cline.json");
    let target = McpTarget {
        path: cline,
        format: McpFormat::Cline,
    };
    set_mcp(&target, "/a", true, temp.path(), "cline").unwrap();
    assert!(mcp_state(&target, "/a").unwrap().installed);
}

#[test]
fn codex_toml_round_trips_and_preserves_neighbors() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("config.toml");
    fs::write(
        &path,
        "model = \"gpt-5\"\n\n[mcp_servers.other]\ncommand = \"x\"\n",
    )
    .unwrap();
    let target = McpTarget {
        path: path.clone(),
        format: McpFormat::Codex,
    };
    set_mcp(&target, "/a", true, temp.path(), "codex").unwrap();
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("model = \"gpt-5\""));
    assert!(text.contains("[mcp_servers.other]"));
    assert!(mcp_state(&target, "/a").unwrap().installed);
    set_mcp(&target, "/a", false, temp.path(), "codex").unwrap();
    assert!(fs::read_to_string(&path)
        .unwrap()
        .contains("[mcp_servers.other]"));
}

#[test]
fn yaml_and_dsh_formats_round_trip() {
    let temp = tempdir().unwrap();
    let hermes = temp.path().join("config.yaml");
    let target = McpTarget {
        path: hermes.clone(),
        format: McpFormat::Hermes,
    };
    set_mcp(&target, "/a", true, temp.path(), "hermes").unwrap();
    assert!(mcp_state(&target, "/a").unwrap().installed);
    set_mcp(&target, "/a", false, temp.path(), "hermes").unwrap();
    assert!(!mcp_state(&target, "/a").unwrap().installed);

    let dsh = temp.path().join("cordis.patch.yml");
    fs::write(&dsh, "[]\n").unwrap();
    let target = McpTarget {
        path: dsh,
        format: McpFormat::Dsh,
    };
    set_mcp(&target, "/a", true, temp.path(), "dsh").unwrap();
    assert!(mcp_state(&target, "/a").unwrap().installed);
    set_mcp(&target, "/a", false, temp.path(), "dsh").unwrap();
    assert!(!mcp_state(&target, "/a").unwrap().installed);
}

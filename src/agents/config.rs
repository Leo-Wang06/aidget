use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use toml_edit::{value, DocumentMut, Item, Table};

use crate::agents::jsonc;
use crate::common::error::{Error, Result};
use crate::common::utils::atomic_write;

/// MCP configuration shape used by one Agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum McpFormat {
    Claude,
    Codex,
    Gemini,
    OpenCode,
    Omp,
    Antigravity,
    Goose,
    Copilot,
    Crush,
    ZCode,
    Zed,
    Kimi,
    Devin,
    Cline,
    CommandCode,
    Simple,
    Hermes,
    Dsh,
}

/// One Agent's MCP destination.
#[derive(Debug, Clone)]
pub struct McpTarget {
    pub path: PathBuf,
    pub format: McpFormat,
}

/// Current state of one resource for one Agent.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentResourceState {
    pub path: String,
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict: Option<String>,
}

pub fn mcp_state(target: &McpTarget, command: &str) -> Result<AgentResourceState> {
    let path = target.path.to_string_lossy().into_owned();
    if !target.path.exists() {
        return Ok(AgentResourceState {
            path,
            installed: false,
            conflict: None,
        });
    }
    match read_mcp(target, command) {
        Ok(Some(true)) => Ok(AgentResourceState {
            path,
            installed: true,
            conflict: None,
        }),
        Ok(Some(false)) => Ok(AgentResourceState {
            path,
            installed: false,
            conflict: Some("a different `aidget` entry already exists".to_string()),
        }),
        Ok(None) => Ok(AgentResourceState {
            path,
            installed: false,
            conflict: None,
        }),
        Err(error) => Ok(AgentResourceState {
            path,
            installed: false,
            conflict: Some(error.to_string()),
        }),
    }
}

pub fn set_mcp(
    target: &McpTarget,
    command: &str,
    enabled: bool,
    backup_root: &Path,
    agent: &str,
) -> Result<()> {
    if target.format == McpFormat::Dsh {
        if target.path.exists() {
            let text = fs::read_to_string(&target.path)?;
            if read_mcp_text(target, &text, command)? == Some(false) {
                return Err(Error::InvalidParams(format!(
                    "{} already has a different `aidget` MCP entry",
                    target.path.display()
                )));
            }
        }
        remove_dsh_profile_entries(target, backup_root)?;
    }
    let current = if target.path.exists() {
        Some(fs::read_to_string(&target.path)?)
    } else {
        None
    };
    if let Some(text) = &current {
        match read_mcp_text(target, text, command)? {
            Some(true) if enabled => return Ok(()),
            Some(false) => {
                return Err(Error::InvalidParams(format!(
                    "{} already has a different `aidget` MCP entry",
                    target.path.display()
                )))
            }
            None if !enabled => return Ok(()),
            _ => {}
        }
    } else if !enabled {
        return Ok(());
    }

    let next = match target.format {
        McpFormat::Codex => edit_toml(&target.path, current.as_deref(), command, enabled)?,
        McpFormat::Hermes => edit_yaml(&target.path, current.as_deref(), command, enabled)?,
        McpFormat::Dsh => edit_dsh(&target.path, current.as_deref(), command, enabled)?,
        _ => edit_json(target.format, current.as_deref(), command, enabled)?,
    };
    if current.as_deref() == Some(next.as_str()) {
        return Ok(());
    }
    if let Some(text) = current {
        backup_bytes(backup_root, agent, &target.path, text.as_bytes())?;
    }
    atomic_write(&target.path, next.as_bytes())
}

/// Remove aidget inserts that older aidget versions wrote into DSH profile
/// patch files. DSH applies the home patch after every profile, so leaving the
/// same insert in both layers registers the server twice.
fn remove_dsh_profile_entries(target: &McpTarget, backup_root: &Path) -> Result<()> {
    let Some(home) = target.path.parent() else {
        return Ok(());
    };
    let profiles = home.join("profiles");
    if !profiles.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(&profiles)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let path = entry.path().join("cordis.patch.yml");
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        if !text.contains("magpie-mcp-aidget") {
            continue;
        }
        let document: serde_yaml::Value = serde_yaml::from_str(&text)
            .map_err(|error| Error::InvalidParams(format!("{}: {error}", path.display())))?;
        let Some(entries) = document.as_sequence() else {
            continue;
        };
        if !entries.iter().any(is_aidget_dsh_insert) {
            continue;
        }
        let next = edit_dsh(&path, Some(&text), "", false)?;
        if next == text {
            continue;
        }
        backup_bytes(backup_root, "dsh", &path, text.as_bytes())?;
        atomic_write(&path, next.as_bytes())?;
    }
    Ok(())
}

fn read_mcp(target: &McpTarget, command: &str) -> Result<Option<bool>> {
    let text = fs::read_to_string(&target.path)?;
    read_mcp_text(target, &text, command)
}

fn read_mcp_text(target: &McpTarget, text: &str, command: &str) -> Result<Option<bool>> {
    match target.format {
        McpFormat::Codex => {
            let document = text.parse::<DocumentMut>().map_err(|error| {
                Error::InvalidParams(format!("{}: {error}", target.path.display()))
            })?;
            let Some(entry) = document
                .get("mcp_servers")
                .and_then(Item::as_table)
                .and_then(|table| table.get("aidget"))
                .and_then(Item::as_table)
            else {
                return Ok(None);
            };
            Ok(Some(toml_entry_matches(entry, command)))
        }
        McpFormat::Hermes => {
            let document: serde_yaml::Value = serde_yaml::from_str(text).map_err(|error| {
                Error::InvalidParams(format!("{}: {error}", target.path.display()))
            })?;
            let entry = document
                .get("mcp_servers")
                .and_then(|value| value.get("aidget"));
            Ok(entry.map(|entry| yaml_command_matches(entry, command)))
        }
        McpFormat::Dsh => Ok(dsh_has_aidget(text, command)?),
        _ => {
            let entry = jsonc::read_path(text, key_path(target.format))?;
            Ok(entry.map(|entry| json_entry_matches(target.format, &entry, command)))
        }
    }
}

fn edit_json(
    format: McpFormat,
    current: Option<&str>,
    command: &str,
    enabled: bool,
) -> Result<String> {
    let text = current.unwrap_or("{\n}\n");
    let path = key_path(format);
    if enabled {
        jsonc::set_path(text, path, json_entry(format, command))
    } else {
        jsonc::remove_path(text, path)
    }
}

fn edit_toml(path: &Path, current: Option<&str>, command: &str, enabled: bool) -> Result<String> {
    let text = current.unwrap_or("");
    let mut document = text
        .parse::<DocumentMut>()
        .map_err(|error| Error::InvalidParams(format!("{}: {error}", path.display())))?;
    if enabled {
        let servers = document
            .entry("mcp_servers")
            .or_insert(Item::Table(Table::new()))
            .as_table_mut()
            .ok_or_else(|| Error::InvalidParams("mcp_servers is not a table".to_string()))?;
        let mut table = Table::new();
        table["command"] = value(command);
        let mut args = toml_edit::Array::new();
        args.push("mcp");
        table["args"] = value(args);
        servers["aidget"] = Item::Table(table);
    } else if let Some(servers) = document.get_mut("mcp_servers").and_then(Item::as_table_mut) {
        servers.remove("aidget");
    }
    Ok(document.to_string())
}

fn edit_yaml(path: &Path, current: Option<&str>, command: &str, enabled: bool) -> Result<String> {
    let text = current.unwrap_or("");
    let mut document: serde_yaml::Value = if text.trim().is_empty() {
        serde_yaml::Value::Mapping(Default::default())
    } else {
        serde_yaml::from_str(text)
            .map_err(|error| Error::InvalidParams(format!("{}: {error}", path.display())))?
    };
    let root = document
        .as_mapping_mut()
        .ok_or_else(|| Error::InvalidParams("YAML root is not a mapping".to_string()))?;
    let key = serde_yaml::Value::String("mcp_servers".to_string());
    if enabled {
        let servers = root
            .entry(key)
            .or_insert_with(|| serde_yaml::Value::Mapping(Default::default()))
            .as_mapping_mut()
            .ok_or_else(|| Error::InvalidParams("mcp_servers is not a mapping".to_string()))?;
        servers.insert(
            serde_yaml::Value::String("aidget".to_string()),
            yaml_entry(command),
        );
    } else if let Some(servers) = root
        .get_mut(serde_yaml::Value::String("mcp_servers".to_string()))
        .and_then(serde_yaml::Value::as_mapping_mut)
    {
        servers.remove(serde_yaml::Value::String("aidget".to_string()));
    }
    serde_yaml::to_string(&document)
        .map_err(|error| Error::InvalidParams(format!("{}: {error}", path.display())))
}

fn edit_dsh(path: &Path, current: Option<&str>, command: &str, enabled: bool) -> Result<String> {
    let text = current.unwrap_or("[]\n");
    let mut document: serde_yaml::Value = serde_yaml::from_str(text)
        .map_err(|error| Error::InvalidParams(format!("{}: {error}", path.display())))?;
    let list = document
        .as_sequence_mut()
        .ok_or_else(|| Error::InvalidParams("dsh patch root is not a list".to_string()))?;
    list.retain(|entry| !is_aidget_dsh_insert(entry));
    if enabled {
        list.push(aidget_dsh_insert(command));
    }
    serde_yaml::to_string(&document)
        .map_err(|error| Error::InvalidParams(format!("{}: {error}", path.display())))
}

fn json_entry(format: McpFormat, command: &str) -> Value {
    match format {
        McpFormat::Claude | McpFormat::Crush | McpFormat::ZCode => json!({
            "type": "stdio",
            "command": command,
            "args": ["mcp"],
            "env": {}
        }),
        McpFormat::Gemini | McpFormat::Antigravity | McpFormat::Simple | McpFormat::Zed => json!({
            "command": command,
            "args": ["mcp"]
        }),
        McpFormat::OpenCode => json!({
            "type": "local",
            "command": [command, "mcp"],
            "enabled": true
        }),
        McpFormat::Goose => json!({
            "enabled": true,
            "name": "aidget",
            "type": "stdio",
            "cmd": command,
            "args": ["mcp"],
            "timeout": 300
        }),
        McpFormat::Copilot => json!({
            "type": "local",
            "command": command,
            "args": ["mcp"],
            "tools": ["*"]
        }),
        McpFormat::Kimi | McpFormat::Omp => json!({
            "command": command,
            "args": ["mcp"]
        }),
        McpFormat::Devin => json!({
            "command": command,
            "args": ["mcp"],
            "transport": "stdio"
        }),
        McpFormat::Cline => json!({
            "transport": {
                "type": "stdio",
                "command": command,
                "args": ["mcp"]
            }
        }),
        McpFormat::CommandCode => json!({
            "transport": "stdio",
            "enabled": true,
            "command": command,
            "args": ["mcp"]
        }),
        _ => unreachable!("non-JSON MCP format"),
    }
}

fn key_path(format: McpFormat) -> &'static [&'static str] {
    match format {
        McpFormat::OpenCode => &["mcp", "aidget"],
        McpFormat::Crush => &["mcp", "aidget"],
        McpFormat::Goose => &["extensions", "aidget"],
        McpFormat::ZCode => &["mcp", "servers", "aidget"],
        McpFormat::Zed => &["context_servers", "aidget"],
        _ => &["mcpServers", "aidget"],
    }
}

fn json_entry_matches(format: McpFormat, entry: &Value, command: &str) -> bool {
    if format == McpFormat::Cline {
        let transport = entry.get("transport");
        return transport
            .and_then(|value| value.get("command"))
            .and_then(Value::as_str)
            == Some(command)
            && transport.and_then(|value| value.get("args")) == Some(&json!(["mcp"]));
    }
    if format == McpFormat::OpenCode {
        return entry.get("command").and_then(Value::as_array)
            == Some(&vec![json!(command), json!("mcp")]);
    }
    let command_match = entry.get("command").and_then(Value::as_str) == Some(command)
        || entry.get("cmd").and_then(Value::as_str) == Some(command);
    let args_match = entry.get("args") == Some(&json!(["mcp"]));
    command_match && args_match
}

fn toml_entry_matches(entry: &Table, command: &str) -> bool {
    let command_match = entry.get("command").and_then(Item::as_str) == Some(command);
    let args_match = entry
        .get("args")
        .and_then(Item::as_array)
        .is_some_and(|args| args.len() == 1 && args.get(0).and_then(|v| v.as_str()) == Some("mcp"));
    command_match && args_match
}

fn yaml_entry(command: &str) -> serde_yaml::Value {
    serde_yaml::Value::Mapping(
        [
            (
                serde_yaml::Value::String("command".to_string()),
                serde_yaml::Value::String(command.to_string()),
            ),
            (
                serde_yaml::Value::String("args".to_string()),
                serde_yaml::Value::Sequence(vec![serde_yaml::Value::String("mcp".to_string())]),
            ),
        ]
        .into_iter()
        .collect(),
    )
}

fn yaml_command_matches(entry: &serde_yaml::Value, command: &str) -> bool {
    entry.get("command").and_then(serde_yaml::Value::as_str) == Some(command)
        && entry
            .get("args")
            .and_then(serde_yaml::Value::as_sequence)
            .is_some_and(|args| args == &[serde_yaml::Value::String("mcp".to_string())])
}

fn dsh_has_aidget(text: &str, command: &str) -> Result<Option<bool>> {
    let document: serde_yaml::Value = serde_yaml::from_str(text)
        .map_err(|error| Error::InvalidParams(format!("dsh patch: {error}")))?;
    let entry = document
        .as_sequence()
        .and_then(|list| list.iter().find(|entry| is_aidget_dsh_insert(entry)));
    Ok(entry.map(|entry| {
        entry
            .get("insert")
            .and_then(serde_yaml::Value::as_sequence)
            .and_then(|items| items.first())
            .and_then(|item| item.get("config"))
            .is_some_and(|config| yaml_command_matches(config, command))
    }))
}

fn is_aidget_dsh_insert(entry: &serde_yaml::Value) -> bool {
    entry
        .get("insert")
        .and_then(serde_yaml::Value::as_sequence)
        .and_then(|items| items.first())
        .and_then(|item| item.get("id"))
        .and_then(serde_yaml::Value::as_str)
        == Some("magpie-mcp-aidget")
}

fn aidget_dsh_insert(command: &str) -> serde_yaml::Value {
    serde_yaml::to_value(json!({
        "insert": [{
            "id": "magpie-mcp-aidget",
            "name": "@deepseek-ai/dsh-mcp-client",
            "config": {
                "serverName": "aidget",
                "transport": "stdio",
                "command": command,
                "args": ["mcp"]
            }
        }]
    }))
    .expect("dsh MCP insert is valid YAML data")
}

fn backup_bytes(root: &Path, agent: &str, path: &Path, bytes: &[u8]) -> Result<()> {
    let stamp = chrono::Local::now().format("%Y%m%dT%H%M%S%.3f");
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let backup = root
        .join("state")
        .join("agent-backups")
        .join(stamp.to_string())
        .join(agent)
        .join(name);
    atomic_write(&backup, bytes)
}

use std::fmt;
use std::fs;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

const SKILL: &str = include_str!("../../../integrations/agent-skills/sunlight/SKILL.md");
const WORKFLOW: &str =
    include_str!("../../../integrations/agent-skills/sunlight/references/workflow.md");
const SETUP: &str = include_str!("../../../integrations/agent-skills/sunlight/references/setup.md");
const CODEX_START: &str = "# BEGIN SUNLIGHT MANAGED MCP";
const CODEX_END: &str = "# END SUNLIGHT MANAGED MCP";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentClient {
    Generic,
    Codex,
    Cursor,
}

impl AgentClient {
    pub(crate) fn parse(value: &str) -> Result<Self, AgentSetupError> {
        match value {
            "generic" => Ok(Self::Generic),
            "codex" => Ok(Self::Codex),
            "cursor" => Ok(Self::Cursor),
            _ => Err(AgentSetupError::new(
                "agent client must be one of: generic, codex, cursor",
            )),
        }
    }

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Generic => "generic",
            Self::Codex => "codex",
            Self::Cursor => "cursor",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct AgentInstallReport {
    pub(crate) client: AgentClient,
    pub(crate) changed: Vec<String>,
    pub(crate) unchanged: Vec<String>,
    pub(crate) restart_required: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct AgentDoctorReport {
    pub(crate) client: AgentClient,
    pub(crate) healthy: bool,
    pub(crate) mcp_binding_verified: bool,
    pub(crate) repository_initialized: bool,
    pub(crate) current: Vec<String>,
    pub(crate) missing_or_stale: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct AgentSetupError {
    message: String,
}

impl AgentSetupError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AgentSetupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

pub(crate) fn install(
    repository: &Path,
    executable: &Path,
    client: AgentClient,
    force: bool,
) -> Result<AgentInstallReport, AgentSetupError> {
    let mut report = AgentInstallReport {
        client,
        changed: Vec::new(),
        unchanged: Vec::new(),
        restart_required: false,
    };

    install_portable_skill(repository, force, &mut report)?;
    match client {
        AgentClient::Generic => {}
        AgentClient::Codex => install_codex(repository, executable, force, &mut report)?,
        AgentClient::Cursor => install_cursor(repository, executable, force, &mut report)?,
    }
    Ok(report)
}

pub(crate) fn doctor(
    repository: &Path,
    executable: &Path,
    client: AgentClient,
) -> AgentDoctorReport {
    let mut report = AgentDoctorReport {
        client,
        healthy: true,
        mcp_binding_verified: false,
        repository_initialized: repository.join(".sunlight/config.toml").is_file(),
        current: Vec::new(),
        missing_or_stale: Vec::new(),
    };

    for (relative, expected) in portable_skill_files() {
        check_exact(repository, relative, expected, &mut report);
    }
    match client {
        AgentClient::Generic => {}
        AgentClient::Codex => {
            let relative = ".codex/config.toml";
            let expected = codex_block(repository, executable);
            let current = fs::read_to_string(repository.join(relative)).unwrap_or_default();
            let server_name = mcp_server_name(repository);
            let matches = (|| -> Result<bool, AgentSetupError> {
                let managed = codex_managed_range(&current)?;
                Ok(current.contains(&expected)
                    && !contains_mcp_server_outside_managed(&current, "sunlight", managed)?
                    && !contains_mcp_server_outside_managed(&current, &server_name, managed)?)
            })()
            .unwrap_or(false);
            record_check(matches, relative, &mut report);
            report.mcp_binding_verified = matches;
        }
        AgentClient::Cursor => {
            let relative = ".cursor/mcp.json";
            let current = fs::read_to_string(repository.join(relative))
                .ok()
                .and_then(|text| serde_json::from_str::<Value>(&text).ok());
            let expected = cursor_server(repository, executable);
            let server_name = mcp_server_name(repository);
            let servers = current
                .as_ref()
                .and_then(|value| value.get("mcpServers"))
                .and_then(Value::as_object);
            let matches = servers.and_then(|servers| servers.get(&server_name)) == Some(&expected)
                && servers.is_some_and(|servers| !servers.contains_key("sunlight"));
            record_check(matches, relative, &mut report);
            report.mcp_binding_verified = matches;
        }
    }
    report.healthy = report.missing_or_stale.is_empty();
    report
}

fn portable_skill_files() -> [(&'static str, &'static str); 3] {
    [
        (".agents/skills/sunlight/SKILL.md", SKILL),
        (".agents/skills/sunlight/references/workflow.md", WORKFLOW),
        (".agents/skills/sunlight/references/setup.md", SETUP),
    ]
}

fn install_portable_skill(
    repository: &Path,
    force: bool,
    report: &mut AgentInstallReport,
) -> Result<(), AgentSetupError> {
    for (relative, content) in portable_skill_files() {
        install_exact(repository, relative, content, force, report)?;
    }
    Ok(())
}

fn install_exact(
    repository: &Path,
    relative: &str,
    content: &str,
    force: bool,
    report: &mut AgentInstallReport,
) -> Result<(), AgentSetupError> {
    let path = repository.join(relative);
    if fs::read_to_string(&path).ok().as_deref() == Some(content) {
        report.unchanged.push(relative.to_string());
        return Ok(());
    }
    if path.exists() && !force {
        return Err(AgentSetupError::new(format!(
            "refusing to replace `{relative}`; rerun with --force after reviewing it"
        )));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AgentSetupError::new(format!("cannot create `{}`: {error}", parent.display()))
        })?;
    }
    fs::write(&path, content)
        .map_err(|error| AgentSetupError::new(format!("cannot write `{relative}`: {error}")))?;
    report.changed.push(relative.to_string());
    Ok(())
}

fn install_codex(
    repository: &Path,
    executable: &Path,
    force: bool,
    report: &mut AgentInstallReport,
) -> Result<(), AgentSetupError> {
    let relative = ".codex/config.toml";
    let path = repository.join(relative);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let block = codex_block(repository, executable);
    let managed = codex_managed_range(&existing)?;
    let server_name = mcp_server_name(repository);
    if contains_mcp_server_outside_managed(&existing, "sunlight", managed)?
        || contains_mcp_server_outside_managed(&existing, &server_name, managed)?
    {
        return Err(AgentSetupError::new(
            "an unmanaged Sunlight MCP entry exists outside the managed block; remove it before installation",
        ));
    }
    let updated = if let Some((start, end)) = managed {
        if existing[start..end] == block {
            report.unchanged.push(relative.to_string());
            return Ok(());
        }
        let mut value = existing.clone();
        value.replace_range(start..end, &block);
        value
    } else {
        if !existing.is_empty() && !force && !existing.ends_with('\n') {
            return Err(AgentSetupError::new(
                "existing .codex/config.toml does not end with a newline; rerun with --force to append safely",
            ));
        }
        let separator = if existing.trim().is_empty() { "" } else { "\n" };
        format!("{existing}{separator}{block}\n")
    };
    write_client_file(&path, relative, &updated, report)?;
    report.restart_required = true;
    Ok(())
}

fn codex_managed_range(existing: &str) -> Result<Option<(usize, usize)>, AgentSetupError> {
    let Some(start) = existing.find(CODEX_START) else {
        return Ok(None);
    };
    let end = existing[start..]
        .find(CODEX_END)
        .map(|offset| start + offset + CODEX_END.len())
        .ok_or_else(|| AgentSetupError::new("Codex Sunlight managed block is incomplete"))?;
    Ok(Some((start, end)))
}

fn contains_mcp_server_outside_managed(
    existing: &str,
    server_name: &str,
    managed: Option<(usize, usize)>,
) -> Result<bool, AgentSetupError> {
    if !codex_config_has_mcp_server(existing, server_name)? {
        return Ok(false);
    }
    let managed_has_server = managed
        .map(|(start, end)| codex_config_has_mcp_server(&existing[start..end], server_name))
        .transpose()?
        .unwrap_or(false);
    Ok(!managed_has_server)
}

fn codex_config_has_mcp_server(config: &str, server_name: &str) -> Result<bool, AgentSetupError> {
    let config = toml::from_str::<toml::Table>(config).map_err(|error| {
        AgentSetupError::new(format!("cannot parse .codex/config.toml: {error}"))
    })?;
    Ok(config
        .get("mcp_servers")
        .and_then(toml::Value::as_table)
        .is_some_and(|servers| servers.contains_key(server_name)))
}

fn install_cursor(
    repository: &Path,
    executable: &Path,
    force: bool,
    report: &mut AgentInstallReport,
) -> Result<(), AgentSetupError> {
    let relative = ".cursor/mcp.json";
    let path = repository.join(relative);
    let mut root = if path.exists() {
        let text = fs::read_to_string(&path)
            .map_err(|error| AgentSetupError::new(format!("cannot read `{relative}`: {error}")))?;
        serde_json::from_str::<Value>(&text).map_err(|error| {
            AgentSetupError::new(format!("cannot parse existing `{relative}`: {error}"))
        })?
    } else {
        json!({})
    };
    let object = root
        .as_object_mut()
        .ok_or_else(|| AgentSetupError::new("existing .cursor/mcp.json must contain an object"))?;
    let servers = object
        .entry("mcpServers")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| AgentSetupError::new(".cursor/mcp.json mcpServers must be an object"))?;
    let expected = cursor_server(repository, executable);
    let server_name = mcp_server_name(repository);
    let has_legacy_binding = servers
        .get("sunlight")
        .is_some_and(|server| cursor_server_targets_repository(server, repository));
    if servers.contains_key("sunlight") && !has_legacy_binding {
        return Err(AgentSetupError::new(
            "a legacy Cursor MCP server named sunlight targets another repository or tool; rename or remove it before installation",
        ));
    }
    if servers.get(&server_name) == Some(&expected) && !has_legacy_binding {
        report.unchanged.push(relative.to_string());
        return Ok(());
    }
    if servers.contains_key(&server_name) && servers.get(&server_name) != Some(&expected) && !force
    {
        return Err(AgentSetupError::new(
            format!(
                "a different Cursor MCP server named {server_name} already exists; rerun with --force after reviewing it"
            ),
        ));
    }
    if has_legacy_binding {
        servers.remove("sunlight");
    }
    servers.insert(server_name, expected);
    let text = serde_json::to_string_pretty(&root)
        .map_err(|error| AgentSetupError::new(format!("cannot encode `{relative}`: {error}")))?;
    write_client_file(&path, relative, &format!("{text}\n"), report)?;
    report.restart_required = true;
    Ok(())
}

fn write_client_file(
    path: &Path,
    relative: &str,
    content: &str,
    report: &mut AgentInstallReport,
) -> Result<(), AgentSetupError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            AgentSetupError::new(format!("cannot create `{}`: {error}", parent.display()))
        })?;
    }
    fs::write(path, content)
        .map_err(|error| AgentSetupError::new(format!("cannot write `{relative}`: {error}")))?;
    report.changed.push(relative.to_string());
    Ok(())
}

fn codex_block(repository: &Path, executable: &Path) -> String {
    let server_name = mcp_server_name(repository);
    format!(
        "{CODEX_START}\n[mcp_servers.{server_name}]\ncommand = '{}'\nargs = ['mcp', 'serve', '--repo', '{}']\nstartup_timeout_sec = 15\ntool_timeout_sec = 900\n{CODEX_END}",
        toml_literal(&display_path(executable)),
        toml_literal(&display_path(repository))
    )
}

pub(crate) fn repository_binding_id(repository: &Path) -> String {
    let mut hasher = Sha256::new();
    hasher.update(display_path(repository).as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

pub(crate) fn mcp_server_name(repository: &Path) -> String {
    let binding = repository_binding_id(repository);
    format!("sunlight_{}", &binding["sha256:".len()..][..16])
}

fn toml_literal(value: &str) -> String {
    value.replace('\'', "''")
}

pub(crate) fn display_path(path: &Path) -> String {
    let value = path.display().to_string();
    #[cfg(windows)]
    {
        if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{unc}");
        }
        if let Some(local) = value.strip_prefix(r"\\?\") {
            return local.to_string();
        }
    }
    value
}

fn cursor_server(repository: &Path, executable: &Path) -> Value {
    json!({
        "command": display_path(executable),
        "args": ["mcp", "serve", "--repo", display_path(repository)]
    })
}

fn cursor_server_targets_repository(server: &Value, repository: &Path) -> bool {
    let Some(args) = server.get("args").and_then(Value::as_array) else {
        return false;
    };
    server.get("command").and_then(Value::as_str).is_some()
        && args.len() == 4
        && args[0].as_str() == Some("mcp")
        && args[1].as_str() == Some("serve")
        && args[2].as_str() == Some("--repo")
        && args[3].as_str() == Some(display_path(repository).as_str())
}

fn check_exact(repository: &Path, relative: &str, expected: &str, report: &mut AgentDoctorReport) {
    let matches = fs::read_to_string(repository.join(relative))
        .ok()
        .as_deref()
        == Some(expected);
    record_check(matches, relative, report);
}

fn record_check(matches: bool, relative: &str, report: &mut AgentDoctorReport) {
    if matches {
        report.current.push(relative.to_string());
    } else {
        report.missing_or_stale.push(relative.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT_TEMP_ID: std::sync::atomic::AtomicU64 =
                std::sync::atomic::AtomicU64::new(0);
            let sequence = NEXT_TEMP_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "sun-agent-setup-{}-{}-{sequence}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn generic_install_is_portable_and_idempotent() {
        let temp = TempDir::new();
        let executable = temp.0.join("sun");
        let first = install(&temp.0, &executable, AgentClient::Generic, false).unwrap();
        assert_eq!(first.changed.len(), 3);
        let second = install(&temp.0, &executable, AgentClient::Generic, false).unwrap();
        assert!(second.changed.is_empty());
        assert_eq!(second.unchanged.len(), 3);
        assert!(doctor(&temp.0, &executable, AgentClient::Generic).healthy);
        assert!(!doctor(&temp.0, &executable, AgentClient::Generic).mcp_binding_verified);
    }

    #[test]
    fn codex_adapter_is_synchronized_with_the_portable_skill() {
        assert_eq!(
            SKILL,
            include_str!("../../../integrations/codex/plugins/sunlight/skills/sunlight/SKILL.md")
        );
        assert_eq!(
            WORKFLOW,
            include_str!(
                "../../../integrations/codex/plugins/sunlight/skills/sunlight/references/workflow.md"
            )
        );
        assert_eq!(
            SETUP,
            include_str!(
                "../../../integrations/codex/plugins/sunlight/skills/sunlight/references/setup.md"
            )
        );
    }

    #[cfg(windows)]
    #[test]
    fn display_path_hides_windows_extended_path_prefixes() {
        assert_eq!(
            display_path(Path::new(r"\\?\C:\source\repo")),
            r"C:\source\repo"
        );
        assert_eq!(
            display_path(Path::new(r"\\?\UNC\server\share\repo")),
            r"\\server\share\repo"
        );
    }

    #[test]
    fn cursor_install_preserves_unrelated_servers() {
        let temp = TempDir::new();
        let cursor = temp.0.join(".cursor");
        fs::create_dir_all(&cursor).unwrap();
        fs::write(
            cursor.join("mcp.json"),
            "{\"mcpServers\":{\"other\":{\"command\":\"other\"}}}",
        )
        .unwrap();
        let executable = temp.0.join("sun");
        install(&temp.0, &executable, AgentClient::Cursor, false).unwrap();
        let value: Value =
            serde_json::from_str(&fs::read_to_string(cursor.join("mcp.json")).unwrap()).unwrap();
        assert_eq!(value["mcpServers"]["other"]["command"], "other");
        let server_name = mcp_server_name(&temp.0);
        assert_eq!(
            value["mcpServers"][server_name]["command"],
            executable.display().to_string()
        );
        assert!(doctor(&temp.0, &executable, AgentClient::Cursor).healthy);
    }

    #[test]
    fn codex_install_updates_only_its_managed_block() {
        let temp = TempDir::new();
        let codex = temp.0.join(".codex");
        fs::create_dir_all(&codex).unwrap();
        fs::write(codex.join("config.toml"), "model = 'example'\n").unwrap();
        let executable = temp.0.join("sun");
        install(&temp.0, &executable, AgentClient::Codex, false).unwrap();
        let config = fs::read_to_string(codex.join("config.toml")).unwrap();
        assert!(config.contains("model = 'example'"));
        assert!(config.contains(CODEX_START));
        assert!(doctor(&temp.0, &executable, AgentClient::Codex).healthy);
    }

    #[test]
    fn codex_install_migrates_the_legacy_managed_server_name() {
        let temp = TempDir::new();
        let codex = temp.0.join(".codex");
        fs::create_dir_all(&codex).unwrap();
        fs::write(
            codex.join("config.toml"),
            format!(
                "model = 'example'\n{CODEX_START}\n[mcp_servers.sunlight]\ncommand = 'old-sun'\nargs = ['mcp', 'serve', '--repo', '{}']\n{CODEX_END}\n",
                display_path(&temp.0)
            ),
        )
        .unwrap();
        let executable = temp.0.join("sun");

        install(&temp.0, &executable, AgentClient::Codex, false).unwrap();

        let config = fs::read_to_string(codex.join("config.toml")).unwrap();
        assert!(config.contains("model = 'example'"));
        assert!(!config.contains("[mcp_servers.sunlight]"));
        assert!(config.contains(&format!("[mcp_servers.{}]", mcp_server_name(&temp.0))));
        assert!(doctor(&temp.0, &executable, AgentClient::Codex).healthy);
    }

    #[test]
    fn codex_doctor_and_install_reject_an_unmanaged_legacy_duplicate() {
        for legacy in [
            "[mcp_servers.\"sunlight\"]\ncommand = 'old-sun'\nargs = ['mcp', 'serve', '--repo', 'legacy']\n",
            "[mcp_servers]\nsunlight = { command = 'old-sun', args = ['mcp', 'serve', '--repo', 'legacy'] }\n",
            "mcp_servers.sunlight.command = 'old-sun'\nmcp_servers.sunlight.args = ['mcp', 'serve', '--repo', 'legacy']\n",
        ] {
            let temp = TempDir::new();
            let executable = temp.0.join("sun");
            fs::create_dir_all(temp.0.join(".codex")).unwrap();
            fs::write(
                temp.0.join(".codex/config.toml"),
                "# [mcp_servers.sunlight]\n",
            )
            .unwrap();
            install(&temp.0, &executable, AgentClient::Codex, false).unwrap();
            let config_path = temp.0.join(".codex/config.toml");
            let config = fs::read_to_string(&config_path).unwrap();
            fs::write(&config_path, format!("{legacy}\n{config}")).unwrap();

            assert!(!doctor(&temp.0, &executable, AgentClient::Codex).healthy);
            assert!(install(&temp.0, &executable, AgentClient::Codex, false).is_err());
        }
    }

    #[test]
    fn repository_mcp_names_are_stable_and_distinct() {
        let first = TempDir::new();
        let second = TempDir::new();
        let first_name = mcp_server_name(&first.0);
        assert_eq!(first_name, mcp_server_name(&first.0));
        assert_ne!(first_name, mcp_server_name(&second.0));
        assert!(first_name.starts_with("sunlight_"));
        assert_eq!(first_name.len(), "sunlight_".len() + 16);
    }

    #[test]
    fn cursor_install_migrates_the_legacy_repository_binding() {
        let temp = TempDir::new();
        let cursor = temp.0.join(".cursor");
        fs::create_dir_all(&cursor).unwrap();
        let executable = temp.0.join("sun");
        let legacy_executable = temp.0.join("old-sun");
        fs::write(
            cursor.join("mcp.json"),
            serde_json::to_string(&json!({
                "mcpServers": {"sunlight": cursor_server(&temp.0, &legacy_executable)}
            }))
            .unwrap(),
        )
        .unwrap();

        install(&temp.0, &executable, AgentClient::Cursor, false).unwrap();

        let value: Value =
            serde_json::from_str(&fs::read_to_string(cursor.join("mcp.json")).unwrap()).unwrap();
        assert!(value["mcpServers"].get("sunlight").is_none());
        assert_eq!(
            value["mcpServers"][mcp_server_name(&temp.0)],
            cursor_server(&temp.0, &executable)
        );
    }

    #[test]
    fn cursor_install_removes_a_legacy_duplicate_before_becoming_healthy() {
        let temp = TempDir::new();
        let executable = temp.0.join("sun");
        install(&temp.0, &executable, AgentClient::Cursor, false).unwrap();
        let config_path = temp.0.join(".cursor/mcp.json");
        let mut value: Value =
            serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        value["mcpServers"]["sunlight"] = cursor_server(&temp.0, &executable);
        fs::write(&config_path, serde_json::to_string_pretty(&value).unwrap()).unwrap();

        assert!(!doctor(&temp.0, &executable, AgentClient::Cursor).healthy);
        install(&temp.0, &executable, AgentClient::Cursor, false).unwrap();
        assert!(doctor(&temp.0, &executable, AgentClient::Cursor).healthy);
        let migrated: Value =
            serde_json::from_str(&fs::read_to_string(&config_path).unwrap()).unwrap();
        assert!(migrated["mcpServers"].get("sunlight").is_none());
    }
}

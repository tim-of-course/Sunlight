use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use sunlight_core::repo_state::{
    directory_is_gitignored_by_repository_file, materialize_private_runtime_dependency_tree,
    path_is_sunignored, real_content_hash, scan_real_execution_projection_files_with_quarantine,
    RealArtifactEntry, RealExecutionRuntimeLayer, RealExecutionRuntimeLayerConstruction,
    RealExecutionRuntimeLayerInput, RealExecutionRuntimeLayerTarget,
};
use sunlight_core::repository::{ExecutionPolicy, DISABLED_NETWORK_POLICY};

use super::{path_exists_without_following, run_bounded_process_with_environment, CliError};

const PROVIDER_ID: &str = "bun_single_root";
const PROVIDER_SEMANTICS: &str =
    "bun_single_root:root_package_json:text_bun_lock:hoisted:frozen:private_environment:exact_content_manifest:source_immutable:private_cow_binding:truthful_construction_policy";
const RUNTIME_LAYER_ROOT: &str = ".runtime-layers";
const CACHE_MANIFEST: &str = "manifest.json";
const TARGET_PATH: &str = "node_modules";
const LOCK_POLL: Duration = Duration::from_millis(10);

static ATTEMPT_NONCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Default)]
pub(crate) struct RuntimeLayerTimings {
    pub provider_discovery_ms: u128,
    pub cache_lookup_ms: u128,
    pub cache_wait_ms: u128,
    pub provider_preparation_ms: u128,
    pub private_binding_ms: u128,
}

#[derive(Debug, Default)]
pub(crate) struct RuntimeLayerResolution {
    pub layers: Vec<RealExecutionRuntimeLayer>,
    pub target_paths: Vec<String>,
    pub timings: RuntimeLayerTimings,
}

#[derive(Debug)]
enum ProviderOutcome {
    NotApplicable,
    Required(BunProviderPlan),
    RecognizedUnsupported(String),
}

#[derive(Debug)]
struct BunProviderPlan {
    root: String,
    declared_version: Option<String>,
    manager_identity: Option<String>,
    inputs: Vec<RealExecutionRuntimeLayerInput>,
}

pub(crate) fn acquire_runtime_layers(
    repo_root: &Path,
    managed_root: &Path,
    projection_root: &Path,
    relative_cwd: &Path,
    view_entries: &[RealArtifactEntry],
    policy: &ExecutionPolicy,
    cancellation: &AtomicBool,
) -> Result<RuntimeLayerResolution, CliError> {
    let acquisition_started = Instant::now();
    let deadline = acquisition_started + Duration::from_millis(policy.timeout_ms);
    let discovery_started = Instant::now();
    let outcome = discover_bun_provider(relative_cwd, view_entries)?;
    let mut result = RuntimeLayerResolution::default();
    result.timings.provider_discovery_ms = discovery_started.elapsed().as_millis();
    let plan = match outcome {
        ProviderOutcome::NotApplicable => return Ok(result),
        ProviderOutcome::Required(plan) => plan,
        ProviderOutcome::RecognizedUnsupported(reason) => {
            return Err(CliError::new(
                "runtime_layer_provider_unsupported",
                "the resolved view uses a recognized dependency layout that this runtime provider cannot prepare",
            )
            .with_detail("provider_id", PROVIDER_ID)
            .with_detail("reason", reason));
        }
    };

    validate_target(repo_root, projection_root, view_entries)?;
    let provider_semantics_digest = real_content_hash(PROVIDER_SEMANTICS.as_bytes());
    let canonical_environment = canonical_construction_environment();
    let lookup_key =
        runtime_layer_lookup_key(&plan, &provider_semantics_digest, &canonical_environment)?;
    let key_digest = lookup_key.trim_start_matches("sha256:");
    let runtime_root = managed_root.join(RUNTIME_LAYER_ROOT);
    let entry_root = runtime_root.join("entries").join(key_digest);

    let lookup_started = Instant::now();
    if let Ok(Some(layer)) = read_valid_cache_entry(
        &entry_root,
        &lookup_key,
        &provider_semantics_digest,
        &plan.inputs,
    ) {
        result.timings.cache_lookup_ms = lookup_started.elapsed().as_millis();
        bind_layer(
            &entry_root,
            projection_root,
            layer,
            &mut result,
            cancellation,
            deadline,
        )?;
        return Ok(result);
    }
    result.timings.cache_lookup_ms = lookup_started.elapsed().as_millis();

    fs::create_dir_all(runtime_root.join("locks")).map_err(|error| {
        runtime_io_error(
            "runtime_layer_storage_failed",
            &runtime_root,
            "failed to create runtime layer storage",
            error,
        )
    })?;
    let lock_path = runtime_root
        .join("locks")
        .join(format!("{key_digest}.lock"));
    let wait_started = Instant::now();
    let lock = RuntimeLayerKeyLock::acquire(&lock_path, deadline, cancellation)?;
    result.timings.cache_wait_ms = wait_started.elapsed().as_millis();

    let recheck_started = Instant::now();
    match read_valid_cache_entry(
        &entry_root,
        &lookup_key,
        &provider_semantics_digest,
        &plan.inputs,
    ) {
        Ok(Some(layer)) => {
            result.timings.cache_lookup_ms += recheck_started.elapsed().as_millis();
            drop(lock);
            bind_layer(
                &entry_root,
                projection_root,
                layer,
                &mut result,
                cancellation,
                deadline,
            )?;
            return Ok(result);
        }
        Ok(None) => {}
        Err(error) => {
            quarantine_entry(&runtime_root, &entry_root, key_digest)?;
            result.timings.cache_lookup_ms += recheck_started.elapsed().as_millis();
            if policy.network_policy == DISABLED_NETWORK_POLICY {
                return Err(CliError::new(
                    "runtime_layer_network_unavailable",
                    "the required runtime layer is not cached and dependency downloads are disabled",
                )
                .with_detail("provider_id", PROVIDER_ID)
                .with_detail("lookup_key", lookup_key)
                .with_detail("cache_error", error.message));
            }
        }
    }
    result.timings.cache_lookup_ms += recheck_started.elapsed().as_millis();

    if policy.network_policy == DISABLED_NETWORK_POLICY {
        return Err(CliError::new(
            "runtime_layer_network_unavailable",
            "the required runtime layer is not cached and dependency downloads are disabled",
        )
        .with_detail("provider_id", PROVIDER_ID)
        .with_detail("lookup_key", lookup_key));
    }
    if cancellation.load(Ordering::Acquire) {
        return Err(cancelled_error("runtime layer acquisition was cancelled"));
    }
    if Instant::now() >= deadline {
        return Err(acquisition_timeout_error(policy.timeout_ms));
    }

    let preparation_started = Instant::now();
    let layer = build_and_publish_layer(
        &runtime_root,
        &entry_root,
        projection_root,
        view_entries,
        &plan,
        &lookup_key,
        &provider_semantics_digest,
        &canonical_environment,
        policy,
        cancellation,
        deadline,
    )?;
    result.timings.provider_preparation_ms = preparation_started.elapsed().as_millis();
    drop(lock);
    bind_layer(
        &entry_root,
        projection_root,
        layer,
        &mut result,
        cancellation,
        deadline,
    )?;
    Ok(result)
}

fn discover_bun_provider(
    relative_cwd: &Path,
    entries: &[RealArtifactEntry],
) -> Result<ProviderOutcome, CliError> {
    let mut candidates = Vec::new();
    let mut current = relative_cwd.to_path_buf();
    loop {
        candidates.push(current.clone());
        if !current.pop() {
            break;
        }
    }
    for root in candidates {
        let package_path = relative_path(&root, "package.json");
        let lock_path = relative_path(&root, "bun.lock");
        let binary_lock_path = relative_path(&root, "bun.lockb");
        let package = entry(entries, &package_path);
        let lock = entry(entries, &lock_path);
        let binary_lock = entry(entries, &binary_lock_path);
        let package_json = match package {
            Some(package) => Some(parse_json_source(package, "package.json")?),
            None => None,
        };
        let manager = package_json
            .as_ref()
            .and_then(|value| value.get("packageManager"))
            .and_then(Value::as_str)
            .map(str::to_string);
        let explicit_bun = manager
            .as_deref()
            .is_some_and(|value| value == "bun" || value.starts_with("bun@"));
        if manager
            .as_deref()
            .is_some_and(|value| value != "bun" && !value.starts_with("bun@"))
            && (lock.is_some() || binary_lock.is_some())
        {
            return Err(CliError::new(
                "runtime_layer_provider_ambiguous",
                "the repository declares a non-Bun package manager but also contains a Bun lockfile",
            )
            .with_detail("package_json", package_path));
        }
        if explicit_bun && lock.is_none() {
            return Ok(ProviderOutcome::RecognizedUnsupported(
                if binary_lock.is_some() {
                    "binary bun.lockb lockfiles are not supported; use the text bun.lock format"
                        .to_string()
                } else {
                    "the Bun package manager is declared but the resolved view has no bun.lock"
                        .to_string()
                },
            ));
        }
        if package.is_some() && lock.is_none() && binary_lock.is_some() {
            return Ok(ProviderOutcome::RecognizedUnsupported(
                "binary bun.lockb lockfiles are not supported; use the text bun.lock format"
                    .to_string(),
            ));
        }
        let (Some(package), Some(package_json), Some(lock)) = (package, package_json, lock) else {
            continue;
        };
        if !root.as_os_str().is_empty() {
            return Ok(ProviderOutcome::RecognizedUnsupported(
                "the initial Bun provider supports only a repository-root install".to_string(),
            ));
        }
        validate_package_json(&package_json)?;
        let lock_json = parse_bun_lock(lock)?;
        validate_bun_lock(&lock_json)?;
        let mut inputs = vec![runtime_input(package), runtime_input(lock)];
        for optional in ["bunfig.toml", ".npmrc"] {
            if let Some(input) = entry(entries, optional) {
                validate_provider_config(optional, &input.bytes)?;
                inputs.push(runtime_input(input));
            }
        }
        inputs.sort_by(|left, right| left.path.cmp(&right.path));
        let declared_version = manager
            .as_deref()
            .and_then(|value| value.strip_prefix("bun@"))
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        return Ok(ProviderOutcome::Required(BunProviderPlan {
            root: String::new(),
            declared_version,
            manager_identity: manager,
            inputs,
        }));
    }
    Ok(ProviderOutcome::NotApplicable)
}

fn validate_package_json(package: &Value) -> Result<(), CliError> {
    if package.get("workspaces").is_some() {
        return Err(provider_unsupported(
            "package.json workspaces are not supported by the initial Bun provider",
        ));
    }
    if let Some(scripts) = package.get("scripts").and_then(Value::as_object) {
        for script in [
            "preinstall",
            "install",
            "postinstall",
            "preprepare",
            "prepare",
            "postprepare",
        ] {
            if scripts.contains_key(script) {
                return Err(provider_unsupported(&format!(
                    "project lifecycle script `{script}` is not supported during runtime layer preparation"
                )));
            }
        }
    }
    if value_contains_unsupported_reference(package) {
        return Err(provider_unsupported(
            "file:, link:, and workspace: dependency references are not supported by the initial Bun provider",
        ));
    }
    if package.get("patchedDependencies").is_some() {
        return Err(provider_unsupported(
            "repository patch dependencies are not supported by the initial Bun provider",
        ));
    }
    Ok(())
}

fn validate_bun_lock(lock: &Value) -> Result<(), CliError> {
    let workspaces = lock
        .get("workspaces")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            provider_unsupported("bun.lock does not contain a recognized workspaces map")
        })?;
    if workspaces.keys().any(|key| !key.is_empty()) {
        return Err(provider_unsupported(
            "bun.lock contains workspace members; only the root workspace is supported",
        ));
    }
    if value_contains_unsupported_reference(lock) {
        return Err(provider_unsupported(
            "bun.lock contains a local file, link, or workspace dependency",
        ));
    }
    if value_contains_key(lock, "patchedDependencies") {
        return Err(provider_unsupported(
            "bun.lock contains repository patch dependencies, which are not supported by the initial provider",
        ));
    }
    Ok(())
}

fn validate_provider_config(path: &str, bytes: &[u8]) -> Result<(), CliError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| provider_unsupported(&format!("{path} must contain UTF-8 text")))?;
    if text.contains("${") {
        return Err(provider_unsupported(&format!(
            "{path} contains an environment substitution; runtime layer keys cannot depend on ambient secrets"
        )));
    }
    if path == "bunfig.toml" {
        let config = toml::from_str::<toml::Value>(text).map_err(|error| {
            provider_unsupported(&format!("bunfig.toml is invalid TOML: {error}"))
        })?;
        let install = config.get("install");
        if install
            .and_then(|value| value.get("linker"))
            .and_then(toml::Value::as_str)
            .is_some_and(|linker| linker != "hoisted")
        {
            return Err(provider_unsupported(
                "bunfig.toml selects a non-hoisted linker, which the initial provider does not support",
            ));
        }
        let has_external_file_input = install.is_some_and(|install| {
            ["cafile", "globalDir", "globalBinDir"]
                .iter()
                .any(|key| install.get(key).is_some())
                || install
                    .get("cache")
                    .and_then(|cache| cache.get("dir"))
                    .is_some()
        });
        if has_external_file_input {
            return Err(provider_unsupported(
                "bunfig.toml selects a filesystem path that is not yet a declared runtime-layer input",
            ));
        }
    }
    for line in text.lines() {
        let key = line
            .split_once('=')
            .map(|(key, _)| key.trim())
            .unwrap_or("");
        let value = line
            .split_once('=')
            .map(|(_, value)| value.trim().trim_matches(['\'', '"']))
            .unwrap_or("");
        if path == ".npmrc"
            && [
                "cache",
                "cafile",
                "certfile",
                "globalconfig",
                "keyfile",
                "prefix",
                "userconfig",
            ]
            .contains(&key)
            && !value.is_empty()
        {
            return Err(provider_unsupported(
                ".npmrc selects a filesystem path that is not yet a declared runtime-layer input",
            ));
        }
        if value.starts_with('/')
            || value.starts_with("../")
            || value.contains("/../")
            || value.starts_with("file:")
        {
            return Err(provider_unsupported(&format!(
                "{path} refers to a path outside the declared resolved-view inputs"
            )));
        }
        if path == ".npmrc" && key == "node-linker" && !value.is_empty() && value != "hoisted" {
            return Err(provider_unsupported(
                ".npmrc selects a non-hoisted linker, which the initial provider does not support",
            ));
        }
    }
    Ok(())
}

fn parse_json_source(entry: &RealArtifactEntry, name: &str) -> Result<Value, CliError> {
    serde_json::from_slice(&entry.bytes)
        .map_err(|error| provider_unsupported(&format!("{name} is not valid JSON: {error}")))
}

fn parse_bun_lock(entry: &RealArtifactEntry) -> Result<Value, CliError> {
    let text = std::str::from_utf8(&entry.bytes)
        .map_err(|_| provider_unsupported("bun.lock must contain UTF-8 text"))?;
    let normalized = normalize_jsonc(text);
    serde_json::from_str(&normalized)
        .map_err(|error| provider_unsupported(&format!("bun.lock is not valid text JSON: {error}")))
}

fn normalize_jsonc(text: &str) -> String {
    let mut without_comments = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if in_string {
            without_comments.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            without_comments.push(ch);
            continue;
        }
        if ch == '/' && chars.peek() == Some(&'/') {
            chars.next();
            for next in chars.by_ref() {
                if next == '\n' {
                    without_comments.push('\n');
                    break;
                }
            }
            continue;
        }
        without_comments.push(ch);
    }
    let mut normalized = String::with_capacity(without_comments.len());
    let mut chars = without_comments.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if in_string {
            normalized.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if ch == '"' {
            in_string = true;
            normalized.push(ch);
            continue;
        }
        if ch == ',' {
            let mut lookahead = chars.clone();
            while lookahead.peek().is_some_and(|value| value.is_whitespace()) {
                lookahead.next();
            }
            if matches!(lookahead.peek(), Some('}' | ']')) {
                continue;
            }
        }
        normalized.push(ch);
    }
    normalized
}

fn value_contains_unsupported_reference(value: &Value) -> bool {
    match value {
        Value::String(value) => ["file:", "link:", "workspace:"]
            .iter()
            .any(|prefix| value.starts_with(prefix)),
        Value::Array(values) => values.iter().any(value_contains_unsupported_reference),
        Value::Object(values) => values.values().any(value_contains_unsupported_reference),
        _ => false,
    }
}

fn value_contains_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Array(values) => values.iter().any(|value| value_contains_key(value, key)),
        Value::Object(values) => {
            values.contains_key(key) || values.values().any(|value| value_contains_key(value, key))
        }
        _ => false,
    }
}

fn validate_target(
    repo_root: &Path,
    projection_root: &Path,
    entries: &[RealArtifactEntry],
) -> Result<(), CliError> {
    if entries
        .iter()
        .filter(|entry| !entry.tombstone)
        .any(|entry| {
            entry.path == TARGET_PATH || entry.path.starts_with(&format!("{TARGET_PATH}/"))
        })
    {
        return Err(CliError::new(
            "runtime_layer_target_conflict",
            "the runtime layer target overlaps native resolved-view content",
        )
        .with_detail("path", TARGET_PATH));
    }
    if path_is_sunignored(repo_root, TARGET_PATH, true).map_err(CliError::from)? {
        return Err(CliError::new(
            "runtime_layer_target_conflict",
            "the runtime layer target is excluded by the repository's .sunignore policy",
        )
        .with_detail("path", TARGET_PATH));
    }
    let gitignored = directory_is_gitignored_by_repository_file(projection_root, TARGET_PATH)
        .map_err(CliError::from)?;
    if !gitignored {
        return Err(CliError::new(
            "runtime_layer_target_not_gitignored",
            "the runtime layer target must be Git-ignored in the exact resolved view",
        )
        .with_detail("path", TARGET_PATH));
    }
    Ok(())
}

fn runtime_layer_lookup_key(
    plan: &BunProviderPlan,
    provider_semantics_digest: &str,
    environment: &BTreeMap<String, String>,
) -> Result<String, CliError> {
    let inputs = plan
        .inputs
        .iter()
        .map(|input| {
            json!({
                "path": input.path,
                "artifact_id": input.artifact_id,
                "content_hash": input.content_hash,
            })
        })
        .collect::<Vec<_>>();
    let value = json!({
        "provider_id": PROVIDER_ID,
        "provider_semantics_digest": provider_semantics_digest,
        "targets": [TARGET_PATH],
        "inputs": inputs,
        "manager_identity": plan.manager_identity,
        "os": env::consts::OS,
        "arch": env::consts::ARCH,
        "environment": environment,
    });
    let bytes = serde_json::to_vec(&value).map_err(|error| {
        CliError::new(
            "runtime_layer_key_failed",
            format!("failed to encode runtime layer key: {error}"),
        )
    })?;
    Ok(real_content_hash(&bytes))
}

fn canonical_construction_environment() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("HOME".to_string(), "PRIVATE_HOME".to_string()),
        ("USERPROFILE".to_string(), "PRIVATE_HOME".to_string()),
        ("HOMEPATH".to_string(), "PRIVATE_HOME".to_string()),
        (
            "XDG_CONFIG_HOME".to_string(),
            "PRIVATE_HOME/config".to_string(),
        ),
        ("APPDATA".to_string(), "PRIVATE_HOME/appdata".to_string()),
        (
            "LOCALAPPDATA".to_string(),
            "PRIVATE_HOME/local-appdata".to_string(),
        ),
        ("TEMP".to_string(), "PRIVATE_TEMP".to_string()),
        ("TMP".to_string(), "PRIVATE_TEMP".to_string()),
        ("TMPDIR".to_string(), "PRIVATE_TEMP".to_string()),
        (
            "BUN_INSTALL_CACHE_DIR".to_string(),
            "PRIVATE_PACKAGE_CACHE".to_string(),
        ),
        ("PATH".to_string(), "CONTROLLED_PATH".to_string()),
    ])
}

#[allow(clippy::too_many_arguments)]
fn build_and_publish_layer(
    runtime_root: &Path,
    entry_root: &Path,
    projection_root: &Path,
    view_entries: &[RealArtifactEntry],
    plan: &BunProviderPlan,
    lookup_key: &str,
    provider_semantics_digest: &str,
    canonical_environment: &BTreeMap<String, String>,
    policy: &ExecutionPolicy,
    cancellation: &AtomicBool,
    deadline: Instant,
) -> Result<RealExecutionRuntimeLayer, CliError> {
    #[cfg(windows)]
    {
        let _ = (
            runtime_root,
            entry_root,
            projection_root,
            view_entries,
            plan,
            lookup_key,
            provider_semantics_digest,
            canonical_environment,
            policy,
            cancellation,
            deadline,
        );
        return Err(CliError::new(
            "runtime_layer_provider_tool_incompatible",
            "the initial Bun runtime provider cannot yet run inside Sunlight's Windows containment",
        )
        .with_detail("provider_id", PROVIDER_ID));
    }
    #[cfg(not(windows))]
    {
        let attempt = attempt_id();
        let attempt_root = runtime_root.join("staging").join(&attempt);
        let staged_entry = attempt_root.join("entry");
        let private_root = attempt_root.join("private");
        let private_home = private_root.join("home");
        let private_temp = private_root.join("tmp");
        let private_cache = private_root.join("package-cache");
        for path in [&staged_entry, &private_home, &private_temp, &private_cache] {
            fs::create_dir_all(path).map_err(|error| {
                runtime_io_error(
                    "runtime_layer_storage_failed",
                    path,
                    "failed to create runtime layer staging directory",
                    error,
                )
            })?;
        }
        let cleanup = AttemptCleanup(attempt_root.clone());
        let bun = find_executable("bun").ok_or_else(|| {
            CliError::new(
                "runtime_layer_provider_tool_missing",
                "the resolved view requires a Bun runtime layer but Bun was not found in PATH",
            )
            .with_detail("provider_id", PROVIDER_ID)
        })?;
        let executable_bytes = fs::read(&bun).map_err(|error| {
            runtime_io_error(
                "runtime_layer_provider_tool_failed",
                &bun,
                "failed to read the Bun executable for provenance",
                error,
            )
        })?;
        let executable_digest = real_content_hash(&executable_bytes);
        let actual_environment =
            actual_construction_environment(&private_home, &private_temp, &private_cache);
        let version_output = run_provider_command(
            &[bun.display().to_string(), "--version".to_string()],
            projection_root,
            policy,
            cancellation,
            deadline,
            &actual_environment,
        )?;
        let reported_version = String::from_utf8_lossy(&version_output.stdout.captured_bytes)
            .trim()
            .to_string();
        if reported_version.is_empty() {
            return Err(CliError::new(
                "runtime_layer_provider_tool_failed",
                "Bun did not report a version",
            ));
        }
        if let Some(expected) = &plan.declared_version {
            if expected != &reported_version {
                return Err(CliError::new(
                    "runtime_layer_provider_tool_version_mismatch",
                    "the installed Bun version does not match package.json packageManager",
                )
                .with_detail("expected", expected.clone())
                .with_detail("actual", reported_version));
            }
        }
        let install_cwd = projection_root.join(&plan.root);
        let actual_argv = vec![
            bun.display().to_string(),
            "install".to_string(),
            "--frozen-lockfile".to_string(),
            "--linker".to_string(),
            "hoisted".to_string(),
            "--concurrent-scripts".to_string(),
            "1".to_string(),
            "--cache-dir".to_string(),
            private_cache.display().to_string(),
        ];
        let install_output = run_provider_command(
            &actual_argv,
            &install_cwd,
            policy,
            cancellation,
            deadline,
            &actual_environment,
        )?;
        if !install_output.status.is_some_and(|status| status.success()) {
            return Err(CliError::new(
                "runtime_layer_preparation_failed",
                "Bun failed while preparing the runtime layer",
            )
            .with_detail(
                "exit_code",
                install_output
                    .status
                    .and_then(|status| status.code())
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
            )
            .with_detail(
                "stderr",
                String::from_utf8_lossy(&install_output.stderr.captured_bytes)
                    .trim()
                    .to_string(),
            ));
        }
        ensure_acquisition_active(cancellation, deadline)?;
        verify_provider_did_not_modify_source(projection_root, view_entries)?;
        let builder_target = projection_root.join(TARGET_PATH);
        if !path_exists_without_following(&builder_target)? {
            return Err(CliError::new(
                "runtime_layer_preparation_failed",
                "Bun completed without creating the declared runtime layer target",
            )
            .with_detail("path", TARGET_PATH));
        }
        let builder_target_metadata = fs::symlink_metadata(&builder_target).map_err(|error| {
            runtime_io_error(
                "runtime_layer_invalid_content",
                &builder_target,
                "failed to inspect the provider target",
                error,
            )
        })?;
        if builder_target_metadata.file_type().is_symlink() || !builder_target_metadata.is_dir() {
            return Err(CliError::new(
                "runtime_layer_invalid_content",
                "the runtime layer provider target must be a real directory",
            )
            .with_detail("path", TARGET_PATH));
        }
        let staged_target = staged_entry.join("targets/root").join(TARGET_PATH);
        fs::create_dir_all(staged_target.parent().expect("target has parent")).map_err(
            |error| {
                runtime_io_error(
                    "runtime_layer_storage_failed",
                    &staged_target,
                    "failed to create staged runtime layer target parent",
                    error,
                )
            },
        )?;
        materialize_private_runtime_dependency_tree(&builder_target, &staged_target, &staged_entry)
            .map_err(|error| {
                CliError::new(
                    "runtime_layer_invalid_content",
                    format!("failed to normalize provider output: {error}"),
                )
            })?;
        ensure_acquisition_active(cancellation, deadline)?;
        let (content_manifest, content_id) = runtime_layer_content_manifest(&staged_target)?;
        ensure_acquisition_active(cancellation, deadline)?;
        let environment_bytes = serde_json::to_vec(canonical_environment).map_err(|error| {
            CliError::new(
                "runtime_layer_publication_failed",
                format!("failed to encode runtime layer environment: {error}"),
            )
        })?;
        let construction = RealExecutionRuntimeLayerConstruction {
            origin: "provider_preparation".to_string(),
            command_argv: vec![
                "bun".to_string(),
                "install".to_string(),
                "--frozen-lockfile".to_string(),
                "--linker".to_string(),
                "hoisted".to_string(),
                "--concurrent-scripts".to_string(),
                "1".to_string(),
                "--cache-dir".to_string(),
                "PRIVATE_PACKAGE_CACHE".to_string(),
            ],
            working_directory: if plan.root.is_empty() {
                ".".to_string()
            } else {
                plan.root.clone()
            },
            tool_name: "bun".to_string(),
            tool_executable_digest: executable_digest,
            tool_reported_version: reported_version,
            environment: canonical_environment.clone(),
            environment_digest: real_content_hash(&environment_bytes),
            network_policy_requested: policy.network_policy.clone(),
            network_policy_effective: "not_enforced".to_string(),
            filesystem_write_policy_effective: "not_enforced".to_string(),
        };
        let layer_set_id =
            real_content_hash(format!("{PROVIDER_ID}\0{lookup_key}\0{content_id}").as_bytes());
        let layer = RealExecutionRuntimeLayer {
            layer_set_id,
            provider_id: PROVIDER_ID.to_string(),
            provider_semantics_digest: provider_semantics_digest.to_string(),
            lookup_key: lookup_key.to_string(),
            lookup_inputs: plan.inputs.clone(),
            content_id,
            acquisition: "provider_preparation".to_string(),
            construction,
            targets: vec![RealExecutionRuntimeLayerTarget {
                path: TARGET_PATH.to_string(),
                materialization_strategy: "pending_private_binding".to_string(),
            }],
        };
        write_cache_manifest(&staged_entry, &layer, &content_manifest)?;
        make_tree_contents_readonly(&staged_entry)?;
        ensure_acquisition_active(cancellation, deadline)?;
        fs::create_dir_all(entry_root.parent().expect("entry has parent")).map_err(|error| {
            runtime_io_error(
                "runtime_layer_storage_failed",
                entry_root,
                "failed to create runtime layer entry parent",
                error,
            )
        })?;
        fs::rename(&staged_entry, entry_root).map_err(|error| {
            runtime_io_error(
                "runtime_layer_publication_failed",
                entry_root,
                "failed to publish runtime layer atomically",
                error,
            )
        })?;
        make_path_readonly(entry_root)?;
        drop(cleanup);
        fs::remove_dir_all(&builder_target).map_err(|error| {
            runtime_io_error(
                "runtime_layer_binding_failed",
                &builder_target,
                "failed to replace provider output with a private runtime layer binding",
                error,
            )
        })?;
        Ok(layer)
    }
}

#[cfg(not(windows))]
fn run_provider_command(
    argv: &[String],
    cwd: &Path,
    policy: &ExecutionPolicy,
    cancellation: &AtomicBool,
    deadline: Instant,
    environment: &BTreeMap<String, String>,
) -> Result<super::BoundedProcessOutput, CliError> {
    let now = Instant::now();
    if now >= deadline {
        return Err(acquisition_timeout_error(policy.timeout_ms));
    }
    let mut preparation_policy = policy.clone();
    preparation_policy.timeout_ms = (deadline - now).as_millis().min(u64::MAX as u128) as u64;
    let output = run_bounded_process_with_environment(
        argv,
        cwd,
        &preparation_policy,
        cancellation,
        Some(environment),
        || Ok(()),
    )
    .map_err(|error| {
        CliError::new(
            "runtime_layer_preparation_failed",
            format!("failed to run the runtime layer provider: {error:?}"),
        )
    })?;
    if output.cancelled {
        return Err(cancelled_error("runtime layer preparation was cancelled"));
    }
    if output.timed_out {
        return Err(acquisition_timeout_error(policy.timeout_ms));
    }
    if output.termination_failed || output.wait_failed || output.resource_termination.is_some() {
        return Err(CliError::new(
            "runtime_layer_preparation_failed",
            "the runtime layer provider could not be completed safely",
        ));
    }
    Ok(output)
}

fn verify_provider_did_not_modify_source(
    projection_root: &Path,
    expected: &[RealArtifactEntry],
) -> Result<(), CliError> {
    let mut actual = Vec::new();
    let mut quarantine = Vec::new();
    scan_real_execution_projection_files_with_quarantine(
        projection_root,
        projection_root,
        &BTreeSet::from([TARGET_PATH.to_string()]),
        &mut actual,
        &mut quarantine,
    )
    .map_err(CliError::from)?;
    let expected = expected
        .iter()
        .filter(|entry| !entry.tombstone)
        .map(|entry| {
            (
                entry.path.clone(),
                (entry.content_hash.clone(), entry.executable),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let actual = actual
        .into_iter()
        .map(|entry| (entry.path, (entry.content_hash, entry.executable)))
        .collect::<BTreeMap<_, _>>();
    if actual != expected {
        return Err(CliError::new(
            "runtime_layer_preparation_modified_source",
            "the runtime layer provider changed resolved-view source files",
        ));
    }
    Ok(())
}

fn bind_layer(
    entry_root: &Path,
    projection_root: &Path,
    mut layer: RealExecutionRuntimeLayer,
    result: &mut RuntimeLayerResolution,
    cancellation: &AtomicBool,
    deadline: Instant,
) -> Result<(), CliError> {
    if cancellation.load(Ordering::Acquire) {
        return Err(cancelled_error("runtime layer binding was cancelled"));
    }
    if Instant::now() >= deadline {
        return Err(CliError::new(
            "runtime_layer_acquisition_timeout",
            "runtime layer acquisition timed out before private binding",
        ));
    }
    let started = Instant::now();
    let source = entry_root.join("targets/root").join(TARGET_PATH);
    let destination = projection_root.join(TARGET_PATH);
    let strategy = materialize_private_layer_binding(&source, &destination, projection_root)?;
    ensure_acquisition_active(cancellation, deadline)?;
    result.timings.private_binding_ms += started.elapsed().as_millis();
    if layer.acquisition != "provider_preparation" {
        layer.acquisition = "cache_reuse".to_string();
    }
    for target in &mut layer.targets {
        target.materialization_strategy = strategy.clone();
    }
    result.target_paths.push(TARGET_PATH.to_string());
    result.layers.push(layer);
    Ok(())
}

fn materialize_private_layer_binding(
    source: &Path,
    destination: &Path,
    projection_root: &Path,
) -> Result<String, CliError> {
    #[cfg(target_os = "macos")]
    {
        let cloned = clone_directory_cow(source, destination).is_ok();
        if cloned {
            let writable = Command::new("/bin/chmod")
                .args(["-R", "u+w"])
                .arg(destination)
                .status()
                .is_ok_and(|status| status.success());
            if writable {
                return Ok("recursive_cow".to_string());
            }
        }
        let _ = make_private_tree_writable(destination);
        let _ = fs::remove_dir_all(destination);
    }
    materialize_private_runtime_dependency_tree(source, destination, projection_root).map_err(
        |error| {
            CliError::new(
                "runtime_layer_binding_failed",
                format!("failed to create a private runtime layer binding: {error}"),
            )
            .with_detail("path", TARGET_PATH)
        },
    )?;
    Ok("cow_or_copy_private".to_string())
}

#[cfg(target_os = "macos")]
fn clone_directory_cow(source: &Path, destination: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    unsafe extern "C" {
        fn clonefile(source: *const i8, destination: *const i8, flags: u32) -> i32;
    }

    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "source path contains NUL"))?;
    let destination = CString::new(destination.as_os_str().as_bytes()).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "destination path contains NUL")
    })?;
    // SAFETY: both C strings remain alive for the call and point to NUL-terminated path bytes.
    if unsafe { clonefile(source.as_ptr(), destination.as_ptr(), 0) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn make_private_tree_writable(root: &Path) -> Result<(), CliError> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::symlink_metadata(root).map_err(|error| {
        runtime_io_error(
            "runtime_layer_binding_failed",
            root,
            "failed to inspect a private runtime layer binding",
            error,
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_dir() {
        for item in fs::read_dir(root).map_err(|error| {
            runtime_io_error(
                "runtime_layer_binding_failed",
                root,
                "failed to read a private runtime layer binding",
                error,
            )
        })? {
            let path = item
                .map_err(|error| {
                    runtime_io_error(
                        "runtime_layer_binding_failed",
                        root,
                        "failed to read a private runtime layer entry",
                        error,
                    )
                })?
                .path();
            make_private_tree_writable(&path)?;
        }
    }
    let mut permissions = metadata.permissions();
    let writable = if metadata.is_dir() { 0o700 } else { 0o200 };
    permissions.set_mode(permissions.mode() | writable);
    fs::set_permissions(root, permissions).map_err(|error| {
        runtime_io_error(
            "runtime_layer_binding_failed",
            root,
            "failed to make a private runtime layer binding writable",
            error,
        )
    })
}

#[cfg(not(unix))]
fn make_private_tree_writable(root: &Path) -> Result<(), CliError> {
    let metadata = fs::symlink_metadata(root).map_err(|error| {
        runtime_io_error(
            "runtime_layer_binding_failed",
            root,
            "failed to inspect a private runtime layer binding",
            error,
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_dir() {
        for item in fs::read_dir(root).map_err(|error| {
            runtime_io_error(
                "runtime_layer_binding_failed",
                root,
                "failed to read a private runtime layer binding",
                error,
            )
        })? {
            make_private_tree_writable(
                &item
                    .map_err(|error| {
                        runtime_io_error(
                            "runtime_layer_binding_failed",
                            root,
                            "failed to read a private runtime layer entry",
                            error,
                        )
                    })?
                    .path(),
            )?;
        }
    }
    let mut permissions = metadata.permissions();
    permissions.set_readonly(false);
    fs::set_permissions(root, permissions).map_err(|error| {
        runtime_io_error(
            "runtime_layer_binding_failed",
            root,
            "failed to make a private runtime layer binding writable",
            error,
        )
    })
}

fn runtime_layer_content_manifest(root: &Path) -> Result<(Value, String), CliError> {
    let mut entries = vec![json!({"path": ".", "kind": "directory"})];
    collect_runtime_layer_manifest_entries(root, root, &mut entries)?;
    entries.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap_or("")
            .cmp(right["path"].as_str().unwrap_or(""))
    });
    let manifest = json!({
        "target": TARGET_PATH,
        "entries": entries,
    });
    let bytes = serde_json::to_vec(&manifest).map_err(|error| {
        CliError::new(
            "runtime_layer_invalid_content",
            format!("failed to encode the runtime layer content manifest: {error}"),
        )
    })?;
    Ok((manifest, real_content_hash(&bytes)))
}

fn collect_runtime_layer_manifest_entries(
    root: &Path,
    current: &Path,
    entries: &mut Vec<Value>,
) -> Result<(), CliError> {
    for item in fs::read_dir(current).map_err(|error| {
        runtime_io_error(
            "runtime_layer_invalid_content",
            current,
            "failed to read runtime layer content",
            error,
        )
    })? {
        let path = item
            .map_err(|error| {
                runtime_io_error(
                    "runtime_layer_invalid_content",
                    current,
                    "failed to read a runtime layer content entry",
                    error,
                )
            })?
            .path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| {
                CliError::new(
                    "runtime_layer_invalid_content",
                    "runtime layer content escaped its declared target",
                )
            })?
            .to_str()
            .ok_or_else(|| {
                CliError::new(
                    "runtime_layer_invalid_content",
                    "runtime layer paths must be UTF-8",
                )
            })?
            .replace('\\', "/");
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            runtime_io_error(
                "runtime_layer_invalid_content",
                &path,
                "failed to inspect runtime layer content",
                error,
            )
        })?;
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&path).map_err(|error| {
                runtime_io_error(
                    "runtime_layer_invalid_content",
                    &path,
                    "failed to read runtime layer symlink",
                    error,
                )
            })?;
            entries.push(json!({
                "path": relative,
                "kind": "symlink",
                "target": target.to_string_lossy(),
            }));
        } else if metadata.is_dir() {
            entries.push(json!({"path": relative, "kind": "directory"}));
            collect_runtime_layer_manifest_entries(root, &path, entries)?;
        } else if metadata.is_file() {
            let bytes = fs::read(&path).map_err(|error| {
                runtime_io_error(
                    "runtime_layer_invalid_content",
                    &path,
                    "failed to read runtime layer file",
                    error,
                )
            })?;
            entries.push(json!({
                "path": relative,
                "kind": "file",
                "content_hash": real_content_hash(&bytes),
                "byte_length": bytes.len(),
                "executable": file_is_executable(&metadata),
            }));
        } else {
            return Err(CliError::new(
                "runtime_layer_invalid_content",
                "runtime layer content contains an unsupported filesystem entry",
            )
            .with_detail("path", path.display().to_string()));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn file_is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn file_is_executable(_metadata: &fs::Metadata) -> bool {
    false
}

fn read_valid_cache_entry(
    entry_root: &Path,
    lookup_key: &str,
    provider_semantics_digest: &str,
    inputs: &[RealExecutionRuntimeLayerInput],
) -> Result<Option<RealExecutionRuntimeLayer>, CacheReadError> {
    let root_metadata = match fs::symlink_metadata(entry_root) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(CacheReadError::new(format!(
                "cache entry is unavailable: {error}"
            )))
        }
    };
    if root_metadata.file_type().is_symlink()
        || !root_metadata.is_dir()
        || !root_metadata.permissions().readonly()
    {
        return Err(CacheReadError::new(
            "cache entry root is not a protected real directory",
        ));
    }
    let manifest_path = entry_root.join(CACHE_MANIFEST);
    let manifest_metadata = fs::symlink_metadata(&manifest_path)
        .map_err(|error| CacheReadError::new(format!("cache manifest is unavailable: {error}")))?;
    if manifest_metadata.file_type().is_symlink()
        || !manifest_metadata.is_file()
        || !manifest_metadata.permissions().readonly()
    {
        return Err(CacheReadError::new(
            "cache manifest type or permissions are unsafe",
        ));
    }
    let manifest: Value =
        serde_json::from_slice(&fs::read(&manifest_path).map_err(|error| {
            CacheReadError::new(format!("cache manifest read failed: {error}"))
        })?)
        .map_err(|error| CacheReadError::new(format!("cache manifest JSON is invalid: {error}")))?;
    let content_manifest = manifest
        .get("content_manifest")
        .ok_or_else(|| CacheReadError::new("cache manifest has no content manifest"))?;
    let mut layer = layer_from_json(
        manifest
            .get("layer")
            .ok_or_else(|| CacheReadError::new("cache manifest has no layer"))?,
    )?;
    if layer.lookup_key != lookup_key
        || layer.provider_id != PROVIDER_ID
        || layer.provider_semantics_digest != provider_semantics_digest
        || layer.lookup_inputs != inputs
    {
        return Err(CacheReadError::new(
            "cache manifest does not match lookup inputs",
        ));
    }
    let expected_set = real_content_hash(
        format!(
            "{}\0{}\0{}",
            layer.provider_id, lookup_key, layer.content_id
        )
        .as_bytes(),
    );
    if layer.layer_set_id != expected_set {
        return Err(CacheReadError::new("cache layer set identity is invalid"));
    }
    let content_manifest_bytes = serde_json::to_vec(content_manifest)
        .map_err(|error| CacheReadError::new(format!("content manifest is invalid: {error}")))?;
    if real_content_hash(&content_manifest_bytes) != layer.content_id {
        return Err(CacheReadError::new(
            "cache content manifest identity is invalid",
        ));
    }
    let target = entry_root.join("targets/root").join(TARGET_PATH);
    let target_metadata = fs::symlink_metadata(&target)
        .map_err(|error| CacheReadError::new(format!("cache target is unavailable: {error}")))?;
    if target_metadata.file_type().is_symlink()
        || !target_metadata.is_dir()
        || !target_metadata.permissions().readonly()
    {
        return Err(CacheReadError::new(
            "cache target is not a protected real directory",
        ));
    }
    layer.acquisition = "cache_reuse".to_string();
    Ok(Some(layer))
}

fn write_cache_manifest(
    entry_root: &Path,
    layer: &RealExecutionRuntimeLayer,
    content_manifest: &Value,
) -> Result<(), CliError> {
    let manifest = json!({
        "record_type": "runtime_layer_manifest",
        "layer": layer_json(layer),
        "content_manifest": content_manifest,
    });
    let bytes = serde_json::to_vec(&manifest).map_err(|error| {
        CliError::new(
            "runtime_layer_publication_failed",
            format!("failed to encode runtime layer manifest: {error}"),
        )
    })?;
    fs::write(entry_root.join(CACHE_MANIFEST), bytes).map_err(|error| {
        runtime_io_error(
            "runtime_layer_publication_failed",
            entry_root,
            "failed to write runtime layer manifest",
            error,
        )
    })
}

fn layer_json(layer: &RealExecutionRuntimeLayer) -> Value {
    json!({
        "layer_set_id": layer.layer_set_id,
        "provider_id": layer.provider_id,
        "provider_semantics_digest": layer.provider_semantics_digest,
        "lookup_key": layer.lookup_key,
        "lookup_inputs": layer.lookup_inputs.iter().map(|input| json!({
            "path": input.path,
            "artifact_id": input.artifact_id,
            "content_hash": input.content_hash,
        })).collect::<Vec<_>>(),
        "content_id": layer.content_id,
        "acquisition": layer.acquisition,
        "construction": {
            "origin": layer.construction.origin,
            "command_argv": layer.construction.command_argv,
            "working_directory": layer.construction.working_directory,
            "tool": {
                "name": layer.construction.tool_name,
                "executable_digest": layer.construction.tool_executable_digest,
                "reported_version": layer.construction.tool_reported_version,
            },
            "environment": layer.construction.environment,
            "environment_digest": layer.construction.environment_digest,
            "network_policy_requested": layer.construction.network_policy_requested,
            "network_policy_effective": layer.construction.network_policy_effective,
            "filesystem_write_policy_effective": layer.construction.filesystem_write_policy_effective,
        },
        "targets": layer.targets.iter().map(|target| json!({
            "path": target.path,
            "materialization_strategy": target.materialization_strategy,
        })).collect::<Vec<_>>(),
    })
}

fn layer_from_json(value: &Value) -> Result<RealExecutionRuntimeLayer, CacheReadError> {
    let object = value
        .as_object()
        .ok_or_else(|| CacheReadError::new("cache layer must be an object"))?;
    let construction = object
        .get("construction")
        .and_then(Value::as_object)
        .ok_or_else(|| CacheReadError::new("cache construction must be an object"))?;
    let tool = construction
        .get("tool")
        .and_then(Value::as_object)
        .ok_or_else(|| CacheReadError::new("cache construction tool must be an object"))?;
    let inputs = object
        .get("lookup_inputs")
        .and_then(Value::as_array)
        .ok_or_else(|| CacheReadError::new("cache lookup_inputs must be an array"))?
        .iter()
        .map(|value| {
            Ok(RealExecutionRuntimeLayerInput {
                path: json_string(value, "path")?,
                artifact_id: json_string(value, "artifact_id")?,
                content_hash: json_string(value, "content_hash")?,
            })
        })
        .collect::<Result<Vec<_>, CacheReadError>>()?;
    let targets = object
        .get("targets")
        .and_then(Value::as_array)
        .ok_or_else(|| CacheReadError::new("cache targets must be an array"))?
        .iter()
        .map(|value| {
            Ok(RealExecutionRuntimeLayerTarget {
                path: json_string(value, "path")?,
                materialization_strategy: json_string(value, "materialization_strategy")?,
            })
        })
        .collect::<Result<Vec<_>, CacheReadError>>()?;
    let environment = construction
        .get("environment")
        .and_then(Value::as_object)
        .ok_or_else(|| CacheReadError::new("cache environment must be an object"))?
        .iter()
        .map(|(name, value)| {
            value
                .as_str()
                .map(|value| (name.clone(), value.to_string()))
                .ok_or_else(|| CacheReadError::new("cache environment values must be strings"))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    let command_argv = construction
        .get("command_argv")
        .and_then(Value::as_array)
        .ok_or_else(|| CacheReadError::new("cache command_argv must be an array"))?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| CacheReadError::new("cache command_argv must contain strings"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RealExecutionRuntimeLayer {
        layer_set_id: json_string(value, "layer_set_id")?,
        provider_id: json_string(value, "provider_id")?,
        provider_semantics_digest: json_string(value, "provider_semantics_digest")?,
        lookup_key: json_string(value, "lookup_key")?,
        lookup_inputs: inputs,
        content_id: json_string(value, "content_id")?,
        acquisition: json_string(value, "acquisition")?,
        construction: RealExecutionRuntimeLayerConstruction {
            origin: json_string_from_map(construction, "origin")?,
            command_argv,
            working_directory: json_string_from_map(construction, "working_directory")?,
            tool_name: json_string_from_map(tool, "name")?,
            tool_executable_digest: json_string_from_map(tool, "executable_digest")?,
            tool_reported_version: json_string_from_map(tool, "reported_version")?,
            environment,
            environment_digest: json_string_from_map(construction, "environment_digest")?,
            network_policy_requested: json_string_from_map(
                construction,
                "network_policy_requested",
            )?,
            network_policy_effective: json_string_from_map(
                construction,
                "network_policy_effective",
            )?,
            filesystem_write_policy_effective: json_string_from_map(
                construction,
                "filesystem_write_policy_effective",
            )?,
        },
        targets,
    })
}

fn json_string(value: &Value, field: &str) -> Result<String, CacheReadError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| CacheReadError::new(format!("cache field `{field}` must be a string")))
}

fn json_string_from_map(
    value: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<String, CacheReadError> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| CacheReadError::new(format!("cache field `{field}` must be a string")))
}

fn make_tree_contents_readonly(root: &Path) -> Result<(), CliError> {
    for item in fs::read_dir(root).map_err(|error| {
        runtime_io_error(
            "runtime_layer_publication_failed",
            root,
            "failed to read staged runtime layer",
            error,
        )
    })? {
        let path = item
            .map_err(|error| {
                runtime_io_error(
                    "runtime_layer_publication_failed",
                    root,
                    "failed to read staged runtime layer entry",
                    error,
                )
            })?
            .path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            runtime_io_error(
                "runtime_layer_publication_failed",
                &path,
                "failed to inspect staged runtime layer entry",
                error,
            )
        })?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            make_tree_contents_readonly(&path)?;
        }
        if !metadata.file_type().is_symlink() {
            let mut permissions = metadata.permissions();
            permissions.set_readonly(true);
            fs::set_permissions(&path, permissions).map_err(|error| {
                runtime_io_error(
                    "runtime_layer_publication_failed",
                    &path,
                    "failed to protect staged runtime layer entry",
                    error,
                )
            })?;
        }
    }
    Ok(())
}

fn make_path_readonly(path: &Path) -> Result<(), CliError> {
    let metadata = fs::metadata(path).map_err(|error| {
        runtime_io_error(
            "runtime_layer_publication_failed",
            path,
            "failed to inspect staged runtime layer root",
            error,
        )
    })?;
    let mut permissions = metadata.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).map_err(|error| {
        runtime_io_error(
            "runtime_layer_publication_failed",
            path,
            "failed to protect staged runtime layer root",
            error,
        )
    })
}

fn quarantine_entry(runtime_root: &Path, entry_root: &Path, key: &str) -> Result<(), CliError> {
    if !entry_root.exists() {
        return Ok(());
    }
    let destination = runtime_root
        .join("quarantine")
        .join(format!("{}-{key}", attempt_id()));
    fs::create_dir_all(destination.parent().expect("quarantine has parent")).map_err(|error| {
        runtime_io_error(
            "runtime_layer_cache_corrupt",
            &destination,
            "failed to create runtime layer quarantine",
            error,
        )
    })?;
    make_path_writable(entry_root)?;
    fs::rename(entry_root, &destination).map_err(|error| {
        runtime_io_error(
            "runtime_layer_cache_corrupt",
            entry_root,
            "failed to quarantine an invalid runtime layer",
            error,
        )
    })
}

fn make_path_writable(path: &Path) -> Result<(), CliError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        runtime_io_error(
            "runtime_layer_cache_corrupt",
            path,
            "failed to inspect an invalid runtime layer",
            error,
        )
    })?;
    let mut permissions = metadata.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(permissions.mode() | 0o200);
    }
    #[cfg(not(unix))]
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions).map_err(|error| {
        runtime_io_error(
            "runtime_layer_cache_corrupt",
            path,
            "failed to prepare an invalid runtime layer for quarantine",
            error,
        )
    })
}

fn actual_construction_environment(
    private_home: &Path,
    private_temp: &Path,
    private_cache: &Path,
) -> BTreeMap<String, String> {
    let home = private_home.display().to_string();
    let temp = private_temp.display().to_string();
    let mut values = BTreeMap::from([
        ("HOME".to_string(), home.clone()),
        ("USERPROFILE".to_string(), home.clone()),
        ("HOMEPATH".to_string(), home.clone()),
        (
            "XDG_CONFIG_HOME".to_string(),
            private_home.join("config").display().to_string(),
        ),
        (
            "APPDATA".to_string(),
            private_home.join("appdata").display().to_string(),
        ),
        (
            "LOCALAPPDATA".to_string(),
            private_home.join("local-appdata").display().to_string(),
        ),
        ("TEMP".to_string(), temp.clone()),
        ("TMP".to_string(), temp.clone()),
        ("TMPDIR".to_string(), temp),
        (
            "BUN_INSTALL_CACHE_DIR".to_string(),
            private_cache.display().to_string(),
        ),
    ]);
    if let Some(path) = env::var_os("PATH") {
        values.insert("PATH".to_string(), path.to_string_lossy().into_owned());
    }
    values
}

fn find_executable(name: &str) -> Option<PathBuf> {
    let path = env::var_os("PATH")?;
    for directory in env::split_paths(&path) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return fs::canonicalize(&candidate).ok().or(Some(candidate));
        }
    }
    None
}

fn entry<'a>(entries: &'a [RealArtifactEntry], path: &str) -> Option<&'a RealArtifactEntry> {
    entries
        .iter()
        .find(|entry| !entry.tombstone && entry.path == path)
}

fn runtime_input(entry: &RealArtifactEntry) -> RealExecutionRuntimeLayerInput {
    RealExecutionRuntimeLayerInput {
        path: entry.path.clone(),
        artifact_id: entry.artifact_id.clone(),
        content_hash: entry.content_hash.clone(),
    }
}

fn relative_path(root: &Path, name: &str) -> String {
    root.join(name).to_string_lossy().replace('\\', "/")
}

fn attempt_id() -> String {
    format!(
        "{}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0),
        ATTEMPT_NONCE.fetch_add(1, Ordering::Relaxed),
    )
}

fn provider_unsupported(reason: &str) -> CliError {
    CliError::new(
        "runtime_layer_provider_unsupported",
        "the resolved view uses a recognized dependency layout that this runtime provider cannot prepare",
    )
    .with_detail("provider_id", PROVIDER_ID)
    .with_detail("reason", reason)
}

fn cancelled_error(message: &str) -> CliError {
    CliError::new("request_cancelled", message)
}

fn ensure_acquisition_active(cancellation: &AtomicBool, deadline: Instant) -> Result<(), CliError> {
    if cancellation.load(Ordering::Acquire) {
        return Err(cancelled_error("runtime layer acquisition was cancelled"));
    }
    if Instant::now() >= deadline {
        return Err(CliError::new(
            "runtime_layer_acquisition_timeout",
            "runtime layer acquisition exceeded the configured command timeout",
        ));
    }
    Ok(())
}

fn acquisition_timeout_error(timeout_ms: u64) -> CliError {
    CliError::new(
        "runtime_layer_acquisition_timeout",
        "runtime layer acquisition exceeded the configured command timeout",
    )
    .with_detail("timeout_ms", timeout_ms.to_string())
}

fn runtime_io_error(code: &'static str, path: &Path, message: &str, error: io::Error) -> CliError {
    let code = if matches!(error.raw_os_error(), Some(28 | 112)) {
        "runtime_layer_storage_full"
    } else {
        code
    };
    CliError::new(code, format!("{message}: {error}"))
        .with_detail("path", path.display().to_string())
}

#[derive(Debug)]
struct CacheReadError {
    message: String,
}

impl CacheReadError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

struct AttemptCleanup(PathBuf);

impl Drop for AttemptCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct RuntimeLayerKeyLock {
    file: File,
}

impl RuntimeLayerKeyLock {
    fn acquire(
        path: &Path,
        deadline: Instant,
        cancellation: &AtomicBool,
    ) -> Result<Self, CliError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|error| {
                runtime_io_error(
                    "runtime_layer_lock_failed",
                    path,
                    "failed to open runtime layer lock",
                    error,
                )
            })?;
        loop {
            if cancellation.load(Ordering::Acquire) {
                return Err(cancelled_error("runtime layer lock wait was cancelled"));
            }
            if try_lock_file(&file).map_err(|error| {
                runtime_io_error(
                    "runtime_layer_lock_failed",
                    path,
                    "failed to acquire runtime layer lock",
                    error,
                )
            })? {
                return Ok(Self { file });
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(CliError::new(
                    "runtime_layer_acquisition_timeout",
                    "runtime layer acquisition timed out while waiting for another builder",
                )
                .with_detail("lock", path.display().to_string()));
            }
            std::thread::sleep(LOCK_POLL.min(deadline - now));
        }
    }
}

#[cfg(unix)]
fn try_lock_file(file: &File) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;
    unsafe extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } == 0 {
        return Ok(true);
    }
    let error = io::Error::last_os_error();
    if error.kind() == io::ErrorKind::WouldBlock {
        Ok(false)
    } else {
        Err(error)
    }
}

#[cfg(windows)]
fn try_lock_file(_file: &File) -> io::Result<bool> {
    Ok(true)
}

#[cfg(not(any(unix, windows)))]
fn try_lock_file(_file: &File) -> io::Result<bool> {
    Ok(true)
}

#[cfg(unix)]
impl Drop for RuntimeLayerKeyLock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;
        const LOCK_UN: i32 = 8;
        unsafe extern "C" {
            fn flock(fd: i32, operation: i32) -> i32;
        }
        let _ = unsafe { flock(self.file.as_raw_fd(), LOCK_UN) };
    }
}

#[cfg(not(unix))]
impl Drop for RuntimeLayerKeyLock {
    fn drop(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonc_normalization_removes_comments_and_trailing_commas() {
        let parsed: Value = serde_json::from_str(&normalize_jsonc(
            "// bun.lock\n{\"workspaces\": {\"\": {},},}\n",
        ))
        .unwrap();
        assert!(parsed["workspaces"][""].is_object());
    }

    #[test]
    fn unsupported_dependency_references_are_recursive() {
        assert!(value_contains_unsupported_reference(&json!({
            "dependencies": {"local": "workspace:*"}
        })));
        assert!(!value_contains_unsupported_reference(&json!({
            "dependencies": {"remote": "1.2.3"}
        })));
    }

    #[test]
    fn provider_configs_reject_undeclared_filesystem_inputs() {
        assert!(validate_provider_config(
            "bunfig.toml",
            b"[install]\ncafile = \"certificates/ca.pem\"\n"
        )
        .is_err());
        assert!(validate_provider_config(".npmrc", b"cafile=certificates/ca.pem\n").is_err());
    }
}

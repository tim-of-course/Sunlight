use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::records::{canonical_json_bytes, parse_json_record, JsonValue, RecordError};

pub const REPO_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealRepoState {
    pub repository_id: String,
    pub base_checkpoint_id: String,
    pub base_resolved_view_id: String,
    pub resolved_view_id: String,
    pub tree_hash: String,
    pub topic_id: Option<String>,
    pub topic_slug: Option<String>,
    pub topic_display_name: Option<String>,
    pub session_id: Option<String>,
    pub actor_id: Option<String>,
    pub generation_number: u64,
    pub revision_number: u64,
    pub head_revision_id: Option<String>,
    pub topics: Vec<RealTopicRecord>,
    pub sessions: Vec<RealSessionRecord>,
    pub entries: Vec<RealArtifactEntry>,
    pub quarantine: Vec<RealQuarantineEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealTopicRecord {
    pub topic_id: String,
    pub slug: String,
    pub display_name: String,
    pub owner_actor_id: String,
    pub base_checkpoint_id: String,
    pub head_revision_id: Option<String>,
    pub revision_number: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealSessionRecord {
    pub session_id: String,
    pub actor_id: String,
    pub write_topic_id: String,
    pub resolved_view_id: String,
    pub session_generation_id: String,
    pub generation_number: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealArtifactEntry {
    pub path: String,
    pub artifact_id: String,
    pub content_hash: String,
    pub executable: bool,
    pub classification: String,
    pub tombstone: bool,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealQuarantineEntry {
    pub path: String,
    pub reason_codes: Vec<String>,
    pub classification: String,
    pub content_hash: String,
    pub byte_length: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepoStateError {
    NotInitialized { path: PathBuf },
    InvalidState { path: PathBuf, message: String },
    Io { path: PathBuf, message: String },
    Json(String),
}

impl Display for RepoStateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotInitialized { path } => {
                write!(
                    f,
                    "Sunlight repository state was not found at {}",
                    path.display()
                )
            }
            Self::InvalidState { path, message } => {
                write!(
                    f,
                    "invalid Sunlight repository state at {}: {message}",
                    path.display()
                )
            }
            Self::Io { path, message } => write!(f, "{message}: {}", path.display()),
            Self::Json(message) => write!(f, "invalid Sunlight repository state JSON: {message}"),
        }
    }
}

impl Error for RepoStateError {}

impl From<RecordError> for RepoStateError {
    fn from(value: RecordError) -> Self {
        Self::Json(value.to_string())
    }
}

impl RealRepoState {
    pub fn ingest(repo_root: &Path, repository_id: &str) -> Result<Self, RepoStateError> {
        let mut entries = Vec::new();
        let mut quarantine = Vec::new();
        scan_real_repo_files_with_quarantine(repo_root, repo_root, &mut entries, &mut quarantine)?;
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        let tree_hash = real_tree_hash(&entries);
        let state = Self {
            repository_id: repository_id.to_string(),
            base_checkpoint_id: "checkpoint_base_0001".to_string(),
            base_resolved_view_id: "view_base_0001".to_string(),
            resolved_view_id: "view_base_0001".to_string(),
            tree_hash,
            topic_id: None,
            topic_slug: None,
            topic_display_name: None,
            session_id: None,
            actor_id: None,
            generation_number: 0,
            revision_number: 0,
            head_revision_id: None,
            topics: Vec::new(),
            sessions: Vec::new(),
            entries,
            quarantine,
        };
        state.persist_blobs(repo_root)?;
        Ok(state)
    }

    pub fn load(repo_root: &Path) -> Result<Self, RepoStateError> {
        let path = real_state_path(repo_root);
        let body = fs::read(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                RepoStateError::NotInitialized { path: path.clone() }
            } else {
                io_error(&path, "failed to read native Sunlight state", error)
            }
        })?;
        let value = parse_json_record(&body)?;
        let JsonValue::Object(object) = value else {
            return Err(invalid_state(&path, "state root must be a JSON object"));
        };
        let schema_version = required_u64(&object, "schema_version", &path)?;
        if schema_version != REPO_STATE_SCHEMA_VERSION as u64 {
            return Err(invalid_state(
                &path,
                format!("unsupported schema_version `{schema_version}`"),
            ));
        }
        let entries = required_array(&object, "entries", &path)?
            .iter()
            .map(|entry| parse_entry(repo_root, entry, &path))
            .collect::<Result<Vec<_>, _>>()?;
        let quarantine = optional_array(&object, "quarantine", &path)?
            .iter()
            .map(|entry| parse_quarantine_entry(entry, &path))
            .collect::<Result<Vec<_>, _>>()?;
        let topic_id = optional_string(&object, "topic_id", &path)?;
        let topic_slug = optional_string(&object, "topic_slug", &path)?;
        let topic_display_name = optional_string(&object, "topic_display_name", &path)?;
        let session_id = optional_string(&object, "session_id", &path)?;
        let actor_id = optional_string(&object, "actor_id", &path)?;
        let generation_number = required_u64(&object, "generation_number", &path)?;
        let revision_number = required_u64(&object, "revision_number", &path)?;
        let head_revision_id = optional_string(&object, "head_revision_id", &path)?;
        let mut topics = optional_array(&object, "topics", &path)?
            .iter()
            .map(|topic| parse_topic(topic, &path))
            .collect::<Result<Vec<_>, _>>()?;
        let mut sessions = optional_array(&object, "sessions", &path)?
            .iter()
            .map(|session| parse_session(session, &path))
            .collect::<Result<Vec<_>, _>>()?;

        if topics.is_empty() {
            if let Some(legacy_topic_id) = topic_id.clone() {
                topics.push(RealTopicRecord {
                    topic_id: legacy_topic_id,
                    slug: topic_slug.clone().unwrap_or_default(),
                    display_name: topic_display_name.clone().unwrap_or_default(),
                    owner_actor_id: actor_id.clone().unwrap_or_else(|| "local".to_string()),
                    base_checkpoint_id: required_string(&object, "base_checkpoint_id", &path)?,
                    head_revision_id: head_revision_id.clone(),
                    revision_number,
                });
            }
        }
        if sessions.is_empty() {
            if let (Some(legacy_session_id), Some(legacy_topic_id)) =
                (session_id.clone(), topic_id.clone())
            {
                sessions.push(RealSessionRecord {
                    session_id: legacy_session_id,
                    actor_id: actor_id.clone().unwrap_or_else(|| "local".to_string()),
                    write_topic_id: legacy_topic_id,
                    resolved_view_id: required_string(&object, "resolved_view_id", &path)?,
                    session_generation_id: format!("gen_native_{:04}", generation_number.max(1)),
                    generation_number,
                });
            }
        }

        let mut state = Self {
            repository_id: required_string(&object, "repository_id", &path)?,
            base_checkpoint_id: required_string(&object, "base_checkpoint_id", &path)?,
            base_resolved_view_id: required_string(&object, "base_resolved_view_id", &path)?,
            resolved_view_id: required_string(&object, "resolved_view_id", &path)?,
            tree_hash: required_string(&object, "tree_hash", &path)?,
            topic_id,
            topic_slug,
            topic_display_name,
            session_id,
            actor_id,
            generation_number,
            revision_number,
            head_revision_id,
            topics,
            sessions,
            entries,
            quarantine,
        };
        state.sync_compat_fields();
        state
            .entries
            .sort_by(|left, right| left.path.cmp(&right.path));
        Ok(state)
    }

    pub fn save(&self, repo_root: &Path) -> Result<(), RepoStateError> {
        self.persist_blobs(repo_root)?;
        let path = real_state_path(repo_root);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_error(parent, "failed to create state directory", error))?;
        }
        let body = canonical_json_bytes(&self.to_json_value())?;
        fs::write(&path, body)
            .map_err(|error| io_error(&path, "failed to write native Sunlight state", error))
    }

    pub fn persist_blobs(&self, repo_root: &Path) -> Result<(), RepoStateError> {
        for entry in &self.entries {
            let path = real_blob_path(repo_root, &entry.content_hash);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| io_error(parent, "failed to create blob directory", error))?;
            }
            if !path.exists() {
                fs::write(&path, &entry.bytes)
                    .map_err(|error| io_error(&path, "failed to write content blob", error))?;
            }
        }
        Ok(())
    }

    pub fn persist_record(
        &self,
        repo_root: &Path,
        dir: &str,
        id: &str,
        json: &str,
    ) -> Result<(), RepoStateError> {
        let path = repo_root
            .join(".sunlight")
            .join(dir)
            .join(format!("{id}.json"));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_error(parent, "failed to create record directory", error))?;
        }
        fs::write(&path, json)
            .map_err(|error| io_error(&path, "failed to write Sunlight record", error))
    }

    pub fn to_json_value(&self) -> JsonValue {
        let mut object = BTreeMap::new();
        object.insert(
            "schema_version".to_string(),
            JsonValue::Number(REPO_STATE_SCHEMA_VERSION.to_string()),
        );
        object.insert(
            "record_type".to_string(),
            JsonValue::String("repo_state".to_string()),
        );
        object.insert(
            "repository_id".to_string(),
            JsonValue::String(self.repository_id.clone()),
        );
        object.insert(
            "base_checkpoint_id".to_string(),
            JsonValue::String(self.base_checkpoint_id.clone()),
        );
        object.insert(
            "base_resolved_view_id".to_string(),
            JsonValue::String(self.base_resolved_view_id.clone()),
        );
        object.insert(
            "resolved_view_id".to_string(),
            JsonValue::String(self.resolved_view_id.clone()),
        );
        object.insert(
            "tree_hash".to_string(),
            JsonValue::String(self.tree_hash.clone()),
        );
        object.insert("topic_id".to_string(), optional_json(&self.topic_id));
        object.insert("topic_slug".to_string(), optional_json(&self.topic_slug));
        object.insert(
            "topic_display_name".to_string(),
            optional_json(&self.topic_display_name),
        );
        object.insert("session_id".to_string(), optional_json(&self.session_id));
        object.insert("actor_id".to_string(), optional_json(&self.actor_id));
        object.insert(
            "generation_number".to_string(),
            JsonValue::Number(self.generation_number.to_string()),
        );
        object.insert(
            "revision_number".to_string(),
            JsonValue::Number(self.revision_number.to_string()),
        );
        object.insert(
            "head_revision_id".to_string(),
            optional_json(&self.head_revision_id),
        );
        object.insert(
            "topics".to_string(),
            JsonValue::Array(self.topics.iter().map(topic_json).collect()),
        );
        object.insert(
            "sessions".to_string(),
            JsonValue::Array(self.sessions.iter().map(session_json).collect()),
        );
        object.insert(
            "entries".to_string(),
            JsonValue::Array(self.entries.iter().map(entry_json).collect()),
        );
        object.insert(
            "quarantine".to_string(),
            JsonValue::Array(self.quarantine.iter().map(quarantine_json).collect()),
        );
        JsonValue::Object(object)
    }

    pub fn entry(&self, path: &str) -> Option<&RealArtifactEntry> {
        self.entries
            .iter()
            .find(|entry| entry.path == path && !entry.tombstone)
    }

    pub fn sync_compat_fields(&mut self) {
        if let Some(topic) = self.topics.last() {
            self.topic_id = Some(topic.topic_id.clone());
            self.topic_slug = Some(topic.slug.clone());
            self.topic_display_name = Some(topic.display_name.clone());
            self.head_revision_id = topic.head_revision_id.clone();
            self.revision_number = self
                .topics
                .iter()
                .map(|topic| topic.revision_number)
                .max()
                .unwrap_or(0)
                .max(self.revision_number);
        }
        if let Some(session) = self.sessions.last() {
            self.session_id = Some(session.session_id.clone());
            self.actor_id = Some(session.actor_id.clone());
            self.generation_number = self
                .sessions
                .iter()
                .map(|session| session.generation_number)
                .max()
                .unwrap_or(0)
                .max(self.generation_number);
        }
    }

    pub fn topic_by_id_or_slug(&self, value: &str) -> Option<&RealTopicRecord> {
        self.topics
            .iter()
            .find(|topic| topic.topic_id == value || topic.slug == value)
    }

    pub fn topic_by_id_or_slug_mut(&mut self, value: &str) -> Option<&mut RealTopicRecord> {
        self.topics
            .iter_mut()
            .find(|topic| topic.topic_id == value || topic.slug == value)
    }

    pub fn session_by_id(&self, session_id: &str) -> Option<&RealSessionRecord> {
        self.sessions
            .iter()
            .find(|session| session.session_id == session_id)
    }

    pub fn session_by_id_mut(&mut self, session_id: &str) -> Option<&mut RealSessionRecord> {
        self.sessions
            .iter_mut()
            .find(|session| session.session_id == session_id)
    }
}

pub fn scan_real_repo_files(
    repo_root: &Path,
    current: &Path,
    entries: &mut Vec<RealArtifactEntry>,
) -> Result<(), RepoStateError> {
    let mut quarantine = Vec::new();
    scan_real_repo_files_with_quarantine(repo_root, current, entries, &mut quarantine)
}

pub fn scan_real_repo_files_with_quarantine(
    repo_root: &Path,
    current: &Path,
    entries: &mut Vec<RealArtifactEntry>,
    quarantine: &mut Vec<RealQuarantineEntry>,
) -> Result<(), RepoStateError> {
    if current == repo_root {
        if let Some(paths) = git_worktree_file_paths(repo_root)? {
            for relative in paths {
                ingest_real_repo_path(repo_root, &relative, entries, quarantine)?;
            }
            return Ok(());
        }
    }
    scan_real_repo_files_fallback(repo_root, current, entries, quarantine)
}

fn scan_real_repo_files_fallback(
    repo_root: &Path,
    current: &Path,
    entries: &mut Vec<RealArtifactEntry>,
    quarantine: &mut Vec<RealQuarantineEntry>,
) -> Result<(), RepoStateError> {
    let children = fs::read_dir(current)
        .map_err(|error| io_error(current, "failed to scan repository worktree", error))?;
    for child in children {
        let child = child.map_err(|error| RepoStateError::Io {
            path: current.to_path_buf(),
            message: format!("failed to scan repository worktree: {error}"),
        })?;
        let path = child.path();
        let name = child.file_name();
        if matches!(name.to_str(), Some(".git" | ".sunlight")) {
            continue;
        }
        let metadata = child
            .metadata()
            .map_err(|error| io_error(&path, "failed to inspect worktree path", error))?;
        if metadata.is_dir() {
            scan_real_repo_files_fallback(repo_root, &path, entries, quarantine)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(repo_root)
                .map_err(|_| invalid_state(repo_root, "failed to normalize worktree path"))?
                .to_string_lossy()
                .replace('\\', "/");
            ingest_real_repo_path(repo_root, &relative, entries, quarantine)?;
        }
    }
    Ok(())
}

fn git_worktree_file_paths(repo_root: &Path) -> Result<Option<Vec<String>>, RepoStateError> {
    let output = match Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .output()
    {
        Ok(output) => output,
        Err(_) => return Ok(None),
    };
    if !output.status.success() {
        return Ok(None);
    }

    let mut paths = Vec::new();
    for raw in output.stdout.split(|byte| *byte == 0) {
        if raw.is_empty() {
            continue;
        }
        let relative = String::from_utf8_lossy(raw).replace('\\', "/");
        if relative == ".git"
            || relative == ".sunlight"
            || relative.starts_with(".git/")
            || relative.starts_with(".sunlight/")
        {
            continue;
        }
        paths.push(relative);
    }
    paths.sort();
    paths.dedup();
    Ok(Some(paths))
}

fn ingest_real_repo_path(
    repo_root: &Path,
    relative: &str,
    entries: &mut Vec<RealArtifactEntry>,
    quarantine: &mut Vec<RealQuarantineEntry>,
) -> Result<(), RepoStateError> {
    let path = repo_root.join(relative);
    let metadata = fs::metadata(&path)
        .map_err(|error| io_error(&path, "failed to inspect worktree path", error))?;
    if !metadata.is_file() {
        return Ok(());
    }
    let bytes =
        fs::read(&path).map_err(|error| io_error(&path, "failed to read worktree file", error))?;
    let secret_reasons = detect_secret_reasons(relative, &bytes);
    if !secret_reasons.is_empty() {
        quarantine.push(RealQuarantineEntry {
            path: relative.to_string(),
            reason_codes: secret_reasons,
            classification: "secret".to_string(),
            content_hash: real_content_hash(&bytes),
            byte_length: bytes.len(),
        });
        return Ok(());
    }
    entries.push(RealArtifactEntry {
        artifact_id: real_artifact_id_for_path(relative),
        path: relative.to_string(),
        content_hash: real_content_hash(&bytes),
        executable: is_executable(&metadata),
        classification: "source".to_string(),
        tombstone: false,
        bytes,
    });
    Ok(())
}

pub fn detect_secret_reasons(path: &str, bytes: &[u8]) -> Vec<String> {
    let mut reasons = Vec::new();
    let normalized_path = path.replace('\\', "/").to_ascii_lowercase();
    let file_name = normalized_path.rsplit('/').next().unwrap_or("");
    let path_secret = file_name == ".env"
        || file_name.starts_with(".env.")
        || normalized_path.ends_with("/.env")
        || normalized_path.contains("/.env.")
        || file_name.ends_with(".pem")
        || file_name.ends_with(".key")
        || file_name == "id_rsa"
        || file_name == "id_dsa"
        || file_name == "id_ecdsa"
        || file_name == "id_ed25519"
        || normalized_path.contains("secret")
        || normalized_path.contains("secrets")
        || normalized_path.contains("credentials");
    if path_secret {
        reasons.push("secret_path".to_string());
    }

    if let Ok(text) = std::str::from_utf8(bytes) {
        let lowered = text.to_ascii_lowercase();
        let token_secret = [
            "api_key",
            "apikey",
            "access_token",
            "auth_token",
            "secret_key",
            "client_secret",
            "private_key",
            "password",
            "-----begin private key-----",
            "-----begin rsa private key-----",
        ]
        .iter()
        .any(|needle| lowered.contains(needle));
        if token_secret {
            reasons.push("secret_token".to_string());
        }
    }
    reasons.sort();
    reasons.dedup();
    reasons
}

pub fn persist_quarantine_report(
    repo_root: &Path,
    quarantine: &[RealQuarantineEntry],
) -> Result<(), RepoStateError> {
    let path = repo_root
        .join(".sunlight")
        .join("quarantine")
        .join("ingest-report.json");
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| io_error(parent, "failed to create quarantine directory", error))?;
    }
    let mut object = BTreeMap::new();
    object.insert(
        "record_type".to_string(),
        JsonValue::String("ingest_quarantine_report".to_string()),
    );
    object.insert(
        "privacy_class".to_string(),
        JsonValue::String("local_only".to_string()),
    );
    object.insert(
        "classification".to_string(),
        JsonValue::String("secret".to_string()),
    );
    object.insert(
        "quarantined_count".to_string(),
        JsonValue::Number(quarantine.len().to_string()),
    );
    object.insert(
        "entries".to_string(),
        JsonValue::Array(quarantine.iter().map(quarantine_json).collect()),
    );
    let body = canonical_json_bytes(&JsonValue::Object(object))?;
    fs::write(&path, body)
        .map_err(|error| io_error(&path, "failed to write quarantine report", error))
}

pub fn real_state_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join(".sunlight")
        .join("records")
        .join("native-state.json")
}

pub fn real_blob_path(repo_root: &Path, content_hash: &str) -> PathBuf {
    repo_root
        .join(".sunlight")
        .join("objects")
        .join("blobs")
        .join("sha256")
        .join(content_hash.trim_start_matches("sha256:"))
}

pub fn read_real_blob(repo_root: &Path, content_hash: &str) -> Result<Vec<u8>, RepoStateError> {
    let path = real_blob_path(repo_root, content_hash);
    fs::read(&path).map_err(|error| io_error(&path, "failed to read content blob", error))
}

pub fn real_content_hash(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

pub fn real_tree_hash(entries: &[RealArtifactEntry]) -> String {
    let mut hasher = Sha256::new();
    for entry in entries.iter().filter(|entry| !entry.tombstone) {
        hasher.update(entry.path.as_bytes());
        hasher.update([0]);
        hasher.update(entry.content_hash.as_bytes());
        hasher.update([u8::from(entry.executable)]);
    }
    format!("tree_{:x}", hasher.finalize())
}

pub fn real_artifact_id_for_path(path: &str) -> String {
    format!("artifact_{}", path.replace(['/', '.', '-'], "_"))
}

pub fn materialize_real_files(state: &RealRepoState, root: &Path) -> Result<(), RepoStateError> {
    if root.exists()
        && fs::read_dir(root)
            .map_err(|error| io_error(root, "failed to inspect projection root", error))?
            .next()
            .is_some()
    {
        return Err(RepoStateError::InvalidState {
            path: root.to_path_buf(),
            message: "projection root must be an empty directory or a creatable path".to_string(),
        });
    }
    fs::create_dir_all(root)
        .map_err(|error| io_error(root, "failed to create projection root", error))?;
    for entry in state.entries.iter().filter(|entry| !entry.tombstone) {
        let path = root.join(&entry.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                io_error(parent, "failed to create projection directory", error)
            })?;
        }
        fs::write(&path, &entry.bytes)
            .map_err(|error| io_error(&path, "failed to write projection file", error))?;
    }
    Ok(())
}

pub fn write_real_files_overwrite(
    state: &RealRepoState,
    root: &Path,
) -> Result<(), RepoStateError> {
    for entry in state.entries.iter().filter(|entry| entry.tombstone) {
        let path = root.join(&entry.path);
        if path.is_file() {
            fs::remove_file(&path).map_err(|error| {
                io_error(&path, "failed to remove tombstoned export file", error)
            })?;
        }
    }
    for entry in state.entries.iter().filter(|entry| !entry.tombstone) {
        let path = root.join(&entry.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_error(parent, "failed to create export directory", error))?;
        }
        fs::write(&path, &entry.bytes)
            .map_err(|error| io_error(&path, "failed to write export file", error))?;
    }
    Ok(())
}

fn parse_entry(
    repo_root: &Path,
    value: &JsonValue,
    state_path: &Path,
) -> Result<RealArtifactEntry, RepoStateError> {
    let JsonValue::Object(object) = value else {
        return Err(invalid_state(state_path, "entry must be a JSON object"));
    };
    let content_hash = required_string(object, "content_hash", state_path)?;
    Ok(RealArtifactEntry {
        path: required_string(object, "path", state_path)?,
        artifact_id: required_string(object, "artifact_id", state_path)?,
        bytes: read_real_blob(repo_root, &content_hash)?,
        content_hash,
        executable: required_bool(object, "executable", state_path)?,
        classification: required_string(object, "classification", state_path)?,
        tombstone: required_bool(object, "tombstone", state_path)?,
    })
}

fn parse_quarantine_entry(
    value: &JsonValue,
    state_path: &Path,
) -> Result<RealQuarantineEntry, RepoStateError> {
    let JsonValue::Object(object) = value else {
        return Err(invalid_state(
            state_path,
            "quarantine entry must be a JSON object",
        ));
    };
    let reason_codes = required_array(object, "reason_codes", state_path)?
        .iter()
        .map(|value| match value {
            JsonValue::String(reason) => Ok(reason.clone()),
            _ => Err(invalid_state(
                state_path,
                "quarantine reason_codes must be strings",
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RealQuarantineEntry {
        path: required_string(object, "path", state_path)?,
        reason_codes,
        classification: required_string(object, "classification", state_path)?,
        content_hash: required_string(object, "content_hash", state_path)?,
        byte_length: required_u64(object, "byte_length", state_path)? as usize,
    })
}

fn parse_topic(value: &JsonValue, state_path: &Path) -> Result<RealTopicRecord, RepoStateError> {
    let JsonValue::Object(object) = value else {
        return Err(invalid_state(state_path, "topic must be a JSON object"));
    };
    Ok(RealTopicRecord {
        topic_id: required_string(object, "topic_id", state_path)?,
        slug: required_string(object, "slug", state_path)?,
        display_name: required_string(object, "display_name", state_path)?,
        owner_actor_id: required_string(object, "owner_actor_id", state_path)?,
        base_checkpoint_id: required_string(object, "base_checkpoint_id", state_path)?,
        head_revision_id: optional_string(object, "head_revision_id", state_path)?,
        revision_number: required_u64(object, "revision_number", state_path)?,
    })
}

fn parse_session(
    value: &JsonValue,
    state_path: &Path,
) -> Result<RealSessionRecord, RepoStateError> {
    let JsonValue::Object(object) = value else {
        return Err(invalid_state(state_path, "session must be a JSON object"));
    };
    Ok(RealSessionRecord {
        session_id: required_string(object, "session_id", state_path)?,
        actor_id: required_string(object, "actor_id", state_path)?,
        write_topic_id: required_string(object, "write_topic_id", state_path)?,
        resolved_view_id: required_string(object, "resolved_view_id", state_path)?,
        session_generation_id: required_string(object, "session_generation_id", state_path)?,
        generation_number: required_u64(object, "generation_number", state_path)?,
    })
}

fn entry_json(entry: &RealArtifactEntry) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert("path".to_string(), JsonValue::String(entry.path.clone()));
    object.insert(
        "artifact_id".to_string(),
        JsonValue::String(entry.artifact_id.clone()),
    );
    object.insert(
        "content_hash".to_string(),
        JsonValue::String(entry.content_hash.clone()),
    );
    object.insert("executable".to_string(), JsonValue::Bool(entry.executable));
    object.insert(
        "classification".to_string(),
        JsonValue::String(entry.classification.clone()),
    );
    object.insert("tombstone".to_string(), JsonValue::Bool(entry.tombstone));
    JsonValue::Object(object)
}

fn topic_json(topic: &RealTopicRecord) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "topic_id".to_string(),
        JsonValue::String(topic.topic_id.clone()),
    );
    object.insert("slug".to_string(), JsonValue::String(topic.slug.clone()));
    object.insert(
        "display_name".to_string(),
        JsonValue::String(topic.display_name.clone()),
    );
    object.insert(
        "owner_actor_id".to_string(),
        JsonValue::String(topic.owner_actor_id.clone()),
    );
    object.insert(
        "base_checkpoint_id".to_string(),
        JsonValue::String(topic.base_checkpoint_id.clone()),
    );
    object.insert(
        "head_revision_id".to_string(),
        optional_json(&topic.head_revision_id),
    );
    object.insert(
        "revision_number".to_string(),
        JsonValue::Number(topic.revision_number.to_string()),
    );
    JsonValue::Object(object)
}

fn session_json(session: &RealSessionRecord) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "session_id".to_string(),
        JsonValue::String(session.session_id.clone()),
    );
    object.insert(
        "actor_id".to_string(),
        JsonValue::String(session.actor_id.clone()),
    );
    object.insert(
        "write_topic_id".to_string(),
        JsonValue::String(session.write_topic_id.clone()),
    );
    object.insert(
        "resolved_view_id".to_string(),
        JsonValue::String(session.resolved_view_id.clone()),
    );
    object.insert(
        "session_generation_id".to_string(),
        JsonValue::String(session.session_generation_id.clone()),
    );
    object.insert(
        "generation_number".to_string(),
        JsonValue::Number(session.generation_number.to_string()),
    );
    JsonValue::Object(object)
}

fn quarantine_json(entry: &RealQuarantineEntry) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert("path".to_string(), JsonValue::String(entry.path.clone()));
    object.insert(
        "reason_codes".to_string(),
        JsonValue::Array(
            entry
                .reason_codes
                .iter()
                .map(|reason| JsonValue::String(reason.clone()))
                .collect(),
        ),
    );
    object.insert(
        "classification".to_string(),
        JsonValue::String(entry.classification.clone()),
    );
    object.insert(
        "content_hash".to_string(),
        JsonValue::String(entry.content_hash.clone()),
    );
    object.insert(
        "byte_length".to_string(),
        JsonValue::Number(entry.byte_length.to_string()),
    );
    JsonValue::Object(object)
}

fn optional_json(value: &Option<String>) -> JsonValue {
    value
        .as_ref()
        .map(|value| JsonValue::String(value.clone()))
        .unwrap_or(JsonValue::Null)
}

fn optional_string(
    object: &BTreeMap<String, JsonValue>,
    field: &'static str,
    path: &Path,
) -> Result<Option<String>, RepoStateError> {
    match object.get(field) {
        Some(JsonValue::Null) | None => Ok(None),
        Some(JsonValue::String(value)) => Ok(Some(value.clone())),
        _ => Err(invalid_state(
            path,
            format!("field `{field}` must be a string or null"),
        )),
    }
}

fn required_string(
    object: &BTreeMap<String, JsonValue>,
    field: &'static str,
    path: &Path,
) -> Result<String, RepoStateError> {
    match object.get(field) {
        Some(JsonValue::String(value)) => Ok(value.clone()),
        _ => Err(invalid_state(
            path,
            format!("field `{field}` must be a string"),
        )),
    }
}

fn required_bool(
    object: &BTreeMap<String, JsonValue>,
    field: &'static str,
    path: &Path,
) -> Result<bool, RepoStateError> {
    match object.get(field) {
        Some(JsonValue::Bool(value)) => Ok(*value),
        _ => Err(invalid_state(
            path,
            format!("field `{field}` must be a boolean"),
        )),
    }
}

fn required_u64(
    object: &BTreeMap<String, JsonValue>,
    field: &'static str,
    path: &Path,
) -> Result<u64, RepoStateError> {
    match object.get(field) {
        Some(JsonValue::Number(value)) => value
            .parse::<u64>()
            .map_err(|_| invalid_state(path, format!("field `{field}` must be an integer"))),
        _ => Err(invalid_state(
            path,
            format!("field `{field}` must be an integer"),
        )),
    }
}

fn required_array<'a>(
    object: &'a BTreeMap<String, JsonValue>,
    field: &'static str,
    path: &Path,
) -> Result<&'a [JsonValue], RepoStateError> {
    match object.get(field) {
        Some(JsonValue::Array(values)) => Ok(values),
        _ => Err(invalid_state(
            path,
            format!("field `{field}` must be an array"),
        )),
    }
}

fn optional_array<'a>(
    object: &'a BTreeMap<String, JsonValue>,
    field: &'static str,
    path: &Path,
) -> Result<&'a [JsonValue], RepoStateError> {
    match object.get(field) {
        Some(JsonValue::Array(values)) => Ok(values),
        None => Ok(&[]),
        _ => Err(invalid_state(
            path,
            format!("field `{field}` must be an array"),
        )),
    }
}

fn invalid_state(path: impl AsRef<Path>, message: impl Into<String>) -> RepoStateError {
    RepoStateError::InvalidState {
        path: path.as_ref().to_path_buf(),
        message: message.into(),
    }
}

fn io_error(path: &Path, message: &str, error: std::io::Error) -> RepoStateError {
    RepoStateError::Io {
        path: path.to_path_buf(),
        message: format!("{message}: {error}"),
    }
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn temp_repo(name: &str) -> PathBuf {
        let mut root = std::env::temp_dir();
        root.push(format!(
            "sunlight-core-repo-state-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        root
    }

    fn git(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(args)
            .output()
            .unwrap_or_else(|error| panic!("failed to run git {}: {error}", args.join(" ")));
        assert!(
            output.status.success(),
            "git {} failed\nstdout:\n{}\nstderr:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn repo_state_round_trips_through_schema_json() {
        let repo = temp_repo("round-trip");
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(repo.join("src/lib.rs"), b"pub fn answer() -> u32 { 42 }\n").unwrap();
        fs::write(repo.join("README.md"), b"# Demo\n").unwrap();

        let mut state = RealRepoState::ingest(&repo, "repo_test").unwrap();
        state.topic_id = Some("topic_demo".to_string());
        state.session_id = Some("session_agent".to_string());
        state.generation_number = 1;
        state.save(&repo).unwrap();

        let loaded = RealRepoState::load(&repo).unwrap();
        assert_eq!(loaded.repository_id, "repo_test");
        assert_eq!(loaded.topic_id.as_deref(), Some("topic_demo"));
        assert_eq!(loaded.session_id.as_deref(), Some("session_agent"));
        assert_eq!(loaded.entries.len(), 2);
        assert!(real_state_path(&repo).is_file());
        assert_eq!(loaded.tree_hash, state.tree_hash);

        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn git_ingestion_uses_exclude_standard_ignore_policy() {
        let repo = temp_repo("git-ignore");
        git(&repo, &["init", "-b", "main"]);
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::create_dir_all(repo.join("target/debug")).unwrap();
        fs::create_dir_all(repo.join(".cache/sun")).unwrap();
        fs::write(repo.join(".gitignore"), b"target/\n.cache/\n").unwrap();
        fs::write(repo.join("src/lib.rs"), b"pub fn kept() {}\n").unwrap();
        fs::write(
            repo.join("target/debug/build.log"),
            b"ignored build cache\n",
        )
        .unwrap();
        fs::write(repo.join(".cache/sun/local.txt"), b"ignored local cache\n").unwrap();

        let state = RealRepoState::ingest(&repo, "repo_git_ignore").unwrap();
        let paths = state
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>();

        assert_eq!(paths, vec![".gitignore", "src/lib.rs"]);
        assert!(state.entry("src/lib.rs").is_some());
        assert!(state.entry("target/debug/build.log").is_none());
        assert!(state.entry(".cache/sun/local.txt").is_none());

        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn ingest_quarantines_secret_path_without_persisting_secret_bytes() {
        let repo = temp_repo("secret-path");
        git(&repo, &["init", "-b", "main"]);
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::write(repo.join("src/lib.rs"), b"pub fn kept() {}\n").unwrap();
        fs::write(
            repo.join(".env"),
            b"API_KEY=super-secret-value-that-must-not-persist\n",
        )
        .unwrap();
        git(&repo, &["add", "src/lib.rs", ".env"]);

        let state = RealRepoState::ingest(&repo, "repo_secret_path").unwrap();
        state.save(&repo).unwrap();
        persist_quarantine_report(&repo, &state.quarantine).unwrap();

        assert_eq!(state.entries.len(), 1);
        assert!(state.entry("src/lib.rs").is_some());
        assert!(state.entry(".env").is_none());
        assert_eq!(state.quarantine.len(), 1);
        assert_eq!(state.quarantine[0].path, ".env");
        assert!(state.quarantine[0]
            .reason_codes
            .contains(&"secret_path".to_string()));
        assert!(state.quarantine[0]
            .reason_codes
            .contains(&"secret_token".to_string()));

        let state_json = fs::read_to_string(real_state_path(&repo)).unwrap();
        assert!(state_json.contains("\"path\":\".env\""));
        assert!(!state_json.contains("super-secret-value-that-must-not-persist"));
        assert!(!real_blob_path(&repo, &state.quarantine[0].content_hash).exists());

        let report =
            fs::read_to_string(repo.join(".sunlight/quarantine/ingest-report.json")).unwrap();
        assert!(report.contains("\"record_type\":\"ingest_quarantine_report\""));
        assert!(report.contains("\"path\":\".env\""));
        assert!(report.contains("\"quarantined_count\":1"));
        assert!(!report.contains("super-secret-value-that-must-not-persist"));

        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn ingest_quarantines_secret_token_in_unignored_source_like_path() {
        let repo = temp_repo("secret-token");
        fs::create_dir_all(repo.join("config")).unwrap();
        fs::write(
            repo.join("config/app.toml"),
            b"client_secret = \"abc123\"\n",
        )
        .unwrap();
        fs::write(repo.join("README.md"), b"# public\n").unwrap();

        let state = RealRepoState::ingest(&repo, "repo_secret_token").unwrap();

        assert_eq!(state.entries.len(), 1);
        assert!(state.entry("README.md").is_some());
        assert!(state.entry("config/app.toml").is_none());
        assert_eq!(state.quarantine.len(), 1);
        assert_eq!(state.quarantine[0].path, "config/app.toml");
        assert_eq!(state.quarantine[0].reason_codes, vec!["secret_token"]);

        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn non_git_ingestion_fallback_still_excludes_sunlight_and_git_dirs() {
        let repo = temp_repo("fallback");
        fs::create_dir_all(repo.join("src")).unwrap();
        fs::create_dir_all(repo.join(".git/objects")).unwrap();
        fs::create_dir_all(repo.join(".sunlight/records")).unwrap();
        fs::write(repo.join("src/lib.rs"), b"pub fn kept() {}\n").unwrap();
        fs::write(repo.join(".git/config"), b"[core]\n").unwrap();
        fs::write(repo.join(".sunlight/records/native-state.json"), b"{}\n").unwrap();

        let state = RealRepoState::ingest(&repo, "repo_fallback").unwrap();
        let paths = state
            .entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>();

        assert_eq!(paths, vec!["src/lib.rs"]);

        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn path_serialization_preserves_tabs_newlines_and_json_metacharacters() {
        let repo = temp_repo("path-safety");
        let odd_path = "dir/tab\tquote\"line\nname.rs";
        let bytes = b"fn main() {}\n".to_vec();
        let entries = vec![RealArtifactEntry {
            path: odd_path.to_string(),
            artifact_id: real_artifact_id_for_path(odd_path),
            content_hash: real_content_hash(&bytes),
            executable: false,
            classification: "source".to_string(),
            tombstone: false,
            bytes,
        }];
        let state = RealRepoState {
            repository_id: "repo_paths".to_string(),
            base_checkpoint_id: "checkpoint_base_0001".to_string(),
            base_resolved_view_id: "view_base_0001".to_string(),
            resolved_view_id: "view_base_0001".to_string(),
            tree_hash: real_tree_hash(&entries),
            topic_id: None,
            topic_slug: None,
            topic_display_name: None,
            session_id: None,
            actor_id: None,
            generation_number: 0,
            revision_number: 0,
            head_revision_id: None,
            topics: Vec::new(),
            sessions: Vec::new(),
            entries,
            quarantine: Vec::new(),
        };
        state.save(&repo).unwrap();
        let state_bytes = fs::read(real_state_path(&repo)).unwrap();
        let state_text = String::from_utf8(state_bytes).unwrap();
        assert!(state_text.contains("\\t"));
        assert!(state_text.contains("\\n"));
        assert!(state_text.contains("\\\""));

        let loaded = RealRepoState::load(&repo).unwrap();
        assert_eq!(loaded.entries[0].path, odd_path.replace('\\', "/"));
        assert_eq!(loaded.entries[0].bytes, b"fn main() {}\n");

        fs::remove_dir_all(repo).unwrap();
    }
}

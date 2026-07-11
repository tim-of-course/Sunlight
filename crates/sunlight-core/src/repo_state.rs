use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use crate::artifacts::{
    PathPolicy, FILE_OPERATION_SEMANTICS_VERSION, POSIX_CASE_SENSITIVE_PATH_POLICY_ID,
};
use crate::records::{canonical_json_bytes, parse_json_record, JsonValue, RecordError};
use crate::resolver::{
    resolve_fixture_view, DeterministicResolverOrder, OperationRef, PathRef, ResolvedViewResult,
    ResolverConflictOrStalenessRecord, ResolverInputFrontier, ResolverMutationKind,
    ResolverRecordKind, SingleRepoTree, TopicRevisionRef, TopicRevisionSelection, TreeEntryState,
};

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
    pub base_entries: Vec<RealArtifactEntry>,
    pub operations: Vec<RealOperationRecord>,
    pub projections: Vec<RealProjectionSnapshot>,
    pub executions: Vec<RealExecutionSnapshot>,
    pub promotions: Vec<RealExecutionPromotionSnapshot>,
    pub checkpoints: Vec<RealCheckpointSnapshot>,
    pub export_maps: Vec<RealExportMapSnapshot>,
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
pub struct RealOperationRecord {
    pub operation_transaction_id: String,
    pub topic_id: String,
    pub topic_revision_id: String,
    pub session_id: String,
    pub artifact_id: String,
    pub path: String,
    pub mutation: String,
    pub base_content_hash: Option<String>,
    pub result_content_hash: String,
    pub authored_context_id: String,
    pub dependency_revision_ids: Vec<String>,
    pub classification: String,
    pub executable: bool,
    pub tombstone: bool,
    pub bytes: Vec<u8>,
    pub compat_projection_id: Option<String>,
    pub compat_candidate_delta_ids: Vec<String>,
    pub effects: Vec<RealOperationEffect>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealOperationEffect {
    pub artifact_id: String,
    pub path: String,
    pub base_content_hash: Option<String>,
    pub result_content_hash: String,
    pub classification: String,
    pub executable: bool,
    pub tombstone: bool,
    pub bytes: Vec<u8>,
}

impl RealOperationRecord {
    pub fn artifact_effects(&self) -> Vec<RealOperationEffect> {
        if !self.effects.is_empty() {
            return self.effects.clone();
        }
        vec![RealOperationEffect {
            artifact_id: self.artifact_id.clone(),
            path: self.path.clone(),
            base_content_hash: self.base_content_hash.clone(),
            result_content_hash: self.result_content_hash.clone(),
            classification: self.classification.clone(),
            executable: self.executable,
            tombstone: self.tombstone,
            bytes: self.bytes.clone(),
        }]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealProjectionSnapshot {
    pub projection_id: String,
    pub repository_id: String,
    pub purpose: String,
    pub resolved_view_id: String,
    pub tree_hash: String,
    pub manifest_digest: String,
    pub created_from_content_tree: String,
    pub materialized_root: Option<String>,
    pub session_id: Option<String>,
    pub session_generation_id: Option<String>,
    pub path_policy_id: String,
    pub operation_semantics_version: String,
    pub strategy: String,
    pub retention_state: String,
    pub privacy_class: String,
    pub last_import_operation_id: Option<String>,
    pub entries: Vec<RealArtifactEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealExecutionSnapshot {
    pub execution_id: String,
    pub projection_id: String,
    pub resolved_view_id: String,
    pub tree_hash: String,
    pub command_argv: Vec<String>,
    pub working_directory: String,
    pub exit_code: Option<i32>,
    pub status: String,
    pub stdout_digest: String,
    pub stdout_byte_length: u64,
    pub stderr_digest: String,
    pub stderr_byte_length: u64,
    pub outputs: Vec<RealExecutionOutputSnapshot>,
    pub started_at: String,
    pub finished_at: String,
    pub privacy_class: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealExecutionOutputSnapshot {
    pub path: String,
    pub classification: String,
    pub before_hash: Option<String>,
    pub after_hash: String,
    pub byte_length: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealExecutionPromotionSnapshot {
    pub execution_id: String,
    pub projection_id: String,
    pub output_path: String,
    pub target_topic_id: String,
    pub classification: String,
    pub before_hash: Option<String>,
    pub after_hash: String,
    pub operation_transaction_id: String,
    pub topic_revision_id: String,
    pub session_generation_id: String,
    pub authored_context_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealCheckpointSnapshot {
    pub checkpoint_id: String,
    pub resolved_view_id: String,
    pub tree_hash: String,
    pub topic_frontier: Vec<(String, String)>,
    pub created_at: String,
    pub entries: Vec<RealArtifactEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealExportMapSnapshot {
    pub export_map_id: String,
    pub checkpoint_id: String,
    pub tree_hash: String,
    pub git_ref: String,
    pub git_commit_ids: Vec<String>,
    pub exported_at: String,
    pub validation_report_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealResolvedRepoView {
    pub result: ResolvedViewResult,
    pub entries: Vec<RealArtifactEntry>,
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
            base_entries: entries.clone(),
            operations: Vec::new(),
            projections: Vec::new(),
            executions: Vec::new(),
            promotions: Vec::new(),
            checkpoints: Vec::new(),
            export_maps: Vec::new(),
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
        let base_entries = match optional_array_field(&object, "base_entries") {
            Some(values) => values
                .iter()
                .map(|entry| parse_entry(repo_root, entry, &path))
                .collect::<Result<Vec<_>, _>>()?,
            None => entries.clone(),
        };
        let operations = optional_array(&object, "operations", &path)?
            .iter()
            .map(|operation| parse_operation(repo_root, operation, &path))
            .collect::<Result<Vec<_>, _>>()?;
        let projections = optional_array(&object, "projections", &path)?
            .iter()
            .map(|projection| parse_projection_snapshot(repo_root, projection, &path))
            .collect::<Result<Vec<_>, _>>()?;
        let executions = optional_array(&object, "executions", &path)?
            .iter()
            .map(|execution| parse_execution_snapshot(execution, &path))
            .collect::<Result<Vec<_>, _>>()?;
        let promotions = optional_array(&object, "promotions", &path)?
            .iter()
            .map(|promotion| parse_execution_promotion_snapshot(promotion, &path))
            .collect::<Result<Vec<_>, _>>()?;
        let checkpoints = optional_array(&object, "checkpoints", &path)?
            .iter()
            .map(|checkpoint| parse_checkpoint_snapshot(repo_root, checkpoint, &path))
            .collect::<Result<Vec<_>, _>>()?;
        let export_maps = optional_array(&object, "export_maps", &path)?
            .iter()
            .map(|export_map| parse_export_map_snapshot(export_map, &path))
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
            base_entries,
            operations,
            projections,
            executions,
            promotions,
            checkpoints,
            export_maps,
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
        for entry in self.entries.iter().chain(self.base_entries.iter()) {
            persist_blob(repo_root, &entry.content_hash, &entry.bytes)?;
        }
        for operation in &self.operations {
            persist_blob(repo_root, &operation.result_content_hash, &operation.bytes)?;
            for effect in &operation.effects {
                persist_blob(repo_root, &effect.result_content_hash, &effect.bytes)?;
            }
        }
        for projection in &self.projections {
            for entry in &projection.entries {
                persist_blob(repo_root, &entry.content_hash, &entry.bytes)?;
            }
        }
        for checkpoint in &self.checkpoints {
            for entry in &checkpoint.entries {
                persist_blob(repo_root, &entry.content_hash, &entry.bytes)?;
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
            "base_entries".to_string(),
            JsonValue::Array(self.base_entries.iter().map(entry_json).collect()),
        );
        object.insert(
            "operations".to_string(),
            JsonValue::Array(self.operations.iter().map(operation_json).collect()),
        );
        object.insert(
            "projections".to_string(),
            JsonValue::Array(
                self.projections
                    .iter()
                    .map(projection_snapshot_json)
                    .collect(),
            ),
        );
        object.insert(
            "executions".to_string(),
            JsonValue::Array(
                self.executions
                    .iter()
                    .map(execution_snapshot_json)
                    .collect(),
            ),
        );
        object.insert(
            "promotions".to_string(),
            JsonValue::Array(
                self.promotions
                    .iter()
                    .map(execution_promotion_snapshot_json)
                    .collect(),
            ),
        );
        object.insert(
            "checkpoints".to_string(),
            JsonValue::Array(
                self.checkpoints
                    .iter()
                    .map(checkpoint_snapshot_json)
                    .collect(),
            ),
        );
        object.insert(
            "export_maps".to_string(),
            JsonValue::Array(
                self.export_maps
                    .iter()
                    .map(export_map_snapshot_json)
                    .collect(),
            ),
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

    pub fn resolve_head_view(&self) -> RealResolvedRepoView {
        let frontier = self
            .topics
            .iter()
            .filter_map(|topic| {
                topic
                    .head_revision_id
                    .as_ref()
                    .map(|revision| TopicRevisionSelection {
                        topic_id: topic.topic_id.clone(),
                        revision_id: revision.clone(),
                    })
            })
            .collect();
        self.resolve_view(frontier)
    }

    pub fn resolve_session_view(&self, session: &RealSessionRecord) -> RealResolvedRepoView {
        let frontier = self
            .topics
            .iter()
            .filter(|topic| topic.topic_id == session.write_topic_id)
            .filter_map(|topic| {
                topic
                    .head_revision_id
                    .as_ref()
                    .map(|revision| TopicRevisionSelection {
                        topic_id: topic.topic_id.clone(),
                        revision_id: revision.clone(),
                    })
            })
            .collect();
        self.resolve_view(frontier)
    }

    pub fn resolve_view(&self, frontier: Vec<TopicRevisionSelection>) -> RealResolvedRepoView {
        resolve_real_repo_view(self, frontier)
    }
}

pub fn resolve_real_repo_view(
    state: &RealRepoState,
    frontier: Vec<TopicRevisionSelection>,
) -> RealResolvedRepoView {
    let input = ResolverInputFrontier {
        repository_id: state.repository_id.clone(),
        base_checkpoint_ids: vec![state.base_checkpoint_id.clone()],
        topic_frontier: frontier,
        operation_semantics_version: FILE_OPERATION_SEMANTICS_VERSION.to_string(),
        path_policy_id: POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string(),
    };
    let base_tree_entries = state
        .base_entries
        .iter()
        .filter(|entry| !entry.tombstone)
        .map(|entry| TreeEntryState {
            artifact_id: entry.artifact_id.clone(),
            path: entry.path.clone(),
            content_hash: entry.content_hash.clone(),
        })
        .collect::<Vec<_>>();
    let revision_refs = state
        .operations
        .iter()
        .map(real_operation_revision_ref)
        .collect::<Vec<_>>();
    let mut result = resolve_fixture_view(input, base_tree_entries, revision_refs);
    if result.records.is_empty() {
        result.resolver_order = DeterministicResolverOrder {
            operation_ids: expanded_operation_order(state, &result.topic_frontier),
        };
        result.dependency_closure.revision_ids = result
            .resolver_order
            .operation_ids
            .iter()
            .filter_map(|operation_id| {
                state
                    .operations
                    .iter()
                    .find(|operation| operation.operation_transaction_id == *operation_id)
                    .map(|operation| operation.topic_revision_id.clone())
            })
            .collect();
        result
            .records
            .extend(expanded_same_artifact_conflicts(state, &result));
    }
    let entries = if result.records.is_empty() {
        materialize_real_resolved_entries(state, &result.resolver_order)
    } else {
        state
            .base_entries
            .iter()
            .filter(|entry| !entry.tombstone)
            .cloned()
            .collect()
    };
    if result.records.is_empty() {
        result.tree_entries = entries
            .iter()
            .filter(|entry| !entry.tombstone)
            .map(|entry| {
                (
                    entry.path.clone(),
                    TreeEntryState {
                        artifact_id: entry.artifact_id.clone(),
                        path: entry.path.clone(),
                        content_hash: entry.content_hash.clone(),
                    },
                )
            })
            .collect();
        result.tree_identity = Some(SingleRepoTree {
            repository_id: state.repository_id.clone(),
            tree_hash: real_tree_hash(&entries),
        });
    }
    RealResolvedRepoView { result, entries }
}

pub fn expanded_operation_order(
    state: &RealRepoState,
    frontier: &BTreeMap<String, String>,
) -> Vec<String> {
    let mut operation_ids = Vec::new();
    for (topic_id, head_revision_id) in frontier {
        for operation in state
            .operations
            .iter()
            .filter(|operation| operation.topic_id == *topic_id)
        {
            operation_ids.push(operation.operation_transaction_id.clone());
            if operation.topic_revision_id == *head_revision_id {
                break;
            }
        }
    }
    operation_ids
}

fn expanded_same_artifact_conflicts(
    state: &RealRepoState,
    result: &ResolvedViewResult,
) -> Vec<ResolverConflictOrStalenessRecord> {
    let mut latest_by_topic_artifact =
        BTreeMap::<(String, String), (&RealOperationRecord, RealOperationEffect)>::new();
    for operation_id in &result.resolver_order.operation_ids {
        let Some(operation) = state
            .operations
            .iter()
            .find(|candidate| candidate.operation_transaction_id == *operation_id)
        else {
            continue;
        };
        for effect in operation.artifact_effects() {
            latest_by_topic_artifact.insert(
                (operation.topic_id.clone(), effect.artifact_id.clone()),
                (operation, effect),
            );
        }
    }

    let mut by_artifact =
        BTreeMap::<String, Vec<(&RealOperationRecord, RealOperationEffect)>>::new();
    for ((_topic_id, artifact_id), operation_effect) in latest_by_topic_artifact {
        by_artifact
            .entry(artifact_id)
            .or_default()
            .push(operation_effect);
    }

    by_artifact
        .into_iter()
        .filter_map(|(artifact_id, operations)| {
            if operations.len() <= 1 {
                return None;
            }
            let candidate_hashes = operations
                .iter()
                .map(|(_, effect)| effect.result_content_hash.clone())
                .collect::<std::collections::BTreeSet<_>>();
            if candidate_hashes.len() <= 1 {
                return None;
            }
            let paths = operations
                .iter()
                .map(|(_, effect)| effect.path.clone())
                .collect::<std::collections::BTreeSet<_>>();
            let mut candidate_refs = BTreeMap::new();
            candidate_refs.insert(
                "base_content_hashes".to_string(),
                operations
                    .iter()
                    .filter_map(|(_, effect)| effect.base_content_hash.clone())
                    .collect::<std::collections::BTreeSet<_>>()
                    .into_iter()
                    .collect(),
            );
            candidate_refs.insert(
                "candidate_hashes".to_string(),
                candidate_hashes.into_iter().collect(),
            );
            candidate_refs.insert(
                "operation_semantics_version".to_string(),
                vec![FILE_OPERATION_SEMANTICS_VERSION.to_string()],
            );
            candidate_refs.insert(
                "path_policy_id".to_string(),
                vec![POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string()],
            );
            Some(ResolverConflictOrStalenessRecord {
                id: format!("conflict_{}_0001", artifact_id.replace("artifact_", "")),
                kind: ResolverRecordKind::SameArtifactConflict,
                resolved_view_id: result.resolved_view_id.clone(),
                artifact_ids: vec![artifact_id],
                path_refs: paths
                    .into_iter()
                    .map(|path| PathRef {
                        path,
                        path_state: "active".to_string(),
                    })
                    .collect(),
                operation_ids: operations
                    .iter()
                    .map(|(operation, _)| operation.operation_transaction_id.clone())
                    .collect(),
                authored_context_ids: operations
                    .iter()
                    .map(|(operation, _)| operation.authored_context_id.clone())
                    .collect(),
                policy_reason:
                    "same artifact operations are not proven commutative under file_ops_v1"
                        .to_string(),
                candidate_refs,
                resolution_operation_id: None,
            })
        })
        .collect()
}

fn materialize_real_resolved_entries(
    state: &RealRepoState,
    order: &DeterministicResolverOrder,
) -> Vec<RealArtifactEntry> {
    let mut entries = state
        .base_entries
        .iter()
        .filter(|entry| !entry.tombstone)
        .cloned()
        .collect::<Vec<_>>();
    for operation_id in &order.operation_ids {
        let Some(operation) = state
            .operations
            .iter()
            .find(|candidate| candidate.operation_transaction_id == *operation_id)
        else {
            continue;
        };
        for effect in operation.artifact_effects() {
            entries.retain(|entry| entry.artifact_id != effect.artifact_id);
            entries.push(RealArtifactEntry {
                path: effect.path,
                artifact_id: effect.artifact_id,
                content_hash: effect.result_content_hash,
                executable: effect.executable,
                classification: effect.classification,
                tombstone: effect.tombstone,
                bytes: effect.bytes,
            });
        }
    }
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    entries
}

fn real_operation_revision_ref(operation: &RealOperationRecord) -> TopicRevisionRef {
    TopicRevisionRef {
        topic_id: operation.topic_id.clone(),
        revision_id: operation.topic_revision_id.clone(),
        operation: OperationRef {
            operation_transaction_id: operation.operation_transaction_id.clone(),
            topic_id: operation.topic_id.clone(),
            topic_revision_id: operation.topic_revision_id.clone(),
            artifact_id: operation.artifact_id.clone(),
            path: operation.path.clone(),
            mutation: if operation.base_content_hash.is_some() {
                ResolverMutationKind::Patch
            } else {
                ResolverMutationKind::Write
            },
            base_content_hash: operation.base_content_hash.clone(),
            result_content_hash: operation.result_content_hash.clone(),
            authored_context_id: operation.authored_context_id.clone(),
        },
        dependency_revision_ids: operation.dependency_revision_ids.clone(),
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

pub fn scan_real_projection_files_with_quarantine(
    projection_root: &Path,
    current: &Path,
    entries: &mut Vec<RealArtifactEntry>,
    quarantine: &mut Vec<RealQuarantineEntry>,
) -> Result<(), RepoStateError> {
    let children = fs::read_dir(current)
        .map_err(|error| io_error(current, "failed to scan projection", error))?;
    for child in children {
        let child = child.map_err(|error| RepoStateError::Io {
            path: current.to_path_buf(),
            message: format!("failed to scan projection: {error}"),
        })?;
        let path = child.path();
        let relative = path
            .strip_prefix(projection_root)
            .map_err(|_| invalid_state(projection_root, "projection path escaped its root"))?
            .to_string_lossy()
            .replace('\\', "/");
        PathPolicy::posix_case_sensitive()
            .validate(&relative)
            .map_err(|error| {
                invalid_state(
                    &path,
                    &format!("projection output path failed configured path policy: {error}"),
                )
            })?;
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| io_error(&path, "failed to inspect projection path", error))?;
        if metadata.file_type().is_symlink() {
            return Err(invalid_state(
                &path,
                "projection symlinks are not supported by the local MVP scanner",
            ));
        }
        if metadata.is_dir() {
            scan_real_projection_files_with_quarantine(
                projection_root,
                &path,
                entries,
                quarantine,
            )?;
        } else if metadata.is_file() {
            ingest_real_repo_path(projection_root, &relative, entries, quarantine)?;
        }
    }
    Ok(())
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

fn persist_blob(repo_root: &Path, content_hash: &str, bytes: &[u8]) -> Result<(), RepoStateError> {
    let path = real_blob_path(repo_root, content_hash);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| io_error(parent, "failed to create blob directory", error))?;
    }
    if !path.exists() {
        fs::write(&path, bytes)
            .map_err(|error| io_error(&path, "failed to write content blob", error))?;
    }
    Ok(())
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
    let path_policy = PathPolicy::posix_case_sensitive();
    for entry in state.entries.iter().filter(|entry| !entry.tombstone) {
        path_policy
            .validate(&entry.path)
            .map_err(|error| RepoStateError::InvalidState {
                path: root.to_path_buf(),
                message: format!(
                    "projection content path `{}` failed configured path policy: {error}",
                    entry.path
                ),
            })?;
    }
    match fs::symlink_metadata(root) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(RepoStateError::InvalidState {
                path: root.to_path_buf(),
                message: "projection root cannot be a symlink".to_string(),
            });
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(RepoStateError::InvalidState {
                path: root.to_path_buf(),
                message: "projection root must be an empty directory or a creatable path"
                    .to_string(),
            });
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(io_error(root, "failed to inspect projection root", error));
        }
    }
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
        set_projection_executable(&path, entry.executable)?;
    }
    Ok(())
}

#[cfg(unix)]
fn set_projection_executable(path: &Path, executable: bool) -> Result<(), RepoStateError> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|error| io_error(path, "failed to inspect projection permissions", error))?
        .permissions();
    let mode = permissions.mode();
    permissions.set_mode(if executable {
        mode | 0o111
    } else {
        mode & !0o111
    });
    fs::set_permissions(path, permissions)
        .map_err(|error| io_error(path, "failed to preserve projection executable bit", error))
}

#[cfg(not(unix))]
fn set_projection_executable(_path: &Path, _executable: bool) -> Result<(), RepoStateError> {
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

fn parse_operation(
    repo_root: &Path,
    value: &JsonValue,
    state_path: &Path,
) -> Result<RealOperationRecord, RepoStateError> {
    let JsonValue::Object(object) = value else {
        return Err(invalid_state(state_path, "operation must be a JSON object"));
    };
    let result_content_hash = required_string(object, "result_content_hash", state_path)?;
    let dependency_revision_ids = optional_array(object, "dependency_revision_ids", state_path)?
        .iter()
        .map(|value| match value {
            JsonValue::String(revision) => Ok(revision.clone()),
            _ => Err(invalid_state(
                state_path,
                "operation dependency_revision_ids must be strings",
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let effects = optional_array(object, "effects", state_path)?
        .iter()
        .map(|value| parse_operation_effect(repo_root, value, state_path))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RealOperationRecord {
        operation_transaction_id: required_string(object, "operation_transaction_id", state_path)?,
        topic_id: required_string(object, "topic_id", state_path)?,
        topic_revision_id: required_string(object, "topic_revision_id", state_path)?,
        session_id: required_string(object, "session_id", state_path)?,
        artifact_id: required_string(object, "artifact_id", state_path)?,
        path: required_string(object, "path", state_path)?,
        mutation: required_string(object, "mutation", state_path)?,
        base_content_hash: optional_string(object, "base_content_hash", state_path)?,
        bytes: read_real_blob(repo_root, &result_content_hash)?,
        result_content_hash,
        authored_context_id: required_string(object, "authored_context_id", state_path)?,
        dependency_revision_ids,
        classification: required_string(object, "classification", state_path)?,
        executable: required_bool(object, "executable", state_path)?,
        tombstone: required_bool(object, "tombstone", state_path)?,
        compat_projection_id: optional_string(object, "compat_projection_id", state_path)?,
        compat_candidate_delta_ids: optional_array(
            object,
            "compat_candidate_delta_ids",
            state_path,
        )?
        .iter()
        .map(|value| match value {
            JsonValue::String(candidate_id) => Ok(candidate_id.clone()),
            _ => Err(invalid_state(
                state_path,
                "operation compat_candidate_delta_ids must be strings",
            )),
        })
        .collect::<Result<Vec<_>, _>>()?,
        effects,
    })
}

fn parse_operation_effect(
    repo_root: &Path,
    value: &JsonValue,
    state_path: &Path,
) -> Result<RealOperationEffect, RepoStateError> {
    let JsonValue::Object(object) = value else {
        return Err(invalid_state(
            state_path,
            "operation effect must be a JSON object",
        ));
    };
    let result_content_hash = required_string(object, "result_content_hash", state_path)?;
    Ok(RealOperationEffect {
        artifact_id: required_string(object, "artifact_id", state_path)?,
        path: required_string(object, "path", state_path)?,
        base_content_hash: optional_string(object, "base_content_hash", state_path)?,
        bytes: read_real_blob(repo_root, &result_content_hash)?,
        result_content_hash,
        classification: required_string(object, "classification", state_path)?,
        executable: required_bool(object, "executable", state_path)?,
        tombstone: required_bool(object, "tombstone", state_path)?,
    })
}

fn parse_projection_snapshot(
    repo_root: &Path,
    value: &JsonValue,
    state_path: &Path,
) -> Result<RealProjectionSnapshot, RepoStateError> {
    let JsonValue::Object(object) = value else {
        return Err(invalid_state(
            state_path,
            "projection must be a JSON object",
        ));
    };
    let entries = required_array(object, "entries", state_path)?
        .iter()
        .map(|entry| parse_entry(repo_root, entry, state_path))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RealProjectionSnapshot {
        projection_id: required_string(object, "projection_id", state_path)?,
        repository_id: optional_string(object, "repository_id", state_path)?.unwrap_or_default(),
        purpose: required_string(object, "purpose", state_path)?,
        resolved_view_id: required_string(object, "resolved_view_id", state_path)?,
        tree_hash: required_string(object, "tree_hash", state_path)?,
        manifest_digest: required_string(object, "manifest_digest", state_path)?,
        created_from_content_tree: required_string(
            object,
            "created_from_content_tree",
            state_path,
        )?,
        materialized_root: optional_string(object, "materialized_root", state_path)?,
        session_id: optional_string(object, "session_id", state_path)?,
        session_generation_id: optional_string(object, "session_generation_id", state_path)?,
        path_policy_id: optional_string(object, "path_policy_id", state_path)?
            .unwrap_or_else(|| POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string()),
        operation_semantics_version: optional_string(
            object,
            "operation_semantics_version",
            state_path,
        )?
        .unwrap_or_else(|| FILE_OPERATION_SEMANTICS_VERSION.to_string()),
        strategy: optional_string(object, "strategy", state_path)?
            .unwrap_or_else(|| "copy".to_string()),
        retention_state: optional_string(object, "retention_state", state_path)?
            .unwrap_or_else(|| "active".to_string()),
        privacy_class: optional_string(object, "privacy_class", state_path)?
            .unwrap_or_else(|| "local_only".to_string()),
        last_import_operation_id: optional_string(object, "last_import_operation_id", state_path)?,
        entries,
    })
}

fn parse_execution_snapshot(
    value: &JsonValue,
    state_path: &Path,
) -> Result<RealExecutionSnapshot, RepoStateError> {
    let JsonValue::Object(object) = value else {
        return Err(invalid_state(state_path, "execution must be a JSON object"));
    };
    let command_argv = required_array(object, "command_argv", state_path)?
        .iter()
        .map(|value| match value {
            JsonValue::String(arg) => Ok(arg.clone()),
            _ => Err(invalid_state(
                state_path,
                "execution command_argv must be strings",
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let outputs = optional_array(object, "outputs", state_path)?
        .iter()
        .map(|output| parse_execution_output_snapshot(output, state_path))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RealExecutionSnapshot {
        execution_id: required_string(object, "execution_id", state_path)?,
        projection_id: required_string(object, "projection_id", state_path)?,
        resolved_view_id: required_string(object, "resolved_view_id", state_path)?,
        tree_hash: required_string(object, "tree_hash", state_path)?,
        command_argv,
        working_directory: required_string(object, "working_directory", state_path)?,
        exit_code: optional_i32(object, "exit_code", state_path)?,
        status: required_string(object, "status", state_path)?,
        stdout_digest: required_string(object, "stdout_digest", state_path)?,
        stdout_byte_length: required_u64(object, "stdout_byte_length", state_path)?,
        stderr_digest: required_string(object, "stderr_digest", state_path)?,
        stderr_byte_length: required_u64(object, "stderr_byte_length", state_path)?,
        outputs,
        started_at: required_string(object, "started_at", state_path)?,
        finished_at: required_string(object, "finished_at", state_path)?,
        privacy_class: required_string(object, "privacy_class", state_path)?,
    })
}

fn parse_execution_output_snapshot(
    value: &JsonValue,
    state_path: &Path,
) -> Result<RealExecutionOutputSnapshot, RepoStateError> {
    let JsonValue::Object(object) = value else {
        return Err(invalid_state(
            state_path,
            "execution output must be a JSON object",
        ));
    };
    Ok(RealExecutionOutputSnapshot {
        path: required_string(object, "path", state_path)?,
        classification: required_string(object, "classification", state_path)?,
        before_hash: optional_string(object, "before_hash", state_path)?,
        after_hash: required_string(object, "after_hash", state_path)?,
        byte_length: required_u64(object, "byte_length", state_path)?,
    })
}

fn parse_execution_promotion_snapshot(
    value: &JsonValue,
    state_path: &Path,
) -> Result<RealExecutionPromotionSnapshot, RepoStateError> {
    let JsonValue::Object(object) = value else {
        return Err(invalid_state(
            state_path,
            "execution promotion must be a JSON object",
        ));
    };
    Ok(RealExecutionPromotionSnapshot {
        execution_id: required_string(object, "execution_id", state_path)?,
        projection_id: required_string(object, "projection_id", state_path)?,
        output_path: required_string(object, "output_path", state_path)?,
        target_topic_id: required_string(object, "target_topic_id", state_path)?,
        classification: required_string(object, "classification", state_path)?,
        before_hash: optional_string(object, "before_hash", state_path)?,
        after_hash: required_string(object, "after_hash", state_path)?,
        operation_transaction_id: required_string(object, "operation_transaction_id", state_path)?,
        topic_revision_id: required_string(object, "topic_revision_id", state_path)?,
        session_generation_id: required_string(object, "session_generation_id", state_path)?,
        authored_context_id: required_string(object, "authored_context_id", state_path)?,
    })
}

fn parse_checkpoint_snapshot(
    repo_root: &Path,
    value: &JsonValue,
    state_path: &Path,
) -> Result<RealCheckpointSnapshot, RepoStateError> {
    let JsonValue::Object(object) = value else {
        return Err(invalid_state(
            state_path,
            "checkpoint must be a JSON object",
        ));
    };
    let entries = required_array(object, "entries", state_path)?
        .iter()
        .map(|entry| parse_entry(repo_root, entry, state_path))
        .collect::<Result<Vec<_>, _>>()?;
    let topic_frontier = optional_array(object, "topic_frontier", state_path)?
        .iter()
        .map(|value| {
            let JsonValue::Object(object) = value else {
                return Err(invalid_state(
                    state_path,
                    "checkpoint topic_frontier entries must be objects",
                ));
            };
            Ok((
                required_string(object, "topic_id", state_path)?,
                required_string(object, "topic_revision_id", state_path)?,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RealCheckpointSnapshot {
        checkpoint_id: required_string(object, "checkpoint_id", state_path)?,
        resolved_view_id: required_string(object, "resolved_view_id", state_path)?,
        tree_hash: required_string(object, "tree_hash", state_path)?,
        topic_frontier,
        created_at: required_string(object, "created_at", state_path)?,
        entries,
    })
}

fn parse_export_map_snapshot(
    value: &JsonValue,
    state_path: &Path,
) -> Result<RealExportMapSnapshot, RepoStateError> {
    let JsonValue::Object(object) = value else {
        return Err(invalid_state(
            state_path,
            "export_map must be a JSON object",
        ));
    };
    let git_commit_ids = optional_array(object, "git_commit_ids", state_path)?
        .iter()
        .map(|value| match value {
            JsonValue::String(commit_id) => Ok(commit_id.clone()),
            _ => Err(invalid_state(
                state_path,
                "export_map git_commit_ids must be strings",
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RealExportMapSnapshot {
        export_map_id: required_string(object, "export_map_id", state_path)?,
        checkpoint_id: required_string(object, "checkpoint_id", state_path)?,
        tree_hash: required_string(object, "tree_hash", state_path)?,
        git_ref: required_string(object, "git_ref", state_path)?,
        git_commit_ids,
        exported_at: required_string(object, "exported_at", state_path)?,
        validation_report_id: optional_string(object, "validation_report_id", state_path)?,
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

fn projection_snapshot_json(projection: &RealProjectionSnapshot) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "projection_id".to_string(),
        JsonValue::String(projection.projection_id.clone()),
    );
    object.insert(
        "repository_id".to_string(),
        JsonValue::String(projection.repository_id.clone()),
    );
    object.insert(
        "purpose".to_string(),
        JsonValue::String(projection.purpose.clone()),
    );
    object.insert(
        "resolved_view_id".to_string(),
        JsonValue::String(projection.resolved_view_id.clone()),
    );
    object.insert(
        "tree_hash".to_string(),
        JsonValue::String(projection.tree_hash.clone()),
    );
    object.insert(
        "manifest_digest".to_string(),
        JsonValue::String(projection.manifest_digest.clone()),
    );
    object.insert(
        "created_from_content_tree".to_string(),
        JsonValue::String(projection.created_from_content_tree.clone()),
    );
    object.insert(
        "materialized_root".to_string(),
        optional_json(&projection.materialized_root),
    );
    object.insert(
        "session_id".to_string(),
        optional_json(&projection.session_id),
    );
    object.insert(
        "session_generation_id".to_string(),
        optional_json(&projection.session_generation_id),
    );
    object.insert(
        "path_policy_id".to_string(),
        JsonValue::String(projection.path_policy_id.clone()),
    );
    object.insert(
        "operation_semantics_version".to_string(),
        JsonValue::String(projection.operation_semantics_version.clone()),
    );
    object.insert(
        "strategy".to_string(),
        JsonValue::String(projection.strategy.clone()),
    );
    object.insert(
        "retention_state".to_string(),
        JsonValue::String(projection.retention_state.clone()),
    );
    object.insert(
        "privacy_class".to_string(),
        JsonValue::String(projection.privacy_class.clone()),
    );
    object.insert(
        "last_import_operation_id".to_string(),
        optional_json(&projection.last_import_operation_id),
    );
    object.insert(
        "entries".to_string(),
        JsonValue::Array(projection.entries.iter().map(entry_json).collect()),
    );
    JsonValue::Object(object)
}

fn execution_snapshot_json(execution: &RealExecutionSnapshot) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "execution_id".to_string(),
        JsonValue::String(execution.execution_id.clone()),
    );
    object.insert(
        "projection_id".to_string(),
        JsonValue::String(execution.projection_id.clone()),
    );
    object.insert(
        "resolved_view_id".to_string(),
        JsonValue::String(execution.resolved_view_id.clone()),
    );
    object.insert(
        "tree_hash".to_string(),
        JsonValue::String(execution.tree_hash.clone()),
    );
    object.insert(
        "command_argv".to_string(),
        JsonValue::Array(
            execution
                .command_argv
                .iter()
                .map(|arg| JsonValue::String(arg.clone()))
                .collect(),
        ),
    );
    object.insert(
        "working_directory".to_string(),
        JsonValue::String(execution.working_directory.clone()),
    );
    object.insert(
        "exit_code".to_string(),
        execution
            .exit_code
            .map(|code| JsonValue::Number(code.to_string()))
            .unwrap_or(JsonValue::Null),
    );
    object.insert(
        "status".to_string(),
        JsonValue::String(execution.status.clone()),
    );
    object.insert(
        "stdout_digest".to_string(),
        JsonValue::String(execution.stdout_digest.clone()),
    );
    object.insert(
        "stdout_byte_length".to_string(),
        JsonValue::Number(execution.stdout_byte_length.to_string()),
    );
    object.insert(
        "stderr_digest".to_string(),
        JsonValue::String(execution.stderr_digest.clone()),
    );
    object.insert(
        "stderr_byte_length".to_string(),
        JsonValue::Number(execution.stderr_byte_length.to_string()),
    );
    object.insert(
        "outputs".to_string(),
        JsonValue::Array(
            execution
                .outputs
                .iter()
                .map(execution_output_snapshot_json)
                .collect(),
        ),
    );
    object.insert(
        "started_at".to_string(),
        JsonValue::String(execution.started_at.clone()),
    );
    object.insert(
        "finished_at".to_string(),
        JsonValue::String(execution.finished_at.clone()),
    );
    object.insert(
        "privacy_class".to_string(),
        JsonValue::String(execution.privacy_class.clone()),
    );
    JsonValue::Object(object)
}

fn execution_output_snapshot_json(output: &RealExecutionOutputSnapshot) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert("path".to_string(), JsonValue::String(output.path.clone()));
    object.insert(
        "classification".to_string(),
        JsonValue::String(output.classification.clone()),
    );
    object.insert(
        "before_hash".to_string(),
        optional_json(&output.before_hash),
    );
    object.insert(
        "after_hash".to_string(),
        JsonValue::String(output.after_hash.clone()),
    );
    object.insert(
        "byte_length".to_string(),
        JsonValue::Number(output.byte_length.to_string()),
    );
    JsonValue::Object(object)
}

fn execution_promotion_snapshot_json(promotion: &RealExecutionPromotionSnapshot) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "execution_id".to_string(),
        JsonValue::String(promotion.execution_id.clone()),
    );
    object.insert(
        "projection_id".to_string(),
        JsonValue::String(promotion.projection_id.clone()),
    );
    object.insert(
        "output_path".to_string(),
        JsonValue::String(promotion.output_path.clone()),
    );
    object.insert(
        "target_topic_id".to_string(),
        JsonValue::String(promotion.target_topic_id.clone()),
    );
    object.insert(
        "classification".to_string(),
        JsonValue::String(promotion.classification.clone()),
    );
    object.insert(
        "before_hash".to_string(),
        optional_json(&promotion.before_hash),
    );
    object.insert(
        "after_hash".to_string(),
        JsonValue::String(promotion.after_hash.clone()),
    );
    object.insert(
        "operation_transaction_id".to_string(),
        JsonValue::String(promotion.operation_transaction_id.clone()),
    );
    object.insert(
        "topic_revision_id".to_string(),
        JsonValue::String(promotion.topic_revision_id.clone()),
    );
    object.insert(
        "session_generation_id".to_string(),
        JsonValue::String(promotion.session_generation_id.clone()),
    );
    object.insert(
        "authored_context_id".to_string(),
        JsonValue::String(promotion.authored_context_id.clone()),
    );
    JsonValue::Object(object)
}

fn checkpoint_snapshot_json(checkpoint: &RealCheckpointSnapshot) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "checkpoint_id".to_string(),
        JsonValue::String(checkpoint.checkpoint_id.clone()),
    );
    object.insert(
        "resolved_view_id".to_string(),
        JsonValue::String(checkpoint.resolved_view_id.clone()),
    );
    object.insert(
        "tree_hash".to_string(),
        JsonValue::String(checkpoint.tree_hash.clone()),
    );
    object.insert(
        "topic_frontier".to_string(),
        JsonValue::Array(
            checkpoint
                .topic_frontier
                .iter()
                .map(|(topic_id, topic_revision_id)| {
                    let mut object = BTreeMap::new();
                    object.insert("topic_id".to_string(), JsonValue::String(topic_id.clone()));
                    object.insert(
                        "topic_revision_id".to_string(),
                        JsonValue::String(topic_revision_id.clone()),
                    );
                    JsonValue::Object(object)
                })
                .collect(),
        ),
    );
    object.insert(
        "created_at".to_string(),
        JsonValue::String(checkpoint.created_at.clone()),
    );
    object.insert(
        "entries".to_string(),
        JsonValue::Array(checkpoint.entries.iter().map(entry_json).collect()),
    );
    JsonValue::Object(object)
}

fn export_map_snapshot_json(export_map: &RealExportMapSnapshot) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "export_map_id".to_string(),
        JsonValue::String(export_map.export_map_id.clone()),
    );
    object.insert(
        "checkpoint_id".to_string(),
        JsonValue::String(export_map.checkpoint_id.clone()),
    );
    object.insert(
        "tree_hash".to_string(),
        JsonValue::String(export_map.tree_hash.clone()),
    );
    object.insert(
        "git_ref".to_string(),
        JsonValue::String(export_map.git_ref.clone()),
    );
    object.insert(
        "git_commit_ids".to_string(),
        JsonValue::Array(
            export_map
                .git_commit_ids
                .iter()
                .map(|commit_id| JsonValue::String(commit_id.clone()))
                .collect(),
        ),
    );
    object.insert(
        "exported_at".to_string(),
        JsonValue::String(export_map.exported_at.clone()),
    );
    if let Some(validation_report_id) = &export_map.validation_report_id {
        object.insert(
            "validation_report_id".to_string(),
            JsonValue::String(validation_report_id.clone()),
        );
    }
    JsonValue::Object(object)
}

fn operation_json(operation: &RealOperationRecord) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "operation_transaction_id".to_string(),
        JsonValue::String(operation.operation_transaction_id.clone()),
    );
    object.insert(
        "topic_id".to_string(),
        JsonValue::String(operation.topic_id.clone()),
    );
    object.insert(
        "topic_revision_id".to_string(),
        JsonValue::String(operation.topic_revision_id.clone()),
    );
    object.insert(
        "session_id".to_string(),
        JsonValue::String(operation.session_id.clone()),
    );
    object.insert(
        "artifact_id".to_string(),
        JsonValue::String(operation.artifact_id.clone()),
    );
    object.insert(
        "path".to_string(),
        JsonValue::String(operation.path.clone()),
    );
    object.insert(
        "mutation".to_string(),
        JsonValue::String(operation.mutation.clone()),
    );
    object.insert(
        "base_content_hash".to_string(),
        optional_json(&operation.base_content_hash),
    );
    object.insert(
        "result_content_hash".to_string(),
        JsonValue::String(operation.result_content_hash.clone()),
    );
    object.insert(
        "authored_context_id".to_string(),
        JsonValue::String(operation.authored_context_id.clone()),
    );
    object.insert(
        "dependency_revision_ids".to_string(),
        JsonValue::Array(
            operation
                .dependency_revision_ids
                .iter()
                .map(|revision| JsonValue::String(revision.clone()))
                .collect(),
        ),
    );
    object.insert(
        "classification".to_string(),
        JsonValue::String(operation.classification.clone()),
    );
    object.insert(
        "executable".to_string(),
        JsonValue::Bool(operation.executable),
    );
    object.insert(
        "tombstone".to_string(),
        JsonValue::Bool(operation.tombstone),
    );
    object.insert(
        "compat_projection_id".to_string(),
        optional_json(&operation.compat_projection_id),
    );
    object.insert(
        "compat_candidate_delta_ids".to_string(),
        JsonValue::Array(
            operation
                .compat_candidate_delta_ids
                .iter()
                .map(|candidate_id| JsonValue::String(candidate_id.clone()))
                .collect(),
        ),
    );
    object.insert(
        "effects".to_string(),
        JsonValue::Array(
            operation
                .effects
                .iter()
                .map(operation_effect_json)
                .collect(),
        ),
    );
    JsonValue::Object(object)
}

fn operation_effect_json(effect: &RealOperationEffect) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "artifact_id".to_string(),
        JsonValue::String(effect.artifact_id.clone()),
    );
    object.insert("path".to_string(), JsonValue::String(effect.path.clone()));
    object.insert(
        "base_content_hash".to_string(),
        optional_json(&effect.base_content_hash),
    );
    object.insert(
        "result_content_hash".to_string(),
        JsonValue::String(effect.result_content_hash.clone()),
    );
    object.insert(
        "classification".to_string(),
        JsonValue::String(effect.classification.clone()),
    );
    object.insert("executable".to_string(), JsonValue::Bool(effect.executable));
    object.insert("tombstone".to_string(), JsonValue::Bool(effect.tombstone));
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

fn optional_i32(
    object: &BTreeMap<String, JsonValue>,
    field: &'static str,
    path: &Path,
) -> Result<Option<i32>, RepoStateError> {
    match object.get(field) {
        Some(JsonValue::Null) | None => Ok(None),
        Some(JsonValue::Number(value)) => value
            .parse::<i32>()
            .map(Some)
            .map_err(|_| invalid_state(path, format!("field `{field}` must be an integer"))),
        _ => Err(invalid_state(
            path,
            format!("field `{field}` must be an integer or null"),
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

fn optional_array_field<'a>(
    object: &'a BTreeMap<String, JsonValue>,
    field: &'static str,
) -> Option<&'a [JsonValue]> {
    match object.get(field) {
        Some(JsonValue::Array(values)) => Some(values),
        _ => None,
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
            base_entries: entries.clone(),
            operations: Vec::new(),
            projections: Vec::new(),
            executions: Vec::new(),
            promotions: Vec::new(),
            checkpoints: Vec::new(),
            export_maps: Vec::new(),
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

    #[test]
    fn repo_state_resolves_independent_heads_and_reports_same_artifact_conflict() {
        let readme = artifact_entry("README.md", b"# Base\n");
        let lib = artifact_entry("src/lib.rs", b"pub fn value() -> u32 { 1 }\n");
        let mut state = RealRepoState {
            repository_id: "repo_resolve".to_string(),
            base_checkpoint_id: "checkpoint_base_0001".to_string(),
            base_resolved_view_id: "view_base_0001".to_string(),
            resolved_view_id: "view_base_0001".to_string(),
            tree_hash: real_tree_hash(&[readme.clone(), lib.clone()]),
            topic_id: None,
            topic_slug: None,
            topic_display_name: None,
            session_id: None,
            actor_id: None,
            generation_number: 0,
            revision_number: 0,
            head_revision_id: None,
            topics: vec![
                RealTopicRecord {
                    topic_id: "topic_docs".to_string(),
                    slug: "docs".to_string(),
                    display_name: "Docs".to_string(),
                    owner_actor_id: "agent-a".to_string(),
                    base_checkpoint_id: "checkpoint_base_0001".to_string(),
                    head_revision_id: Some("rev_docs_0001".to_string()),
                    revision_number: 1,
                },
                RealTopicRecord {
                    topic_id: "topic_code".to_string(),
                    slug: "code".to_string(),
                    display_name: "Code".to_string(),
                    owner_actor_id: "agent-b".to_string(),
                    base_checkpoint_id: "checkpoint_base_0001".to_string(),
                    head_revision_id: Some("rev_code_0001".to_string()),
                    revision_number: 1,
                },
            ],
            sessions: Vec::new(),
            base_entries: vec![readme.clone(), lib.clone()],
            operations: vec![
                operation(
                    "op_docs_0001",
                    "topic_docs",
                    "rev_docs_0001",
                    &readme,
                    b"# Base\n\nDocs\n",
                ),
                operation(
                    "op_code_0001",
                    "topic_code",
                    "rev_code_0001",
                    &lib,
                    b"pub fn value() -> u32 { 2 }\n",
                ),
            ],
            projections: Vec::new(),
            executions: Vec::new(),
            promotions: Vec::new(),
            checkpoints: Vec::new(),
            export_maps: Vec::new(),
            entries: vec![readme.clone(), lib.clone()],
            quarantine: Vec::new(),
        };

        let merged = state.resolve_head_view();
        assert!(merged.result.conflict_free());
        assert_eq!(
            merged.result.resolver_order.operation_ids,
            vec!["op_code_0001", "op_docs_0001"]
        );
        assert_eq!(
            merged
                .entries
                .iter()
                .find(|entry| entry.path == "README.md")
                .unwrap()
                .bytes,
            b"# Base\n\nDocs\n"
        );
        assert_eq!(
            merged
                .entries
                .iter()
                .find(|entry| entry.path == "src/lib.rs")
                .unwrap()
                .bytes,
            b"pub fn value() -> u32 { 2 }\n"
        );

        state.topics.push(RealTopicRecord {
            topic_id: "topic_alt_code".to_string(),
            slug: "alt-code".to_string(),
            display_name: "Alt Code".to_string(),
            owner_actor_id: "agent-c".to_string(),
            base_checkpoint_id: "checkpoint_base_0001".to_string(),
            head_revision_id: Some("rev_alt_code_0001".to_string()),
            revision_number: 1,
        });
        state.operations.push(operation(
            "op_alt_code_0001",
            "topic_alt_code",
            "rev_alt_code_0001",
            &lib,
            b"pub fn value() -> u32 { 3 }\n",
        ));

        let conflicted = state.resolve_head_view();
        assert!(!conflicted.result.conflict_free());
        let conflict = conflicted.result.conflicts().next().unwrap();
        assert_eq!(conflict.id, "conflict_src_lib_rs_0001");
        assert_eq!(conflict.kind.as_str(), "same_artifact_conflict");
        assert_eq!(
            conflict.operation_ids,
            vec!["op_alt_code_0001", "op_code_0001"]
        );
        assert_eq!(conflict.path_refs[0].path, "src/lib.rs");
    }

    fn artifact_entry(path: &str, bytes: &[u8]) -> RealArtifactEntry {
        RealArtifactEntry {
            path: path.to_string(),
            artifact_id: real_artifact_id_for_path(path),
            content_hash: real_content_hash(bytes),
            executable: false,
            classification: "source".to_string(),
            tombstone: false,
            bytes: bytes.to_vec(),
        }
    }

    fn operation(
        operation_id: &str,
        topic_id: &str,
        revision_id: &str,
        before: &RealArtifactEntry,
        after_bytes: &[u8],
    ) -> RealOperationRecord {
        RealOperationRecord {
            operation_transaction_id: operation_id.to_string(),
            topic_id: topic_id.to_string(),
            topic_revision_id: revision_id.to_string(),
            session_id: format!("session_{topic_id}"),
            artifact_id: before.artifact_id.clone(),
            path: before.path.clone(),
            mutation: "write".to_string(),
            base_content_hash: Some(before.content_hash.clone()),
            result_content_hash: real_content_hash(after_bytes),
            authored_context_id: "ctx_base".to_string(),
            dependency_revision_ids: Vec::new(),
            classification: before.classification.clone(),
            executable: before.executable,
            tombstone: false,
            bytes: after_bytes.to_vec(),
            compat_projection_id: None,
            compat_candidate_delta_ids: Vec::new(),
            effects: Vec::new(),
        }
    }

    #[test]
    fn multi_effect_operation_materializes_all_artifacts_as_one_revision() {
        let base = artifact_entry("src/lib.rs", b"old\n");
        let removed = artifact_entry("README.md", b"remove me\n");
        let mut state = RealRepoState::ingest(&temp_repo("multi-effect"), "repo_multi").unwrap();
        state.base_entries = vec![base.clone(), removed.clone()];
        let mut transaction = operation("op_multi", "topic_code", "rev_code_0001", &base, b"new\n");
        transaction.effects = vec![
            RealOperationEffect {
                artifact_id: base.artifact_id.clone(),
                path: base.path.clone(),
                base_content_hash: Some(base.content_hash.clone()),
                result_content_hash: real_content_hash(b"new\n"),
                classification: "source".to_string(),
                executable: false,
                tombstone: false,
                bytes: b"new\n".to_vec(),
            },
            RealOperationEffect {
                artifact_id: real_artifact_id_for_path("src/added.rs"),
                path: "src/added.rs".to_string(),
                base_content_hash: None,
                result_content_hash: real_content_hash(b"added\n"),
                classification: "source".to_string(),
                executable: false,
                tombstone: false,
                bytes: b"added\n".to_vec(),
            },
            RealOperationEffect {
                artifact_id: removed.artifact_id.clone(),
                path: removed.path.clone(),
                base_content_hash: Some(removed.content_hash.clone()),
                result_content_hash: removed.content_hash.clone(),
                classification: "source".to_string(),
                executable: false,
                tombstone: true,
                bytes: removed.bytes.clone(),
            },
        ];
        state.operations = vec![transaction];
        let entries = materialize_real_resolved_entries(
            &state,
            &DeterministicResolverOrder {
                operation_ids: vec!["op_multi".to_string()],
            },
        );
        assert_eq!(entries.iter().filter(|entry| !entry.tombstone).count(), 2);
        assert!(entries
            .iter()
            .any(|entry| entry.path == "src/lib.rs" && entry.bytes == b"new\n"));
        assert!(entries.iter().any(|entry| entry.path == "src/added.rs"));
        assert!(entries
            .iter()
            .any(|entry| entry.path == "README.md" && entry.tombstone));
    }
}

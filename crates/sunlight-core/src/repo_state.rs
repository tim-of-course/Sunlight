use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::artifacts::{
    PathPolicy, FILE_OPERATION_SEMANTICS_VERSION, POSIX_CASE_SENSITIVE_PATH_POLICY_ID,
};
use crate::checkpoint::{EvidenceRef, ExecutionEvidenceRef};
use crate::execution::ExecutionStatus;
use crate::projection::{
    ProjectionCacheKey, ProjectionPurpose, ProjectionStrategy, WritablePolicy,
};
use crate::records::{canonical_json_bytes, parse_json_record, JsonValue, RecordError};
use crate::resolver::{
    resolve_fixture_view, DeterministicResolverOrder, OperationRef, PathRef, ResolvedViewResult,
    ResolverConflictOrStalenessRecord, ResolverInputFrontier, ResolverMutationKind,
    ResolverRecordKind, SingleRepoTree, TopicRevisionRef, TopicRevisionSelection, TreeEntryState,
};

pub const REPO_STATE_SCHEMA_VERSION: u32 = 1;

const PROJECTION_CACHE_SCHEMA_VERSION: u32 = 1;
const PROJECTION_CACHE_ROOT: &str = ".sunlight/cache/projections/v1";
const PROJECTION_CACHE_MANIFEST_FILE: &str = "manifest.json";
const PROJECTION_CACHE_CONTENT_ROOT: &str = "root";
static PROJECTION_CACHE_NONCE: AtomicU64 = AtomicU64::new(1);

const DERIVED_RECORD_NAMESPACES: &[&str] = &[
    "checkpoints",
    "compat-imports",
    "conflicts",
    "executions",
    "export-map",
    "operations",
    "projections",
    "promotions",
    "quarantine",
    "records",
    "session-generations",
    "topics",
    "views",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedRecordPublication {
    relative_path: String,
    canonical_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealRepoState {
    pub publication_sequence: u64,
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
    pub session_generations: Vec<RealSessionGenerationRecord>,
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
    pub visibility: String,
    pub acceptance_criteria: Vec<String>,
    pub base_checkpoint_id: String,
    pub head_revision_id: Option<String>,
    pub revision_number: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicMetadataValidationError {
    InvalidOwner,
    UnsupportedVisibility,
    InvalidAcceptanceCriteria,
}

impl Display for TopicMetadataValidationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOwner => write!(
                f,
                "topic owner must be a non-empty actor identifier of at most 128 characters"
            ),
            Self::UnsupportedVisibility => {
                write!(f, "topic visibility must be one of: local, private")
            }
            Self::InvalidAcceptanceCriteria => write!(
                f,
                "each acceptance criterion must be non-empty, at most 1024 characters, and at most 64 criteria may be supplied"
            ),
        }
    }
}

pub fn validate_topic_metadata(
    owner_actor_id: &str,
    visibility: &str,
    acceptance_criteria: &[String],
) -> Result<(), TopicMetadataValidationError> {
    if owner_actor_id.trim().is_empty()
        || owner_actor_id.len() > 128
        || owner_actor_id.chars().any(char::is_control)
    {
        return Err(TopicMetadataValidationError::InvalidOwner);
    }
    if !matches!(visibility, "local" | "private") {
        return Err(TopicMetadataValidationError::UnsupportedVisibility);
    }
    if acceptance_criteria.len() > 64
        || acceptance_criteria.iter().any(|criterion| {
            criterion.trim().is_empty()
                || criterion.len() > 1024
                || criterion.chars().any(char::is_control)
        })
    {
        return Err(TopicMetadataValidationError::InvalidAcceptanceCriteria);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealSessionRecord {
    pub session_id: String,
    pub actor_id: String,
    pub write_topic_id: String,
    pub resolved_view_id: String,
    pub session_generation_id: String,
    pub generation_number: u64,
    pub topic_frontier: BTreeMap<String, String>,
    pub refresh_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealSessionGenerationRecord {
    pub session_generation_id: String,
    pub session_id: String,
    pub write_topic_id: String,
    pub base_resolved_view_id: String,
    pub resolved_view_id: String,
    pub topic_frontier: BTreeMap<String, String>,
    pub generation_number: u64,
    pub refresh_policy: String,
    pub created_by: String,
}

pub fn native_session_generation_id(session_id: &str, generation_number: u64) -> String {
    let session_identity = session_id.strip_prefix("session_").unwrap_or(session_id);
    format!("gen_{session_identity}_{generation_number:04}")
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
    pub cache_key: String,
    pub strategy: String,
    pub materialization: Option<RealProjectionMaterializationMetrics>,
    pub retention_state: String,
    pub privacy_class: String,
    pub last_import_operation_id: Option<String>,
    pub entries: Vec<RealArtifactEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealProjectionMaterializationMetrics {
    pub elapsed_ms: u64,
    pub logical_bytes: u64,
    pub physically_materialized_bytes: Option<u64>,
    pub physical_allocation_bytes: Option<u64>,
    pub file_count: u64,
    pub cache_hit: bool,
    pub reuse: String,
    pub integrity_revalidated: bool,
    pub storage_amplification_millionths: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealProjectionStrategy {
    Copy,
    Reflink,
    HardlinkReadonly,
    OverlayCopyup,
}

impl RealProjectionStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Reflink => "reflink",
            Self::HardlinkReadonly => "hardlink_readonly",
            Self::OverlayCopyup => "overlay_copyup",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealProjectionMaterializationRequest {
    pub purpose: ProjectionPurpose,
    pub writable_policy: WritablePolicy,
    pub path_policy_id: String,
    pub operation_semantics_version: String,
    pub required_strategy: Option<RealProjectionStrategy>,
    pub fallback_to_copy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealProjectionMaterialization {
    pub cache_key: String,
    pub strategy: RealProjectionStrategy,
    pub metrics: RealProjectionMaterializationMetrics,
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
    pub command_started: bool,
    pub timed_out: bool,
    pub termination_reason: Option<String>,
    pub termination_failed: bool,
    pub wait_failed: bool,
    pub stdout_observed_digest: String,
    pub stdout_byte_length: u64,
    pub stdout_captured_byte_length: u64,
    pub stdout_truncated: bool,
    pub stdout_capture_failed: bool,
    pub stderr_observed_digest: String,
    pub stderr_byte_length: u64,
    pub stderr_captured_byte_length: u64,
    pub stderr_truncated: bool,
    pub stderr_capture_failed: bool,
    pub timeout_ms: Option<u64>,
    pub process_memory_limit_bytes: Option<u64>,
    pub job_memory_limit_bytes: Option<u64>,
    pub cpu_time_limit_ms: Option<u64>,
    pub active_process_limit: Option<u32>,
    pub process_tree_policy: String,
    pub cpu_policy: String,
    pub memory_policy: String,
    pub environment_policy: String,
    pub environment_allowlist: Vec<String>,
    pub network_policy_requested: String,
    pub network_policy: String,
    pub filesystem_write_policy_requested: String,
    pub filesystem_write_policy: String,
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
    pub evidence_refs: Vec<EvidenceRef>,
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
    NotInitialized {
        path: PathBuf,
    },
    InvalidState {
        path: PathBuf,
        message: String,
    },
    Io {
        path: PathBuf,
        message: String,
    },
    Json(String),
    Recovery {
        canonical: PathBuf,
        staged: PathBuf,
        backup: PathBuf,
        journal: PathBuf,
        message: String,
    },
    PublicationRecovery {
        manifest: PathBuf,
        message: String,
    },
    WriterBusy {
        lock: PathBuf,
        timeout_ms: u64,
    },
    ConcurrentStateUpdate {
        path: PathBuf,
        expected_sequence: u64,
        actual_sequence: Option<u64>,
    },
    ProjectionStrategyUnsupported {
        strategy: String,
        path: PathBuf,
        reason: String,
    },
    ProjectionCacheIntegrity {
        cache_key: String,
        path: PathBuf,
        reason: String,
    },
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
            Self::Recovery {
                canonical,
                staged,
                backup,
                journal,
                message,
            } => write!(
                f,
                "Sunlight state recovery failed: {message}; canonical={}, staged={}, backup={}, journal={}",
                canonical.display(),
                staged.display(),
                backup.display(),
                journal.display()
            ),
            Self::PublicationRecovery { manifest, message } => write!(
                f,
                "Sunlight publication outbox recovery failed: {message}; manifest={}",
                manifest.display()
            ),
            Self::WriterBusy { lock, timeout_ms } => write!(
                f,
                "Sunlight repository writer is busy after {timeout_ms}ms; lock={}",
                lock.display()
            ),
            Self::ConcurrentStateUpdate {
                path,
                expected_sequence,
                actual_sequence,
            } => write!(
                f,
                "concurrent Sunlight state update: expected publication sequence {expected_sequence}, actual {}; state={}",
                actual_sequence
                    .map(|sequence| sequence.to_string())
                    .unwrap_or_else(|| "missing".to_string()),
                path.display()
            ),
            Self::ProjectionStrategyUnsupported {
                strategy,
                path,
                reason,
            } => write!(
                f,
                "projection strategy `{strategy}` is unsupported at {}: {reason}",
                path.display()
            ),
            Self::ProjectionCacheIntegrity {
                cache_key,
                path,
                reason,
            } => write!(
                f,
                "projection cache integrity failure for `{cache_key}` at {}: {reason}",
                path.display()
            ),
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
            publication_sequence: 0,
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
            session_generations: Vec::new(),
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
        let _writer_lock = RepositoryWriterLock::acquire(repo_root)?;
        recover_state_publication(repo_root)?;
        recover_publication_outbox(repo_root)?;
        let path = real_state_path(repo_root);
        let state = Self::load_from_path(repo_root, &path)?;
        state.reconcile_session_generation_records(repo_root)?;
        Ok(state)
    }

    fn load_from_path(repo_root: &Path, path: &Path) -> Result<Self, RepoStateError> {
        let body = fs::read(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                RepoStateError::NotInitialized {
                    path: path.to_path_buf(),
                }
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
        let mut session_generations = optional_array(&object, "session_generations", &path)?
            .iter()
            .map(|generation| parse_session_generation(generation, &path))
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
                let legacy_topic = RealTopicRecord {
                    topic_id: legacy_topic_id,
                    slug: topic_slug.clone().unwrap_or_default(),
                    display_name: topic_display_name.clone().unwrap_or_default(),
                    owner_actor_id: actor_id.clone().unwrap_or_else(|| "local".to_string()),
                    visibility: "local".to_string(),
                    acceptance_criteria: Vec::new(),
                    base_checkpoint_id: required_string(&object, "base_checkpoint_id", &path)?,
                    head_revision_id: head_revision_id.clone(),
                    revision_number,
                };
                validate_topic_metadata(
                    &legacy_topic.owner_actor_id,
                    &legacy_topic.visibility,
                    &legacy_topic.acceptance_criteria,
                )
                .map_err(|error| {
                    invalid_state(&path, format!("invalid topic metadata: {error}"))
                })?;
                topics.push(legacy_topic);
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
                    session_generation_id: native_session_generation_id(
                        "session_native",
                        generation_number.max(1),
                    ),
                    generation_number,
                    topic_frontier: BTreeMap::new(),
                    refresh_policy: "none".to_string(),
                });
            }
        }

        // Native state written before session frontiers were durable resolved a session from
        // its write topic head. Snapshot that effective view once while loading so the next save
        // pins the same context instead of changing legacy behavior.
        for session in &mut sessions {
            if session.topic_frontier.is_empty() {
                if let Some(revision_id) = topics
                    .iter()
                    .find(|topic| topic.topic_id == session.write_topic_id)
                    .and_then(|topic| topic.head_revision_id.clone())
                {
                    session
                        .topic_frontier
                        .insert(session.write_topic_id.clone(), revision_id);
                }
            }
            if session.refresh_policy == "pinned_except_own_topic" {
                session.refresh_policy = "none".to_string();
            }
            let generation_belongs_to_session = session_generations.iter().any(|generation| {
                generation.session_generation_id == session.session_generation_id
                    && generation.session_id == session.session_id
            });
            if !generation_belongs_to_session {
                let generation_id_is_owned_by_another_session =
                    session_generations.iter().any(|generation| {
                        generation.session_generation_id == session.session_generation_id
                            && generation.session_id != session.session_id
                    });
                if generation_id_is_owned_by_another_session {
                    session.session_generation_id = native_session_generation_id(
                        &session.session_id,
                        session.generation_number,
                    );
                }
                session_generations.push(RealSessionGenerationRecord {
                    session_generation_id: session.session_generation_id.clone(),
                    session_id: session.session_id.clone(),
                    write_topic_id: session.write_topic_id.clone(),
                    base_resolved_view_id: required_string(
                        &object,
                        "base_resolved_view_id",
                        &path,
                    )?,
                    resolved_view_id: session.resolved_view_id.clone(),
                    topic_frontier: session.topic_frontier.clone(),
                    generation_number: session.generation_number,
                    refresh_policy: session.refresh_policy.clone(),
                    created_by: "legacy_state_migration".to_string(),
                });
            }
        }

        let mut state = Self {
            publication_sequence: optional_u64(&object, "publication_sequence", &path)?
                .unwrap_or(0),
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
            session_generations,
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
        self.save_with_records(repo_root, &[])
    }

    pub fn record_publication(
        &self,
        dir: &str,
        id: &str,
        json: &str,
    ) -> Result<DerivedRecordPublication, RepoStateError> {
        if !DERIVED_RECORD_NAMESPACES.contains(&dir) || !is_portable_record_id(id) {
            return Err(invalid_state(
                &real_state_path(Path::new(".")),
                format!("derived record path is outside an allowed .sunlight namespace: {dir}/{id}.json"),
            ));
        }
        let value =
            parse_json_record(json.as_bytes()).map_err(|error| RepoStateError::InvalidState {
                path: PathBuf::from(format!(".sunlight/{dir}/{id}.json")),
                message: format!("derived record is not valid JSON: {error}"),
            })?;
        Ok(DerivedRecordPublication {
            relative_path: format!(".sunlight/{dir}/{id}.json"),
            canonical_bytes: canonical_json_bytes(&value)?,
        })
    }

    pub fn save_with_records(
        &self,
        repo_root: &Path,
        records: &[DerivedRecordPublication],
    ) -> Result<(), RepoStateError> {
        self.persist_blobs(repo_root)?;
        let path = real_state_path(repo_root);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_error(parent, "failed to create state directory", error))?;
        }
        let _writer_lock = RepositoryWriterLock::acquire(repo_root)?;
        recover_state_publication(repo_root)?;
        recover_publication_outbox(repo_root)?;
        let actual_sequence = if path.exists() {
            Some(Self::load_from_path(repo_root, &path)?.publication_sequence)
        } else {
            None
        };
        let initialization = actual_sequence.is_none() && self.publication_sequence == 0;
        if !initialization && actual_sequence != Some(self.publication_sequence) {
            return Err(RepoStateError::ConcurrentStateUpdate {
                path,
                expected_sequence: self.publication_sequence,
                actual_sequence,
            });
        }
        let mut published = self.clone();
        published.publication_sequence = self
            .publication_sequence
            .checked_add(1)
            .ok_or_else(|| invalid_state(&path, "publication_sequence overflow"))?;
        let body = canonical_json_bytes(&published.to_json_value())?;
        publish_state_and_records(
            repo_root,
            &path,
            published.publication_sequence,
            &body,
            records,
        )
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
        let _writer_lock = RepositoryWriterLock::acquire(repo_root)?;
        let path = repo_root
            .join(".sunlight")
            .join(dir)
            .join(format!("{id}.json"));
        let value =
            parse_json_record(json.as_bytes()).map_err(|error| RepoStateError::InvalidState {
                path: path.clone(),
                message: format!("derived record is not valid JSON: {error}"),
            })?;
        let bytes = canonical_json_bytes(&value)?;
        durable_publish_json_bytes(repo_root, &path, &bytes, "derived_record_after_prepare")
    }

    fn reconcile_session_generation_records(&self, repo_root: &Path) -> Result<(), RepoStateError> {
        for generation in &self.session_generations {
            let path = repo_root
                .join(".sunlight")
                .join("session-generations")
                .join(format!("{}.json", generation.session_generation_id));
            if !path.exists() {
                let value = session_generation_json(generation);
                let bytes = canonical_json_bytes(&value)?;
                durable_publish_json_bytes(
                    repo_root,
                    &path,
                    &bytes,
                    "derived_record_after_prepare",
                )?;
            }
        }
        Ok(())
    }

    pub fn to_json_value(&self) -> JsonValue {
        let mut object = BTreeMap::new();
        object.insert(
            "schema_version".to_string(),
            JsonValue::Number(REPO_STATE_SCHEMA_VERSION.to_string()),
        );
        object.insert(
            "publication_sequence".to_string(),
            JsonValue::Number(self.publication_sequence.to_string()),
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
            "session_generations".to_string(),
            JsonValue::Array(
                self.session_generations
                    .iter()
                    .map(session_generation_json)
                    .collect(),
            ),
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
        let frontier = session
            .topic_frontier
            .iter()
            .map(|(topic_id, revision_id)| TopicRevisionSelection {
                topic_id: topic_id.clone(),
                revision_id: revision_id.clone(),
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
    if !result.records.is_empty() {
        result.tree_identity = None;
        result.tree_entries.clear();
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
    let body = quarantine_report_bytes(quarantine)?;
    durable_publish_json_bytes(repo_root, &path, &body, "derived_record_after_prepare")
}

pub fn quarantine_report_publication(
    quarantine: &[RealQuarantineEntry],
) -> Result<DerivedRecordPublication, RepoStateError> {
    Ok(DerivedRecordPublication {
        relative_path: ".sunlight/quarantine/ingest-report.json".to_string(),
        canonical_bytes: quarantine_report_bytes(quarantine)?,
    })
}

fn quarantine_report_bytes(quarantine: &[RealQuarantineEntry]) -> Result<Vec<u8>, RepoStateError> {
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
    canonical_json_bytes(&JsonValue::Object(object)).map_err(RepoStateError::from)
}

pub fn real_state_path(repo_root: &Path) -> PathBuf {
    repo_root
        .join(".sunlight")
        .join("records")
        .join("native-state.json")
}

#[cfg(debug_assertions)]
const STATE_PUBLICATION_FAILPOINT_ENV: &str = "SUNLIGHT_TEST_FAILPOINT";
const PUBLICATION_OUTBOX_SCHEMA_VERSION: u64 = 1;
const WRITER_LOCK_TIMEOUT_MS: u64 = 0;

struct RepositoryWriterLock {
    file: File,
    #[cfg(not(windows))]
    path: PathBuf,
}

impl RepositoryWriterLock {
    fn acquire(repo_root: &Path) -> Result<Self, RepoStateError> {
        let path = repo_root
            .join(".sunlight")
            .join("local")
            .join("command-transaction.lock");
        let parent = path.parent().expect("writer lock has a parent");
        fs::create_dir_all(parent)
            .map_err(|error| io_error(parent, "failed to create writer lock directory", error))?;
        acquire_repository_writer_lock(&path)
    }
}

#[cfg(windows)]
fn acquire_repository_writer_lock(path: &Path) -> Result<RepositoryWriterLock, RepoStateError> {
    use std::os::windows::fs::OpenOptionsExt;

    // A zero share mode gives the open file an OS-owned, process-scoped exclusive lease. It is
    // released automatically even if the process exits while publishing.
    match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .share_mode(0)
        .open(path)
    {
        Ok(file) => Ok(RepositoryWriterLock { file }),
        Err(error) if matches!(error.raw_os_error(), Some(32) | Some(33)) => {
            Err(RepoStateError::WriterBusy {
                lock: path.to_path_buf(),
                timeout_ms: WRITER_LOCK_TIMEOUT_MS,
            })
        }
        Err(error) => Err(io_error(path, "failed to acquire writer lock", error)),
    }
}

#[cfg(not(windows))]
fn acquire_repository_writer_lock(path: &Path) -> Result<RepositoryWriterLock, RepoStateError> {
    use std::os::fd::AsRawFd;

    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;
    unsafe extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|error| io_error(path, "failed to open writer lock", error))?;
    // SAFETY: `file` owns a valid descriptor for the duration of this call and the guard.
    if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } == 0 {
        Ok(RepositoryWriterLock {
            file,
            path: path.to_path_buf(),
        })
    } else {
        let error = std::io::Error::last_os_error();
        if matches!(error.kind(), std::io::ErrorKind::WouldBlock) {
            Err(RepoStateError::WriterBusy {
                lock: path.to_path_buf(),
                timeout_ms: WRITER_LOCK_TIMEOUT_MS,
            })
        } else {
            Err(io_error(path, "failed to acquire writer lock", error))
        }
    }
}

#[cfg(not(windows))]
impl Drop for RepositoryWriterLock {
    fn drop(&mut self) {
        use std::os::fd::AsRawFd;

        const LOCK_UN: i32 = 8;
        unsafe extern "C" {
            fn flock(fd: i32, operation: i32) -> i32;
        }
        // SAFETY: the descriptor remains owned by `self.file` until after this method returns.
        let _ = unsafe { flock(self.file.as_raw_fd(), LOCK_UN) };
        let _ = &self.path;
    }
}

#[cfg(windows)]
impl Drop for RepositoryWriterLock {
    fn drop(&mut self) {
        let _ = &self.file;
    }
}

#[derive(Debug)]
struct PublicationManifest {
    transaction_id: String,
    target_sequence: u64,
    target_digest: String,
    records: Vec<PublicationManifestRecord>,
}

#[derive(Debug)]
struct PublicationManifestRecord {
    final_relative_path: String,
    canonical_digest: String,
    staged_relative_path: String,
}

fn publication_outbox_root(repo_root: &Path) -> PathBuf {
    repo_root
        .join(".sunlight")
        .join("local")
        .join("publication-outbox")
}

fn publish_state_and_records(
    repo_root: &Path,
    canonical: &Path,
    sequence: u64,
    state_bytes: &[u8],
    records: &[DerivedRecordPublication],
) -> Result<(), RepoStateError> {
    let state_digest = sha256_digest(state_bytes);
    let transaction_id = format!(
        "publication-{sequence}-{}",
        state_digest
            .strip_prefix("sha256:")
            .unwrap_or(&state_digest)[..16]
            .to_string()
    );
    let outbox_root = publication_outbox_root(repo_root);
    let transaction_root = outbox_root.join(&transaction_id);
    let manifest_path = transaction_root.join("manifest.json");
    if transaction_root.exists() {
        return Err(publication_recovery_error(
            &manifest_path,
            "transaction directory already exists; evidence was retained",
        ));
    }
    fs::create_dir_all(transaction_root.join("staged")).map_err(|error| {
        io_error(
            &transaction_root,
            "failed to create publication outbox transaction",
            error,
        )
    })?;

    let mut manifest_records = Vec::with_capacity(records.len());
    let mut seen_paths = std::collections::BTreeSet::new();
    for (index, record) in records.iter().enumerate() {
        validate_derived_relative_path(repo_root, &record.relative_path, &manifest_path)?;
        if !seen_paths.insert(record.relative_path.clone()) {
            return Err(publication_recovery_error(
                &manifest_path,
                format!(
                    "derived record path is declared more than once: {}",
                    record.relative_path
                ),
            ));
        }
        let staged_relative_path = format!("staged/{index:04}.json");
        write_flushed_file(
            &transaction_root.join(Path::new(&staged_relative_path)),
            &record.canonical_bytes,
        )?;
        manifest_records.push(PublicationManifestRecord {
            final_relative_path: record.relative_path.clone(),
            canonical_digest: sha256_digest(&record.canonical_bytes),
            staged_relative_path,
        });
    }
    let manifest = PublicationManifest {
        transaction_id,
        target_sequence: sequence,
        target_digest: state_digest,
        records: manifest_records,
    };
    let preparing = transaction_root.join("manifest.preparing.json");
    write_flushed_file(&preparing, &publication_manifest_bytes(&manifest)?)?;
    atomic_replace_file(&preparing, &manifest_path, None)?;

    trigger_failpoint("batch_before_canonical_commit", canonical)?;
    publish_native_state(repo_root, canonical, sequence, state_bytes)?;
    trigger_failpoint("batch_after_canonical_commit", canonical)?;
    publish_committed_record_batch(repo_root, &transaction_root, &manifest, true)?;
    Ok(())
}

fn recover_publication_outbox(repo_root: &Path) -> Result<(), RepoStateError> {
    let root = publication_outbox_root(repo_root);
    let Ok(entries) = fs::read_dir(&root) else {
        return Ok(());
    };
    let mut transaction_roots = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            publication_recovery_error(&root, format!("failed to inspect outbox: {error}"))
        })?;
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            return Err(publication_recovery_error(
                &entry.path(),
                "unexpected non-directory outbox evidence was retained",
            ));
        }
        transaction_roots.push(entry.path());
    }
    transaction_roots.sort();
    for transaction_root in transaction_roots {
        let manifest_path = transaction_root.join("manifest.json");
        let manifest = load_and_validate_publication_manifest(repo_root, &transaction_root)?;
        let canonical_path = real_state_path(repo_root);
        let canonical_bytes = fs::read(&canonical_path).ok();
        let canonical_state = RealRepoState::load_from_path(repo_root, &canonical_path).ok();
        let committed = canonical_bytes.as_ref().is_some_and(|bytes| {
            sha256_digest(bytes) == manifest.target_digest
                && canonical_state
                    .as_ref()
                    .is_some_and(|state| state.publication_sequence == manifest.target_sequence)
        });
        if committed {
            publish_committed_record_batch(repo_root, &transaction_root, &manifest, false)?;
        } else if canonical_state
            .as_ref()
            .is_none_or(|state| state.publication_sequence < manifest.target_sequence)
        {
            remove_publication_transaction(&root, &transaction_root)?;
        } else {
            return Err(publication_recovery_error(
                &manifest_path,
                format!(
                    "canonical state does not match declared sequence {} and digest {}; evidence was retained",
                    manifest.target_sequence, manifest.target_digest
                ),
            ));
        }
    }
    remove_dir_if_empty(&root)?;
    if let Some(local) = root.parent() {
        remove_dir_if_empty(local)?;
    }
    Ok(())
}

fn publish_committed_record_batch(
    repo_root: &Path,
    transaction_root: &Path,
    manifest: &PublicationManifest,
    run_failpoints: bool,
) -> Result<(), RepoStateError> {
    validate_staged_publication_payloads(repo_root, transaction_root, manifest)?;
    for record in &manifest.records {
        let staged = transaction_root.join(Path::new(&record.staged_relative_path));
        let bytes = fs::read(&staged).map_err(|error| {
            publication_recovery_error(
                &transaction_root.join("manifest.json"),
                format!("failed to read declared staged payload: {error}"),
            )
        })?;
        let final_path = repo_root.join(Path::new(&record.final_relative_path));
        if fs::read(&final_path)
            .ok()
            .is_none_or(|existing| existing != bytes)
        {
            durable_publish_json_bytes(
                repo_root,
                &final_path,
                &bytes,
                "batch_record_after_prepare",
            )?;
        }
        if run_failpoints {
            trigger_failpoint("batch_mid_derived_publication", &final_path)?;
        }
    }
    let completed_path = transaction_root.join("completed.json");
    if !completed_path.exists() {
        let completed = JsonValue::Object(BTreeMap::from([
            (
                "record_type".to_string(),
                JsonValue::String("publication_outbox_completion".to_string()),
            ),
            (
                "schema_version".to_string(),
                JsonValue::Number(PUBLICATION_OUTBOX_SCHEMA_VERSION.to_string()),
            ),
            (
                "transaction_id".to_string(),
                JsonValue::String(manifest.transaction_id.clone()),
            ),
            (
                "target_canonical_sha256".to_string(),
                JsonValue::String(manifest.target_digest.clone()),
            ),
        ]));
        write_flushed_file(&completed_path, &canonical_json_bytes(&completed)?)?;
    }
    if run_failpoints {
        trigger_failpoint("batch_after_completion_marker", &completed_path)?;
    }
    let outbox_root = publication_outbox_root(repo_root);
    remove_publication_transaction(&outbox_root, transaction_root)?;
    remove_dir_if_empty(&outbox_root)?;
    if let Some(local) = outbox_root.parent() {
        remove_dir_if_empty(local)?;
    }
    Ok(())
}

fn validate_staged_publication_payloads(
    repo_root: &Path,
    transaction_root: &Path,
    manifest: &PublicationManifest,
) -> Result<(), RepoStateError> {
    let manifest_path = transaction_root.join("manifest.json");
    for record in &manifest.records {
        validate_derived_relative_path(repo_root, &record.final_relative_path, &manifest_path)?;
        let staged = transaction_root.join(Path::new(&record.staged_relative_path));
        let bytes = fs::read(&staged).map_err(|error| {
            publication_recovery_error(
                &manifest_path,
                format!(
                    "declared staged payload {} is missing or unreadable: {error}; evidence was retained",
                    record.staged_relative_path
                ),
            )
        })?;
        if sha256_digest(&bytes) != record.canonical_digest {
            return Err(publication_recovery_error(
                &manifest_path,
                format!(
                    "declared staged payload {} has a digest mismatch; evidence was retained",
                    record.staged_relative_path
                ),
            ));
        }
        let parsed = parse_json_record(&bytes).map_err(|error| {
            publication_recovery_error(
                &manifest_path,
                format!("declared staged payload is invalid JSON: {error}; evidence was retained"),
            )
        })?;
        if canonical_json_bytes(&parsed)? != bytes {
            return Err(publication_recovery_error(
                &manifest_path,
                "declared staged payload is not canonical JSON; evidence was retained",
            ));
        }
    }
    Ok(())
}

fn load_and_validate_publication_manifest(
    repo_root: &Path,
    transaction_root: &Path,
) -> Result<PublicationManifest, RepoStateError> {
    let published_manifest = transaction_root.join("manifest.json");
    let preparing_manifest = transaction_root.join("manifest.preparing.json");
    let manifest_path = if published_manifest.exists() {
        published_manifest
    } else if preparing_manifest.exists() {
        preparing_manifest
    } else {
        published_manifest
    };
    let bytes = fs::read(&manifest_path).map_err(|error| {
        publication_recovery_error(
            &manifest_path,
            format!("manifest is missing or unreadable: {error}; evidence was retained"),
        )
    })?;
    let value = parse_json_record(&bytes).map_err(|error| {
        publication_recovery_error(
            &manifest_path,
            format!("manifest is malformed: {error}; evidence was retained"),
        )
    })?;
    if canonical_json_bytes(&value)? != bytes {
        return Err(publication_recovery_error(
            &manifest_path,
            "manifest is not canonical JSON; evidence was retained",
        ));
    }
    let JsonValue::Object(object) = value else {
        return Err(publication_recovery_error(
            &manifest_path,
            "manifest root is not an object; evidence was retained",
        ));
    };
    let manifest_field_error = |error: RepoStateError| {
        publication_recovery_error(
            &manifest_path,
            format!("manifest field is invalid: {error}; evidence was retained"),
        )
    };
    let record_type =
        required_string(&object, "record_type", &manifest_path).map_err(&manifest_field_error)?;
    let schema_version =
        required_u64(&object, "schema_version", &manifest_path).map_err(&manifest_field_error)?;
    let transaction_id = required_string(&object, "transaction_id", &manifest_path)
        .map_err(&manifest_field_error)?;
    if record_type != "publication_outbox" || schema_version != PUBLICATION_OUTBOX_SCHEMA_VERSION {
        return Err(publication_recovery_error(
            &manifest_path,
            "manifest type or schema version is unsupported; evidence was retained",
        ));
    }
    if transaction_root.file_name().and_then(|name| name.to_str()) != Some(&transaction_id) {
        return Err(publication_recovery_error(
            &manifest_path,
            "manifest transaction ID does not match its directory; evidence was retained",
        ));
    }
    let target_sequence = required_u64(&object, "target_publication_sequence", &manifest_path)
        .map_err(&manifest_field_error)?;
    let target_digest = required_string(&object, "target_canonical_sha256", &manifest_path)
        .map_err(&manifest_field_error)?;
    let values =
        required_array(&object, "records", &manifest_path).map_err(&manifest_field_error)?;
    let mut records = Vec::with_capacity(values.len());
    let mut final_paths = std::collections::BTreeSet::new();
    for (index, value) in values.iter().enumerate() {
        let JsonValue::Object(record) = value else {
            return Err(publication_recovery_error(
                &manifest_path,
                "manifest record declaration is not an object; evidence was retained",
            ));
        };
        let final_relative_path = required_string(record, "final_relative_path", &manifest_path)
            .map_err(&manifest_field_error)?;
        let canonical_digest = required_string(record, "canonical_sha256", &manifest_path)
            .map_err(&manifest_field_error)?;
        let staged_relative_path = required_string(record, "staged_payload", &manifest_path)
            .map_err(&manifest_field_error)?;
        if staged_relative_path != format!("staged/{index:04}.json") {
            return Err(publication_recovery_error(
                &manifest_path,
                "manifest staged payload reference is not confined to its transaction; evidence was retained",
            ));
        }
        validate_derived_relative_path(repo_root, &final_relative_path, &manifest_path)?;
        if !final_paths.insert(final_relative_path.clone()) {
            return Err(publication_recovery_error(
                &manifest_path,
                "manifest declares a duplicate final path; evidence was retained",
            ));
        }
        records.push(PublicationManifestRecord {
            final_relative_path,
            canonical_digest,
            staged_relative_path,
        });
    }
    let manifest = PublicationManifest {
        transaction_id,
        target_sequence,
        target_digest,
        records,
    };
    validate_staged_publication_payloads(repo_root, transaction_root, &manifest)?;
    Ok(manifest)
}

fn publication_manifest_bytes(manifest: &PublicationManifest) -> Result<Vec<u8>, RepoStateError> {
    let records = manifest
        .records
        .iter()
        .map(|record| {
            JsonValue::Object(BTreeMap::from([
                (
                    "canonical_sha256".to_string(),
                    JsonValue::String(record.canonical_digest.clone()),
                ),
                (
                    "final_relative_path".to_string(),
                    JsonValue::String(record.final_relative_path.clone()),
                ),
                (
                    "staged_payload".to_string(),
                    JsonValue::String(record.staged_relative_path.clone()),
                ),
            ]))
        })
        .collect();
    canonical_json_bytes(&JsonValue::Object(BTreeMap::from([
        (
            "record_type".to_string(),
            JsonValue::String("publication_outbox".to_string()),
        ),
        (
            "schema_version".to_string(),
            JsonValue::Number(PUBLICATION_OUTBOX_SCHEMA_VERSION.to_string()),
        ),
        (
            "transaction_id".to_string(),
            JsonValue::String(manifest.transaction_id.clone()),
        ),
        (
            "target_canonical_sha256".to_string(),
            JsonValue::String(manifest.target_digest.clone()),
        ),
        (
            "target_publication_sequence".to_string(),
            JsonValue::Number(manifest.target_sequence.to_string()),
        ),
        ("records".to_string(), JsonValue::Array(records)),
    ])))
    .map_err(RepoStateError::from)
}

fn validate_derived_relative_path(
    repo_root: &Path,
    relative: &str,
    evidence_path: &Path,
) -> Result<(), RepoStateError> {
    let parts = relative.split('/').collect::<Vec<_>>();
    let valid = parts.len() == 3
        && parts[0] == ".sunlight"
        && DERIVED_RECORD_NAMESPACES.contains(&parts[1])
        && parts[2]
            .strip_suffix(".json")
            .is_some_and(is_portable_record_id)
        && relative
            == format!(
                ".sunlight/{}/{}.json",
                parts.get(1).unwrap_or(&""),
                parts
                    .get(2)
                    .and_then(|name| name.strip_suffix(".json"))
                    .unwrap_or("")
            );
    if !valid {
        return Err(publication_recovery_error(
            evidence_path,
            format!("derived final path does not use the portable .sunlight record filename grammar: {relative}; evidence was retained"),
        ));
    }
    for path in [
        repo_root.join(".sunlight"),
        repo_root.join(".sunlight").join(parts[1]),
        repo_root.join(Path::new(relative)),
    ] {
        if fs::symlink_metadata(&path)
            .ok()
            .is_some_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(publication_recovery_error(
                evidence_path,
                format!(
                    "derived final path traverses a symlink: {}; evidence was retained",
                    path.display()
                ),
            ));
        }
    }
    Ok(())
}

fn is_portable_record_id(id: &str) -> bool {
    let mut bytes = id.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !first.is_ascii_alphanumeric()
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        || id.eq_ignore_ascii_case("native-state")
    {
        return false;
    }
    let upper = id.to_ascii_uppercase();
    !matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        && !(upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(upper.as_bytes()[3], b'1'..=b'9'))
}

fn remove_publication_transaction(
    outbox_root: &Path,
    transaction_root: &Path,
) -> Result<(), RepoStateError> {
    if transaction_root.parent() != Some(outbox_root) {
        return Err(publication_recovery_error(
            transaction_root,
            "refused to clean a transaction outside the publication outbox",
        ));
    }
    fs::remove_dir_all(transaction_root).map_err(|error| {
        io_error(
            transaction_root,
            "failed to clean completed publication transaction",
            error,
        )
    })
}

fn publication_recovery_error(path: &Path, message: impl Into<String>) -> RepoStateError {
    RepoStateError::PublicationRecovery {
        manifest: path.to_path_buf(),
        message: message.into(),
    }
}

fn sha256_digest(bytes: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(bytes))
}

struct StateRecoveryPaths {
    root: PathBuf,
    staged: PathBuf,
    backup: PathBuf,
    journal: PathBuf,
}

fn state_recovery_paths(repo_root: &Path) -> StateRecoveryPaths {
    let root = repo_root
        .join(".sunlight")
        .join("local")
        .join("recovery")
        .join("native-state");
    StateRecoveryPaths {
        staged: root.join("staged.json"),
        backup: root.join("backup.json"),
        journal: root.join("journal.json"),
        root,
    }
}

fn publish_native_state(
    repo_root: &Path,
    canonical: &Path,
    sequence: u64,
    bytes: &[u8],
) -> Result<(), RepoStateError> {
    let recovery = state_recovery_paths(repo_root);
    fs::create_dir_all(&recovery.root).map_err(|error| {
        io_error(
            &recovery.root,
            "failed to create native-state recovery directory",
            error,
        )
    })?;
    remove_file_if_exists(&recovery.staged)?;
    remove_file_if_exists(&recovery.backup)?;
    remove_file_if_exists(&recovery.journal)?;

    write_flushed_file(&recovery.staged, bytes)?;
    let digest = format!("sha256:{:x}", Sha256::digest(bytes));
    let journal_value = JsonValue::Object(BTreeMap::from([
        (
            "record_type".to_string(),
            JsonValue::String("native_state_publication".to_string()),
        ),
        (
            "schema_version".to_string(),
            JsonValue::Number("1".to_string()),
        ),
        (
            "publication_sequence".to_string(),
            JsonValue::Number(sequence.to_string()),
        ),
        ("intended_sha256".to_string(), JsonValue::String(digest)),
    ]));
    let journal_bytes = canonical_json_bytes(&journal_value)?;
    let journal_stage = recovery.root.join("journal.preparing.json");
    remove_file_if_exists(&journal_stage)?;
    write_flushed_file(&journal_stage, &journal_bytes)?;
    atomic_replace_file(&journal_stage, &recovery.journal, None)?;

    trigger_failpoint("state_after_prepare", canonical)?;
    atomic_replace_file(&recovery.staged, canonical, Some(recovery.backup.as_path()))?;
    trigger_failpoint("state_after_replace", canonical)?;

    RealRepoState::load_from_path(repo_root, canonical)?;
    cleanup_state_recovery(&recovery)?;
    cleanup_recovery_evidence(&recovery)?;
    Ok(())
}

fn recover_state_publication(repo_root: &Path) -> Result<(), RepoStateError> {
    let canonical = real_state_path(repo_root);
    let recovery = state_recovery_paths(repo_root);
    if !recovery.journal.exists() {
        if canonical.exists() {
            if RealRepoState::load_from_path(repo_root, &canonical).is_ok() {
                cleanup_state_recovery(&recovery)?;
                return Ok(());
            }
            return Err(recovery_error(
                &canonical,
                &recovery,
                "canonical state is malformed and no valid publication journal exists; evidence was retained"
                    .to_string(),
            ));
        }
        if recovery.staged.exists() || recovery.backup.exists() {
            return Err(recovery_error(
                &canonical,
                &recovery,
                "canonical state is missing and unjournaled recovery candidates exist; evidence was retained"
                    .to_string(),
            ));
        }
        return Ok(());
    }

    let journal_bytes = fs::read(&recovery.journal).map_err(|error| {
        recovery_error(
            &canonical,
            &recovery,
            format!("failed to read journal: {error}"),
        )
    })?;
    let journal = parse_json_record(&journal_bytes).map_err(|error| {
        recovery_error(
            &canonical,
            &recovery,
            format!("journal is malformed and evidence was retained: {error}"),
        )
    })?;
    let JsonValue::Object(journal) = journal else {
        return Err(recovery_error(
            &canonical,
            &recovery,
            "journal root is not an object; evidence was retained".to_string(),
        ));
    };
    let sequence =
        required_u64(&journal, "publication_sequence", &recovery.journal).map_err(|error| {
            recovery_error(
                &canonical,
                &recovery,
                format!("journal sequence is invalid and evidence was retained: {error}"),
            )
        })?;
    let digest =
        required_string(&journal, "intended_sha256", &recovery.journal).map_err(|error| {
            recovery_error(
                &canonical,
                &recovery,
                format!("journal digest is invalid and evidence was retained: {error}"),
            )
        })?;

    let intended = |path: &Path| -> bool {
        let Ok(bytes) = fs::read(path) else {
            return false;
        };
        if format!("sha256:{:x}", Sha256::digest(&bytes)) != digest {
            return false;
        }
        RealRepoState::load_from_path(repo_root, path)
            .is_ok_and(|state| state.publication_sequence == sequence)
    };

    if intended(&canonical) {
        cleanup_state_recovery(&recovery)?;
        return Ok(());
    }
    if intended(&recovery.staged) {
        remove_file_if_exists(&recovery.backup)?;
        atomic_replace_file(
            &recovery.staged,
            &canonical,
            Some(recovery.backup.as_path()),
        )?;
        if !intended(&canonical) {
            return Err(recovery_error(
                &canonical,
                &recovery,
                "recovered canonical state failed post-publication validation; evidence was retained"
                    .to_string(),
            ));
        }
        cleanup_state_recovery(&recovery)?;
        return Ok(());
    }

    let canonical_state = RealRepoState::load_from_path(repo_root, &canonical).ok();
    let backup_state = RealRepoState::load_from_path(repo_root, &recovery.backup).ok();
    let fallback = match (&canonical_state, &backup_state) {
        (Some(canonical_state), Some(backup_state)) => {
            if backup_state.publication_sequence > canonical_state.publication_sequence {
                Some((recovery.backup.as_path(), backup_state.publication_sequence))
            } else {
                Some((canonical.as_path(), canonical_state.publication_sequence))
            }
        }
        (Some(state), None) => Some((canonical.as_path(), state.publication_sequence)),
        (None, Some(state)) => Some((recovery.backup.as_path(), state.publication_sequence)),
        (None, None) => None,
    };
    if let Some((fallback_path, fallback_sequence)) = fallback {
        let evidence = recovery.root.join(format!("evidence-{sequence}"));
        fs::create_dir_all(&evidence).map_err(|error| {
            io_error(
                &evidence,
                "failed to preserve interrupted publication evidence",
                error,
            )
        })?;
        if fallback_path == recovery.backup {
            atomic_replace_file(
                &recovery.backup,
                &canonical,
                Some(&evidence.join("rejected-canonical.json")),
            )?;
        }
        preserve_recovery_evidence(&recovery.staged, &evidence.join("rejected-staged.json"))?;
        preserve_recovery_evidence(&recovery.journal, &evidence.join("journal.json"))?;
        remove_file_if_exists(&recovery.backup)?;
        RealRepoState::load_from_path(repo_root, &canonical)?;
        let notice = JsonValue::Object(BTreeMap::from([
            (
                "record_type".to_string(),
                JsonValue::String("native_state_recovery_rollback".to_string()),
            ),
            (
                "intended_publication_sequence".to_string(),
                JsonValue::Number(sequence.to_string()),
            ),
            (
                "recovered_publication_sequence".to_string(),
                JsonValue::Number(fallback_sequence.to_string()),
            ),
        ]));
        write_flushed_file(
            &evidence.join("recovery.json"),
            &canonical_json_bytes(&notice)?,
        )?;
        return Ok(());
    }

    Err(recovery_error(
        &canonical,
        &recovery,
        format!(
            "no fully valid candidate matches intended publication sequence {sequence} and digest {digest}; evidence was retained"
        ),
    ))
}

fn preserve_recovery_evidence(source: &Path, destination: &Path) -> Result<(), RepoStateError> {
    if !source.exists() {
        return Ok(());
    }
    fs::rename(source, destination)
        .map_err(|error| io_error(destination, "failed to preserve recovery evidence", error))
}

fn recovery_error(
    canonical: &Path,
    recovery: &StateRecoveryPaths,
    message: String,
) -> RepoStateError {
    RepoStateError::Recovery {
        canonical: canonical.to_path_buf(),
        staged: recovery.staged.clone(),
        backup: recovery.backup.clone(),
        journal: recovery.journal.clone(),
        message,
    }
}

fn cleanup_state_recovery(recovery: &StateRecoveryPaths) -> Result<(), RepoStateError> {
    remove_file_if_exists(&recovery.staged)?;
    remove_file_if_exists(&recovery.backup)?;
    remove_file_if_exists(&recovery.root.join("journal.preparing.json"))?;
    remove_file_if_exists(&recovery.journal)
}

fn cleanup_recovery_evidence(recovery: &StateRecoveryPaths) -> Result<(), RepoStateError> {
    let Ok(entries) = fs::read_dir(&recovery.root) else {
        return Ok(());
    };
    for entry in entries {
        let entry = entry.map_err(|error| {
            io_error(&recovery.root, "failed to inspect recovery evidence", error)
        })?;
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false)
            && entry.file_name().to_string_lossy().starts_with("evidence-")
        {
            fs::remove_dir_all(entry.path()).map_err(|error| {
                io_error(
                    &entry.path(),
                    "failed to clean superseded recovery evidence",
                    error,
                )
            })?;
        }
    }
    Ok(())
}

pub fn durable_publish_json_bytes(
    repo_root: &Path,
    final_path: &Path,
    bytes: &[u8],
    failpoint: &str,
) -> Result<(), RepoStateError> {
    let value = parse_json_record(bytes).map_err(|error| RepoStateError::InvalidState {
        path: final_path.to_path_buf(),
        message: format!("durable record is not valid JSON: {error}"),
    })?;
    let canonical = canonical_json_bytes(&value)?;
    let mut hasher = Sha256::new();
    hasher.update(final_path.to_string_lossy().as_bytes());
    let name = format!("{:x}", hasher.finalize());
    let root = repo_root
        .join(".sunlight")
        .join("local")
        .join("record-publication");
    fs::create_dir_all(&root)
        .map_err(|error| io_error(&root, "failed to create record staging directory", error))?;
    let staged = root.join(format!("{name}.staged"));
    let backup = root.join(format!("{name}.backup"));
    remove_file_if_exists(&staged)?;
    remove_file_if_exists(&backup)?;
    write_flushed_file(&staged, &canonical)?;
    trigger_failpoint(failpoint, final_path)?;
    atomic_replace_file(&staged, final_path, Some(&backup))?;
    remove_file_if_exists(&backup)?;
    remove_dir_if_empty(&root)?;
    if let Some(local_root) = root.parent() {
        remove_dir_if_empty(local_root)?;
    }
    Ok(())
}

fn write_flushed_file(path: &Path, bytes: &[u8]) -> Result<(), RepoStateError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            io_error(parent, "failed to create durable staging directory", error)
        })?;
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map_err(|error| io_error(path, "failed to create durable staged file", error))?;
    file.write_all(bytes)
        .map_err(|error| io_error(path, "failed to write durable staged file", error))?;
    file.sync_all()
        .map_err(|error| io_error(path, "failed to flush durable staged file", error))
}

fn remove_file_if_exists(path: &Path) -> Result<(), RepoStateError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(
            path,
            "failed to clean publication artifact",
            error,
        )),
    }
}

fn remove_dir_if_empty(path: &Path) -> Result<(), RepoStateError> {
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(io_error(
            path,
            "failed to clean empty publication directory",
            error,
        )),
    }
}

fn trigger_failpoint(_name: &str, _path: &Path) -> Result<(), RepoStateError> {
    #[cfg(debug_assertions)]
    {
        let name = _name;
        let path = _path;
        let normalized_path = normalize_failpoint_target(path);
        let scoped_name = format!("{name}|{}", normalized_path.display());
        if std::env::var(STATE_PUBLICATION_FAILPOINT_ENV).as_deref() == Ok(scoped_name.as_str()) {
            return Err(RepoStateError::Io {
                path: path.to_path_buf(),
                message: format!("deterministic test failpoint `{name}`"),
            });
        }
    }
    Ok(())
}

#[cfg(debug_assertions)]
fn normalize_failpoint_target(path: &Path) -> PathBuf {
    let mut ancestor = path;
    let mut suffix = Vec::new();
    loop {
        if let Ok(mut normalized) = fs::canonicalize(ancestor) {
            for component in suffix.iter().rev() {
                normalized.push(component);
            }
            return normalized;
        }
        let Some(name) = ancestor.file_name() else {
            return path.to_path_buf();
        };
        suffix.push(name.to_os_string());
        let Some(parent) = ancestor.parent() else {
            return path.to_path_buf();
        };
        ancestor = parent;
    }
}

#[cfg(windows)]
fn atomic_replace_file(
    staged: &Path,
    final_path: &Path,
    backup: Option<&Path>,
) -> Result<(), RepoStateError> {
    use std::os::windows::ffi::OsStrExt;

    extern "system" {
        fn ReplaceFileW(
            replaced_file_name: *const u16,
            replacement_file_name: *const u16,
            backup_file_name: *const u16,
            replace_flags: u32,
            exclude: *mut std::ffi::c_void,
            reserved: *mut std::ffi::c_void,
        ) -> i32;
        fn MoveFileExW(
            existing_file_name: *const u16,
            new_file_name: *const u16,
            flags: u32,
        ) -> i32;
    }
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| io_error(parent, "failed to create publication directory", error))?;
    }
    let windows_path = |path: &Path| -> Result<PathBuf, RepoStateError> {
        if path.exists() {
            return fs::canonicalize(path).map_err(|error| {
                io_error(path, "failed to normalize Windows publication path", error)
            });
        }
        let parent = path.parent().ok_or_else(|| RepoStateError::Io {
            path: path.to_path_buf(),
            message: "Windows publication path has no parent".to_string(),
        })?;
        let parent = fs::canonicalize(parent).map_err(|error| {
            io_error(
                parent,
                "failed to normalize Windows publication parent",
                error,
            )
        })?;
        Ok(
            parent.join(path.file_name().ok_or_else(|| RepoStateError::Io {
                path: path.to_path_buf(),
                message: "Windows publication path has no file name".to_string(),
            })?),
        )
    };
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>()
    };
    let staged_api_path = windows_path(staged)?;
    let final_api_path = windows_path(final_path)?;
    let backup_api_path = backup.map(windows_path).transpose()?;
    let staged_wide = wide(&staged_api_path);
    let final_wide = wide(&final_api_path);
    let result = if final_path.exists() {
        let backup_wide = backup_api_path.as_deref().map(wide);
        unsafe {
            ReplaceFileW(
                final_wide.as_ptr(),
                staged_wide.as_ptr(),
                backup_wide
                    .as_ref()
                    .map_or(std::ptr::null(), |path| path.as_ptr()),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        }
    } else {
        unsafe {
            MoveFileExW(
                staged_wide.as_ptr(),
                final_wide.as_ptr(),
                MOVEFILE_WRITE_THROUGH,
            )
        }
    };
    if result == 0 {
        return Err(io_error(
            final_path,
            "failed to atomically publish durable file",
            std::io::Error::last_os_error(),
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace_file(
    staged: &Path,
    final_path: &Path,
    backup: Option<&Path>,
) -> Result<(), RepoStateError> {
    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| io_error(parent, "failed to create publication directory", error))?;
    }
    if final_path.exists() {
        if let Some(backup) = backup {
            fs::hard_link(final_path, backup).map_err(|error| {
                io_error(backup, "failed to preserve publication backup", error)
            })?;
        }
    }
    fs::rename(staged, final_path).map_err(|error| {
        io_error(
            final_path,
            "failed to atomically publish durable file",
            error,
        )
    })
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
    if real_content_hash(bytes) != content_hash {
        return Err(RepoStateError::InvalidState {
            path,
            message: "content blob bytes do not match their content-addressed path".to_string(),
        });
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| io_error(parent, "failed to create blob directory", error))?;
    }
    if path.exists() {
        let existing = fs::read(&path)
            .map_err(|error| io_error(&path, "failed to validate existing content blob", error))?;
        if existing != bytes {
            return Err(RepoStateError::InvalidState {
                path,
                message: "existing content blob is malformed; evidence was retained".to_string(),
            });
        }
        return Ok(());
    }

    let root = repo_root
        .join(".sunlight")
        .join("local")
        .join("blob-publication");
    fs::create_dir_all(&root)
        .map_err(|error| io_error(&root, "failed to create blob staging directory", error))?;
    let staged = root.join(format!(
        "{}.staged",
        content_hash.trim_start_matches("sha256:")
    ));
    remove_file_if_exists(&staged)?;
    write_flushed_file(&staged, bytes)?;
    atomic_replace_file(&staged, &path, None)?;
    remove_dir_if_empty(&root)?;
    if let Some(local_root) = root.parent() {
        remove_dir_if_empty(local_root)?;
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
    materialize_real_projection(
        Path::new("."),
        state,
        root,
        &RealProjectionMaterializationRequest {
            purpose: ProjectionPurpose::Export,
            writable_policy: WritablePolicy::ExportMaterializationOnly,
            path_policy_id: POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string(),
            operation_semantics_version: FILE_OPERATION_SEMANTICS_VERSION.to_string(),
            required_strategy: Some(RealProjectionStrategy::Copy),
            fallback_to_copy: false,
        },
    )?;
    Ok(())
}

pub fn materialize_real_projection(
    repo_root: &Path,
    state: &RealRepoState,
    root: &Path,
    request: &RealProjectionMaterializationRequest,
) -> Result<RealProjectionMaterialization, RepoStateError> {
    validate_real_projection_root(state, root, &request.path_policy_id)?;
    if state.repository_id.is_empty() || real_tree_hash(&state.entries) != state.tree_hash {
        return Err(RepoStateError::InvalidState {
            path: root.to_path_buf(),
            message: "resolved projection source does not match its repository/tree identity"
                .to_string(),
        });
    }
    let started = Instant::now();
    let cache_key = ProjectionCacheKey {
        repository_id: state.repository_id.clone(),
        resolved_view_id: state.resolved_view_id.clone(),
        tree_hash: state.tree_hash.clone(),
        path_policy_id: request.path_policy_id.clone(),
        operation_semantics_version: request.operation_semantics_version.clone(),
        purpose: request.purpose,
        strategy: ProjectionStrategy::Copy,
        writable_policy: request.writable_policy,
    }
    .stable_string();
    let cache = ensure_real_projection_cache_entry(repo_root, state, request, &cache_key)?;
    let preferred = request
        .required_strategy
        .unwrap_or(RealProjectionStrategy::Reflink);
    let strategies = if preferred == RealProjectionStrategy::Copy {
        vec![RealProjectionStrategy::Copy]
    } else if request.fallback_to_copy {
        vec![preferred, RealProjectionStrategy::Copy]
    } else {
        vec![preferred]
    };
    let mut unsupported_reason = None;
    for strategy in strategies {
        let staging = projection_staging_path(root);
        cleanup_projection_staging(&staging);
        fs::create_dir_all(&staging).map_err(|error| {
            io_error(&staging, "failed to create projection staging root", error)
        })?;
        let result =
            materialize_real_projection_strategy(&cache.content_root, state, &staging, strategy);
        match result {
            Ok(attempt) => {
                if let Err(error) = publish_projection_staging(root, &staging) {
                    cleanup_projection_staging(&staging);
                    return Err(error);
                }
                let logical_bytes = state
                    .entries
                    .iter()
                    .filter(|entry| !entry.tombstone)
                    .map(|entry| entry.bytes.len() as u64)
                    .sum::<u64>();
                let file_count = state
                    .entries
                    .iter()
                    .filter(|entry| !entry.tombstone)
                    .count() as u64;
                let physically_materialized_bytes = match strategy {
                    RealProjectionStrategy::Copy | RealProjectionStrategy::Reflink => Some(
                        cache
                            .physically_materialized_bytes
                            .saturating_add(attempt.bytes_copied),
                    ),
                    _ => None,
                };
                return Ok(RealProjectionMaterialization {
                    cache_key,
                    strategy,
                    metrics: RealProjectionMaterializationMetrics {
                        elapsed_ms: started.elapsed().as_millis().min(u64::MAX as u128) as u64,
                        logical_bytes,
                        physically_materialized_bytes,
                        physical_allocation_bytes: None,
                        file_count,
                        cache_hit: cache.cache_hit,
                        reuse: cache.reuse,
                        integrity_revalidated: true,
                        storage_amplification_millionths: if logical_bytes == 0 {
                            None
                        } else {
                            physically_materialized_bytes
                                .map(|bytes| bytes.saturating_mul(1_000_000) / logical_bytes)
                        },
                    },
                });
            }
            Err(StrategyAttemptError::Unsupported(reason)) => {
                cleanup_projection_staging(&staging);
                unsupported_reason = Some(reason);
            }
            Err(StrategyAttemptError::State(error)) => {
                cleanup_projection_staging(&staging);
                return Err(error);
            }
        }
    }
    Err(RepoStateError::ProjectionStrategyUnsupported {
        strategy: preferred.as_str().to_string(),
        path: root.to_path_buf(),
        reason: unsupported_reason
            .unwrap_or_else(|| "no safe implementation is available".to_string()),
    })
}

struct RealProjectionCacheEntry {
    content_root: PathBuf,
    cache_hit: bool,
    reuse: String,
    physically_materialized_bytes: u64,
}

fn ensure_real_projection_cache_entry(
    repo_root: &Path,
    state: &RealRepoState,
    request: &RealProjectionMaterializationRequest,
    cache_key: &str,
) -> Result<RealProjectionCacheEntry, RepoStateError> {
    let cache_root = repo_root.join(PROJECTION_CACHE_ROOT);
    ensure_safe_projection_cache_root(repo_root, &cache_root, cache_key)?;
    let key_digest = format!("{:x}", Sha256::digest(cache_key.as_bytes()));
    let entry_root = cache_root.join(&key_digest);
    let expected_manifest = real_projection_cache_manifest_bytes(state, request, cache_key)?;
    let mut rebuilt_after_quarantine = false;

    for _ in 0..3 {
        if projection_cache_path_exists(&entry_root)? {
            match validate_real_projection_cache_entry(&entry_root, state, &expected_manifest) {
                Ok(()) => {
                    return Ok(RealProjectionCacheEntry {
                        content_root: entry_root.join(PROJECTION_CACHE_CONTENT_ROOT),
                        cache_hit: true,
                        reuse: "reused".to_string(),
                        physically_materialized_bytes: 0,
                    });
                }
                Err(reason) => {
                    quarantine_real_projection_cache_entry(
                        repo_root,
                        &entry_root,
                        cache_key,
                        &reason,
                    )?;
                    rebuilt_after_quarantine = true;
                }
            }
        }

        let staging = cache_root.join(format!(
            ".staging-{key_digest}-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0),
            PROJECTION_CACHE_NONCE.fetch_add(1, Ordering::Relaxed),
        ));
        let content_root = staging.join(PROJECTION_CACHE_CONTENT_ROOT);
        fs::create_dir(&staging).map_err(|error| {
            io_error(
                &staging,
                "failed to create projection cache staging root",
                error,
            )
        })?;
        let build_result =
            build_real_projection_cache_entry(state, &staging, &content_root, &expected_manifest);
        if let Err(error) = build_result {
            cleanup_projection_staging(&staging);
            return Err(error);
        }
        if let Err(reason) =
            validate_real_projection_cache_entry(&staging, state, &expected_manifest)
        {
            cleanup_projection_staging(&staging);
            return Err(RepoStateError::ProjectionCacheIntegrity {
                cache_key: cache_key.to_string(),
                path: staging,
                reason,
            });
        }

        match fs::rename(&staging, &entry_root) {
            Ok(()) => {
                return Ok(RealProjectionCacheEntry {
                    content_root: entry_root.join(PROJECTION_CACHE_CONTENT_ROOT),
                    cache_hit: false,
                    reuse: if rebuilt_after_quarantine {
                        "rebuilt_after_quarantine".to_string()
                    } else {
                        "created".to_string()
                    },
                    physically_materialized_bytes: state_logical_bytes(state),
                });
            }
            Err(error) if projection_cache_path_exists(&entry_root)? => {
                cleanup_projection_staging(&staging);
                match validate_real_projection_cache_entry(&entry_root, state, &expected_manifest) {
                    Ok(()) => {
                        return Ok(RealProjectionCacheEntry {
                            content_root: entry_root.join(PROJECTION_CACHE_CONTENT_ROOT),
                            cache_hit: true,
                            reuse: "reused_concurrent_publication".to_string(),
                            physically_materialized_bytes: 0,
                        });
                    }
                    Err(reason) => {
                        quarantine_real_projection_cache_entry(
                            repo_root,
                            &entry_root,
                            cache_key,
                            &format!("concurrent publication failed validation: {reason}; rename error: {error}"),
                        )?;
                        rebuilt_after_quarantine = true;
                    }
                }
            }
            Err(error) => {
                cleanup_projection_staging(&staging);
                return Err(io_error(
                    &entry_root,
                    "failed to publish projection cache entry atomically",
                    error,
                ));
            }
        }
    }

    Err(RepoStateError::ProjectionCacheIntegrity {
        cache_key: cache_key.to_string(),
        path: entry_root,
        reason: "cache entry could not be published after concurrent integrity retries".to_string(),
    })
}

fn state_logical_bytes(state: &RealRepoState) -> u64 {
    state
        .entries
        .iter()
        .filter(|entry| !entry.tombstone)
        .map(|entry| entry.bytes.len() as u64)
        .sum()
}

fn ensure_safe_projection_cache_root(
    repo_root: &Path,
    cache_root: &Path,
    cache_key: &str,
) -> Result<(), RepoStateError> {
    let mut current = repo_root.to_path_buf();
    for component in Path::new(PROJECTION_CACHE_ROOT).components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if projection_metadata_is_reparse(&metadata) || !metadata.is_dir() => {
                return Err(RepoStateError::ProjectionCacheIntegrity {
                    cache_key: cache_key.to_string(),
                    path: current,
                    reason: "repository-local projection cache storage is not a safe directory"
                        .to_string(),
                });
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        let metadata = fs::symlink_metadata(&current).map_err(|error| {
                            io_error(
                                &current,
                                "failed to validate concurrently created projection cache",
                                error,
                            )
                        })?;
                        if projection_metadata_is_reparse(&metadata) || !metadata.is_dir() {
                            return Err(RepoStateError::ProjectionCacheIntegrity {
                                cache_key: cache_key.to_string(),
                                path: current,
                                reason: "concurrently created projection cache storage is unsafe"
                                    .to_string(),
                            });
                        }
                    }
                    Err(error) => {
                        return Err(io_error(
                            &current,
                            "failed to create repository-local projection cache",
                            error,
                        ));
                    }
                }
            }
            Err(error) => {
                return Err(io_error(
                    &current,
                    "failed to inspect repository-local projection cache",
                    error,
                ));
            }
        }
    }
    if current != cache_root {
        return Err(RepoStateError::ProjectionCacheIntegrity {
            cache_key: cache_key.to_string(),
            path: cache_root.to_path_buf(),
            reason: "projection cache root normalization mismatch".to_string(),
        });
    }
    Ok(())
}

fn real_projection_cache_manifest_bytes(
    state: &RealRepoState,
    request: &RealProjectionMaterializationRequest,
    cache_key: &str,
) -> Result<Vec<u8>, RepoStateError> {
    let mut entries = Vec::new();
    for entry in state.entries.iter().filter(|entry| !entry.tombstone) {
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
        object.insert(
            "byte_length".to_string(),
            JsonValue::Number(entry.bytes.len().to_string()),
        );
        object.insert("executable".to_string(), JsonValue::Bool(entry.executable));
        object.insert(
            "classification".to_string(),
            JsonValue::String(entry.classification.clone()),
        );
        entries.push(JsonValue::Object(object));
    }
    let mut tree_identity = BTreeMap::new();
    tree_identity.insert(
        "repository_id".to_string(),
        JsonValue::String(state.repository_id.clone()),
    );
    tree_identity.insert(
        "tree_hash".to_string(),
        JsonValue::String(state.tree_hash.clone()),
    );
    let mut object = BTreeMap::new();
    object.insert(
        "schema_version".to_string(),
        JsonValue::Number(PROJECTION_CACHE_SCHEMA_VERSION.to_string()),
    );
    object.insert(
        "record_type".to_string(),
        JsonValue::String("projection_cache_manifest".to_string()),
    );
    object.insert(
        "cache_key".to_string(),
        JsonValue::String(cache_key.to_string()),
    );
    object.insert(
        "resolved_view_id".to_string(),
        JsonValue::String(state.resolved_view_id.clone()),
    );
    object.insert(
        "tree_identity".to_string(),
        JsonValue::Object(tree_identity),
    );
    object.insert(
        "purpose".to_string(),
        JsonValue::String(request.purpose.as_str().to_string()),
    );
    object.insert(
        "writable_policy".to_string(),
        JsonValue::String(request.writable_policy.as_str().to_string()),
    );
    object.insert(
        "path_policy_id".to_string(),
        JsonValue::String(request.path_policy_id.clone()),
    );
    object.insert(
        "operation_semantics_version".to_string(),
        JsonValue::String(request.operation_semantics_version.clone()),
    );
    object.insert(
        "cache_materialization_strategy".to_string(),
        JsonValue::String(ProjectionStrategy::Copy.as_str().to_string()),
    );
    object.insert("entries".to_string(), JsonValue::Array(entries));
    canonical_json_bytes(&JsonValue::Object(object)).map_err(RepoStateError::from)
}

fn build_real_projection_cache_entry(
    state: &RealRepoState,
    staging: &Path,
    content_root: &Path,
    manifest: &[u8],
) -> Result<(), RepoStateError> {
    fs::create_dir(content_root).map_err(|error| {
        io_error(
            content_root,
            "failed to create projection cache content root",
            error,
        )
    })?;
    for entry in state.entries.iter().filter(|entry| !entry.tombstone) {
        if real_content_hash(&entry.bytes) != entry.content_hash {
            return Err(RepoStateError::InvalidState {
                path: real_blob_path(Path::new("."), &entry.content_hash),
                message: format!(
                    "source content for `{}` failed digest verification before cache publication",
                    entry.path
                ),
            });
        }
        let path = content_root.join(&entry.path);
        write_flushed_file(&path, &entry.bytes)?;
        set_cache_file_permissions(&path, entry.executable)?;
    }
    let manifest_path = staging.join(PROJECTION_CACHE_MANIFEST_FILE);
    write_flushed_file(&manifest_path, manifest)?;
    set_cache_file_permissions(&manifest_path, false)?;
    set_cache_directory_permissions(content_root)?;
    Ok(())
}

fn validate_real_projection_cache_entry(
    entry_root: &Path,
    state: &RealRepoState,
    expected_manifest: &[u8],
) -> Result<(), String> {
    let root_metadata = fs::symlink_metadata(entry_root)
        .map_err(|error| format!("cache entry root is unavailable: {error}"))?;
    if projection_metadata_is_reparse(&root_metadata) || !root_metadata.is_dir() {
        return Err("cache entry root is not a safe directory".to_string());
    }
    let manifest_path = entry_root.join(PROJECTION_CACHE_MANIFEST_FILE);
    let manifest_metadata = fs::symlink_metadata(&manifest_path)
        .map_err(|error| format!("cache manifest is unavailable: {error}"))?;
    if projection_metadata_is_reparse(&manifest_metadata)
        || !manifest_metadata.is_file()
        || !cache_file_permissions_are_immutable(&manifest_metadata, false)
    {
        return Err("cache manifest type or permissions are unsafe".to_string());
    }
    let manifest = fs::read(&manifest_path)
        .map_err(|error| format!("cache manifest could not be read: {error}"))?;
    if manifest != expected_manifest {
        return Err("cache manifest identity or entries do not match semantic inputs".to_string());
    }

    let content_root = entry_root.join(PROJECTION_CACHE_CONTENT_ROOT);
    let content_metadata = fs::symlink_metadata(&content_root)
        .map_err(|error| format!("cache content root is unavailable: {error}"))?;
    if projection_metadata_is_reparse(&content_metadata) || !content_metadata.is_dir() {
        return Err("cache content root is not a safe directory".to_string());
    }
    let expected_files = state
        .entries
        .iter()
        .filter(|entry| !entry.tombstone)
        .map(|entry| (PathBuf::from(&entry.path), entry))
        .collect::<BTreeMap<_, _>>();
    let mut expected_dirs = BTreeSet::new();
    for path in expected_files.keys() {
        let mut parent = path.parent();
        while let Some(value) = parent {
            if value.as_os_str().is_empty() {
                break;
            }
            expected_dirs.insert(value.to_path_buf());
            parent = value.parent();
        }
    }
    let mut seen_files = BTreeSet::new();
    validate_real_projection_cache_directory(
        &content_root,
        &content_root,
        &expected_files,
        &expected_dirs,
        &mut seen_files,
    )?;
    if seen_files.len() != expected_files.len() {
        return Err("cache content tree is missing manifest files".to_string());
    }
    Ok(())
}

fn validate_real_projection_cache_directory(
    root: &Path,
    current: &Path,
    expected_files: &BTreeMap<PathBuf, &RealArtifactEntry>,
    expected_dirs: &BTreeSet<PathBuf>,
    seen_files: &mut BTreeSet<PathBuf>,
) -> Result<(), String> {
    let metadata = fs::symlink_metadata(current)
        .map_err(|error| format!("cache directory is unavailable: {error}"))?;
    if projection_metadata_is_reparse(&metadata) || !metadata.is_dir() {
        return Err(format!(
            "cache path `{}` is not a safe directory",
            current.display()
        ));
    }
    for item in fs::read_dir(current)
        .map_err(|error| format!("cache directory could not be read: {error}"))?
    {
        let item = item.map_err(|error| format!("cache directory entry failed: {error}"))?;
        let path = item.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "cache path escaped its content root".to_string())?
            .to_path_buf();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("cache path metadata failed: {error}"))?;
        if projection_metadata_is_reparse(&metadata) {
            return Err(format!(
                "cache path `{}` is a reparse point or symlink",
                relative.display()
            ));
        }
        if metadata.is_dir() {
            if !expected_dirs.contains(&relative) {
                return Err(format!(
                    "cache contains unexpected directory `{}`",
                    relative.display()
                ));
            }
            validate_real_projection_cache_directory(
                root,
                &path,
                expected_files,
                expected_dirs,
                seen_files,
            )?;
            continue;
        }
        let Some(expected) = expected_files.get(&relative) else {
            return Err(format!(
                "cache contains unexpected file `{}`",
                relative.display()
            ));
        };
        if !metadata.is_file()
            || metadata.len() != expected.bytes.len() as u64
            || !cache_file_permissions_are_immutable(&metadata, expected.executable)
        {
            return Err(format!(
                "cache file `{}` has unsafe type, length, or permissions",
                relative.display()
            ));
        }
        let bytes = fs::read(&path).map_err(|error| {
            format!(
                "cache file `{}` could not be read: {error}",
                relative.display()
            )
        })?;
        if real_content_hash(&bytes) != expected.content_hash {
            return Err(format!(
                "cache file `{}` failed digest verification",
                relative.display()
            ));
        }
        seen_files.insert(relative);
    }
    Ok(())
}

fn quarantine_real_projection_cache_entry(
    repo_root: &Path,
    entry_root: &Path,
    cache_key: &str,
    reason: &str,
) -> Result<(), RepoStateError> {
    if !projection_cache_path_exists(entry_root)? {
        return Ok(());
    }
    let quarantine_root = repo_root.join(".sunlight/quarantine/projection-cache");
    ensure_safe_projection_cache_quarantine_root(repo_root, &quarantine_root, cache_key)?;
    let name = entry_root
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("cache-entry");
    let nonce = PROJECTION_CACHE_NONCE.fetch_add(1, Ordering::Relaxed);
    let quarantined = quarantine_root.join(format!("{name}-{nonce}"));
    fs::rename(entry_root, &quarantined).map_err(|error| {
        RepoStateError::ProjectionCacheIntegrity {
            cache_key: cache_key.to_string(),
            path: entry_root.to_path_buf(),
            reason: format!("failed to quarantine invalid cache entry: {error}"),
        }
    })?;
    let mut report = BTreeMap::new();
    report.insert(
        "record_type".to_string(),
        JsonValue::String("projection_cache_quarantine".to_string()),
    );
    report.insert(
        "cache_key".to_string(),
        JsonValue::String(cache_key.to_string()),
    );
    report.insert("reason".to_string(), JsonValue::String(reason.to_string()));
    report.insert(
        "quarantined_entry".to_string(),
        JsonValue::String(quarantined.display().to_string()),
    );
    let report_bytes = canonical_json_bytes(&JsonValue::Object(report))?;
    let report_path = quarantine_root.join(format!("{name}-{nonce}.json"));
    write_flushed_file(&report_path, &report_bytes)?;
    Ok(())
}

fn projection_cache_path_exists(path: &Path) -> Result<bool, RepoStateError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(
            path,
            "failed to inspect projection cache path",
            error,
        )),
    }
}

fn ensure_safe_projection_cache_quarantine_root(
    repo_root: &Path,
    quarantine_root: &Path,
    cache_key: &str,
) -> Result<(), RepoStateError> {
    let relative = Path::new(".sunlight/quarantine/projection-cache");
    let mut current = repo_root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if projection_metadata_is_reparse(&metadata) || !metadata.is_dir() => {
                return Err(RepoStateError::ProjectionCacheIntegrity {
                    cache_key: cache_key.to_string(),
                    path: current,
                    reason: "projection cache quarantine storage is not a safe directory"
                        .to_string(),
                });
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                        let metadata = fs::symlink_metadata(&current).map_err(|error| {
                            io_error(
                                &current,
                                "failed to validate concurrently created cache quarantine root",
                                error,
                            )
                        })?;
                        if projection_metadata_is_reparse(&metadata) || !metadata.is_dir() {
                            return Err(RepoStateError::ProjectionCacheIntegrity {
                                cache_key: cache_key.to_string(),
                                path: current,
                                reason: "concurrently created cache quarantine storage is unsafe"
                                    .to_string(),
                            });
                        }
                    }
                    Err(error) => {
                        return Err(io_error(
                            &current,
                            "failed to create projection cache quarantine root",
                            error,
                        ));
                    }
                }
            }
            Err(error) => {
                return Err(io_error(
                    &current,
                    "failed to inspect projection cache quarantine root",
                    error,
                ));
            }
        }
    }
    if current != quarantine_root {
        return Err(RepoStateError::ProjectionCacheIntegrity {
            cache_key: cache_key.to_string(),
            path: quarantine_root.to_path_buf(),
            reason: "projection cache quarantine root normalization mismatch".to_string(),
        });
    }
    Ok(())
}

#[cfg(windows)]
fn projection_metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x0000_0400 != 0
}

#[cfg(not(windows))]
fn projection_metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(unix)]
fn set_cache_file_permissions(path: &Path, executable: bool) -> Result<(), RepoStateError> {
    use std::os::unix::fs::PermissionsExt;
    let mode = if executable { 0o555 } else { 0o444 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|error| {
        io_error(
            path,
            "failed to make projection cache file immutable",
            error,
        )
    })
}

#[cfg(windows)]
fn set_cache_file_permissions(path: &Path, _executable: bool) -> Result<(), RepoStateError> {
    let mut permissions = fs::metadata(path)
        .map_err(|error| {
            io_error(
                path,
                "failed to inspect projection cache permissions",
                error,
            )
        })?
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).map_err(|error| {
        io_error(
            path,
            "failed to make projection cache file immutable",
            error,
        )
    })
}

#[cfg(not(any(unix, windows)))]
fn set_cache_file_permissions(path: &Path, _executable: bool) -> Result<(), RepoStateError> {
    let mut permissions = fs::metadata(path)
        .map_err(|error| {
            io_error(
                path,
                "failed to inspect projection cache permissions",
                error,
            )
        })?
        .permissions();
    permissions.set_readonly(true);
    fs::set_permissions(path, permissions).map_err(|error| {
        io_error(
            path,
            "failed to make projection cache file immutable",
            error,
        )
    })
}

#[cfg(unix)]
fn set_cache_directory_permissions(root: &Path) -> Result<(), RepoStateError> {
    use std::os::unix::fs::PermissionsExt;
    let mut directories = Vec::new();
    collect_projection_cache_directories(root, &mut directories)?;
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for path in directories {
        fs::set_permissions(&path, fs::Permissions::from_mode(0o555)).map_err(|error| {
            io_error(
                &path,
                "failed to make projection cache directory immutable",
                error,
            )
        })?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_cache_directory_permissions(_root: &Path) -> Result<(), RepoStateError> {
    Ok(())
}

#[cfg(unix)]
fn collect_projection_cache_directories(
    current: &Path,
    directories: &mut Vec<PathBuf>,
) -> Result<(), RepoStateError> {
    directories.push(current.to_path_buf());
    for item in fs::read_dir(current).map_err(|error| {
        io_error(
            current,
            "failed to inspect projection cache directories",
            error,
        )
    })? {
        let path = item
            .map_err(|error| io_error(current, "failed to inspect projection cache entry", error))?
            .path();
        if path.is_dir() {
            collect_projection_cache_directories(&path, directories)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn cache_file_permissions_are_immutable(metadata: &fs::Metadata, executable: bool) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let mode = metadata.permissions().mode();
    mode & 0o222 == 0 && (mode & 0o111 != 0) == executable
}

#[cfg(windows)]
fn cache_file_permissions_are_immutable(metadata: &fs::Metadata, _executable: bool) -> bool {
    metadata.permissions().readonly()
}

#[cfg(not(any(unix, windows)))]
fn cache_file_permissions_are_immutable(metadata: &fs::Metadata, _executable: bool) -> bool {
    metadata.permissions().readonly()
}

fn validate_real_projection_root(
    state: &RealRepoState,
    root: &Path,
    path_policy_id: &str,
) -> Result<(), RepoStateError> {
    let path_policy = PathPolicy {
        id: path_policy_id.to_string(),
    };
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
        Ok(metadata) if projection_metadata_is_reparse(&metadata) => {
            return Err(RepoStateError::InvalidState {
                path: root.to_path_buf(),
                message: "projection root cannot be a reparse point or symlink".to_string(),
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
    Ok(())
}

enum StrategyAttemptError {
    Unsupported(String),
    State(RepoStateError),
}

struct StrategyAttemptMetrics {
    bytes_copied: u64,
}

fn materialize_real_projection_strategy(
    cache_content_root: &Path,
    state: &RealRepoState,
    root: &Path,
    strategy: RealProjectionStrategy,
) -> Result<StrategyAttemptMetrics, StrategyAttemptError> {
    if matches!(
        strategy,
        RealProjectionStrategy::HardlinkReadonly | RealProjectionStrategy::OverlayCopyup
    ) {
        return Err(StrategyAttemptError::Unsupported(
            "the writable local MVP has no safe hardlink/copy-up or overlay implementation"
                .to_string(),
        ));
    }
    let mut bytes_copied = 0u64;
    let mut bytes_cloned = 0u64;
    for entry in state.entries.iter().filter(|entry| !entry.tombstone) {
        let path = root.join(&entry.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                StrategyAttemptError::State(io_error(
                    parent,
                    "failed to create projection directory",
                    error,
                ))
            })?;
        }
        let source = cache_content_root.join(&entry.path);
        match strategy {
            RealProjectionStrategy::Copy => {
                fs::copy(&source, &path).map_err(|error| {
                    StrategyAttemptError::State(io_error(
                        &path,
                        "failed to copy immutable cache file into private projection",
                        error,
                    ))
                })?;
                bytes_copied = bytes_copied.saturating_add(entry.bytes.len() as u64);
            }
            RealProjectionStrategy::Reflink => {
                let cloned = clone_file_cow(&source, &path, entry.bytes.len() as u64)
                    .map_err(StrategyAttemptError::Unsupported)?;
                bytes_cloned = bytes_cloned.saturating_add(cloned);
                bytes_copied = bytes_copied.saturating_add(entry.bytes.len() as u64 - cloned);
            }
            _ => unreachable!(),
        }
        set_private_projection_permissions(&path, entry.executable)
            .map_err(StrategyAttemptError::State)?;
        let materialized = fs::read(&path).map_err(|error| {
            StrategyAttemptError::State(io_error(&path, "failed to verify projection file", error))
        })?;
        if real_content_hash(&materialized) != entry.content_hash {
            return Err(StrategyAttemptError::State(RepoStateError::InvalidState {
                path,
                message: "materialized projection content failed integrity verification"
                    .to_string(),
            }));
        }
    }
    if strategy == RealProjectionStrategy::Reflink && bytes_cloned == 0 {
        return Err(StrategyAttemptError::Unsupported(
            "the view has no cluster-aligned extent that the volume can block-clone".to_string(),
        ));
    }
    Ok(StrategyAttemptMetrics { bytes_copied })
}

#[cfg(unix)]
fn set_private_projection_permissions(path: &Path, executable: bool) -> Result<(), RepoStateError> {
    use std::os::unix::fs::PermissionsExt;

    let mode = if executable { 0o755 } else { 0o644 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|error| {
        io_error(
            path,
            "failed to make private projection file writable",
            error,
        )
    })
}

#[cfg(windows)]
fn set_private_projection_permissions(
    path: &Path,
    _executable: bool,
) -> Result<(), RepoStateError> {
    let mut permissions = fs::metadata(path)
        .map_err(|error| {
            io_error(
                path,
                "failed to inspect private projection permissions",
                error,
            )
        })?
        .permissions();
    permissions.set_readonly(false);
    fs::set_permissions(path, permissions).map_err(|error| {
        io_error(
            path,
            "failed to make private projection file writable",
            error,
        )
    })
}

#[cfg(not(any(unix, windows)))]
fn set_private_projection_permissions(path: &Path, executable: bool) -> Result<(), RepoStateError> {
    set_projection_executable(path, executable)
}

fn projection_staging_path(root: &Path) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("projection");
    root.with_file_name(format!(
        ".{name}.sunlight-staging-{}-{nonce}",
        std::process::id()
    ))
}

fn cleanup_projection_staging(path: &Path) {
    if path.is_dir() {
        let _ = fs::remove_dir_all(path);
    } else if path.exists() {
        let _ = fs::remove_file(path);
    }
}

fn publish_projection_staging(root: &Path, staging: &Path) -> Result<(), RepoStateError> {
    if let Some(parent) = root.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| io_error(parent, "failed to create projection parent", error))?;
    }
    if root.exists() {
        fs::remove_dir(root)
            .map_err(|error| io_error(root, "failed to replace empty projection root", error))?;
    }
    fs::rename(staging, root)
        .map_err(|error| io_error(root, "failed to publish projection atomically", error))
}

#[cfg(windows)]
fn clone_file_cow(source: &Path, destination: &Path, length: u64) -> Result<u64, String> {
    use std::ffi::c_void;
    use std::fs::OpenOptions;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;

    #[repr(C)]
    struct DuplicateExtentsData {
        file_handle: *mut c_void,
        source_file_offset: i64,
        target_file_offset: i64,
        byte_count: i64,
    }
    extern "system" {
        fn DeviceIoControl(
            device: *mut c_void,
            control_code: u32,
            input: *mut c_void,
            input_size: u32,
            output: *mut c_void,
            output_size: u32,
            bytes_returned: *mut u32,
            overlapped: *mut c_void,
        ) -> i32;
        fn GetDiskFreeSpaceW(
            root_path: *const u16,
            sectors_per_cluster: *mut u32,
            bytes_per_sector: *mut u32,
            free_clusters: *mut u32,
            total_clusters: *mut u32,
        ) -> i32;
        fn GetVolumePathNameW(
            file_name: *const u16,
            volume_path_name: *mut u16,
            buffer_length: u32,
        ) -> i32;
    }
    const FSCTL_DUPLICATE_EXTENTS_TO_FILE: u32 = 0x0009_8344;

    let mut source_file = OpenOptions::new()
        .read(true)
        .open(source)
        .map_err(|error| format!("failed to open immutable blob for block clone: {error}"))?;
    let mut destination_file = OpenOptions::new()
        .create_new(true)
        .read(true)
        .write(true)
        .open(destination)
        .map_err(|error| format!("failed to create block-clone destination: {error}"))?;
    destination_file
        .set_len(length)
        .map_err(|error| format!("failed to size block-clone destination: {error}"))?;
    if length == 0 {
        return Ok(0);
    }
    let mut file_path = destination.as_os_str().encode_wide().collect::<Vec<_>>();
    file_path.push(0);
    let mut root_path = vec![0u16; 32_768];
    let volume_ok = unsafe {
        GetVolumePathNameW(
            file_path.as_ptr(),
            root_path.as_mut_ptr(),
            root_path.len() as u32,
        )
    };
    if volume_ok == 0 {
        return Err(format!(
            "failed to resolve Windows block-clone volume: {}",
            std::io::Error::last_os_error()
        ));
    }
    let (mut sectors, mut bytes_per_sector, mut free, mut total) = (0, 0, 0, 0);
    let disk_ok = unsafe {
        GetDiskFreeSpaceW(
            root_path.as_ptr(),
            &mut sectors,
            &mut bytes_per_sector,
            &mut free,
            &mut total,
        )
    };
    if disk_ok == 0 || sectors == 0 || bytes_per_sector == 0 {
        return Err(format!(
            "failed to determine Windows volume cluster size: {}",
            std::io::Error::last_os_error()
        ));
    }
    let cluster = u64::from(sectors) * u64::from(bytes_per_sector);
    let clone_length = length / cluster * cluster;
    let max_chunk = ((4u64 * 1024 * 1024 * 1024) - cluster) / cluster * cluster;
    let mut offset = 0u64;
    while offset < clone_length {
        let chunk = (clone_length - offset).min(max_chunk);
        let mut input = DuplicateExtentsData {
            file_handle: source_file.as_raw_handle(),
            source_file_offset: offset as i64,
            target_file_offset: offset as i64,
            byte_count: chunk as i64,
        };
        let mut returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                destination_file.as_raw_handle(),
                FSCTL_DUPLICATE_EXTENTS_TO_FILE,
                (&mut input as *mut DuplicateExtentsData).cast(),
                std::mem::size_of::<DuplicateExtentsData>() as u32,
                std::ptr::null_mut(),
                0,
                &mut returned,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(format!(
                "Windows block cloning is unavailable: {}",
                std::io::Error::last_os_error()
            ));
        }
        offset += chunk;
    }
    if clone_length < length {
        source_file
            .seek(SeekFrom::Start(clone_length))
            .map_err(|error| format!("failed to seek block-clone source tail: {error}"))?;
        destination_file
            .seek(SeekFrom::Start(clone_length))
            .map_err(|error| format!("failed to seek block-clone destination tail: {error}"))?;
        let mut tail = Vec::with_capacity((length - clone_length) as usize);
        source_file
            .read_to_end(&mut tail)
            .map_err(|error| format!("failed to read block-clone source tail: {error}"))?;
        destination_file
            .write_all(&tail)
            .map_err(|error| format!("failed to write block-clone destination tail: {error}"))?;
    }
    Ok(clone_length)
}

#[cfg(not(windows))]
fn clone_file_cow(_source: &Path, _destination: &Path, _length: u64) -> Result<u64, String> {
    Err("this build has no platform COW clone implementation".to_string())
}

#[cfg(not(any(unix, windows)))]
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
    let topic = RealTopicRecord {
        topic_id: required_string(object, "topic_id", state_path)?,
        slug: required_string(object, "slug", state_path)?,
        display_name: required_string(object, "display_name", state_path)?,
        owner_actor_id: required_string(object, "owner_actor_id", state_path)?,
        visibility: optional_string(object, "visibility", state_path)?
            .unwrap_or_else(|| "local".to_string()),
        acceptance_criteria: optional_array(object, "acceptance_criteria", state_path)?
            .iter()
            .map(|value| match value {
                JsonValue::String(criterion) => Ok(criterion.clone()),
                _ => Err(invalid_state(
                    state_path,
                    "topic acceptance_criteria entries must be strings",
                )),
            })
            .collect::<Result<Vec<_>, _>>()?,
        base_checkpoint_id: required_string(object, "base_checkpoint_id", state_path)?,
        head_revision_id: optional_string(object, "head_revision_id", state_path)?,
        revision_number: required_u64(object, "revision_number", state_path)?,
    };
    validate_topic_metadata(
        &topic.owner_actor_id,
        &topic.visibility,
        &topic.acceptance_criteria,
    )
    .map_err(|error| invalid_state(state_path, format!("invalid topic metadata: {error}")))?;
    Ok(topic)
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
        topic_frontier: parse_string_map(object, "topic_frontier", state_path)?,
        refresh_policy: optional_string(object, "refresh_policy", state_path)?
            .unwrap_or_else(|| "none".to_string()),
    })
}

fn parse_session_generation(
    value: &JsonValue,
    state_path: &Path,
) -> Result<RealSessionGenerationRecord, RepoStateError> {
    let JsonValue::Object(object) = value else {
        return Err(invalid_state(
            state_path,
            "session generation must be a JSON object",
        ));
    };
    Ok(RealSessionGenerationRecord {
        session_generation_id: required_string(object, "session_generation_id", state_path)?,
        session_id: required_string(object, "session_id", state_path)?,
        write_topic_id: required_string(object, "write_topic_id", state_path)?,
        base_resolved_view_id: required_string(object, "base_resolved_view_id", state_path)?,
        resolved_view_id: required_string(object, "resolved_view_id", state_path)?,
        topic_frontier: parse_string_map(object, "topic_frontier", state_path)?,
        generation_number: required_u64(object, "generation_number", state_path)?,
        refresh_policy: required_string(object, "refresh_policy", state_path)?,
        created_by: required_string(object, "created_by", state_path)?,
    })
}

fn parse_string_map(
    object: &BTreeMap<String, JsonValue>,
    field: &str,
    state_path: &Path,
) -> Result<BTreeMap<String, String>, RepoStateError> {
    let Some(value) = object.get(field) else {
        return Ok(BTreeMap::new());
    };
    let JsonValue::Object(values) = value else {
        return Err(invalid_state(
            state_path,
            format!("{field} must be a JSON object"),
        ));
    };
    values
        .iter()
        .map(|(key, value)| match value {
            JsonValue::String(value) => Ok((key.clone(), value.clone())),
            _ => Err(invalid_state(
                state_path,
                format!("{field} values must be strings"),
            )),
        })
        .collect()
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
        cache_key: optional_string(object, "cache_key", state_path)?.unwrap_or_default(),
        strategy: optional_string(object, "strategy", state_path)?
            .unwrap_or_else(|| "copy".to_string()),
        materialization: parse_projection_materialization_metrics(object, state_path)?,
        retention_state: optional_string(object, "retention_state", state_path)?
            .unwrap_or_else(|| "active".to_string()),
        privacy_class: optional_string(object, "privacy_class", state_path)?
            .unwrap_or_else(|| "local_only".to_string()),
        last_import_operation_id: optional_string(object, "last_import_operation_id", state_path)?,
        entries,
    })
}

fn parse_projection_materialization_metrics(
    object: &std::collections::BTreeMap<String, JsonValue>,
    state_path: &Path,
) -> Result<Option<RealProjectionMaterializationMetrics>, RepoStateError> {
    let Some(value) = object.get("materialization") else {
        return Ok(None);
    };
    let JsonValue::Object(metrics) = value else {
        return Err(invalid_state(
            state_path,
            "projection materialization must be an object",
        ));
    };
    Ok(Some(RealProjectionMaterializationMetrics {
        elapsed_ms: required_u64(metrics, "elapsed_ms", state_path)?,
        logical_bytes: required_u64(metrics, "logical_bytes", state_path)?,
        physically_materialized_bytes: optional_u64(
            metrics,
            "physically_materialized_bytes",
            state_path,
        )?,
        physical_allocation_bytes: optional_u64(metrics, "physical_allocation_bytes", state_path)?,
        file_count: required_u64(metrics, "file_count", state_path)?,
        cache_hit: required_bool(metrics, "cache_hit", state_path)?,
        reuse: required_string(metrics, "reuse", state_path)?,
        integrity_revalidated: required_bool(metrics, "integrity_revalidated", state_path)?,
        storage_amplification_millionths: optional_u64(
            metrics,
            "storage_amplification_millionths",
            state_path,
        )?,
    }))
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
        command_started: optional_bool(object, "command_started", state_path)?.unwrap_or(true),
        timed_out: optional_bool(object, "timed_out", state_path)?.unwrap_or_else(|| {
            required_string(object, "status", state_path).is_ok_and(|value| value == "timeout")
        }),
        termination_reason: optional_string(object, "termination_reason", state_path)?,
        termination_failed: optional_bool(object, "termination_failed", state_path)?
            .unwrap_or(false),
        wait_failed: optional_bool(object, "wait_failed", state_path)?.unwrap_or(false),
        stdout_observed_digest: match optional_string(object, "stdout_observed_digest", state_path)?
        {
            Some(digest) => digest,
            None => required_string(object, "stdout_digest", state_path)?,
        },
        stdout_byte_length: required_u64(object, "stdout_byte_length", state_path)?,
        stdout_captured_byte_length: optional_u64(
            object,
            "stdout_captured_byte_length",
            state_path,
        )?
        .unwrap_or_else(|| required_u64(object, "stdout_byte_length", state_path).unwrap_or(0)),
        stdout_truncated: optional_bool(object, "stdout_truncated", state_path)?.unwrap_or(false),
        stdout_capture_failed: optional_bool(object, "stdout_capture_failed", state_path)?
            .unwrap_or(false),
        stderr_observed_digest: match optional_string(object, "stderr_observed_digest", state_path)?
        {
            Some(digest) => digest,
            None => required_string(object, "stderr_digest", state_path)?,
        },
        stderr_byte_length: required_u64(object, "stderr_byte_length", state_path)?,
        stderr_captured_byte_length: optional_u64(
            object,
            "stderr_captured_byte_length",
            state_path,
        )?
        .unwrap_or_else(|| required_u64(object, "stderr_byte_length", state_path).unwrap_or(0)),
        stderr_truncated: optional_bool(object, "stderr_truncated", state_path)?.unwrap_or(false),
        stderr_capture_failed: optional_bool(object, "stderr_capture_failed", state_path)?
            .unwrap_or(false),
        timeout_ms: optional_u64(object, "timeout_ms", state_path)?,
        process_memory_limit_bytes: optional_u64(object, "process_memory_limit_bytes", state_path)?,
        job_memory_limit_bytes: optional_u64(object, "job_memory_limit_bytes", state_path)?,
        cpu_time_limit_ms: optional_u64(object, "cpu_time_limit_ms", state_path)?,
        active_process_limit: optional_u64(object, "active_process_limit", state_path)?
            .and_then(|value| u32::try_from(value).ok()),
        process_tree_policy: optional_string(object, "process_tree_policy", state_path)?
            .unwrap_or_else(|| "legacy_unrecorded".to_string()),
        cpu_policy: optional_string(object, "cpu_policy", state_path)?
            .unwrap_or_else(|| "legacy_unrecorded".to_string()),
        memory_policy: optional_string(object, "memory_policy", state_path)?
            .unwrap_or_else(|| "legacy_unrecorded".to_string()),
        environment_policy: optional_string(object, "environment_policy", state_path)?
            .unwrap_or_else(|| "legacy_unrecorded".to_string()),
        environment_allowlist: optional_array(object, "environment_allowlist", state_path)?
            .iter()
            .map(|value| match value {
                JsonValue::String(name) => Ok(name.clone()),
                _ => Err(invalid_state(
                    state_path,
                    "execution environment_allowlist must contain strings",
                )),
            })
            .collect::<Result<Vec<_>, _>>()?,
        network_policy_requested: optional_string(object, "network_policy_requested", state_path)?
            .unwrap_or_else(|| "legacy_unrecorded".to_string()),
        network_policy: optional_string(object, "network_policy", state_path)?
            .unwrap_or_else(|| "legacy_unrecorded".to_string()),
        filesystem_write_policy_requested: optional_string(
            object,
            "filesystem_write_policy_requested",
            state_path,
        )?
        .unwrap_or_else(|| "legacy_unrecorded".to_string()),
        filesystem_write_policy: optional_string(object, "filesystem_write_policy", state_path)?
            .unwrap_or_else(|| "legacy_unrecorded".to_string()),
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
    let evidence_refs = optional_array(object, "evidence_refs", state_path)?
        .iter()
        .map(|value| parse_checkpoint_evidence(value, state_path))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(RealCheckpointSnapshot {
        checkpoint_id: required_string(object, "checkpoint_id", state_path)?,
        resolved_view_id: required_string(object, "resolved_view_id", state_path)?,
        tree_hash: required_string(object, "tree_hash", state_path)?,
        topic_frontier,
        evidence_refs,
        created_at: required_string(object, "created_at", state_path)?,
        entries,
    })
}

fn parse_checkpoint_evidence(
    value: &JsonValue,
    state_path: &Path,
) -> Result<EvidenceRef, RepoStateError> {
    let JsonValue::Object(object) = value else {
        return Err(invalid_state(
            state_path,
            "checkpoint evidence_refs entries must be objects",
        ));
    };
    let kind = required_string(object, "kind", state_path)?;
    if kind != "execution" {
        return Err(invalid_state(
            state_path,
            format!("unsupported checkpoint evidence kind `{kind}`"),
        ));
    }
    let result = match required_string(object, "result", state_path)?.as_str() {
        "pass" => ExecutionStatus::Pass,
        "fail" => ExecutionStatus::Fail,
        "timeout" => ExecutionStatus::Timeout,
        "canceled" => ExecutionStatus::Canceled,
        "flaky" => ExecutionStatus::Flaky,
        "unknown" => ExecutionStatus::Unknown,
        "policy_blocked" => ExecutionStatus::PolicyBlocked,
        value => {
            return Err(invalid_state(
                state_path,
                format!("unsupported checkpoint execution result `{value}`"),
            ));
        }
    };
    Ok(EvidenceRef::Execution(ExecutionEvidenceRef {
        execution_id: required_string(object, "execution_id", state_path)?,
        result,
        resolved_view_id: required_string(object, "resolved_view_id", state_path)?,
        tree_identity: SingleRepoTree {
            repository_id: required_string(object, "repository_id", state_path)?,
            tree_hash: required_string(object, "tree_hash", state_path)?,
        },
    }))
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
        "cache_key".to_string(),
        JsonValue::String(projection.cache_key.clone()),
    );
    object.insert(
        "strategy".to_string(),
        JsonValue::String(projection.strategy.clone()),
    );
    object.insert(
        "materialization".to_string(),
        projection
            .materialization
            .as_ref()
            .map(projection_materialization_metrics_json)
            .unwrap_or(JsonValue::Null),
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

fn projection_materialization_metrics_json(
    metrics: &RealProjectionMaterializationMetrics,
) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "elapsed_ms".to_string(),
        JsonValue::Number(metrics.elapsed_ms.to_string()),
    );
    object.insert(
        "logical_bytes".to_string(),
        JsonValue::Number(metrics.logical_bytes.to_string()),
    );
    object.insert(
        "physically_materialized_bytes".to_string(),
        metrics
            .physically_materialized_bytes
            .map(|value| JsonValue::Number(value.to_string()))
            .unwrap_or(JsonValue::Null),
    );
    object.insert(
        "physical_allocation_bytes".to_string(),
        metrics
            .physical_allocation_bytes
            .map(|value| JsonValue::Number(value.to_string()))
            .unwrap_or(JsonValue::Null),
    );
    object.insert(
        "file_count".to_string(),
        JsonValue::Number(metrics.file_count.to_string()),
    );
    object.insert("cache_hit".to_string(), JsonValue::Bool(metrics.cache_hit));
    object.insert(
        "reuse".to_string(),
        JsonValue::String(metrics.reuse.clone()),
    );
    object.insert(
        "integrity_revalidated".to_string(),
        JsonValue::Bool(metrics.integrity_revalidated),
    );
    object.insert(
        "storage_amplification_millionths".to_string(),
        metrics
            .storage_amplification_millionths
            .map(|value| JsonValue::Number(value.to_string()))
            .unwrap_or(JsonValue::Null),
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
        "command_started".to_string(),
        JsonValue::Bool(execution.command_started),
    );
    object.insert(
        "timed_out".to_string(),
        JsonValue::Bool(execution.timed_out),
    );
    object.insert(
        "termination_reason".to_string(),
        execution
            .termination_reason
            .as_ref()
            .map(|value| JsonValue::String(value.clone()))
            .unwrap_or(JsonValue::Null),
    );
    object.insert(
        "termination_failed".to_string(),
        JsonValue::Bool(execution.termination_failed),
    );
    object.insert(
        "wait_failed".to_string(),
        JsonValue::Bool(execution.wait_failed),
    );
    object.insert(
        "stdout_observed_digest".to_string(),
        JsonValue::String(execution.stdout_observed_digest.clone()),
    );
    object.insert(
        "stdout_byte_length".to_string(),
        JsonValue::Number(execution.stdout_byte_length.to_string()),
    );
    object.insert(
        "stdout_captured_byte_length".to_string(),
        JsonValue::Number(execution.stdout_captured_byte_length.to_string()),
    );
    object.insert(
        "stdout_truncated".to_string(),
        JsonValue::Bool(execution.stdout_truncated),
    );
    object.insert(
        "stdout_capture_failed".to_string(),
        JsonValue::Bool(execution.stdout_capture_failed),
    );
    object.insert(
        "stderr_observed_digest".to_string(),
        JsonValue::String(execution.stderr_observed_digest.clone()),
    );
    object.insert(
        "stderr_byte_length".to_string(),
        JsonValue::Number(execution.stderr_byte_length.to_string()),
    );
    object.insert(
        "stderr_captured_byte_length".to_string(),
        JsonValue::Number(execution.stderr_captured_byte_length.to_string()),
    );
    object.insert(
        "stderr_truncated".to_string(),
        JsonValue::Bool(execution.stderr_truncated),
    );
    object.insert(
        "stderr_capture_failed".to_string(),
        JsonValue::Bool(execution.stderr_capture_failed),
    );
    object.insert(
        "timeout_ms".to_string(),
        execution
            .timeout_ms
            .map(|value| JsonValue::Number(value.to_string()))
            .unwrap_or(JsonValue::Null),
    );
    for (key, value) in [
        (
            "process_memory_limit_bytes",
            execution.process_memory_limit_bytes,
        ),
        ("job_memory_limit_bytes", execution.job_memory_limit_bytes),
        ("cpu_time_limit_ms", execution.cpu_time_limit_ms),
        (
            "active_process_limit",
            execution.active_process_limit.map(u64::from),
        ),
    ] {
        object.insert(
            key.to_string(),
            value
                .map(|value| JsonValue::Number(value.to_string()))
                .unwrap_or(JsonValue::Null),
        );
    }
    object.insert(
        "process_tree_policy".to_string(),
        JsonValue::String(execution.process_tree_policy.clone()),
    );
    object.insert(
        "cpu_policy".to_string(),
        JsonValue::String(execution.cpu_policy.clone()),
    );
    object.insert(
        "memory_policy".to_string(),
        JsonValue::String(execution.memory_policy.clone()),
    );
    object.insert(
        "environment_policy".to_string(),
        JsonValue::String(execution.environment_policy.clone()),
    );
    object.insert(
        "environment_allowlist".to_string(),
        JsonValue::Array(
            execution
                .environment_allowlist
                .iter()
                .map(|name| JsonValue::String(name.clone()))
                .collect(),
        ),
    );
    object.insert(
        "network_policy_requested".to_string(),
        JsonValue::String(execution.network_policy_requested.clone()),
    );
    object.insert(
        "network_policy".to_string(),
        JsonValue::String(execution.network_policy.clone()),
    );
    object.insert(
        "filesystem_write_policy_requested".to_string(),
        JsonValue::String(execution.filesystem_write_policy_requested.clone()),
    );
    object.insert(
        "filesystem_write_policy".to_string(),
        JsonValue::String(execution.filesystem_write_policy.clone()),
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
        "evidence_refs".to_string(),
        JsonValue::Array(
            checkpoint
                .evidence_refs
                .iter()
                .map(checkpoint_evidence_json)
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

fn checkpoint_evidence_json(evidence: &EvidenceRef) -> JsonValue {
    match evidence {
        EvidenceRef::Execution(execution) => {
            let mut object = BTreeMap::new();
            object.insert(
                "kind".to_string(),
                JsonValue::String("execution".to_string()),
            );
            object.insert(
                "execution_id".to_string(),
                JsonValue::String(execution.execution_id.clone()),
            );
            object.insert(
                "result".to_string(),
                JsonValue::String(execution.result.as_str().to_string()),
            );
            object.insert(
                "resolved_view_id".to_string(),
                JsonValue::String(execution.resolved_view_id.clone()),
            );
            object.insert(
                "repository_id".to_string(),
                JsonValue::String(execution.tree_identity.repository_id.clone()),
            );
            object.insert(
                "tree_hash".to_string(),
                JsonValue::String(execution.tree_identity.tree_hash.clone()),
            );
            JsonValue::Object(object)
        }
    }
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
        "visibility".to_string(),
        JsonValue::String(topic.visibility.clone()),
    );
    object.insert(
        "acceptance_criteria".to_string(),
        JsonValue::Array(
            topic
                .acceptance_criteria
                .iter()
                .cloned()
                .map(JsonValue::String)
                .collect(),
        ),
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
    object.insert(
        "topic_frontier".to_string(),
        string_map_json(&session.topic_frontier),
    );
    object.insert(
        "refresh_policy".to_string(),
        JsonValue::String(session.refresh_policy.clone()),
    );
    JsonValue::Object(object)
}

fn session_generation_json(generation: &RealSessionGenerationRecord) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "schema_version".to_string(),
        JsonValue::Number("1".to_string()),
    );
    object.insert(
        "record_type".to_string(),
        JsonValue::String("session_generation".to_string()),
    );
    object.insert(
        "id".to_string(),
        JsonValue::String(generation.session_generation_id.clone()),
    );
    object.insert(
        "session_generation_id".to_string(),
        JsonValue::String(generation.session_generation_id.clone()),
    );
    object.insert(
        "session_id".to_string(),
        JsonValue::String(generation.session_id.clone()),
    );
    object.insert(
        "write_topic_id".to_string(),
        JsonValue::String(generation.write_topic_id.clone()),
    );
    object.insert(
        "base_resolved_view_id".to_string(),
        JsonValue::String(generation.base_resolved_view_id.clone()),
    );
    object.insert(
        "resolved_view_id".to_string(),
        JsonValue::String(generation.resolved_view_id.clone()),
    );
    object.insert(
        "topic_frontier".to_string(),
        string_map_json(&generation.topic_frontier),
    );
    object.insert(
        "generation_number".to_string(),
        JsonValue::Number(generation.generation_number.to_string()),
    );
    object.insert(
        "refresh_policy".to_string(),
        JsonValue::String(generation.refresh_policy.clone()),
    );
    object.insert(
        "created_by".to_string(),
        JsonValue::String(generation.created_by.clone()),
    );
    object.insert(
        "privacy_class".to_string(),
        JsonValue::String("local_only".to_string()),
    );
    JsonValue::Object(object)
}

fn string_map_json(values: &BTreeMap<String, String>) -> JsonValue {
    JsonValue::Object(
        values
            .iter()
            .map(|(key, value)| (key.clone(), JsonValue::String(value.clone())))
            .collect(),
    )
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

fn optional_u64(
    object: &BTreeMap<String, JsonValue>,
    field: &'static str,
    path: &Path,
) -> Result<Option<u64>, RepoStateError> {
    match object.get(field) {
        Some(JsonValue::Null) | None => Ok(None),
        Some(JsonValue::Number(value)) => value
            .parse::<u64>()
            .map(Some)
            .map_err(|_| invalid_state(path, format!("field `{field}` must be an integer"))),
        _ => Err(invalid_state(
            path,
            format!("field `{field}` must be an integer or null"),
        )),
    }
}

fn optional_bool(
    object: &BTreeMap<String, JsonValue>,
    field: &'static str,
    path: &Path,
) -> Result<Option<bool>, RepoStateError> {
    match object.get(field) {
        Some(JsonValue::Null) | None => Ok(None),
        Some(JsonValue::Bool(value)) => Ok(Some(*value)),
        _ => Err(invalid_state(
            path,
            format!("field `{field}` must be a boolean or null"),
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
    use std::sync::{Arc, Barrier, Mutex};
    use std::thread;
    use std::time::Duration;

    static FAILPOINT_ENV_LOCK: Mutex<()> = Mutex::new(());

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

    fn select_publication_failpoint(name: &str, path: &Path) {
        std::env::set_var(
            STATE_PUBLICATION_FAILPOINT_ENV,
            format!("{name}|{}", normalize_failpoint_target(path).display()),
        );
    }

    fn completion_marker_path(repo: &Path, state: &RealRepoState) -> PathBuf {
        let mut published = state.clone();
        published.publication_sequence += 1;
        let body = canonical_json_bytes(&published.to_json_value()).unwrap();
        let digest = sha256_digest(&body);
        let transaction_id = format!(
            "publication-{}-{}",
            published.publication_sequence,
            &digest.strip_prefix("sha256:").unwrap_or(&digest)[..16]
        );
        publication_outbox_root(repo)
            .join(transaction_id)
            .join("completed.json")
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
    fn bare_publication_failpoint_name_is_ignored() {
        let _failpoint_guard = FAILPOINT_ENV_LOCK.lock().unwrap();
        let repo = temp_repo("bare-failpoint-ignored");
        fs::write(repo.join("README.md"), b"# bare failpoint\n").unwrap();
        let mut state = RealRepoState::ingest(&repo, "repo_bare_failpoint").unwrap();
        state.save(&repo).unwrap();
        state = RealRepoState::load(&repo).unwrap();
        state.generation_number = 1;
        let record = state
            .record_publication("operations", "op_bare", "{\"step\":1}")
            .unwrap();

        std::env::set_var(
            STATE_PUBLICATION_FAILPOINT_ENV,
            "batch_after_completion_marker",
        );
        let result = state.save_with_records(&repo, &[record]);
        std::env::remove_var(STATE_PUBLICATION_FAILPOINT_ENV);

        result.expect("bare failpoint names must not affect repository publication");
        assert!(repo.join(".sunlight/operations/op_bare.json").is_file());
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
    fn durable_publication_recovers_failpoints_and_repairs_generation_records() {
        let _failpoint_guard = FAILPOINT_ENV_LOCK.lock().unwrap();
        let repo = temp_repo("durable-publication");
        fs::write(repo.join("README.md"), b"# durable\n").unwrap();
        let mut state = RealRepoState::ingest(&repo, "repo_durable").unwrap();
        state.save(&repo).unwrap();
        state = RealRepoState::load(&repo).unwrap();
        let canonical = real_state_path(&repo);
        let old_bytes = fs::read(&canonical).unwrap();
        assert_eq!(state.publication_sequence, 1);

        state.generation_number = 1;
        select_publication_failpoint("state_after_prepare", &canonical);
        let error = state.save(&repo).unwrap_err();
        std::env::remove_var(STATE_PUBLICATION_FAILPOINT_ENV);
        assert!(error.to_string().contains("state_after_prepare"));
        assert_eq!(fs::read(&canonical).unwrap(), old_bytes);

        let recovered = RealRepoState::load(&repo).unwrap();
        assert_eq!(recovered.generation_number, 1);
        assert_eq!(recovered.publication_sequence, 2);
        parse_json_record(&fs::read(&canonical).unwrap()).unwrap();
        let recovery = state_recovery_paths(&repo);
        assert!(!recovery.journal.exists());
        assert!(!recovery.staged.exists());
        assert!(!recovery.backup.exists());

        let mut next = recovered.clone();
        next.generation_number = 2;
        select_publication_failpoint("state_after_replace", &canonical);
        let error = next.save(&repo).unwrap_err();
        std::env::remove_var(STATE_PUBLICATION_FAILPOINT_ENV);
        assert!(error.to_string().contains("state_after_replace"));
        let visible = RealRepoState::load_from_path(&repo, &canonical).unwrap();
        assert_eq!(visible.generation_number, 2);
        assert_eq!(visible.publication_sequence, 3);
        fs::write(&canonical, b"{\"interrupted\":").unwrap();
        let recovered = RealRepoState::load(&repo).unwrap();
        assert_eq!(recovered.generation_number, 1);
        assert_eq!(recovered.publication_sequence, 2);
        parse_json_record(&fs::read(&canonical).unwrap()).unwrap();
        assert!(fs::read_dir(&recovery.root)
            .unwrap()
            .flatten()
            .any(|entry| entry.file_name().to_string_lossy().starts_with("evidence-")));

        let record_path = repo
            .join(".sunlight")
            .join("operations")
            .join("op_atomic.json");
        select_publication_failpoint("derived_record_after_prepare", &record_path);
        let error = recovered
            .persist_record(&repo, "operations", "op_atomic", "{\"new\":true}")
            .unwrap_err();
        std::env::remove_var(STATE_PUBLICATION_FAILPOINT_ENV);
        assert!(error.to_string().contains("derived_record_after_prepare"));
        assert!(!record_path.exists());
        recovered
            .persist_record(&repo, "operations", "op_atomic", "{ \"new\" : true }")
            .unwrap();
        assert_eq!(fs::read(&record_path).unwrap(), b"{\"new\":true}");
        parse_json_record(&fs::read(&record_path).unwrap()).unwrap();

        let mut with_generation = recovered.clone();
        with_generation.sessions.push(RealSessionRecord {
            session_id: "session_repair".to_string(),
            actor_id: "agent-repair".to_string(),
            write_topic_id: "topic_repair".to_string(),
            resolved_view_id: with_generation.resolved_view_id.clone(),
            session_generation_id: "gen_repair_0001".to_string(),
            generation_number: 1,
            topic_frontier: BTreeMap::new(),
            refresh_policy: "none".to_string(),
        });
        with_generation
            .session_generations
            .push(RealSessionGenerationRecord {
                session_generation_id: "gen_repair_0001".to_string(),
                session_id: "session_repair".to_string(),
                write_topic_id: "topic_repair".to_string(),
                base_resolved_view_id: with_generation.base_resolved_view_id.clone(),
                resolved_view_id: with_generation.resolved_view_id.clone(),
                topic_frontier: BTreeMap::new(),
                generation_number: 1,
                refresh_policy: "none".to_string(),
                created_by: "test".to_string(),
            });
        with_generation.save(&repo).unwrap();
        let generation_path = repo.join(".sunlight/session-generations/gen_repair_0001.json");
        assert!(!generation_path.exists());
        with_generation = RealRepoState::load(&repo).unwrap();
        let generation = parse_json_record(&fs::read(&generation_path).unwrap()).unwrap();
        let JsonValue::Object(generation) = generation else {
            panic!("generation record must be an object");
        };
        assert_eq!(
            generation.get("record_type"),
            Some(&JsonValue::String("session_generation".to_string()))
        );

        select_publication_failpoint("state_after_prepare", &canonical);
        let mut interrupted = with_generation.clone();
        interrupted.generation_number = 9;
        interrupted.save(&repo).unwrap_err();
        std::env::remove_var(STATE_PUBLICATION_FAILPOINT_ENV);
        fs::write(&recovery.staged, b"{\"truncated\":").unwrap();
        fs::write(&canonical, b"{\"also_truncated\":").unwrap();
        let error = RealRepoState::load(&repo).unwrap_err();
        assert!(matches!(error, RepoStateError::Recovery { .. }));
        assert!(recovery.staged.exists());
        assert!(recovery.journal.exists());

        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn publication_outbox_rolls_back_replays_blocks_tamper_and_cleans() {
        let _failpoint_guard = FAILPOINT_ENV_LOCK.lock().unwrap();
        let repo = temp_repo("publication-outbox");
        fs::write(repo.join("README.md"), b"# outbox\n").unwrap();
        let mut state = RealRepoState::ingest(&repo, "repo_outbox").unwrap();
        state.save(&repo).unwrap();
        state = RealRepoState::load(&repo).unwrap();
        let canonical = real_state_path(&repo);
        let old_canonical = fs::read(&canonical).unwrap();

        state.generation_number = 1;
        let before_record = state
            .record_publication("operations", "op_before", "{\"step\":1}")
            .unwrap();
        select_publication_failpoint("batch_before_canonical_commit", &canonical);
        assert!(state
            .save_with_records(&repo, &[before_record])
            .unwrap_err()
            .to_string()
            .contains("batch_before_canonical_commit"));
        std::env::remove_var(STATE_PUBLICATION_FAILPOINT_ENV);
        assert_eq!(fs::read(&canonical).unwrap(), old_canonical);
        let prepared_transaction = fs::read_dir(publication_outbox_root(&repo))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let prepared_manifest =
            fs::read_to_string(prepared_transaction.join("manifest.json")).unwrap();
        assert!(prepared_manifest.contains("\"transaction_id\":\"publication-2-"));
        assert!(prepared_manifest.contains("\"target_publication_sequence\":2"));
        assert!(prepared_manifest.contains("\"target_canonical_sha256\":\"sha256:"));
        assert!(prepared_manifest
            .contains("\"final_relative_path\":\".sunlight/operations/op_before.json\""));
        assert!(prepared_manifest.contains("\"staged_payload\":\"staged/0000.json\""));
        assert!(prepared_manifest.contains("\"canonical_sha256\":\"sha256:"));
        let recovered = RealRepoState::load(&repo).unwrap();
        assert_eq!(recovered.generation_number, 0);
        assert!(!repo.join(".sunlight/operations/op_before.json").exists());
        assert!(!publication_outbox_root(&repo).exists());

        let mut after = recovered.clone();
        after.generation_number = 2;
        let after_record = after
            .record_publication("operations", "op_after", "{\"step\":2}")
            .unwrap();
        select_publication_failpoint("batch_after_canonical_commit", &canonical);
        assert!(after
            .save_with_records(&repo, &[after_record])
            .unwrap_err()
            .to_string()
            .contains("batch_after_canonical_commit"));
        std::env::remove_var(STATE_PUBLICATION_FAILPOINT_ENV);
        assert!(!repo.join(".sunlight/operations/op_after.json").exists());
        let recovered = RealRepoState::load(&repo).unwrap();
        assert_eq!(recovered.generation_number, 2);
        assert_eq!(
            fs::read(repo.join(".sunlight/operations/op_after.json")).unwrap(),
            b"{\"step\":2}"
        );
        RealRepoState::load(&repo).unwrap();
        assert!(!publication_outbox_root(&repo).exists());

        let mut middle = recovered.clone();
        middle.generation_number = 3;
        let middle_records = [
            middle
                .record_publication("operations", "op_middle", "{\"step\":3}")
                .unwrap(),
            middle
                .record_publication("views", "view_middle", "{\"step\":3}")
                .unwrap(),
        ];
        select_publication_failpoint(
            "batch_mid_derived_publication",
            &repo.join(".sunlight/operations/op_middle.json"),
        );
        assert!(middle
            .save_with_records(&repo, &middle_records)
            .unwrap_err()
            .to_string()
            .contains("batch_mid_derived_publication"));
        std::env::remove_var(STATE_PUBLICATION_FAILPOINT_ENV);
        assert!(repo.join(".sunlight/operations/op_middle.json").exists());
        assert!(!repo.join(".sunlight/views/view_middle.json").exists());
        let recovered = RealRepoState::load(&repo).unwrap();
        assert_eq!(recovered.generation_number, 3);
        assert!(repo.join(".sunlight/views/view_middle.json").exists());

        let mut completed = recovered.clone();
        completed.generation_number = 4;
        let completed_record = completed
            .record_publication("operations", "op_completed", "{\"step\":4}")
            .unwrap();
        select_publication_failpoint(
            "batch_after_completion_marker",
            &completion_marker_path(&repo, &completed),
        );
        assert!(completed
            .save_with_records(&repo, &[completed_record])
            .unwrap_err()
            .to_string()
            .contains("batch_after_completion_marker"));
        std::env::remove_var(STATE_PUBLICATION_FAILPOINT_ENV);
        let transaction_root = fs::read_dir(publication_outbox_root(&repo))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert!(transaction_root.join("completed.json").exists());
        RealRepoState::load(&repo).unwrap();
        assert!(!publication_outbox_root(&repo).exists());

        let mut corrupt = RealRepoState::load(&repo).unwrap();
        corrupt.generation_number = 5;
        let corrupt_record = corrupt
            .record_publication("operations", "op_corrupt", "{\"step\":5}")
            .unwrap();
        select_publication_failpoint("batch_after_canonical_commit", &canonical);
        corrupt
            .save_with_records(&repo, &[corrupt_record])
            .unwrap_err();
        std::env::remove_var(STATE_PUBLICATION_FAILPOINT_ENV);
        let transaction_root = fs::read_dir(publication_outbox_root(&repo))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        fs::write(
            transaction_root.join("staged/0000.json"),
            b"{\"tampered\":true}",
        )
        .unwrap();
        let error = RealRepoState::load(&repo).unwrap_err();
        assert!(matches!(error, RepoStateError::PublicationRecovery { .. }));
        assert!(transaction_root.join("manifest.json").exists());
        assert!(!repo.join(".sunlight/operations/op_corrupt.json").exists());

        let path_repo = temp_repo("publication-outbox-path-tamper");
        fs::write(path_repo.join("README.md"), b"# path tamper\n").unwrap();
        let mut path_state = RealRepoState::ingest(&path_repo, "repo_path_tamper").unwrap();
        path_state.save(&path_repo).unwrap();
        path_state = RealRepoState::load(&path_repo).unwrap();
        path_state.generation_number = 1;
        let path_record = path_state
            .record_publication("operations", "op_path", "{\"step\":1}")
            .unwrap();
        select_publication_failpoint(
            "batch_before_canonical_commit",
            &real_state_path(&path_repo),
        );
        path_state
            .save_with_records(&path_repo, &[path_record])
            .unwrap_err();
        std::env::remove_var(STATE_PUBLICATION_FAILPOINT_ENV);
        let transaction_root = fs::read_dir(publication_outbox_root(&path_repo))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let manifest_path = transaction_root.join("manifest.json");
        let manifest = fs::read_to_string(&manifest_path)
            .unwrap()
            .replace(".sunlight/operations/op_path.json", "../outside.json");
        fs::write(&manifest_path, manifest).unwrap();
        let error = RealRepoState::load(&path_repo).unwrap_err();
        assert!(matches!(error, RepoStateError::PublicationRecovery { .. }));
        assert!(manifest_path.exists());
        assert!(!path_repo.join("outside.json").exists());

        fs::remove_dir_all(repo).unwrap();
        fs::remove_dir_all(path_repo).unwrap();
    }

    #[test]
    fn record_publication_rejects_non_portable_and_windows_device_ids() {
        let repo = temp_repo("portable-record-names");
        let state = RealRepoState::ingest(&repo, "repo_record_names").unwrap();
        for id in [
            "op:ads",
            "op.json:ads",
            "op.name",
            "op name",
            "op/name",
            "op\\name",
            ".op",
            "op.",
            "CON",
            "con",
            "PRN",
            "AUX",
            "NUL",
            "COM1",
            "com9",
            "LPT1",
            "lpt9",
            "native-state",
        ] {
            let error = state
                .record_publication("operations", id, "{}")
                .unwrap_err();
            assert!(matches!(error, RepoStateError::InvalidState { .. }), "{id}");
        }
        for id in ["op_0001", "operation-ABC-123", "view9"] {
            state.record_publication("operations", id, "{}").unwrap();
        }
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn tampered_manifest_rejects_ads_filename_and_retains_evidence() {
        let _failpoint_guard = FAILPOINT_ENV_LOCK.lock().unwrap();
        let repo = temp_repo("publication-outbox-ads-tamper");
        fs::write(repo.join("README.md"), b"# ads tamper\n").unwrap();
        let mut state = RealRepoState::ingest(&repo, "repo_ads_tamper").unwrap();
        state.save(&repo).unwrap();
        state = RealRepoState::load(&repo).unwrap();
        state.generation_number = 1;
        let record = state
            .record_publication("operations", "op_ads", "{\"step\":1}")
            .unwrap();
        select_publication_failpoint("batch_before_canonical_commit", &real_state_path(&repo));
        state.save_with_records(&repo, &[record]).unwrap_err();
        std::env::remove_var(STATE_PUBLICATION_FAILPOINT_ENV);

        let transaction_root = fs::read_dir(publication_outbox_root(&repo))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let manifest_path = transaction_root.join("manifest.json");
        let manifest = fs::read_to_string(&manifest_path)
            .unwrap()
            .replace("op_ads.json", "op_ads:stream.json");
        fs::write(&manifest_path, manifest).unwrap();

        let error = RealRepoState::load(&repo).unwrap_err();
        assert!(matches!(error, RepoStateError::PublicationRecovery { .. }));
        assert!(error.to_string().contains("portable"));
        assert!(manifest_path.exists());
        assert!(transaction_root.join("staged/0000.json").exists());
        assert!(!repo
            .join(".sunlight/operations/op_ads:stream.json")
            .exists());

        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn separately_loaded_states_use_sequence_compare_and_swap() {
        let repo = temp_repo("publication-sequence-cas");
        fs::write(repo.join("README.md"), b"# cas\n").unwrap();
        let state = RealRepoState::ingest(&repo, "repo_cas").unwrap();
        state.save(&repo).unwrap();
        let mut winner = RealRepoState::load(&repo).unwrap();
        let mut loser = RealRepoState::load(&repo).unwrap();
        assert_eq!(winner.publication_sequence, 1);
        assert_eq!(loser.publication_sequence, 1);

        winner.generation_number = 1;
        let winner_record = winner
            .record_publication("operations", "op_winner", "{\"agent\":1}")
            .unwrap();
        winner.save_with_records(&repo, &[winner_record]).unwrap();

        loser.generation_number = 2;
        let loser_record = loser
            .record_publication("operations", "op_loser", "{\"agent\":2}")
            .unwrap();
        let error = loser
            .save_with_records(&repo, &[loser_record.clone()])
            .unwrap_err();
        assert_eq!(
            error,
            RepoStateError::ConcurrentStateUpdate {
                path: real_state_path(&repo),
                expected_sequence: 1,
                actual_sequence: Some(2),
            }
        );
        assert!(!publication_outbox_root(&repo).exists());
        assert!(!repo.join(".sunlight/operations/op_loser.json").exists());

        let visible = RealRepoState::load(&repo).unwrap();
        assert_eq!(visible.publication_sequence, 2);
        assert_eq!(visible.generation_number, 1);
        assert!(repo.join(".sunlight/operations/op_winner.json").exists());

        let mut retry = visible;
        retry.generation_number = 2;
        retry.save_with_records(&repo, &[loser_record]).unwrap();
        let retried = RealRepoState::load(&repo).unwrap();
        assert_eq!(retried.publication_sequence, 3);
        assert_eq!(retried.generation_number, 2);
        assert!(repo.join(".sunlight/operations/op_loser.json").exists());

        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn writer_lock_child_helper() {
        let Some(repo) = std::env::var_os("SUNLIGHT_TEST_WRITER_LOCK_REPO") else {
            return;
        };
        let repo = PathBuf::from(repo);
        let _lock = RepositoryWriterLock::acquire(&repo).unwrap();
        fs::write(repo.join("writer-lock-ready"), b"ready").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !repo.join("writer-lock-release").exists() {
            assert!(
                Instant::now() < deadline,
                "parent did not release writer lock helper"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn reader_cannot_recover_prepared_batch_while_other_process_holds_writer_lock() {
        let _failpoint_guard = FAILPOINT_ENV_LOCK.lock().unwrap();
        let repo = temp_repo("publication-reader-writer-lock");
        fs::write(repo.join("README.md"), b"# lock\n").unwrap();
        let mut state = RealRepoState::ingest(&repo, "repo_lock").unwrap();
        state.save(&repo).unwrap();
        state = RealRepoState::load(&repo).unwrap();
        state.generation_number = 1;
        let record = state
            .record_publication("operations", "op_prepared", "{\"step\":1}")
            .unwrap();
        select_publication_failpoint("batch_before_canonical_commit", &real_state_path(&repo));
        state.save_with_records(&repo, &[record]).unwrap_err();
        std::env::remove_var(STATE_PUBLICATION_FAILPOINT_ENV);
        let transaction_root = fs::read_dir(publication_outbox_root(&repo))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();

        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("repo_state::tests::writer_lock_child_helper")
            .arg("--nocapture")
            .env("SUNLIGHT_TEST_WRITER_LOCK_REPO", &repo)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !repo.join("writer-lock-ready").exists() {
            assert!(
                Instant::now() < deadline,
                "writer lock helper did not become ready"
            );
            thread::sleep(Duration::from_millis(10));
        }

        let started = Instant::now();
        let error = RealRepoState::load(&repo).unwrap_err();
        assert_eq!(
            error,
            RepoStateError::WriterBusy {
                lock: repo.join(".sunlight/local/command-transaction.lock"),
                timeout_ms: 0,
            }
        );
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(transaction_root.join("manifest.json").exists());
        assert!(transaction_root.join("staged/0000.json").exists());

        fs::write(repo.join("writer-lock-release"), b"release").unwrap();
        assert!(child.wait().unwrap().success());
        let loaded = RealRepoState::load(&repo).unwrap();
        assert_eq!(loaded.publication_sequence, 1);
        assert!(!publication_outbox_root(&repo).exists());
        assert!(!repo.join(".sunlight/operations/op_prepared.json").exists());

        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn persisted_topic_metadata_rejects_unsupported_visibility() {
        let repo = temp_repo("topic-visibility-load-validation");
        let mut state = RealRepoState::ingest(&repo, "repo_topic_validation").unwrap();
        let topic = test_topic("shared");
        let parse_error =
            parse_topic(&topic_json(&topic), Path::new("native-state.json")).unwrap_err();
        assert!(matches!(
            parse_error,
            RepoStateError::InvalidState { ref message, .. }
                if message == "invalid topic metadata: topic visibility must be one of: local, private"
        ));
        state.topics.push(topic);
        write_state_json(&repo, state.to_json_value());

        let error = RealRepoState::load(&repo).unwrap_err();
        assert!(
            matches!(
                &error,
                RepoStateError::Recovery { ref message, .. }
                    if message.contains("canonical state is malformed")
            ),
            "unexpected load error: {error:?}"
        );

        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn persisted_topic_metadata_rejects_invalid_owner_and_acceptance_criteria() {
        for (name, owner, criteria, expected) in [
            (
                "owner",
                "",
                Vec::new(),
                "invalid topic metadata: topic owner must be a non-empty actor identifier of at most 128 characters",
            ),
            (
                "criteria",
                "agent-a",
                vec![" ".to_string()],
                "invalid topic metadata: each acceptance criterion must be non-empty, at most 1024 characters, and at most 64 criteria may be supplied",
            ),
        ] {
            let mut topic = test_topic("local");
            topic.owner_actor_id = owner.to_string();
            topic.acceptance_criteria = criteria;

            let error =
                parse_topic(&topic_json(&topic), Path::new("native-state.json")).unwrap_err();
            assert!(
                matches!(
                    &error,
                    RepoStateError::InvalidState { ref message, .. } if message == expected
                ),
                "unexpected {name} parse error: {error:?}"
            );
        }
    }

    #[test]
    fn legacy_session_state_loads_with_effective_frontier_and_pinned_policy() {
        let repo = temp_repo("legacy-session-frontier");
        let mut state = RealRepoState::ingest(&repo, "repo_legacy").unwrap();
        state.topics.push(RealTopicRecord {
            topic_id: "topic_legacy".to_string(),
            slug: "legacy".to_string(),
            display_name: "Legacy".to_string(),
            owner_actor_id: "legacy-agent".to_string(),
            visibility: "local".to_string(),
            acceptance_criteria: Vec::new(),
            base_checkpoint_id: state.base_checkpoint_id.clone(),
            head_revision_id: Some("rev_legacy_0001".to_string()),
            revision_number: 1,
        });
        state.sessions.push(RealSessionRecord {
            session_id: "session_legacy".to_string(),
            actor_id: "legacy-agent".to_string(),
            write_topic_id: "topic_legacy".to_string(),
            resolved_view_id: "view_legacy".to_string(),
            session_generation_id: "gen_legacy_0001".to_string(),
            generation_number: 1,
            topic_frontier: BTreeMap::from([(
                "topic_legacy".to_string(),
                "rev_legacy_0001".to_string(),
            )]),
            refresh_policy: "none".to_string(),
        });
        let mut json = state.to_json_value();
        let JsonValue::Object(root) = &mut json else {
            unreachable!()
        };
        root.remove("session_generations");
        let JsonValue::Array(topics) = root.get_mut("topics").unwrap() else {
            unreachable!()
        };
        let JsonValue::Object(topic) = &mut topics[0] else {
            unreachable!()
        };
        topic.remove("visibility");
        topic.remove("acceptance_criteria");
        let JsonValue::Array(sessions) = root.get_mut("sessions").unwrap() else {
            unreachable!()
        };
        let JsonValue::Object(session) = &mut sessions[0] else {
            unreachable!()
        };
        session.remove("topic_frontier");
        session.remove("refresh_policy");
        let path = real_state_path(&repo);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, canonical_json_bytes(&json).unwrap()).unwrap();

        let loaded = RealRepoState::load(&repo).unwrap();
        let topic = loaded.topic_by_id_or_slug("topic_legacy").unwrap();
        assert_eq!(topic.visibility, "local");
        assert!(topic.acceptance_criteria.is_empty());
        let session = loaded.session_by_id("session_legacy").unwrap();
        assert_eq!(session.refresh_policy, "none");
        assert_eq!(
            session
                .topic_frontier
                .get("topic_legacy")
                .map(String::as_str),
            Some("rev_legacy_0001")
        );
        assert_eq!(loaded.session_generations.len(), 1);
        assert_eq!(
            loaded.session_generations[0].created_by,
            "legacy_state_migration"
        );

        fs::remove_dir_all(repo).unwrap();
    }

    fn test_topic(visibility: &str) -> RealTopicRecord {
        RealTopicRecord {
            topic_id: "topic_validation".to_string(),
            slug: "validation".to_string(),
            display_name: "Validation".to_string(),
            owner_actor_id: "agent-a".to_string(),
            visibility: visibility.to_string(),
            acceptance_criteria: vec!["focused behavior is verified".to_string()],
            base_checkpoint_id: "checkpoint_base_0001".to_string(),
            head_revision_id: None,
            revision_number: 0,
        }
    }

    fn write_state_json(repo: &Path, value: JsonValue) {
        let path = real_state_path(repo);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, canonical_json_bytes(&value).unwrap()).unwrap();
    }

    #[test]
    fn legacy_actor_scoped_generation_collision_is_relinked_per_session() {
        let repo = temp_repo("legacy-generation-collision");
        let mut state = RealRepoState::ingest(&repo, "repo_legacy_collision").unwrap();
        for topic_id in ["topic_first", "topic_second"] {
            state.topics.push(RealTopicRecord {
                topic_id: topic_id.to_string(),
                slug: topic_id.trim_start_matches("topic_").to_string(),
                display_name: topic_id.to_string(),
                owner_actor_id: "shared-agent".to_string(),
                visibility: "local".to_string(),
                acceptance_criteria: Vec::new(),
                base_checkpoint_id: state.base_checkpoint_id.clone(),
                head_revision_id: None,
                revision_number: 0,
            });
        }
        for (session_id, write_topic_id) in [
            ("session_shared_agent", "topic_first"),
            ("session_shared_agent_second", "topic_second"),
        ] {
            state.sessions.push(RealSessionRecord {
                session_id: session_id.to_string(),
                actor_id: "shared-agent".to_string(),
                write_topic_id: write_topic_id.to_string(),
                resolved_view_id: state.base_resolved_view_id.clone(),
                session_generation_id: "gen_shared_agent_0001".to_string(),
                generation_number: 1,
                topic_frontier: BTreeMap::new(),
                refresh_policy: "none".to_string(),
            });
        }
        state.session_generations.push(RealSessionGenerationRecord {
            session_generation_id: "gen_shared_agent_0001".to_string(),
            session_id: "session_shared_agent".to_string(),
            write_topic_id: "topic_first".to_string(),
            base_resolved_view_id: state.base_resolved_view_id.clone(),
            resolved_view_id: state.base_resolved_view_id.clone(),
            topic_frontier: BTreeMap::new(),
            generation_number: 1,
            refresh_policy: "none".to_string(),
            created_by: "session_start".to_string(),
        });
        state.save(&repo).unwrap();

        let loaded = RealRepoState::load(&repo).unwrap();
        assert_eq!(
            loaded
                .session_by_id("session_shared_agent")
                .unwrap()
                .session_generation_id,
            "gen_shared_agent_0001"
        );
        let repaired = loaded.session_by_id("session_shared_agent_second").unwrap();
        assert_eq!(
            repaired.session_generation_id,
            "gen_shared_agent_second_0001"
        );
        assert!(loaded.session_generations.iter().any(|generation| {
            generation.session_generation_id == repaired.session_generation_id
                && generation.session_id == repaired.session_id
        }));

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
            publication_sequence: 0,
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
            session_generations: Vec::new(),
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
            publication_sequence: 0,
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
                    visibility: "local".to_string(),
                    acceptance_criteria: Vec::new(),
                    base_checkpoint_id: "checkpoint_base_0001".to_string(),
                    head_revision_id: Some("rev_docs_0001".to_string()),
                    revision_number: 1,
                },
                RealTopicRecord {
                    topic_id: "topic_code".to_string(),
                    slug: "code".to_string(),
                    display_name: "Code".to_string(),
                    owner_actor_id: "agent-b".to_string(),
                    visibility: "local".to_string(),
                    acceptance_criteria: Vec::new(),
                    base_checkpoint_id: "checkpoint_base_0001".to_string(),
                    head_revision_id: Some("rev_code_0001".to_string()),
                    revision_number: 1,
                },
            ],
            sessions: Vec::new(),
            session_generations: Vec::new(),
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
            visibility: "local".to_string(),
            acceptance_criteria: Vec::new(),
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

        let followup = artifact_entry("src/followup.rs", b"pub fn followup() {}\n");
        state.base_entries.push(followup.clone());
        state.entries.push(followup.clone());
        state.operations.push(operation(
            "op_code_0002",
            "topic_code",
            "rev_code_0002",
            &followup,
            b"pub fn followup() { println!(\"done\"); }\n",
        ));
        let code_topic = state
            .topics
            .iter_mut()
            .find(|topic| topic.topic_id == "topic_code")
            .unwrap();
        code_topic.head_revision_id = Some("rev_code_0002".to_string());
        code_topic.revision_number = 2;

        let expanded_conflict = state.resolve_head_view();
        assert!(!expanded_conflict.result.conflict_free());
        assert!(expanded_conflict.result.tree_identity.is_none());
        assert!(expanded_conflict.result.tree_entries.is_empty());
        assert_eq!(
            expanded_conflict.result.conflicts().next().unwrap().id,
            "conflict_src_lib_rs_0001"
        );
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

    #[test]
    fn real_projection_cache_publishes_once_concurrently_and_keys_semantic_policy() {
        let repo = temp_repo("projection-cache-concurrent");
        fs::write(repo.join("source.txt"), b"source truth\n").unwrap();
        let state = Arc::new(RealRepoState::ingest(&repo, "repo_projection_cache").unwrap());
        let request = RealProjectionMaterializationRequest {
            purpose: ProjectionPurpose::Inspection,
            writable_policy: WritablePolicy::ReadOnly,
            path_policy_id: POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string(),
            operation_semantics_version: FILE_OPERATION_SEMANTICS_VERSION.to_string(),
            required_strategy: Some(RealProjectionStrategy::Copy),
            fallback_to_copy: false,
        };
        let barrier = Arc::new(Barrier::new(2));
        let mut workers = Vec::new();
        for suffix in ["a", "b"] {
            let repo = repo.clone();
            let state = Arc::clone(&state);
            let request = request.clone();
            let barrier = Arc::clone(&barrier);
            workers.push(thread::spawn(move || {
                barrier.wait();
                materialize_real_projection(
                    &repo,
                    &state,
                    &repo.join(format!("projection-{suffix}")),
                    &request,
                )
                .unwrap()
            }));
        }
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results[0].cache_key, results[1].cache_key);
        assert!(results.iter().any(|result| !result.metrics.cache_hit));
        assert!(results.iter().any(|result| result.metrics.cache_hit));
        assert_eq!(published_projection_cache_entries(&repo), 1);
        assert_eq!(
            fs::read(repo.join("projection-a/source.txt")).unwrap(),
            b"source truth\n"
        );
        assert_eq!(
            fs::read(repo.join("projection-b/source.txt")).unwrap(),
            b"source truth\n"
        );
        let interrupted = repo
            .join(PROJECTION_CACHE_ROOT)
            .join(".staging-interrupted-publication");
        fs::create_dir(&interrupted).unwrap();
        fs::write(interrupted.join("partial"), b"must never be reused").unwrap();
        let after_interruption = materialize_real_projection(
            &repo,
            &state,
            &repo.join("projection-after-interruption"),
            &request,
        )
        .unwrap();
        assert!(after_interruption.metrics.cache_hit);
        assert_eq!(
            fs::read(repo.join("projection-after-interruption/source.txt")).unwrap(),
            b"source truth\n"
        );
        assert_eq!(published_projection_cache_entries(&repo), 1);

        let mut changed_policy = request.clone();
        changed_policy.operation_semantics_version = "file_ops_test_v2".to_string();
        let policy_result = materialize_real_projection(
            &repo,
            &state,
            &repo.join("projection-policy"),
            &changed_policy,
        )
        .unwrap();
        assert!(!policy_result.metrics.cache_hit);

        let changed_purpose = RealProjectionMaterializationRequest {
            purpose: ProjectionPurpose::Export,
            writable_policy: WritablePolicy::ExportMaterializationOnly,
            ..request.clone()
        };
        let purpose_result = materialize_real_projection(
            &repo,
            &state,
            &repo.join("projection-purpose"),
            &changed_purpose,
        )
        .unwrap();
        assert!(!purpose_result.metrics.cache_hit);

        let mut changed_tree = (*state).clone();
        changed_tree.entries[0] = artifact_entry("source.txt", b"changed tree\n");
        changed_tree.tree_hash = real_tree_hash(&changed_tree.entries);
        changed_tree.resolved_view_id = "view_changed_tree".to_string();
        let tree_result = materialize_real_projection(
            &repo,
            &changed_tree,
            &repo.join("projection-tree"),
            &request,
        )
        .unwrap();
        assert!(!tree_result.metrics.cache_hit);
        assert_eq!(published_projection_cache_entries(&repo), 4);
    }

    fn published_projection_cache_entries(repo: &Path) -> usize {
        fs::read_dir(repo.join(PROJECTION_CACHE_ROOT))
            .unwrap()
            .flatten()
            .filter(|entry| {
                entry.path().is_dir()
                    && !entry.file_name().to_string_lossy().starts_with(".staging-")
            })
            .count()
    }
}

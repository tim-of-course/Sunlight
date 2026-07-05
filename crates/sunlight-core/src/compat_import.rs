use std::collections::{BTreeMap, BTreeSet};

use crate::artifacts::{
    ArtifactIoError, ExpectedHash, MutationArtifactRef, MutationKind, MutationPreconditions,
    MutationRefs, PathPolicy, WriteSetEntry, FILE_OPERATION_SEMANTICS_VERSION, FIXTURE_ACTOR_ID,
    FIXTURE_SESSION_ID, FIXTURE_WRITE_TOPIC_ID,
};
use crate::projection::{ProjectionPurpose, ProjectionRecord};
use crate::records::PrivacyClass;
use crate::resolver::{ResolvedViewResult, SingleRepoTree};

pub const FIXTURE_COMPAT_IMPORT_OPERATION_ID: &str = "op_compat_import_auth_0001";
pub const FIXTURE_COMPAT_IMPORT_TOPIC_REVISION_ID: &str = "rev_auth_nullability_compat_0001";
pub const FIXTURE_COMPAT_IMPORT_SESSION_GENERATION_ID: &str = "gen_agent_a_compat_0002";
pub const FIXTURE_COMPAT_IMPORT_RESOLVED_VIEW_ID: &str = "view_agent_a_after_compat_import_0001";
pub const FIXTURE_COMPAT_IMPORT_TREE_HASH: &str = "tree_after_compat_import_0001";
pub const FIXTURE_COMPAT_IMPORT_CONTEXT_ID: &str = "ctx_compat_projection_0001";
pub const FIXTURE_COMPAT_BASELINE_MANIFEST_DIGEST: &str = "sha256:compat_baseline";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatImportRequest {
    pub projection_id: String,
    pub session_id: String,
    pub session_generation_id: String,
    pub resolved_view_id: String,
    pub write_topic_id: String,
    pub parent_topic_revision_id: Option<String>,
    pub selected_candidate_delta_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatImportResponse {
    pub command: &'static str,
    pub repository_id: String,
    pub projection_id: String,
    pub session_id: String,
    pub operation_id: String,
    pub topic_revision_id: String,
    pub session_generation_id: String,
    pub resolved_view_id: String,
    pub tree_identity: SingleRepoTree,
    pub imported_artifacts: Vec<CompatImportedArtifact>,
    pub ignored_candidate_delta_ids: Vec<String>,
    pub quarantine_refs: Vec<String>,
    pub plan: CompatImportPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatImportedArtifact {
    pub candidate_delta_id: String,
    pub artifact_id: String,
    pub path: String,
    pub operation_kind: CompatFileOperationKind,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub classification: String,
    pub privacy_class: PrivacyClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatImportPlan {
    pub operation: CompatOperationTransactionPlan,
    pub topic_revision: CompatTopicRevisionPlan,
    pub session_generation: CompatSessionGenerationPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatOperationTransactionPlan {
    pub id: String,
    pub repository_id: String,
    pub topic_id: String,
    pub session_id: String,
    pub session_generation_id: String,
    pub actor_id: String,
    pub authored_context_id: String,
    pub preconditions: CompatImportPreconditions,
    pub read_set: CompatImportReadSet,
    pub write_set: Vec<WriteSetEntry>,
    pub mutation_payload: CompatImportMutationPayload,
    pub before_refs: MutationRefs,
    pub after_refs: MutationRefs,
    pub classification: String,
    pub parent_topic_revision_id: Option<String>,
    pub next_topic_revision_number: u64,
    pub parents: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatImportPreconditions {
    pub projection_id: String,
    pub projection_purpose: ProjectionPurpose,
    pub projection_baseline_resolved_view_id: String,
    pub projection_baseline_tree_identity: SingleRepoTree,
    pub session_id: String,
    pub session_generation_id: String,
    pub resolved_view_id: String,
    pub write_topic_id: String,
    pub parent_topic_revision_id: Option<String>,
    pub path_policy_id: String,
    pub operation_semantics_version: String,
    pub selected_candidate_delta_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatImportReadSet {
    pub mode: String,
    pub resolved_view_id: String,
    pub projection_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatImportMutationPayload {
    pub kind: String,
    pub projection_id: String,
    pub baseline_manifest_digest: String,
    pub selected_deltas: Vec<CompatSelectedDeltaPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatSelectedDeltaPlan {
    pub candidate_delta_id: String,
    pub operation_kind: CompatFileOperationKind,
    pub path: String,
    pub patch_digest: Option<String>,
    pub base_content_hash: Option<String>,
    pub result_content_hash: Option<String>,
    pub classification: String,
    pub privacy_class: PrivacyClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatTopicRevisionPlan {
    pub id: String,
    pub repository_id: String,
    pub topic_id: String,
    pub revision_number: u64,
    pub parent_revision_id: Option<String>,
    pub operation_transaction_id: String,
    pub tree_delta_ref: String,
    pub dependency_revision_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatSessionGenerationPlan {
    pub id: String,
    pub repository_id: String,
    pub session_id: String,
    pub write_topic_id: String,
    pub base_resolved_view_id: String,
    pub resolved_view_id: String,
    pub topic_frontier: BTreeMap<String, String>,
    pub generation_number: u64,
    pub refresh_policy: String,
    pub created_by_operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatCandidateDelta {
    pub candidate_delta_id: String,
    pub kind: CompatCandidateKind,
    pub operation_kind: CompatFileOperationKind,
    pub artifact_id: Option<String>,
    pub path: String,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub byte_length: u64,
    pub executable: bool,
    pub media_type: String,
    pub classification: String,
    pub privacy_class: PrivacyClass,
    pub path_policy_result: CompatPathPolicyResult,
    pub quarantine_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatCandidateKind {
    ModifiedSource,
    CreatedSource,
    DeletedSource,
    MovedOrRenamed,
    MetadataChanged,
    GeneratedSource,
    BinaryOrLarge,
    CacheOrBuildOutput,
    SecretLike,
    IgnoredPath,
    PathPolicyBlocked,
    ConflictedDelta,
}

impl CompatCandidateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ModifiedSource => "modified_source",
            Self::CreatedSource => "created_source",
            Self::DeletedSource => "deleted_source",
            Self::MovedOrRenamed => "moved_or_renamed",
            Self::MetadataChanged => "metadata_changed",
            Self::GeneratedSource => "generated_source",
            Self::BinaryOrLarge => "binary_or_large",
            Self::CacheOrBuildOutput => "cache_or_build_output",
            Self::SecretLike => "secret_like",
            Self::IgnoredPath => "ignored_path",
            Self::PathPolicyBlocked => "path_policy_blocked",
            Self::ConflictedDelta => "conflicted_delta",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatFileOperationKind {
    Patch,
    Write,
    Delete,
    Move,
    Metadata,
}

impl CompatFileOperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Patch => "patch",
            Self::Write => "write",
            Self::Delete => "delete",
            Self::Move => "move",
            Self::Metadata => "metadata",
        }
    }

    fn mutation_kind(self) -> MutationKind {
        match self {
            Self::Patch => MutationKind::Patch,
            Self::Write | Self::Delete | Self::Move | Self::Metadata => MutationKind::Write,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatPathPolicyResult {
    pub allowed: bool,
    pub normalized_path: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatImportValidationError {
    pub code: CompatImportErrorCode,
    pub projection_id: String,
    pub session_id: String,
    pub candidate_delta_ids: Vec<String>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatImportErrorCode {
    ProjectionNotFound,
    ProjectionInvalid,
    ProjectionStale,
    ProjectionIntegrityFailed,
    DiffFailed,
    NoSelectedChanges,
    PathPolicyFailed,
    SecretDetected,
    CacheBlocked,
    PreconditionFailed,
    ConflictedDelta,
    AmbiguousRename,
    PolicyFailed,
    PartialWriteBlocked,
}

impl CompatImportErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProjectionNotFound => "compat_projection_not_found",
            Self::ProjectionInvalid => "compat_projection_invalid",
            Self::ProjectionStale => "compat_projection_stale",
            Self::ProjectionIntegrityFailed => "compat_projection_integrity_failed",
            Self::DiffFailed => "compat_diff_failed",
            Self::NoSelectedChanges => "compat_no_selected_changes",
            Self::PathPolicyFailed => "compat_path_policy_failed",
            Self::SecretDetected => "compat_secret_detected",
            Self::CacheBlocked => "compat_cache_blocked",
            Self::PreconditionFailed => "compat_precondition_failed",
            Self::ConflictedDelta => "compat_conflicted_delta",
            Self::AmbiguousRename => "compat_ambiguous_rename",
            Self::PolicyFailed => "compat_policy_failed",
            Self::PartialWriteBlocked => "compat_partial_write_blocked",
        }
    }
}

pub fn fixture_basic_app_candidate_deltas() -> Vec<CompatCandidateDelta> {
    vec![
        CompatCandidateDelta {
            candidate_delta_id: "compat_delta_src_auth_ts_0001".to_string(),
            kind: CompatCandidateKind::ModifiedSource,
            operation_kind: CompatFileOperationKind::Patch,
            artifact_id: Some("artifact_src_auth_ts".to_string()),
            path: "src/auth.ts".to_string(),
            before_hash: Some("sha256:auth_base".to_string()),
            after_hash: Some("sha256:auth_projection_after".to_string()),
            byte_length: 109,
            executable: false,
            media_type: "text/typescript; charset=utf-8".to_string(),
            classification: "source".to_string(),
            privacy_class: PrivacyClass::PolicyGated,
            path_policy_result: CompatPathPolicyResult {
                allowed: true,
                normalized_path: Some("src/auth.ts".to_string()),
                reason: None,
            },
            quarantine_ref: None,
        },
        CompatCandidateDelta {
            candidate_delta_id: "compat_delta_src_auth_delete_0001".to_string(),
            kind: CompatCandidateKind::DeletedSource,
            operation_kind: CompatFileOperationKind::Delete,
            artifact_id: Some("artifact_src_auth_ts".to_string()),
            path: "src/auth.ts".to_string(),
            before_hash: Some("sha256:auth_base".to_string()),
            after_hash: None,
            byte_length: 109,
            executable: false,
            media_type: "text/typescript; charset=utf-8".to_string(),
            classification: "source".to_string(),
            privacy_class: PrivacyClass::PolicyGated,
            path_policy_result: CompatPathPolicyResult {
                allowed: true,
                normalized_path: Some("src/auth.ts".to_string()),
                reason: None,
            },
            quarantine_ref: None,
        },
        CompatCandidateDelta {
            candidate_delta_id: "compat_delta_auth_rename_ambiguous_0001".to_string(),
            kind: CompatCandidateKind::MovedOrRenamed,
            operation_kind: CompatFileOperationKind::Move,
            artifact_id: Some("artifact_src_auth_ts".to_string()),
            path: "src/auth-renamed.ts".to_string(),
            before_hash: Some("sha256:auth_base".to_string()),
            after_hash: Some("sha256:auth_projection_after".to_string()),
            byte_length: 109,
            executable: false,
            media_type: "text/typescript; charset=utf-8".to_string(),
            classification: "source".to_string(),
            privacy_class: PrivacyClass::PolicyGated,
            path_policy_result: CompatPathPolicyResult {
                allowed: true,
                normalized_path: Some("src/auth-renamed.ts".to_string()),
                reason: None,
            },
            quarantine_ref: None,
        },
        CompatCandidateDelta {
            candidate_delta_id: "compat_delta_src_session_ts_0001".to_string(),
            kind: CompatCandidateKind::CreatedSource,
            operation_kind: CompatFileOperationKind::Write,
            artifact_id: Some("artifact_src_session_ts".to_string()),
            path: "src/session.ts".to_string(),
            before_hash: None,
            after_hash: Some("sha256:session_projection_new".to_string()),
            byte_length: 44,
            executable: false,
            media_type: "text/typescript; charset=utf-8".to_string(),
            classification: "source".to_string(),
            privacy_class: PrivacyClass::PolicyGated,
            path_policy_result: CompatPathPolicyResult {
                allowed: true,
                normalized_path: Some("src/session.ts".to_string()),
                reason: None,
            },
            quarantine_ref: None,
        },
        CompatCandidateDelta {
            candidate_delta_id: "compat_delta_src_auth_conflict_0001".to_string(),
            kind: CompatCandidateKind::ConflictedDelta,
            operation_kind: CompatFileOperationKind::Patch,
            artifact_id: Some("artifact_src_auth_ts".to_string()),
            path: "src/auth.conflicted.ts".to_string(),
            before_hash: Some("sha256:auth_base".to_string()),
            after_hash: Some("sha256:auth_conflicted_projection_after".to_string()),
            byte_length: 121,
            executable: false,
            media_type: "text/typescript; charset=utf-8".to_string(),
            classification: "source".to_string(),
            privacy_class: PrivacyClass::PolicyGated,
            path_policy_result: CompatPathPolicyResult {
                allowed: true,
                normalized_path: Some("src/auth.conflicted.ts".to_string()),
                reason: None,
            },
            quarantine_ref: None,
        },
        CompatCandidateDelta {
            candidate_delta_id: "compat_delta_generated_schema_0001".to_string(),
            kind: CompatCandidateKind::GeneratedSource,
            operation_kind: CompatFileOperationKind::Write,
            artifact_id: None,
            path: "src/generated/schema.ts".to_string(),
            before_hash: None,
            after_hash: Some("sha256:generated_schema_projection".to_string()),
            byte_length: 512,
            executable: false,
            media_type: "text/typescript; charset=utf-8".to_string(),
            classification: "generated".to_string(),
            privacy_class: PrivacyClass::PolicyGated,
            path_policy_result: CompatPathPolicyResult {
                allowed: true,
                normalized_path: Some("src/generated/schema.ts".to_string()),
                reason: None,
            },
            quarantine_ref: None,
        },
        CompatCandidateDelta {
            candidate_delta_id: "compat_delta_dist_bundle_0001".to_string(),
            kind: CompatCandidateKind::CacheOrBuildOutput,
            operation_kind: CompatFileOperationKind::Write,
            artifact_id: None,
            path: "dist/bundle.js".to_string(),
            before_hash: None,
            after_hash: Some("sha256:dist_bundle_local".to_string()),
            byte_length: 2048,
            executable: false,
            media_type: "application/javascript".to_string(),
            classification: "cache".to_string(),
            privacy_class: PrivacyClass::LocalOnly,
            path_policy_result: CompatPathPolicyResult {
                allowed: true,
                normalized_path: Some("dist/bundle.js".to_string()),
                reason: None,
            },
            quarantine_ref: None,
        },
        CompatCandidateDelta {
            candidate_delta_id: "compat_delta_env_secret_0001".to_string(),
            kind: CompatCandidateKind::SecretLike,
            operation_kind: CompatFileOperationKind::Write,
            artifact_id: None,
            path: ".env".to_string(),
            before_hash: None,
            after_hash: Some("sha256:env_secret_local".to_string()),
            byte_length: 32,
            executable: false,
            media_type: "text/plain; charset=utf-8".to_string(),
            classification: "secret".to_string(),
            privacy_class: PrivacyClass::Secret,
            path_policy_result: CompatPathPolicyResult {
                allowed: true,
                normalized_path: Some(".env".to_string()),
                reason: None,
            },
            quarantine_ref: Some(
                "quarantine://compat/projection_compat_agent_a_0001/env".to_string(),
            ),
        },
        CompatCandidateDelta {
            candidate_delta_id: "compat_delta_reserved_sunlight_0001".to_string(),
            kind: CompatCandidateKind::PathPolicyBlocked,
            operation_kind: CompatFileOperationKind::Write,
            artifact_id: None,
            path: ".sunlight/config.toml".to_string(),
            before_hash: None,
            after_hash: Some("sha256:reserved_sunlight_config".to_string()),
            byte_length: 8,
            executable: false,
            media_type: "text/plain; charset=utf-8".to_string(),
            classification: "policy".to_string(),
            privacy_class: PrivacyClass::LocalOnly,
            path_policy_result: CompatPathPolicyResult {
                allowed: false,
                normalized_path: None,
                reason: Some("reserved_path".to_string()),
            },
            quarantine_ref: None,
        },
    ]
}

pub fn plan_fixture_basic_app_import(
    projection: &ProjectionRecord,
    current_view: &ResolvedViewResult,
    request: CompatImportRequest,
    candidates: &[CompatCandidateDelta],
) -> Result<CompatImportResponse, CompatImportValidationError> {
    validate_projection(projection, current_view, &request)?;
    validate_request_preconditions(current_view, &request)?;

    let selected = select_candidates(&request, candidates)?;
    validate_selected_candidates(current_view, &request, &selected)?;

    Ok(build_response(projection, current_view, request, selected))
}

fn validate_projection(
    projection: &ProjectionRecord,
    current_view: &ResolvedViewResult,
    request: &CompatImportRequest,
) -> Result<(), CompatImportValidationError> {
    if projection.id != request.projection_id {
        return Err(error(
            CompatImportErrorCode::ProjectionNotFound,
            &request.projection_id,
            &request.session_id,
            Vec::new(),
            "projection id does not match supplied projection record",
        ));
    }
    if projection.purpose != ProjectionPurpose::Compatibility {
        return Err(error(
            CompatImportErrorCode::ProjectionInvalid,
            &request.projection_id,
            &request.session_id,
            Vec::new(),
            "projection purpose must be compatibility",
        ));
    }
    if projection.baseline_manifest_ref.is_none() {
        return Err(error(
            CompatImportErrorCode::ProjectionInvalid,
            &request.projection_id,
            &request.session_id,
            Vec::new(),
            "compatibility projection requires a baseline manifest",
        ));
    }

    let Some(tree_identity) = &current_view.tree_identity else {
        return Err(error(
            CompatImportErrorCode::ProjectionStale,
            &request.projection_id,
            &request.session_id,
            Vec::new(),
            "current view has no tree identity",
        ));
    };

    if projection.resolved_view_id != current_view.resolved_view_id
        || projection.tree_identity != *tree_identity
        || projection.path_policy_id != current_view.path_policy_id
        || projection.operation_semantics_version != current_view.operation_semantics_version
    {
        return Err(error(
            CompatImportErrorCode::ProjectionStale,
            &request.projection_id,
            &request.session_id,
            Vec::new(),
            "projection baseline does not match the supplied current view",
        ));
    }

    Ok(())
}

fn validate_request_preconditions(
    current_view: &ResolvedViewResult,
    request: &CompatImportRequest,
) -> Result<(), CompatImportValidationError> {
    if request.selected_candidate_delta_ids.is_empty() {
        return Err(error(
            CompatImportErrorCode::NoSelectedChanges,
            &request.projection_id,
            &request.session_id,
            Vec::new(),
            "no candidate deltas were selected",
        ));
    }
    if request.session_id != FIXTURE_SESSION_ID {
        return Err(error(
            CompatImportErrorCode::PreconditionFailed,
            &request.projection_id,
            &request.session_id,
            Vec::new(),
            "fixture import requires the fixture session",
        ));
    }
    if request.write_topic_id != FIXTURE_WRITE_TOPIC_ID {
        return Err(error(
            CompatImportErrorCode::PreconditionFailed,
            &request.projection_id,
            &request.session_id,
            Vec::new(),
            "fixture import requires exactly one fixture write topic",
        ));
    }
    if request.session_generation_id != "gen_agent_a_0001" {
        return Err(error(
            CompatImportErrorCode::PreconditionFailed,
            &request.projection_id,
            &request.session_id,
            Vec::new(),
            "session generation does not match the fixture current generation",
        ));
    }
    if request.resolved_view_id != current_view.resolved_view_id {
        return Err(error(
            CompatImportErrorCode::PreconditionFailed,
            &request.projection_id,
            &request.session_id,
            Vec::new(),
            "resolved view precondition does not match the supplied current view",
        ));
    }
    if current_view.operation_semantics_version != FILE_OPERATION_SEMANTICS_VERSION {
        return Err(error(
            CompatImportErrorCode::PreconditionFailed,
            &request.projection_id,
            &request.session_id,
            Vec::new(),
            "fixture import requires file_ops_v1 operation semantics",
        ));
    }

    Ok(())
}

fn select_candidates<'a>(
    request: &CompatImportRequest,
    candidates: &'a [CompatCandidateDelta],
) -> Result<Vec<&'a CompatCandidateDelta>, CompatImportValidationError> {
    let by_id = candidates
        .iter()
        .map(|candidate| (candidate.candidate_delta_id.as_str(), candidate))
        .collect::<BTreeMap<_, _>>();
    let mut selected = Vec::new();
    let mut missing = Vec::new();
    let mut seen = BTreeSet::new();

    for id in &request.selected_candidate_delta_ids {
        if !seen.insert(id.as_str()) {
            continue;
        }
        match by_id.get(id.as_str()) {
            Some(candidate) => selected.push(*candidate),
            None => missing.push(id.clone()),
        }
    }

    if !missing.is_empty() {
        return Err(error(
            CompatImportErrorCode::DiffFailed,
            &request.projection_id,
            &request.session_id,
            missing,
            "selected candidate delta was not present in fixture diff output",
        ));
    }

    Ok(selected)
}

fn validate_selected_candidates(
    current_view: &ResolvedViewResult,
    request: &CompatImportRequest,
    selected: &[&CompatCandidateDelta],
) -> Result<(), CompatImportValidationError> {
    let path_policy = PathPolicy {
        id: current_view.path_policy_id.clone(),
    };

    for candidate in selected {
        let candidate_ids = vec![candidate.candidate_delta_id.clone()];
        if !candidate.path_policy_result.allowed {
            return Err(error(
                CompatImportErrorCode::PathPolicyFailed,
                &request.projection_id,
                &request.session_id,
                candidate_ids,
                candidate
                    .path_policy_result
                    .reason
                    .clone()
                    .unwrap_or_else(|| {
                        "selected candidate is blocked by recorded path policy result".to_string()
                    }),
            ));
        }
        if let Err(path_error) = path_policy.validate(&candidate.path) {
            return Err(error(
                CompatImportErrorCode::PathPolicyFailed,
                &request.projection_id,
                &request.session_id,
                candidate_ids,
                path_error_message(&path_error),
            ));
        }
        match candidate.kind {
            CompatCandidateKind::SecretLike => {
                return Err(error(
                    CompatImportErrorCode::SecretDetected,
                    &request.projection_id,
                    &request.session_id,
                    candidate_ids,
                    "secret-like candidate cannot be imported as source",
                ));
            }
            CompatCandidateKind::CacheOrBuildOutput | CompatCandidateKind::IgnoredPath => {
                return Err(error(
                    CompatImportErrorCode::CacheBlocked,
                    &request.projection_id,
                    &request.session_id,
                    candidate_ids,
                    "cache, build, and ignored candidates are blocked by default",
                ));
            }
            CompatCandidateKind::PathPolicyBlocked => {
                return Err(error(
                    CompatImportErrorCode::PathPolicyFailed,
                    &request.projection_id,
                    &request.session_id,
                    candidate_ids,
                    "path-policy-blocked candidate cannot be imported",
                ));
            }
            CompatCandidateKind::ConflictedDelta => {
                return Err(error(
                    CompatImportErrorCode::ConflictedDelta,
                    &request.projection_id,
                    &request.session_id,
                    candidate_ids,
                    "conflicted candidate cannot be imported",
                ));
            }
            CompatCandidateKind::GeneratedSource | CompatCandidateKind::BinaryOrLarge => {
                return Err(error(
                    CompatImportErrorCode::PolicyFailed,
                    &request.projection_id,
                    &request.session_id,
                    candidate_ids,
                    "generated or binary candidates require an explicit policy conversion",
                ));
            }
            CompatCandidateKind::MovedOrRenamed => {
                return Err(error(
                    CompatImportErrorCode::AmbiguousRename,
                    &request.projection_id,
                    &request.session_id,
                    candidate_ids,
                    "fixture foundation does not resolve rename identity",
                ));
            }
            CompatCandidateKind::ModifiedSource
            | CompatCandidateKind::CreatedSource
            | CompatCandidateKind::DeletedSource
            | CompatCandidateKind::MetadataChanged => {}
        }

        validate_candidate_precondition(current_view, request, candidate)?;
    }

    Ok(())
}

fn validate_candidate_precondition(
    current_view: &ResolvedViewResult,
    request: &CompatImportRequest,
    candidate: &CompatCandidateDelta,
) -> Result<(), CompatImportValidationError> {
    let active_entry = current_view.tree_entries.get(&candidate.path);
    match (&candidate.before_hash, active_entry) {
        (Some(expected), Some(entry)) if entry.content_hash == *expected => Ok(()),
        (Some(expected), Some(entry)) => Err(error(
            CompatImportErrorCode::PreconditionFailed,
            &request.projection_id,
            &request.session_id,
            vec![candidate.candidate_delta_id.clone()],
            format!(
                "candidate before hash `{expected}` does not match current hash `{}`",
                entry.content_hash
            ),
        )),
        (Some(_), None) => Err(error(
            CompatImportErrorCode::PreconditionFailed,
            &request.projection_id,
            &request.session_id,
            vec![candidate.candidate_delta_id.clone()],
            "candidate expects an existing path that is absent in the current view",
        )),
        (None, Some(_)) => Err(error(
            CompatImportErrorCode::PreconditionFailed,
            &request.projection_id,
            &request.session_id,
            vec![candidate.candidate_delta_id.clone()],
            "candidate expects a new path that already exists in the current view",
        )),
        (None, None) => Ok(()),
    }
}

fn build_response(
    projection: &ProjectionRecord,
    current_view: &ResolvedViewResult,
    request: CompatImportRequest,
    selected: Vec<&CompatCandidateDelta>,
) -> CompatImportResponse {
    let before_tree_identity = current_view.tree_identity.clone().expect("validated tree");
    let after_tree_identity = SingleRepoTree {
        repository_id: current_view.repository_id.clone(),
        tree_hash: FIXTURE_COMPAT_IMPORT_TREE_HASH.to_string(),
    };
    let selected_ids = request.selected_candidate_delta_ids.clone();
    let imported_artifacts = selected
        .iter()
        .map(|candidate| CompatImportedArtifact {
            candidate_delta_id: candidate.candidate_delta_id.clone(),
            artifact_id: artifact_id_for_candidate(candidate),
            path: candidate.path.clone(),
            operation_kind: candidate.operation_kind,
            before_hash: candidate.before_hash.clone(),
            after_hash: candidate.after_hash.clone(),
            classification: candidate.classification.clone(),
            privacy_class: candidate.privacy_class,
        })
        .collect::<Vec<_>>();
    let write_set = selected
        .iter()
        .map(|candidate| WriteSetEntry {
            artifact_id: artifact_id_for_candidate(candidate),
            path: candidate.path.clone(),
            mutation: candidate.operation_kind.mutation_kind(),
        })
        .collect::<Vec<_>>();
    let selected_deltas = selected
        .iter()
        .map(|candidate| CompatSelectedDeltaPlan {
            candidate_delta_id: candidate.candidate_delta_id.clone(),
            operation_kind: candidate.operation_kind,
            path: candidate.path.clone(),
            patch_digest: (candidate.operation_kind == CompatFileOperationKind::Patch)
                .then(|| format!("sha256:{}_patch", candidate.candidate_delta_id)),
            base_content_hash: candidate.before_hash.clone(),
            result_content_hash: candidate.after_hash.clone(),
            classification: candidate.classification.clone(),
            privacy_class: candidate.privacy_class,
        })
        .collect::<Vec<_>>();
    let before_refs = selected
        .iter()
        .map(|candidate| MutationArtifactRef {
            artifact_id: candidate
                .before_hash
                .as_ref()
                .map(|_| artifact_id_for_candidate(candidate)),
            path: candidate.path.clone(),
            path_state: if candidate.before_hash.is_some() {
                "active".to_string()
            } else {
                "absent".to_string()
            },
            content_hash: candidate.before_hash.clone(),
            executable: candidate.before_hash.as_ref().map(|_| candidate.executable),
            classification: candidate
                .before_hash
                .as_ref()
                .map(|_| candidate.classification.clone()),
        })
        .collect::<Vec<_>>();
    let after_refs = selected
        .iter()
        .map(|candidate| MutationArtifactRef {
            artifact_id: Some(artifact_id_for_candidate(candidate)),
            path: candidate.path.clone(),
            path_state: match candidate.operation_kind {
                CompatFileOperationKind::Delete => "tombstone",
                _ => "active",
            }
            .to_string(),
            content_hash: candidate.after_hash.clone(),
            executable: Some(candidate.executable),
            classification: Some(candidate.classification.clone()),
        })
        .collect::<Vec<_>>();

    let preconditions = CompatImportPreconditions {
        projection_id: projection.id.clone(),
        projection_purpose: projection.purpose,
        projection_baseline_resolved_view_id: projection.resolved_view_id.clone(),
        projection_baseline_tree_identity: projection.tree_identity.clone(),
        session_id: request.session_id.clone(),
        session_generation_id: request.session_generation_id.clone(),
        resolved_view_id: request.resolved_view_id.clone(),
        write_topic_id: request.write_topic_id.clone(),
        parent_topic_revision_id: request.parent_topic_revision_id.clone(),
        path_policy_id: projection.path_policy_id.clone(),
        operation_semantics_version: projection.operation_semantics_version.clone(),
        selected_candidate_delta_ids: selected_ids,
    };
    let read_set = CompatImportReadSet {
        mode: "projection_baseline".to_string(),
        resolved_view_id: projection.resolved_view_id.clone(),
        projection_id: projection.id.clone(),
    };
    let operation = CompatOperationTransactionPlan {
        id: FIXTURE_COMPAT_IMPORT_OPERATION_ID.to_string(),
        repository_id: current_view.repository_id.clone(),
        topic_id: request.write_topic_id.clone(),
        session_id: request.session_id.clone(),
        session_generation_id: request.session_generation_id.clone(),
        actor_id: FIXTURE_ACTOR_ID.to_string(),
        authored_context_id: FIXTURE_COMPAT_IMPORT_CONTEXT_ID.to_string(),
        preconditions,
        read_set,
        write_set,
        mutation_payload: CompatImportMutationPayload {
            kind: "compat_import".to_string(),
            projection_id: projection.id.clone(),
            baseline_manifest_digest: FIXTURE_COMPAT_BASELINE_MANIFEST_DIGEST.to_string(),
            selected_deltas,
        },
        before_refs: MutationRefs {
            artifacts: before_refs,
            tree_identity: tree_identity_view(&before_tree_identity),
        },
        after_refs: MutationRefs {
            artifacts: after_refs,
            tree_identity: tree_identity_view(&after_tree_identity),
        },
        classification: "source".to_string(),
        parent_topic_revision_id: request.parent_topic_revision_id.clone(),
        next_topic_revision_number: 1,
        parents: request.parent_topic_revision_id.iter().cloned().collect(),
    };
    let topic_revision = CompatTopicRevisionPlan {
        id: FIXTURE_COMPAT_IMPORT_TOPIC_REVISION_ID.to_string(),
        repository_id: current_view.repository_id.clone(),
        topic_id: request.write_topic_id.clone(),
        revision_number: 1,
        parent_revision_id: request.parent_topic_revision_id.clone(),
        operation_transaction_id: FIXTURE_COMPAT_IMPORT_OPERATION_ID.to_string(),
        tree_delta_ref: "delta_compat_import_0001".to_string(),
        dependency_revision_ids: Vec::new(),
    };
    let session_generation = CompatSessionGenerationPlan {
        id: FIXTURE_COMPAT_IMPORT_SESSION_GENERATION_ID.to_string(),
        repository_id: current_view.repository_id.clone(),
        session_id: request.session_id.clone(),
        write_topic_id: request.write_topic_id.clone(),
        base_resolved_view_id: projection.resolved_view_id.clone(),
        resolved_view_id: FIXTURE_COMPAT_IMPORT_RESOLVED_VIEW_ID.to_string(),
        topic_frontier: BTreeMap::from([(
            request.write_topic_id.clone(),
            FIXTURE_COMPAT_IMPORT_TOPIC_REVISION_ID.to_string(),
        )]),
        generation_number: 2,
        refresh_policy: "pinned_except_own_topic".to_string(),
        created_by_operation_id: FIXTURE_COMPAT_IMPORT_OPERATION_ID.to_string(),
    };

    CompatImportResponse {
        command: "compat.import",
        repository_id: current_view.repository_id.clone(),
        projection_id: projection.id.clone(),
        session_id: request.session_id,
        operation_id: FIXTURE_COMPAT_IMPORT_OPERATION_ID.to_string(),
        topic_revision_id: FIXTURE_COMPAT_IMPORT_TOPIC_REVISION_ID.to_string(),
        session_generation_id: FIXTURE_COMPAT_IMPORT_SESSION_GENERATION_ID.to_string(),
        resolved_view_id: FIXTURE_COMPAT_IMPORT_RESOLVED_VIEW_ID.to_string(),
        tree_identity: after_tree_identity,
        imported_artifacts,
        ignored_candidate_delta_ids: Vec::new(),
        quarantine_refs: Vec::new(),
        plan: CompatImportPlan {
            operation,
            topic_revision,
            session_generation,
        },
    }
}

fn artifact_id_for_candidate(candidate: &CompatCandidateDelta) -> String {
    candidate
        .artifact_id
        .clone()
        .unwrap_or_else(|| format!("artifact_{}", candidate.path.replace(['/', '.'], "_")))
}

fn tree_identity_view(tree_identity: &SingleRepoTree) -> crate::artifacts::TreeIdentityView {
    crate::artifacts::TreeIdentityView {
        kind: "SingleRepoTree".to_string(),
        repository_id: tree_identity.repository_id.clone(),
        tree_hash: tree_identity.tree_hash.clone(),
    }
}

fn path_error_message(error: &ArtifactIoError) -> String {
    match error {
        ArtifactIoError::PathPolicyViolation { reason, .. } => {
            format!("path policy failed: {}", reason.as_str())
        }
        _ => error.to_string(),
    }
}

fn error(
    code: CompatImportErrorCode,
    projection_id: &str,
    session_id: &str,
    candidate_delta_ids: Vec<String>,
    message: impl Into<String>,
) -> CompatImportValidationError {
    CompatImportValidationError {
        code,
        projection_id: projection_id.to_string(),
        session_id: session_id.to_string(),
        candidate_delta_ids,
        message: message.into(),
    }
}

#[allow(dead_code)]
fn mutation_precondition_shape(preconditions: &CompatImportPreconditions) -> MutationPreconditions {
    MutationPreconditions {
        resolved_view_id: preconditions.resolved_view_id.clone(),
        session_generation_id: preconditions.session_generation_id.clone(),
        write_topic_id: preconditions.write_topic_id.clone(),
        parent_topic_revision_id: preconditions.parent_topic_revision_id.clone(),
        path_policy_id: preconditions.path_policy_id.clone(),
        operation_semantics_version: preconditions.operation_semantics_version.clone(),
        expected_path: String::new(),
        expected_hash: ExpectedHash::New,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::FIXTURE_REPOSITORY_ID;
    use crate::projection::fixture_compatibility_projection_from_resolved_view;
    use crate::resolver::{fixture_base_entries, fixture_resolver_input, resolve_fixture_view};

    #[test]
    fn fixture_import_modified_file_creates_one_operation_plan() {
        let view = base_view();
        let projection =
            fixture_compatibility_projection_from_resolved_view(&view, "gen_agent_a_0001").unwrap();
        let response = plan_fixture_basic_app_import(
            &projection,
            &view,
            request(vec!["compat_delta_src_auth_ts_0001"]),
            &fixture_basic_app_candidate_deltas(),
        )
        .unwrap();

        assert_eq!(response.command, "compat.import");
        assert_eq!(response.operation_id, FIXTURE_COMPAT_IMPORT_OPERATION_ID);
        assert_eq!(
            response.topic_revision_id,
            FIXTURE_COMPAT_IMPORT_TOPIC_REVISION_ID
        );
        assert_eq!(response.imported_artifacts.len(), 1);
        assert_eq!(
            response.imported_artifacts[0].artifact_id,
            "artifact_src_auth_ts"
        );
        assert_eq!(
            response.plan.operation.mutation_payload.kind,
            "compat_import"
        );
        assert_eq!(response.plan.operation.write_set.len(), 1);
        assert_eq!(
            response
                .plan
                .operation
                .preconditions
                .selected_candidate_delta_ids,
            vec!["compat_delta_src_auth_ts_0001"]
        );
        assert_eq!(response.plan.operation.read_set.mode, "projection_baseline");
        assert_eq!(response.plan.topic_revision.revision_number, 1);
        assert_eq!(response.plan.session_generation.generation_number, 2);
    }

    #[test]
    fn fixture_import_multiple_safe_deltas_is_one_transaction() {
        let view = base_view();
        let projection =
            fixture_compatibility_projection_from_resolved_view(&view, "gen_agent_a_0001").unwrap();
        let response = plan_fixture_basic_app_import(
            &projection,
            &view,
            request(vec![
                "compat_delta_src_auth_ts_0001",
                "compat_delta_src_session_ts_0001",
            ]),
            &fixture_basic_app_candidate_deltas(),
        )
        .unwrap();

        assert_eq!(response.operation_id, FIXTURE_COMPAT_IMPORT_OPERATION_ID);
        assert_eq!(response.imported_artifacts.len(), 2);
        assert_eq!(response.plan.operation.write_set.len(), 2);
        assert_eq!(
            response
                .plan
                .operation
                .mutation_payload
                .selected_deltas
                .len(),
            2
        );
        assert_eq!(
            response.plan.operation.after_refs.tree_identity.tree_hash,
            FIXTURE_COMPAT_IMPORT_TREE_HASH
        );
    }

    #[test]
    fn import_rejects_non_compatibility_projection() {
        let view = base_view();
        let mut projection =
            fixture_compatibility_projection_from_resolved_view(&view, "gen_agent_a_0001").unwrap();
        projection.purpose = ProjectionPurpose::Execution;

        let error = plan_fixture_basic_app_import(
            &projection,
            &view,
            request(vec!["compat_delta_src_auth_ts_0001"]),
            &fixture_basic_app_candidate_deltas(),
        )
        .unwrap_err();

        assert_eq!(error.code, CompatImportErrorCode::ProjectionInvalid);
        assert_eq!(error.code.as_str(), "compat_projection_invalid");
    }

    #[test]
    fn import_rejects_no_selected_changes() {
        let view = base_view();
        let projection =
            fixture_compatibility_projection_from_resolved_view(&view, "gen_agent_a_0001").unwrap();

        let error = plan_fixture_basic_app_import(
            &projection,
            &view,
            request(Vec::new()),
            &fixture_basic_app_candidate_deltas(),
        )
        .unwrap_err();

        assert_eq!(error.code, CompatImportErrorCode::NoSelectedChanges);
    }

    #[test]
    fn import_rejects_stale_session_generation() {
        let view = base_view();
        let projection =
            fixture_compatibility_projection_from_resolved_view(&view, "gen_agent_a_0001").unwrap();
        let mut request = request(vec!["compat_delta_src_auth_ts_0001"]);
        request.session_generation_id = "gen_agent_a_9999".to_string();

        let error = plan_fixture_basic_app_import(
            &projection,
            &view,
            request,
            &fixture_basic_app_candidate_deltas(),
        )
        .unwrap_err();

        assert_eq!(error.code, CompatImportErrorCode::PreconditionFailed);
    }

    #[test]
    fn import_rejects_path_policy_failure_before_planning() {
        let view = base_view();
        let projection =
            fixture_compatibility_projection_from_resolved_view(&view, "gen_agent_a_0001").unwrap();
        let mut candidates = fixture_basic_app_candidate_deltas();
        candidates.push(CompatCandidateDelta {
            candidate_delta_id: "compat_delta_reserved_sunlight_0001".to_string(),
            kind: CompatCandidateKind::CreatedSource,
            operation_kind: CompatFileOperationKind::Write,
            artifact_id: None,
            path: ".sunlight/config.toml".to_string(),
            before_hash: None,
            after_hash: Some("sha256:reserved".to_string()),
            byte_length: 8,
            executable: false,
            media_type: "text/plain; charset=utf-8".to_string(),
            classification: "source".to_string(),
            privacy_class: PrivacyClass::PolicyGated,
            path_policy_result: CompatPathPolicyResult {
                allowed: false,
                normalized_path: None,
                reason: Some("reserved_path".to_string()),
            },
            quarantine_ref: None,
        });

        let error = plan_fixture_basic_app_import(
            &projection,
            &view,
            request(vec!["compat_delta_reserved_sunlight_0001"]),
            &candidates,
        )
        .unwrap_err();

        assert_eq!(error.code, CompatImportErrorCode::PathPolicyFailed);
    }

    #[test]
    fn import_rejects_secret_and_cache_candidates_atomically() {
        let view = base_view();
        let projection =
            fixture_compatibility_projection_from_resolved_view(&view, "gen_agent_a_0001").unwrap();

        let secret_error = plan_fixture_basic_app_import(
            &projection,
            &view,
            request(vec![
                "compat_delta_src_auth_ts_0001",
                "compat_delta_env_secret_0001",
            ]),
            &fixture_basic_app_candidate_deltas(),
        )
        .unwrap_err();
        assert_eq!(secret_error.code, CompatImportErrorCode::SecretDetected);

        let cache_error = plan_fixture_basic_app_import(
            &projection,
            &view,
            request(vec!["compat_delta_dist_bundle_0001"]),
            &fixture_basic_app_candidate_deltas(),
        )
        .unwrap_err();
        assert_eq!(cache_error.code, CompatImportErrorCode::CacheBlocked);
    }

    #[test]
    fn import_rejects_new_file_when_path_exists_in_current_view() {
        let mut view = base_view();
        view.tree_entries.insert(
            "src/session.ts".to_string(),
            crate::resolver::TreeEntryState {
                artifact_id: "artifact_existing_session".to_string(),
                path: "src/session.ts".to_string(),
                content_hash: "sha256:existing_session".to_string(),
            },
        );
        let projection =
            fixture_compatibility_projection_from_resolved_view(&view, "gen_agent_a_0001").unwrap();

        let error = plan_fixture_basic_app_import(
            &projection,
            &view,
            request(vec!["compat_delta_src_session_ts_0001"]),
            &fixture_basic_app_candidate_deltas(),
        )
        .unwrap_err();

        assert_eq!(error.code, CompatImportErrorCode::PreconditionFailed);
    }

    fn base_view() -> ResolvedViewResult {
        resolve_fixture_view(
            fixture_resolver_input(Vec::new()),
            fixture_base_entries(),
            [],
        )
    }

    fn request(selected: Vec<&str>) -> CompatImportRequest {
        let view = base_view();
        CompatImportRequest {
            projection_id: "projection_compat_agent_a_0001".to_string(),
            session_id: FIXTURE_SESSION_ID.to_string(),
            session_generation_id: "gen_agent_a_0001".to_string(),
            resolved_view_id: view.resolved_view_id,
            write_topic_id: FIXTURE_WRITE_TOPIC_ID.to_string(),
            parent_topic_revision_id: None,
            selected_candidate_delta_ids: selected.into_iter().map(str::to_string).collect(),
        }
    }

    #[test]
    fn constants_stay_on_fixture_repository() {
        assert_eq!(FIXTURE_REPOSITORY_ID, "repo_fixture_basic_app");
    }
}

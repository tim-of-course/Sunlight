use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::artifacts::{
    ArtifactIoError, ExpectedHash, MutationArtifactRef, MutationKind, MutationPreconditions,
    MutationRefs, PathPolicy, WriteSetEntry, FILE_OPERATION_SEMANTICS_VERSION, FIXTURE_ACTOR_ID,
    FIXTURE_SESSION_ID, FIXTURE_WRITE_TOPIC_ID, POSIX_CASE_SENSITIVE_PATH_POLICY_ID,
};
use crate::projection::{ProjectionPurpose, ProjectionRecord};
use crate::records::PrivacyClass;
use crate::repo_state::{
    detect_secret_reasons, real_content_hash, RealArtifactEntry, RealProjectionSnapshot,
};
use crate::resolver::{ResolvedViewResult, SingleRepoTree};

pub const FIXTURE_COMPAT_IMPORT_OPERATION_ID: &str = "op_compat_import_auth_0001";
pub const FIXTURE_COMPAT_IMPORT_TOPIC_REVISION_ID: &str = "rev_auth_nullability_compat_0001";
pub const FIXTURE_COMPAT_IMPORT_SESSION_GENERATION_ID: &str = "gen_agent_a_compat_0002";
pub const FIXTURE_COMPAT_IMPORT_RESOLVED_VIEW_ID: &str = "view_agent_a_after_compat_import_0001";
pub const FIXTURE_COMPAT_IMPORT_TREE_HASH: &str = "tree_after_compat_import_0001";
pub const FIXTURE_COMPAT_IMPORT_CONTEXT_ID: &str = "ctx_compat_projection_0001";
pub const FIXTURE_COMPAT_BASELINE_MANIFEST_DIGEST: &str = "sha256:compat_baseline";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RealCompatDiff {
    pub candidates: Vec<CompatCandidateDelta>,
    pub after_bytes: BTreeMap<String, Vec<u8>>,
}

pub fn real_compat_baseline_manifest_digest(
    repository_id: &str,
    projection_id: &str,
    session_id: &str,
    session_generation_id: &str,
    resolved_view_id: &str,
    tree_hash: &str,
    entries: &[RealArtifactEntry],
) -> String {
    let mut hasher = Sha256::new();
    for value in [
        repository_id,
        projection_id,
        session_id,
        session_generation_id,
        resolved_view_id,
        tree_hash,
        POSIX_CASE_SENSITIVE_PATH_POLICY_ID,
        FILE_OPERATION_SEMANTICS_VERSION,
    ] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    let mut entries = entries
        .iter()
        .filter(|entry| !entry.tombstone)
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    for entry in entries {
        hasher.update(entry.path.as_bytes());
        hasher.update([0]);
        hasher.update(entry.artifact_id.as_bytes());
        hasher.update([0]);
        hasher.update(entry.content_hash.as_bytes());
        hasher.update([u8::from(entry.executable)]);
        hasher.update(entry.classification.as_bytes());
        hasher.update([0]);
    }
    format!("sha256:{:x}", hasher.finalize())
}

pub fn diff_real_compat_projection(
    repo_root: &Path,
    managed_root: &Path,
    projection: &RealProjectionSnapshot,
) -> Result<RealCompatDiff, CompatImportValidationError> {
    let session_id = projection.session_id.as_deref().unwrap_or("");
    if projection.purpose != ProjectionPurpose::Compatibility.as_str() {
        return Err(real_error(
            CompatImportErrorCode::ProjectionInvalid,
            projection,
            Vec::new(),
            "projection purpose must be compatibility",
        ));
    }
    let Some(root_value) = projection.materialized_root.as_deref() else {
        return Err(real_error(
            CompatImportErrorCode::ProjectionInvalid,
            projection,
            Vec::new(),
            "compatibility projection has no persisted materialized root",
        ));
    };
    if session_id.is_empty() || projection.session_generation_id.is_none() {
        return Err(real_error(
            CompatImportErrorCode::ProjectionInvalid,
            projection,
            Vec::new(),
            "compatibility projection is not bound to a session generation",
        ));
    }

    let root = resolve_projection_root(repo_root, root_value);
    if fs::symlink_metadata(&root)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(real_error(
            CompatImportErrorCode::ProjectionInvalid,
            projection,
            Vec::new(),
            "compatibility projection root cannot be a symlink",
        ));
    }
    let canonical_root = fs::canonicalize(&root).map_err(|_| {
        real_error(
            CompatImportErrorCode::ProjectionNotFound,
            projection,
            Vec::new(),
            "compatibility projection root was not found",
        )
    })?;
    let canonical_managed = fs::canonicalize(managed_root).map_err(|_| {
        real_error(
            CompatImportErrorCode::ProjectionInvalid,
            projection,
            Vec::new(),
            "managed projection root was not found",
        )
    })?;
    if !canonical_root.starts_with(&canonical_managed) {
        return Err(real_error(
            CompatImportErrorCode::ProjectionInvalid,
            projection,
            Vec::new(),
            "compatibility projection root is outside the managed projection root",
        ));
    }
    let expected_root = managed_root
        .join("compat")
        .join(&projection.projection_id)
        .join("root");
    let canonical_expected = fs::canonicalize(&expected_root).map_err(|_| {
        real_error(
            CompatImportErrorCode::ProjectionInvalid,
            projection,
            Vec::new(),
            "configured compatibility projection root was not found",
        )
    })?;
    if canonical_root != canonical_expected {
        return Err(real_error(
            CompatImportErrorCode::ProjectionInvalid,
            projection,
            Vec::new(),
            "compatibility projection root does not match its configured managed subtree",
        ));
    }

    let expected_manifest = real_compat_baseline_manifest_digest(
        &projection.repository_id,
        &projection.projection_id,
        session_id,
        projection.session_generation_id.as_deref().unwrap_or(""),
        &projection.resolved_view_id,
        &projection.tree_hash,
        &projection.entries,
    );
    if projection.manifest_digest != expected_manifest {
        return Err(real_error(
            CompatImportErrorCode::ProjectionStale,
            projection,
            Vec::new(),
            "compatibility projection baseline manifest no longer matches persisted metadata",
        ));
    }

    let mut scanned = BTreeMap::new();
    scan_projection_files(&canonical_root, &canonical_root, &mut scanned).map_err(|message| {
        real_error(
            CompatImportErrorCode::DiffFailed,
            projection,
            Vec::new(),
            message,
        )
    })?;
    let baseline = projection
        .entries
        .iter()
        .filter(|entry| !entry.tombstone)
        .map(|entry| (entry.path.as_str(), entry))
        .collect::<BTreeMap<_, _>>();
    let mut candidates = Vec::new();
    let mut after_bytes = BTreeMap::new();

    for (path, entry) in &baseline {
        match scanned.remove(*path) {
            None => candidates.push(real_candidate(
                projection,
                CompatCandidateKind::DeletedSource,
                CompatFileOperationKind::Delete,
                Some(entry.artifact_id.clone()),
                (*path).to_string(),
                Some(entry.content_hash.clone()),
                None,
                entry.bytes.len() as u64,
                entry.executable,
                entry.classification.clone(),
                PrivacyClass::PolicyGated,
                CompatPathPolicyResult {
                    allowed: true,
                    normalized_path: Some((*path).to_string()),
                    reason: None,
                },
                None,
            )),
            Some(file) if file.blocked_reason.is_some() => {
                candidates.push(blocked_candidate(projection, *path, file));
            }
            Some(file) => {
                let after_hash = real_content_hash(&file.bytes);
                if after_hash != entry.content_hash || file.executable != entry.executable {
                    let (kind, classification, privacy, quarantine, policy) =
                        classify_projection_path(projection, path, &file.bytes);
                    let operation_kind = if after_hash == entry.content_hash {
                        CompatFileOperationKind::Metadata
                    } else {
                        CompatFileOperationKind::Patch
                    };
                    let candidate_kind = if operation_kind == CompatFileOperationKind::Metadata {
                        CompatCandidateKind::MetadataChanged
                    } else if kind == CompatCandidateKind::CreatedSource {
                        CompatCandidateKind::ModifiedSource
                    } else {
                        kind
                    };
                    let candidate = real_candidate(
                        projection,
                        candidate_kind,
                        operation_kind,
                        Some(entry.artifact_id.clone()),
                        (*path).to_string(),
                        Some(entry.content_hash.clone()),
                        Some(after_hash),
                        file.bytes.len() as u64,
                        file.executable,
                        classification,
                        privacy,
                        policy,
                        quarantine,
                    );
                    after_bytes.insert(candidate.candidate_delta_id.clone(), file.bytes);
                    candidates.push(candidate);
                }
            }
        }
    }

    for (path, file) in scanned {
        if file.blocked_reason.is_some() {
            candidates.push(blocked_candidate(projection, &path, file));
            continue;
        }
        let after_hash = real_content_hash(&file.bytes);
        let (kind, classification, privacy, quarantine, policy) =
            classify_projection_path(projection, &path, &file.bytes);
        let candidate = real_candidate(
            projection,
            kind,
            CompatFileOperationKind::Write,
            None,
            path,
            None,
            Some(after_hash),
            file.bytes.len() as u64,
            file.executable,
            classification,
            privacy,
            policy,
            quarantine,
        );
        after_bytes.insert(candidate.candidate_delta_id.clone(), file.bytes);
        candidates.push(candidate);
    }
    infer_exact_content_renames(projection, &mut candidates, &mut after_bytes);
    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(RealCompatDiff {
        candidates,
        after_bytes,
    })
}

pub fn validate_real_compat_selection(
    projection: &RealProjectionSnapshot,
    selected_candidate_delta_ids: &[String],
    diff: &RealCompatDiff,
) -> Result<Vec<CompatCandidateDelta>, CompatImportValidationError> {
    if selected_candidate_delta_ids.is_empty() {
        return Err(real_error(
            CompatImportErrorCode::NoSelectedChanges,
            projection,
            Vec::new(),
            "no candidate deltas were selected",
        ));
    }
    let by_id = diff
        .candidates
        .iter()
        .map(|candidate| (candidate.candidate_delta_id.as_str(), candidate))
        .collect::<BTreeMap<_, _>>();
    let mut selected = Vec::new();
    let mut missing = Vec::new();
    let mut seen = BTreeSet::new();
    for id in selected_candidate_delta_ids {
        if !seen.insert(id.as_str()) {
            continue;
        }
        if let Some(candidate) = by_id.get(id.as_str()) {
            selected.push((*candidate).clone());
        } else {
            missing.push(id.clone());
        }
    }
    if !missing.is_empty() {
        return Err(real_error(
            CompatImportErrorCode::DiffFailed,
            projection,
            missing,
            "selected compatibility candidate was not present in the current projection diff",
        ));
    }
    for candidate in &selected {
        let ids = vec![candidate.candidate_delta_id.clone()];
        if !candidate.path_policy_result.allowed
            || candidate.kind == CompatCandidateKind::PathPolicyBlocked
        {
            return Err(real_error(
                CompatImportErrorCode::PathPolicyFailed,
                projection,
                ids,
                "selected compatibility candidate violates path policy",
            ));
        }
        match candidate.kind {
            CompatCandidateKind::SecretLike => {
                return Err(real_error(
                    CompatImportErrorCode::SecretDetected,
                    projection,
                    ids,
                    "selected compatibility candidate contains secret-like bytes",
                ));
            }
            CompatCandidateKind::CacheOrBuildOutput | CompatCandidateKind::IgnoredPath => {
                return Err(real_error(
                    CompatImportErrorCode::CacheBlocked,
                    projection,
                    ids,
                    "selected compatibility candidate is cache, build, or editor output",
                ));
            }
            CompatCandidateKind::GeneratedSource | CompatCandidateKind::BinaryOrLarge => {
                return Err(real_error(
                    CompatImportErrorCode::PolicyFailed,
                    projection,
                    ids,
                    "selected compatibility candidate requires an explicit policy conversion",
                ));
            }
            CompatCandidateKind::ConflictedDelta => {
                return Err(real_error(
                    CompatImportErrorCode::ConflictedDelta,
                    projection,
                    ids,
                    "selected compatibility candidate is conflicted",
                ));
            }
            CompatCandidateKind::MovedOrRenamed => {
                if candidate.source_path.is_none() {
                    return Err(real_error(
                        CompatImportErrorCode::AmbiguousRename,
                        projection,
                        ids,
                        "selected compatibility rename has multiple possible sources or targets",
                    ));
                }
                if candidate.before_hash != candidate.after_hash {
                    return Err(real_error(
                        CompatImportErrorCode::AmbiguousRename,
                        projection,
                        ids,
                        "rename-plus-edit identity is unresolved without a reliable identity signal",
                    ));
                }
            }
            _ => {}
        }
    }
    Ok(selected)
}

fn infer_exact_content_renames(
    projection: &RealProjectionSnapshot,
    candidates: &mut Vec<CompatCandidateDelta>,
    after_bytes: &mut BTreeMap<String, Vec<u8>>,
) {
    // Projection snapshots have no stable file identity beyond the baseline artifact and bytes.
    // Exact hashes are therefore the only safe signal here; changed-content renames stay split.
    let mut deleted_by_hash = BTreeMap::<String, Vec<usize>>::new();
    let mut created_by_hash = BTreeMap::<String, Vec<usize>>::new();
    for (index, candidate) in candidates.iter().enumerate() {
        if candidate.kind == CompatCandidateKind::DeletedSource {
            if let Some(hash) = &candidate.before_hash {
                deleted_by_hash.entry(hash.clone()).or_default().push(index);
            }
        } else if candidate.before_hash.is_none()
            && candidate.operation_kind == CompatFileOperationKind::Write
        {
            if let Some(hash) = &candidate.after_hash {
                created_by_hash.entry(hash.clone()).or_default().push(index);
            }
        }
    }

    let mut consumed = BTreeSet::new();
    let mut inferred = Vec::new();
    for (hash, deleted_indices) in deleted_by_hash {
        let Some(created_indices) = created_by_hash.get(&hash) else {
            continue;
        };
        let unambiguous = deleted_indices.len() == 1 && created_indices.len() == 1;
        let sole_target_is_safe_source = unambiguous
            && candidates[created_indices[0]].kind == CompatCandidateKind::CreatedSource
            && candidates[deleted_indices[0]].classification == "source";
        if unambiguous && !sole_target_is_safe_source {
            continue;
        }

        let source_paths = deleted_indices
            .iter()
            .map(|index| candidates[*index].path.clone())
            .collect::<Vec<_>>();
        for target_index in created_indices {
            let target = &candidates[*target_index];
            let source = sole_target_is_safe_source.then(|| &candidates[deleted_indices[0]]);
            let identity = format!(
                "{}\0{}\0{}\0{}\0{}\0{}",
                projection.projection_id,
                source_paths.join("\0"),
                target.path,
                hash,
                target.classification,
                if unambiguous { "exact" } else { "ambiguous" },
            );
            let digest = format!("{:x}", Sha256::digest(identity.as_bytes()));
            let mut rename = target.clone();
            rename.candidate_delta_id = format!("compat_delta_{}", &digest[..24]);
            rename.kind = CompatCandidateKind::MovedOrRenamed;
            rename.operation_kind = CompatFileOperationKind::Move;
            rename.artifact_id = source.and_then(|candidate| candidate.artifact_id.clone());
            rename.source_path = source.map(|candidate| candidate.path.clone());
            rename.before_hash = Some(hash.clone());
            if let Some(bytes) = after_bytes.remove(&target.candidate_delta_id) {
                after_bytes.insert(rename.candidate_delta_id.clone(), bytes);
            }
            inferred.push(rename);
            consumed.insert(*target_index);
        }
        consumed.extend(deleted_indices);
    }

    if consumed.is_empty() {
        return;
    }
    let mut index = 0usize;
    candidates.retain(|_| {
        let keep = !consumed.contains(&index);
        index += 1;
        keep
    });
    candidates.extend(inferred);
}

#[derive(Debug)]
struct ScannedProjectionFile {
    bytes: Vec<u8>,
    executable: bool,
    blocked_reason: Option<String>,
}

fn resolve_projection_root(repo_root: &Path, root: &str) -> PathBuf {
    let root = PathBuf::from(root);
    if root.is_absolute() {
        root
    } else {
        repo_root.join(root)
    }
}

fn scan_projection_files(
    root: &Path,
    current: &Path,
    files: &mut BTreeMap<String, ScannedProjectionFile>,
) -> Result<(), String> {
    let children = fs::read_dir(current)
        .map_err(|error| format!("failed to read compatibility projection: {error}"))?;
    for child in children {
        let child = child.map_err(|error| format!("failed to read compatibility path: {error}"))?;
        let path = child.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|_| "compatibility path escaped projection root".to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("failed to inspect compatibility path: {error}"))?;
        if metadata.file_type().is_symlink() {
            files.insert(
                relative,
                ScannedProjectionFile {
                    bytes: Vec::new(),
                    executable: false,
                    blocked_reason: Some("symlink_not_allowed".to_string()),
                },
            );
        } else if metadata.is_dir() {
            scan_projection_files(root, &path, files)?;
        } else if metadata.is_file() {
            let bytes = fs::read(&path)
                .map_err(|error| format!("failed to read compatibility file: {error}"))?;
            files.insert(
                relative,
                ScannedProjectionFile {
                    bytes,
                    executable: false,
                    blocked_reason: None,
                },
            );
        }
    }
    Ok(())
}

fn classify_projection_path(
    projection: &RealProjectionSnapshot,
    path: &str,
    bytes: &[u8],
) -> (
    CompatCandidateKind,
    String,
    PrivacyClass,
    Option<String>,
    CompatPathPolicyResult,
) {
    let normalized = path.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    if let Err(ArtifactIoError::PathPolicyViolation { reason, .. }) =
        PathPolicy::posix_case_sensitive().validate(&normalized)
    {
        return (
            CompatCandidateKind::PathPolicyBlocked,
            "policy".to_string(),
            PrivacyClass::LocalOnly,
            None,
            CompatPathPolicyResult {
                allowed: false,
                normalized_path: None,
                reason: Some(reason.as_str().to_string()),
            },
        );
    }
    let secret_reasons = detect_secret_reasons(&normalized, bytes);
    if !secret_reasons.is_empty() {
        return (
            CompatCandidateKind::SecretLike,
            "secret".to_string(),
            PrivacyClass::Secret,
            Some(format!(
                "local://.sunlight/quarantine/compat/{}/{}",
                projection.projection_id,
                real_content_hash(normalized.as_bytes()).trim_start_matches("sha256:")
            )),
            allowed_path(&normalized),
        );
    }
    let segments = lower.split('/').collect::<Vec<_>>();
    if segments.iter().any(|segment| {
        matches!(
            *segment,
            "target" | "dist" | "build" | "coverage" | "node_modules" | ".cache"
        )
    }) {
        return (
            CompatCandidateKind::CacheOrBuildOutput,
            "cache".to_string(),
            PrivacyClass::LocalOnly,
            None,
            allowed_path(&normalized),
        );
    }
    let file_name = lower.rsplit('/').next().unwrap_or("");
    if file_name.ends_with(".swp")
        || file_name.ends_with('~')
        || matches!(file_name, ".ds_store" | "thumbs.db")
    {
        return (
            CompatCandidateKind::IgnoredPath,
            "ignored".to_string(),
            PrivacyClass::LocalOnly,
            None,
            allowed_path(&normalized),
        );
    }
    (
        if bytes.len() > 10 * 1024 * 1024 || bytes.iter().any(|byte| *byte == 0) {
            CompatCandidateKind::BinaryOrLarge
        } else {
            CompatCandidateKind::CreatedSource
        },
        "source".to_string(),
        PrivacyClass::PolicyGated,
        None,
        allowed_path(&normalized),
    )
}

fn allowed_path(path: &str) -> CompatPathPolicyResult {
    CompatPathPolicyResult {
        allowed: true,
        normalized_path: Some(path.to_string()),
        reason: None,
    }
}

fn blocked_candidate(
    projection: &RealProjectionSnapshot,
    path: &str,
    file: ScannedProjectionFile,
) -> CompatCandidateDelta {
    real_candidate(
        projection,
        CompatCandidateKind::PathPolicyBlocked,
        CompatFileOperationKind::Write,
        None,
        path.to_string(),
        None,
        None,
        0,
        false,
        "policy".to_string(),
        PrivacyClass::LocalOnly,
        CompatPathPolicyResult {
            allowed: false,
            normalized_path: None,
            reason: file.blocked_reason,
        },
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn real_candidate(
    projection: &RealProjectionSnapshot,
    kind: CompatCandidateKind,
    operation_kind: CompatFileOperationKind,
    artifact_id: Option<String>,
    path: String,
    before_hash: Option<String>,
    after_hash: Option<String>,
    byte_length: u64,
    executable: bool,
    classification: String,
    privacy_class: PrivacyClass,
    path_policy_result: CompatPathPolicyResult,
    quarantine_ref: Option<String>,
) -> CompatCandidateDelta {
    let identity = format!(
        "{}\0{}\0{}\0{}\0{}\0{}\0{}",
        projection.projection_id,
        path,
        kind.as_str(),
        operation_kind.as_str(),
        before_hash.as_deref().unwrap_or("new"),
        after_hash.as_deref().unwrap_or("deleted"),
        classification,
    );
    let digest = format!("{:x}", Sha256::digest(identity.as_bytes()));
    CompatCandidateDelta {
        candidate_delta_id: format!("compat_delta_{}", &digest[..24]),
        kind,
        operation_kind,
        artifact_id,
        path: path.clone(),
        source_path: None,
        before_hash,
        after_hash,
        byte_length,
        executable,
        media_type: media_type_for_compat_path(&path).to_string(),
        classification,
        privacy_class,
        path_policy_result,
        quarantine_ref,
    }
}

fn media_type_for_compat_path(path: &str) -> &'static str {
    match Path::new(path).extension().and_then(|value| value.to_str()) {
        Some("rs") => "text/rust; charset=utf-8",
        Some("js" | "mjs" | "cjs") => "text/javascript; charset=utf-8",
        Some("ts" | "tsx") => "text/typescript; charset=utf-8",
        Some("json") => "application/json",
        Some("toml" | "md" | "txt" | "yml" | "yaml") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn real_error(
    code: CompatImportErrorCode,
    projection: &RealProjectionSnapshot,
    candidate_delta_ids: Vec<String>,
    message: impl Into<String>,
) -> CompatImportValidationError {
    CompatImportValidationError {
        code,
        projection_id: projection.projection_id.clone(),
        session_id: projection.session_id.clone().unwrap_or_default(),
        candidate_delta_ids,
        message: message.into(),
    }
}

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
    pub operations: Vec<CompatSelectedDeltaOperationPlan>,
    pub classification: String,
    pub privacy_class: PrivacyClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatSelectedDeltaOperationPlan {
    pub operation_kind: CompatFileOperationKind,
    pub source_path: Option<String>,
    pub target_path: String,
    pub base_content_hash: Option<String>,
    pub result_content_hash: Option<String>,
    pub patch_digest: Option<String>,
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
    pub source_path: Option<String>,
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
            source_path: None,
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
            candidate_delta_id: "compat_delta_src_auth_metadata_0001".to_string(),
            kind: CompatCandidateKind::MetadataChanged,
            operation_kind: CompatFileOperationKind::Metadata,
            artifact_id: Some("artifact_src_auth_ts".to_string()),
            path: "src/auth.ts".to_string(),
            source_path: None,
            before_hash: Some("sha256:auth_base".to_string()),
            after_hash: Some("sha256:auth_base".to_string()),
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
            source_path: None,
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
            source_path: None,
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
            candidate_delta_id: "compat_delta_src_auth_rename_0001".to_string(),
            kind: CompatCandidateKind::MovedOrRenamed,
            operation_kind: CompatFileOperationKind::Move,
            artifact_id: Some("artifact_src_auth_ts".to_string()),
            path: "src/auth.renamed.ts".to_string(),
            source_path: Some("src/auth.ts".to_string()),
            before_hash: Some("sha256:auth_base".to_string()),
            after_hash: Some("sha256:auth_base".to_string()),
            byte_length: 109,
            executable: false,
            media_type: "text/typescript; charset=utf-8".to_string(),
            classification: "source".to_string(),
            privacy_class: PrivacyClass::PolicyGated,
            path_policy_result: CompatPathPolicyResult {
                allowed: true,
                normalized_path: Some("src/auth.renamed.ts".to_string()),
                reason: None,
            },
            quarantine_ref: None,
        },
        CompatCandidateDelta {
            candidate_delta_id: "compat_delta_src_auth_rename_edit_0001".to_string(),
            kind: CompatCandidateKind::MovedOrRenamed,
            operation_kind: CompatFileOperationKind::Move,
            artifact_id: Some("artifact_src_auth_ts".to_string()),
            path: "src/auth.renamed-edited.ts".to_string(),
            source_path: Some("src/auth.ts".to_string()),
            before_hash: Some("sha256:auth_base".to_string()),
            after_hash: Some("sha256:auth_rename_edit_projection_after".to_string()),
            byte_length: 128,
            executable: false,
            media_type: "text/typescript; charset=utf-8".to_string(),
            classification: "source".to_string(),
            privacy_class: PrivacyClass::PolicyGated,
            path_policy_result: CompatPathPolicyResult {
                allowed: true,
                normalized_path: Some("src/auth.renamed-edited.ts".to_string()),
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
            source_path: None,
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
            source_path: None,
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
            source_path: None,
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
            source_path: None,
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
            candidate_delta_id: "compat_delta_ignored_editor_swap_0001".to_string(),
            kind: CompatCandidateKind::IgnoredPath,
            operation_kind: CompatFileOperationKind::Write,
            artifact_id: None,
            path: "tmp/auth.ts.swp".to_string(),
            source_path: None,
            before_hash: None,
            after_hash: Some("sha256:ignored_editor_swap_local".to_string()),
            byte_length: 96,
            executable: false,
            media_type: "application/octet-stream".to_string(),
            classification: "ignored".to_string(),
            privacy_class: PrivacyClass::LocalOnly,
            path_policy_result: CompatPathPolicyResult {
                allowed: true,
                normalized_path: Some("tmp/auth.ts.swp".to_string()),
                reason: Some("ignored_path".to_string()),
            },
            quarantine_ref: None,
        },
        CompatCandidateDelta {
            candidate_delta_id: "compat_delta_env_secret_0001".to_string(),
            kind: CompatCandidateKind::SecretLike,
            operation_kind: CompatFileOperationKind::Write,
            artifact_id: None,
            path: ".env".to_string(),
            source_path: None,
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
            source_path: None,
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
            request.selected_candidate_delta_ids.clone(),
            format!(
                "session generation `{}` does not match current generation `gen_agent_a_0001`",
                request.session_generation_id
            ),
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
        if let Some(source_path) = candidate.source_path.as_deref() {
            if let Err(path_error) = path_policy.validate(source_path) {
                return Err(error(
                    CompatImportErrorCode::PathPolicyFailed,
                    &request.projection_id,
                    &request.session_id,
                    candidate_ids,
                    path_error_message(&path_error),
                ));
            }
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
                if candidate.source_path.is_none() {
                    return Err(error(
                        CompatImportErrorCode::AmbiguousRename,
                        &request.projection_id,
                        &request.session_id,
                        candidate_ids,
                        "fixture foundation does not resolve rename identity",
                    ));
                }
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
    let precondition_path = candidate
        .source_path
        .as_deref()
        .unwrap_or(candidate.path.as_str());
    let active_entry = current_view.tree_entries.get(precondition_path);
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
    }?;

    if candidate.kind == CompatCandidateKind::MovedOrRenamed
        && candidate.source_path.as_deref() != Some(candidate.path.as_str())
        && current_view.tree_entries.contains_key(&candidate.path)
    {
        return Err(error(
            CompatImportErrorCode::PreconditionFailed,
            &request.projection_id,
            &request.session_id,
            vec![candidate.candidate_delta_id.clone()],
            "move candidate target path already exists in the current view",
        ));
    }

    Ok(())
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
            operations: selected_delta_operations(candidate),
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
            path: candidate
                .source_path
                .clone()
                .unwrap_or_else(|| candidate.path.clone()),
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

fn selected_delta_operations(
    candidate: &CompatCandidateDelta,
) -> Vec<CompatSelectedDeltaOperationPlan> {
    if candidate.kind == CompatCandidateKind::MovedOrRenamed
        && candidate.before_hash.is_some()
        && candidate.after_hash.is_some()
        && candidate.before_hash != candidate.after_hash
    {
        return vec![
            CompatSelectedDeltaOperationPlan {
                operation_kind: CompatFileOperationKind::Move,
                source_path: candidate.source_path.clone(),
                target_path: candidate.path.clone(),
                base_content_hash: candidate.before_hash.clone(),
                result_content_hash: candidate.before_hash.clone(),
                patch_digest: None,
            },
            CompatSelectedDeltaOperationPlan {
                operation_kind: CompatFileOperationKind::Patch,
                source_path: Some(candidate.path.clone()),
                target_path: candidate.path.clone(),
                base_content_hash: candidate.before_hash.clone(),
                result_content_hash: candidate.after_hash.clone(),
                patch_digest: Some(format!("sha256:{}_patch", candidate.candidate_delta_id)),
            },
        ];
    }

    vec![CompatSelectedDeltaOperationPlan {
        operation_kind: candidate.operation_kind,
        source_path: candidate.source_path.clone(),
        target_path: candidate.path.clone(),
        base_content_hash: candidate.before_hash.clone(),
        result_content_hash: candidate.after_hash.clone(),
        patch_digest: (candidate.operation_kind == CompatFileOperationKind::Patch)
            .then(|| format!("sha256:{}_patch", candidate.candidate_delta_id)),
    }]
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
    use crate::repo_state::{real_artifact_id_for_path, real_tree_hash};
    use crate::resolver::{fixture_base_entries, fixture_resolver_input, resolve_fixture_view};
    use std::time::{SystemTime, UNIX_EPOCH};

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
            source_path: None,
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

        let ignored_error = plan_fixture_basic_app_import(
            &projection,
            &view,
            request(vec!["compat_delta_ignored_editor_swap_0001"]),
            &fixture_basic_app_candidate_deltas(),
        )
        .unwrap_err();
        assert_eq!(ignored_error.code, CompatImportErrorCode::CacheBlocked);
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

    #[test]
    fn real_projection_diff_reads_only_persisted_root_and_classifies_source_operations() {
        let (repo, projection) = real_projection_fixture();
        let root = PathBuf::from(projection.materialized_root.as_ref().unwrap());
        fs::write(root.join("src/lib.rs"), b"pub fn value() -> u32 { 2 }\n").unwrap();
        fs::write(root.join("src/new.rs"), b"pub fn new_value() {}\n").unwrap();
        fs::remove_file(root.join("README.md")).unwrap();
        fs::write(repo.join("outside.txt"), b"must not be diffed\n").unwrap();

        let diff =
            diff_real_compat_projection(&repo, &repo.join(".sunlight/projections"), &projection)
                .unwrap();
        let by_path = diff
            .candidates
            .iter()
            .map(|candidate| (candidate.path.as_str(), candidate))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            by_path["src/lib.rs"].kind,
            CompatCandidateKind::ModifiedSource
        );
        assert_eq!(
            by_path["src/new.rs"].kind,
            CompatCandidateKind::CreatedSource
        );
        assert_eq!(
            by_path["README.md"].kind,
            CompatCandidateKind::DeletedSource
        );
        assert!(!by_path.contains_key("outside.txt"));
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn real_projection_diff_infers_only_unambiguous_exact_content_rename() {
        let (repo, projection) = real_projection_fixture();
        let root = PathBuf::from(projection.materialized_root.as_ref().unwrap());
        fs::create_dir_all(root.join("docs")).unwrap();
        fs::rename(root.join("README.md"), root.join("docs/README.md")).unwrap();

        let diff =
            diff_real_compat_projection(&repo, &repo.join(".sunlight/projections"), &projection)
                .unwrap();
        let candidate = diff
            .candidates
            .iter()
            .find(|candidate| candidate.path == "docs/README.md")
            .unwrap();
        assert_eq!(candidate.kind, CompatCandidateKind::MovedOrRenamed);
        assert_eq!(candidate.operation_kind, CompatFileOperationKind::Move);
        assert_eq!(candidate.source_path.as_deref(), Some("README.md"));
        assert_eq!(
            candidate.artifact_id.as_deref(),
            Some(real_artifact_id_for_path("README.md").as_str())
        );
        assert_eq!(candidate.before_hash, candidate.after_hash);
        assert!(!diff
            .candidates
            .iter()
            .any(|candidate| candidate.path == "README.md"));
        assert!(diff.after_bytes.contains_key(&candidate.candidate_delta_id));
        validate_real_compat_selection(
            &projection,
            std::slice::from_ref(&candidate.candidate_delta_id),
            &diff,
        )
        .unwrap();
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn real_projection_diff_policy_gates_one_source_with_multiple_exact_targets() {
        let (repo, projection) = real_projection_fixture();
        let root = PathBuf::from(projection.materialized_root.as_ref().unwrap());
        let bytes = fs::read(root.join("README.md")).unwrap();
        fs::remove_file(root.join("README.md")).unwrap();
        fs::write(root.join("COPY-A.md"), &bytes).unwrap();
        fs::write(root.join("COPY-B.md"), &bytes).unwrap();

        let diff =
            diff_real_compat_projection(&repo, &repo.join(".sunlight/projections"), &projection)
                .unwrap();
        let ambiguous = diff
            .candidates
            .iter()
            .filter(|candidate| candidate.kind == CompatCandidateKind::MovedOrRenamed)
            .collect::<Vec<_>>();
        assert_eq!(ambiguous.len(), 2);
        assert!(ambiguous
            .iter()
            .all(|candidate| candidate.source_path.is_none()));
        assert!(!diff
            .candidates
            .iter()
            .any(|candidate| candidate.path == "README.md"));
        let error = validate_real_compat_selection(
            &projection,
            std::slice::from_ref(&ambiguous[0].candidate_delta_id),
            &diff,
        )
        .unwrap_err();
        assert_eq!(error.code, CompatImportErrorCode::AmbiguousRename);
        fs::remove_dir_all(repo).unwrap();
    }

    #[test]
    fn real_projection_selection_blocks_secret_cache_and_reserved_candidates() {
        let (repo, projection) = real_projection_fixture();
        let root = PathBuf::from(projection.materialized_root.as_ref().unwrap());
        fs::write(root.join(".env"), b"API_KEY=do-not-import\n").unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::write(root.join("target/output.txt"), b"cache\n").unwrap();
        fs::create_dir_all(root.join(".sunlight")).unwrap();
        fs::write(root.join(".sunlight/config.toml"), b"blocked\n").unwrap();

        let diff =
            diff_real_compat_projection(&repo, &repo.join(".sunlight/projections"), &projection)
                .unwrap();
        for (path, expected) in [
            (".env", CompatImportErrorCode::SecretDetected),
            ("target/output.txt", CompatImportErrorCode::CacheBlocked),
            (
                ".sunlight/config.toml",
                CompatImportErrorCode::PathPolicyFailed,
            ),
        ] {
            let id = diff
                .candidates
                .iter()
                .find(|candidate| candidate.path == path)
                .unwrap()
                .candidate_delta_id
                .clone();
            let error = validate_real_compat_selection(&projection, &[id], &diff).unwrap_err();
            assert_eq!(error.code, expected);
        }
        fs::remove_dir_all(repo).unwrap();
    }

    fn real_projection_fixture() -> (PathBuf, RealProjectionSnapshot) {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let repo = std::env::temp_dir().join(format!(
            "sunlight-real-compat-core-{}-{suffix}",
            std::process::id()
        ));
        let root = repo
            .join(".sunlight")
            .join("projections")
            .join("compat")
            .join("projection_test")
            .join("root");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), b"pub fn value() -> u32 { 1 }\n").unwrap();
        fs::write(root.join("README.md"), b"# baseline\n").unwrap();
        let entries = [
            ("src/lib.rs", b"pub fn value() -> u32 { 1 }\n".as_slice()),
            ("README.md", b"# baseline\n".as_slice()),
        ]
        .into_iter()
        .map(|(path, bytes)| RealArtifactEntry {
            path: path.to_string(),
            artifact_id: real_artifact_id_for_path(path),
            content_hash: real_content_hash(bytes),
            executable: false,
            classification: "source".to_string(),
            tombstone: false,
            bytes: bytes.to_vec(),
        })
        .collect::<Vec<_>>();
        let tree_hash = real_tree_hash(&entries);
        let manifest_digest = real_compat_baseline_manifest_digest(
            "repo_test",
            "projection_test",
            "session_test",
            "gen_test_0001",
            "view_test",
            &tree_hash,
            &entries,
        );
        (
            repo,
            RealProjectionSnapshot {
                projection_id: "projection_test".to_string(),
                repository_id: "repo_test".to_string(),
                purpose: "compatibility".to_string(),
                resolved_view_id: "view_test".to_string(),
                tree_hash: tree_hash.clone(),
                topic_frontier: BTreeMap::new(),
                manifest_digest,
                created_from_content_tree: tree_hash,
                materialized_root: Some(root.display().to_string()),
                session_id: Some("session_test".to_string()),
                session_generation_id: Some("gen_test_0001".to_string()),
                path_policy_id: POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string(),
                operation_semantics_version: FILE_OPERATION_SEMANTICS_VERSION.to_string(),
                cache_key: "projection-cache:test".to_string(),
                strategy: "copy".to_string(),
                materialization: None,
                retention_state: "active".to_string(),
                privacy_class: "local_only".to_string(),
                last_import_operation_id: None,
                entries,
            },
        )
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

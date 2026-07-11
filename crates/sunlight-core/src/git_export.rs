use crate::artifacts::{PathPolicy, POSIX_CASE_SENSITIVE_PATH_POLICY_ID};
use crate::checkpoint::{
    CheckpointRecord, ExportShape, ExportShapeKind, GitExportMapRecord, FIXTURE_CREATED_AT,
    FIXTURE_EXPORTED_GIT_REF, FIXTURE_EXPORT_MAP_ID, FIXTURE_GIT_COMMIT_ID,
    FIXTURE_VALIDATION_REPORT_ID,
};
use crate::records::PrivacyClass;
use crate::repo_state::{
    detect_secret_reasons, expanded_operation_order, real_content_hash, real_tree_hash,
    RealArtifactEntry, RealRepoState,
};
use crate::repository::{RepositoryConfig, CONSERVATIVE_SUNLIGHT_COMMIT_POLICY};
use crate::resolver::{ResolvedViewResult, SingleRepoTree};
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMPORARY_GIT_INDEX_SEQUENCE: AtomicU64 = AtomicU64::new(0);
pub const CONSERVATIVE_EXPORT_POLICY_ID: &str =
    "git_interop.sunlight_commit_policy.conservative.v1";

pub struct PersistedGitExportValidationInput<'a> {
    pub config: &'a RepositoryConfig,
    pub request: &'a GitExportRequest,
    pub resolved_view: &'a ResolvedViewResult,
    pub entries: &'a [RealArtifactEntry],
    pub state: &'a RealRepoState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportRequest {
    pub checkpoint: CheckpointRecord,
    pub git_remote: Option<String>,
    pub git_ref: String,
    pub export_shape: ExportShape,
    pub validation_report_id: String,
    pub generated_output_requirements: Vec<GeneratedOutputExportRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedOutputExportRequirement {
    pub path: String,
    pub provenance_requirement: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportValidationReport {
    pub id: String,
    pub policy_id: String,
    pub checkpoint_id: String,
    pub resolved_view_id: String,
    pub tree_identity: SingleRepoTree,
    pub git_ref: String,
    pub ok: bool,
    pub summary: GitExportValidationSummary,
    pub failures: Vec<GitExportValidationFailure>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportValidationSummary {
    pub records_checked: u32,
    pub payloads_checked: u32,
    pub warnings: u32,
    pub blocked: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportValidationFailure {
    pub check: GitExportValidationCheck,
    pub code: GitExportValidationFailureCode,
    pub field: Option<String>,
    pub value: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitExportValidationCheck {
    ConflictGate,
    RepositoryConfig,
    PolicyClass,
    PathScope,
    Reachability,
    SecretScan,
    UnsafeReference,
    ExportShape,
    GitRef,
    ExecutionRawExclusion,
    GeneratedPolicy,
    ReportIntegrity,
}

impl GitExportValidationCheck {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ConflictGate => "conflict_gate",
            Self::RepositoryConfig => "repository_config",
            Self::PolicyClass => "policy_class",
            Self::PathScope => "path_scope",
            Self::Reachability => "reachability",
            Self::SecretScan => "secret_scan",
            Self::UnsafeReference => "unsafe_reference",
            Self::ExportShape => "export_shape",
            Self::GitRef => "git_ref",
            Self::ExecutionRawExclusion => "execution_raw_exclusion",
            Self::GeneratedPolicy => "generated_policy",
            Self::ReportIntegrity => "report_integrity",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitExportValidationFailureCode {
    CheckpointConflictedView,
    RepositoryScopeMismatch,
    ResolvedViewMismatch,
    TreeIdentityMismatch,
    PathPolicyMismatch,
    ExportPathUnsafe,
    ContentHashMismatch,
    SecretDetected,
    ExportPolicyFailed,
    ExportMetadataPolicyFailed,
    ExportRefInvalid,
    MovingSelector,
    LocalOnlyEvidenceReference,
    SecretOrLocalOnlyRecord,
    GeneratedOutputRequiresPromotion,
    ValidationReportMissing,
}

impl GitExportValidationFailureCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CheckpointConflictedView => "checkpoint_conflicted_view",
            Self::RepositoryScopeMismatch => "repository_scope_mismatch",
            Self::ResolvedViewMismatch => "resolved_view_mismatch",
            Self::TreeIdentityMismatch => "tree_identity_mismatch",
            Self::PathPolicyMismatch => "path_policy_mismatch",
            Self::ExportPathUnsafe => "export_path_unsafe",
            Self::ContentHashMismatch => "content_hash_mismatch",
            Self::SecretDetected => "secret_detected",
            Self::ExportPolicyFailed => "export_policy_failed",
            Self::ExportMetadataPolicyFailed => "export_metadata_policy_failed",
            Self::ExportRefInvalid => "export_ref_invalid",
            Self::MovingSelector => "moving_selector",
            Self::LocalOnlyEvidenceReference => "local_only_evidence_reference",
            Self::SecretOrLocalOnlyRecord => "secret_or_local_only_record",
            Self::GeneratedOutputRequiresPromotion => "generated_output_requires_promotion",
            Self::ValidationReportMissing => "validation_report_missing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportResponse {
    pub command: String,
    pub checkpoint_id: String,
    pub validation_report: GitExportValidationReport,
    pub git_ref: String,
    pub git_commit_ids: Vec<String>,
    pub export_map: GitExportMapRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportError {
    pub code: GitExportErrorCode,
    pub validation_report: GitExportValidationReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitExportErrorCode {
    ExportPolicyFailed,
    ExportParentNotFound,
    ExportParentAmbiguous,
    ExportTargetRefInvalid,
    ExportTargetRefConflict,
    ExportRepositoryInvalid,
    ExportGitFailed,
    ExportRefUpdateFailed,
    ExportMapWriteFailed,
}

impl GitExportErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExportPolicyFailed => "export_policy_failed",
            Self::ExportParentNotFound => "export_parent_not_found",
            Self::ExportParentAmbiguous => "export_parent_ambiguous",
            Self::ExportTargetRefInvalid => "export_target_ref_invalid",
            Self::ExportTargetRefConflict => "export_target_ref_conflict",
            Self::ExportRepositoryInvalid => "export_repository_invalid",
            Self::ExportGitFailed => "export_git_failed",
            Self::ExportRefUpdateFailed => "export_ref_update_failed",
            Self::ExportMapWriteFailed => "export_map_write_failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportWriterInput {
    pub request: GitExportRequest,
    pub validation_report: GitExportValidationReport,
    pub repository: GitExportRepositoryState,
    pub base_checkpoint_ids: Vec<String>,
    pub imported_base_commits: Vec<ImportedBaseGitCommit>,
    pub prior_export_maps: Vec<GitExportMapRecord>,
    pub planned_commit_id: String,
    pub export_map_id: String,
    pub exported_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportRepositoryState {
    pub repository_id: String,
    pub git_root: String,
    pub sunlight_repo_root: String,
    pub reachable_commit_ids: Vec<String>,
    pub refs: Vec<GitRefState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRefState {
    pub git_ref: String,
    pub commit_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedBaseGitCommit {
    pub checkpoint_id: String,
    pub git_commit_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportTargetRef {
    pub full_name: String,
    pub branch_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportParentSelectionInput {
    pub base_checkpoint_ids: Vec<String>,
    pub imported_base_commits: Vec<ImportedBaseGitCommit>,
    pub reachable_commit_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportTargetRefUpdateInput {
    pub checkpoint_id: String,
    pub target_ref: String,
    pub selected_parent_commit_id: String,
    pub planned_commit_id: String,
    pub existing_target_ref: Option<GitRefState>,
    pub prior_export_maps: Vec<GitExportMapRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportWriterValidationError {
    pub code: GitExportErrorCode,
    pub target_ref: Option<String>,
    pub parent_commit_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportWriterPlan {
    pub parent: GitExportParentPlan,
    pub commit: GitExportCommitPlan,
    pub ref_update: GitExportRefUpdatePlan,
    pub export_map: GitExportMapRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportExecutionFixture {
    pub commit_creation: GitExportExecutionStepFixture,
    pub ref_update: GitExportExecutionStepFixture,
    pub export_map_write: GitExportExecutionStepFixture,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitExportExecutionStepFixture {
    Succeed,
    Fail { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportExecutionResult {
    pub lifecycle_state: GitExportExecutionLifecycleState,
    pub checkpoint_id: String,
    pub validation_report_id: String,
    pub target_ref: String,
    pub parent_commit_id: String,
    pub created_commit_id: Option<String>,
    pub summary: GitExportExecutionSummary,
    pub export_map: Option<GitExportMapRecord>,
    pub error: Option<GitExportExecutionError>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportContentFile {
    pub path: String,
    pub bytes: Vec<u8>,
    pub executable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedGitExportMap {
    pub export_map: GitExportMapRecord,
}

pub trait GitExportMapStore {
    fn persist_git_export_map(
        &mut self,
        export_map: GitExportMapRecord,
    ) -> Result<PersistedGitExportMap, String>;
}

#[derive(Debug, Default)]
pub struct InMemoryGitExportMapStore {
    pub export_maps: Vec<GitExportMapRecord>,
}

impl GitExportMapStore for InMemoryGitExportMapStore {
    fn persist_git_export_map(
        &mut self,
        export_map: GitExportMapRecord,
    ) -> Result<PersistedGitExportMap, String> {
        self.export_maps.push(export_map.clone());
        Ok(PersistedGitExportMap { export_map })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitExportExecutionLifecycleState {
    Exported,
    Partial,
    Failed,
}

impl GitExportExecutionLifecycleState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exported => "exported",
            Self::Partial => "partial",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportExecutionSummary {
    pub commit_created: bool,
    pub ref_updated: bool,
    pub export_map_written: bool,
    pub completed_steps: Vec<GitExportExecutionStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitExportExecutionStep {
    CommitCreated,
    RefUpdated,
    ExportMapWritten,
}

impl GitExportExecutionStep {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CommitCreated => "commit_created",
            Self::RefUpdated => "ref_updated",
            Self::ExportMapWritten => "export_map_written",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportExecutionError {
    pub code: GitExportErrorCode,
    pub failed_step: GitExportExecutionStep,
    pub checkpoint_id: String,
    pub validation_report_id: String,
    pub target_ref: String,
    pub parent_commit_id: String,
    pub created_commit_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportParentPlan {
    pub checkpoint_id: String,
    pub commit_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportCommitPlan {
    pub checkpoint_id: String,
    pub resolved_view_id: String,
    pub tree_identity: SingleRepoTree,
    pub parent_commit_id: String,
    pub planned_commit_id: String,
    pub export_shape: ExportShape,
    pub validation_report_id: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportRefUpdatePlan {
    pub git_ref: String,
    pub expected_old_commit_id: Option<String>,
    pub new_commit_id: String,
    pub allowed_reason: GitExportRefUpdateReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitExportRefUpdateReason {
    CreateRef,
    ReplaceSelectedParent,
    ReplacePriorExportForSameCheckpoint,
}

impl GitExportRefUpdateReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CreateRef => "create_ref",
            Self::ReplaceSelectedParent => "replace_selected_parent",
            Self::ReplacePriorExportForSameCheckpoint => "replace_prior_export_for_same_checkpoint",
        }
    }
}

impl GitExportExecutionFixture {
    pub fn success() -> Self {
        Self {
            commit_creation: GitExportExecutionStepFixture::Succeed,
            ref_update: GitExportExecutionStepFixture::Succeed,
            export_map_write: GitExportExecutionStepFixture::Succeed,
        }
    }
}

impl GitExportExecutionStepFixture {
    pub fn fail(message: impl Into<String>) -> Self {
        Self::Fail {
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportPlanningError {
    pub code: GitExportErrorCode,
    pub checkpoint_id: Option<String>,
    pub validation_report_id: Option<String>,
    pub target_ref: Option<String>,
    pub parent_commit_id: Option<String>,
    pub created_commit_id: Option<String>,
    pub message: String,
}

pub fn plan_git_export_writer(
    input: GitExportWriterInput,
) -> Result<GitExportWriterPlan, GitExportPlanningError> {
    validate_writer_repository(&input)?;
    validate_writer_report(&input)?;

    let parent = select_base_parent(&input)?;
    let ref_update = plan_ref_update(&input, &parent)?;
    let commit = GitExportCommitPlan {
        checkpoint_id: input.request.checkpoint.id.clone(),
        resolved_view_id: input.request.checkpoint.resolved_view_id.clone(),
        tree_identity: input.request.checkpoint.tree_identity.clone(),
        parent_commit_id: parent.commit_id.clone(),
        planned_commit_id: input.planned_commit_id.clone(),
        export_shape: input.request.export_shape.clone(),
        validation_report_id: input.validation_report.id.clone(),
        message: commit_message(&input.request, &input.validation_report),
    };
    let export_map = GitExportMapRecord {
        id: input.export_map_id,
        repository_id: input.request.checkpoint.repository_id.clone(),
        checkpoint_id: input.request.checkpoint.id.clone(),
        tree_identity: input.request.checkpoint.tree_identity.clone(),
        git_remote: input.request.git_remote.clone(),
        git_ref: input.request.git_ref.clone(),
        git_commit_ids: vec![commit.planned_commit_id.clone()],
        export_shape: input.request.export_shape,
        validation_report_id: input.validation_report.id,
        exported_at: input.exported_at,
        privacy_class: PrivacyClass::CommitDefault,
    };

    Ok(GitExportWriterPlan {
        parent,
        commit,
        ref_update,
        export_map,
    })
}

pub fn execute_git_export_writer_plan_fixture(
    plan: &GitExportWriterPlan,
    fixture: GitExportExecutionFixture,
) -> GitExportExecutionResult {
    let base = GitExportExecutionResultBuilder::new(plan);

    if let GitExportExecutionStepFixture::Fail { message } = fixture.commit_creation {
        return base.failed(
            GitExportExecutionStep::CommitCreated,
            GitExportErrorCode::ExportGitFailed,
            None,
            summary(false, false, false),
            message,
        );
    }

    let created_commit_id = plan.commit.planned_commit_id.clone();
    if let GitExportExecutionStepFixture::Fail { message } = fixture.ref_update {
        return base.partial(
            GitExportExecutionStep::RefUpdated,
            GitExportErrorCode::ExportRefUpdateFailed,
            Some(created_commit_id),
            summary(true, false, false),
            message,
        );
    }

    if let GitExportExecutionStepFixture::Fail { message } = fixture.export_map_write {
        return base.partial(
            GitExportExecutionStep::ExportMapWritten,
            GitExportErrorCode::ExportMapWriteFailed,
            Some(created_commit_id),
            summary(true, true, false),
            message,
        );
    }

    GitExportExecutionResult {
        lifecycle_state: GitExportExecutionLifecycleState::Exported,
        checkpoint_id: plan.commit.checkpoint_id.clone(),
        validation_report_id: plan.commit.validation_report_id.clone(),
        target_ref: plan.ref_update.git_ref.clone(),
        parent_commit_id: plan.commit.parent_commit_id.clone(),
        created_commit_id: Some(created_commit_id),
        summary: summary(true, true, true),
        export_map: Some(plan.export_map.clone()),
        error: None,
    }
}

pub fn execute_local_git_export_writer(
    input: GitExportWriterInput,
    content_files: Vec<GitExportContentFile>,
    export_map_store: &mut impl GitExportMapStore,
) -> Result<GitExportExecutionResult, GitExportPlanningError> {
    validate_local_git_repository_root(&input)?;
    let git_root = input.repository.git_root.clone();
    let plan = plan_git_export_writer(input)?;
    execute_local_git_export_writer_plan_with_root(
        &git_root,
        &plan,
        &content_files,
        export_map_store,
    )
}

pub fn execute_local_git_export_writer_plan_with_root(
    git_root: impl AsRef<Path>,
    plan: &GitExportWriterPlan,
    content_files: &[GitExportContentFile],
    export_map_store: &mut impl GitExportMapStore,
) -> Result<GitExportExecutionResult, GitExportPlanningError> {
    let git_root = git_root.as_ref();
    let base = GitExportExecutionResultBuilder::new(plan);

    let tree_id = match write_git_tree(git_root, content_files) {
        Ok(tree_id) => tree_id,
        Err(message) => {
            return Ok(base.failed(
                GitExportExecutionStep::CommitCreated,
                GitExportErrorCode::ExportGitFailed,
                None,
                summary(false, false, false),
                message,
            ));
        }
    };

    let created_commit_id = match create_git_commit(git_root, plan, &tree_id) {
        Ok(commit_id) => commit_id,
        Err(message) => {
            return Ok(base.failed(
                GitExportExecutionStep::CommitCreated,
                GitExportErrorCode::ExportGitFailed,
                None,
                summary(false, false, false),
                message,
            ));
        }
    };

    if let Err(message) = update_git_ref(git_root, plan, &created_commit_id) {
        return Ok(base.partial(
            GitExportExecutionStep::RefUpdated,
            GitExportErrorCode::ExportRefUpdateFailed,
            Some(created_commit_id),
            summary(true, false, false),
            message,
        ));
    }

    let mut export_map = plan.export_map.clone();
    export_map.git_commit_ids = vec![created_commit_id.clone()];
    match export_map_store.persist_git_export_map(export_map) {
        Ok(persisted) => Ok(GitExportExecutionResult {
            lifecycle_state: GitExportExecutionLifecycleState::Exported,
            checkpoint_id: plan.commit.checkpoint_id.clone(),
            validation_report_id: plan.commit.validation_report_id.clone(),
            target_ref: plan.ref_update.git_ref.clone(),
            parent_commit_id: plan.commit.parent_commit_id.clone(),
            created_commit_id: Some(created_commit_id),
            summary: summary(true, true, true),
            export_map: Some(persisted.export_map),
            error: None,
        }),
        Err(message) => Ok(base.partial(
            GitExportExecutionStep::ExportMapWritten,
            GitExportErrorCode::ExportMapWriteFailed,
            Some(created_commit_id),
            summary(true, true, false),
            message,
        )),
    }
}

fn validate_local_git_repository_root(
    input: &GitExportWriterInput,
) -> Result<(), GitExportPlanningError> {
    let git_root = Path::new(&input.repository.git_root);
    let sunlight_root = Path::new(&input.repository.sunlight_repo_root);
    let discovered =
        run_git(git_root, &["rev-parse", "--show-toplevel"], None, &[]).map_err(|message| {
            planning_error(
                GitExportErrorCode::ExportRepositoryInvalid,
                input,
                None,
                None,
                &message,
            )
        })?;

    let discovered_root = std::fs::canonicalize(discovered.trim()).map_err(|error| {
        planning_error(
            GitExportErrorCode::ExportRepositoryInvalid,
            input,
            None,
            None,
            &format!("failed to resolve discovered Git root: {error}"),
        )
    })?;
    let configured_git_root = std::fs::canonicalize(git_root).map_err(|error| {
        planning_error(
            GitExportErrorCode::ExportRepositoryInvalid,
            input,
            None,
            None,
            &format!("failed to resolve configured Git root: {error}"),
        )
    })?;
    let configured_sunlight_root = std::fs::canonicalize(sunlight_root).map_err(|error| {
        planning_error(
            GitExportErrorCode::ExportRepositoryInvalid,
            input,
            None,
            None,
            &format!("failed to resolve configured Sunlight root: {error}"),
        )
    })?;

    if discovered_root != configured_git_root || configured_git_root != configured_sunlight_root {
        return Err(planning_error(
            GitExportErrorCode::ExportRepositoryInvalid,
            input,
            None,
            None,
            "Git repository root must match the configured Sunlight repository scope",
        ));
    }

    Ok(())
}

fn write_git_tree(
    git_root: &Path,
    content_files: &[GitExportContentFile],
) -> Result<String, String> {
    let mut entries = content_files.to_vec();
    entries.sort_by(|left, right| left.path.cmp(&right.path));

    let index_path = temporary_git_index_path();
    let index_path_string = index_path.to_string_lossy().to_string();
    let index_env = [("GIT_INDEX_FILE", index_path_string.as_str())];
    let mut index_info = Vec::new();
    for entry in entries {
        validate_export_file_path(&entry.path)?;
        let object_id = run_git(
            git_root,
            &["hash-object", "-w", "--stdin"],
            Some(&entry.bytes),
            &[],
        )?;
        let mode = if entry.executable { "100755" } else { "100644" };
        index_info.extend_from_slice(mode.as_bytes());
        index_info.extend_from_slice(b" blob ");
        index_info.extend_from_slice(object_id.trim().as_bytes());
        index_info.push(b'\t');
        index_info.extend_from_slice(entry.path.as_bytes());
        index_info.push(b'\n');
    }

    let result = run_git(
        git_root,
        &["update-index", "--index-info"],
        Some(&index_info),
        &index_env,
    )
    .and_then(|_| run_git(git_root, &["write-tree"], None, &index_env))
    .map(|tree_id| tree_id.trim().to_string());
    let _ = std::fs::remove_file(index_path);
    result
}

fn temporary_git_index_path() -> std::path::PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let sequence = TEMPORARY_GIT_INDEX_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "sunlight-git-export-index-{}-{timestamp}-{sequence}",
        std::process::id(),
    ))
}

fn create_git_commit(
    git_root: &Path,
    plan: &GitExportWriterPlan,
    tree_id: &str,
) -> Result<String, String> {
    let env = [
        ("GIT_AUTHOR_NAME", "Sunlight Export Writer"),
        ("GIT_AUTHOR_EMAIL", "sunlight-export@example.invalid"),
        ("GIT_AUTHOR_DATE", plan.export_map.exported_at.as_str()),
        ("GIT_COMMITTER_NAME", "Sunlight Export Writer"),
        ("GIT_COMMITTER_EMAIL", "sunlight-export@example.invalid"),
        ("GIT_COMMITTER_DATE", plan.export_map.exported_at.as_str()),
    ];
    run_git(
        git_root,
        &["commit-tree", tree_id, "-p", &plan.commit.parent_commit_id],
        Some(plan.commit.message.as_bytes()),
        &env,
    )
    .map(|commit_id| commit_id.trim().to_string())
}

fn update_git_ref(
    git_root: &Path,
    plan: &GitExportWriterPlan,
    created_commit_id: &str,
) -> Result<(), String> {
    let zero = "0000000000000000000000000000000000000000";
    let expected_old = plan
        .ref_update
        .expected_old_commit_id
        .as_deref()
        .unwrap_or(zero);
    run_git(
        git_root,
        &[
            "update-ref",
            &plan.ref_update.git_ref,
            created_commit_id,
            expected_old,
        ],
        None,
        &[],
    )
    .map(|_| ())
}

fn validate_export_file_path(path: &str) -> Result<(), String> {
    let path = Path::new(path);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
    {
        return Err("export file paths must be normalized repository-relative paths".to_string());
    }

    Ok(())
}

fn run_git(
    git_root: &Path,
    args: &[&str],
    stdin: Option<&[u8]>,
    env: &[(&str, &str)],
) -> Result<String, String> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(git_root)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }
    for (key, value) in env {
        command.env(key, value);
    }

    let mut child = command
        .spawn()
        .map_err(|error| format!("failed to start git {}: {error}", args.join(" ")))?;

    if let Some(input) = stdin {
        let mut child_stdin = child
            .stdin
            .take()
            .ok_or_else(|| "failed to open git stdin".to_string())?;
        child_stdin
            .write_all(input)
            .map_err(|error| format!("failed to write git stdin: {error}"))?;
    }

    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to wait for git {}: {error}", args.join(" ")))?;
    if output.status.success() {
        return String::from_utf8(output.stdout)
            .map_err(|error| format!("git output was not UTF-8: {error}"));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(format!("git {} failed: {}", args.join(" "), stderr.trim()))
}

pub fn export_map_write_failed_error(
    checkpoint_id: impl Into<String>,
    validation_report_id: impl Into<String>,
    target_ref: impl Into<String>,
    parent_commit_id: impl Into<String>,
    created_commit_id: impl Into<String>,
) -> GitExportPlanningError {
    let created_commit_id = created_commit_id.into();
    GitExportPlanningError {
        code: GitExportErrorCode::ExportMapWriteFailed,
        checkpoint_id: Some(checkpoint_id.into()),
        validation_report_id: Some(validation_report_id.into()),
        target_ref: Some(target_ref.into()),
        parent_commit_id: Some(parent_commit_id.into()),
        created_commit_id: Some(created_commit_id.clone()),
        message: format!(
            "Git commit `{created_commit_id}` was created, but the native git_export_map was not persisted"
        ),
    }
}

pub fn ref_update_failed_error(
    checkpoint_id: impl Into<String>,
    validation_report_id: impl Into<String>,
    target_ref: impl Into<String>,
    parent_commit_id: impl Into<String>,
    created_commit_id: impl Into<String>,
) -> GitExportPlanningError {
    let target_ref = target_ref.into();
    let created_commit_id = created_commit_id.into();
    GitExportPlanningError {
        code: GitExportErrorCode::ExportRefUpdateFailed,
        checkpoint_id: Some(checkpoint_id.into()),
        validation_report_id: Some(validation_report_id.into()),
        target_ref: Some(target_ref.clone()),
        parent_commit_id: Some(parent_commit_id.into()),
        created_commit_id: Some(created_commit_id.clone()),
        message: format!(
            "Git commit `{created_commit_id}` was created, but `{target_ref}` was not updated"
        ),
    }
}

struct GitExportExecutionResultBuilder<'a> {
    plan: &'a GitExportWriterPlan,
}

impl<'a> GitExportExecutionResultBuilder<'a> {
    fn new(plan: &'a GitExportWriterPlan) -> Self {
        Self { plan }
    }

    fn failed(
        &self,
        failed_step: GitExportExecutionStep,
        code: GitExportErrorCode,
        created_commit_id: Option<String>,
        summary: GitExportExecutionSummary,
        message: String,
    ) -> GitExportExecutionResult {
        self.result(
            GitExportExecutionLifecycleState::Failed,
            failed_step,
            code,
            created_commit_id,
            summary,
            message,
        )
    }

    fn partial(
        &self,
        failed_step: GitExportExecutionStep,
        code: GitExportErrorCode,
        created_commit_id: Option<String>,
        summary: GitExportExecutionSummary,
        message: String,
    ) -> GitExportExecutionResult {
        self.result(
            GitExportExecutionLifecycleState::Partial,
            failed_step,
            code,
            created_commit_id,
            summary,
            message,
        )
    }

    fn result(
        &self,
        lifecycle_state: GitExportExecutionLifecycleState,
        failed_step: GitExportExecutionStep,
        code: GitExportErrorCode,
        created_commit_id: Option<String>,
        summary: GitExportExecutionSummary,
        message: String,
    ) -> GitExportExecutionResult {
        let error = GitExportExecutionError {
            code,
            failed_step,
            checkpoint_id: self.plan.commit.checkpoint_id.clone(),
            validation_report_id: self.plan.commit.validation_report_id.clone(),
            target_ref: self.plan.ref_update.git_ref.clone(),
            parent_commit_id: self.plan.commit.parent_commit_id.clone(),
            created_commit_id: created_commit_id.clone(),
            message,
        };

        GitExportExecutionResult {
            lifecycle_state,
            checkpoint_id: self.plan.commit.checkpoint_id.clone(),
            validation_report_id: self.plan.commit.validation_report_id.clone(),
            target_ref: self.plan.ref_update.git_ref.clone(),
            parent_commit_id: self.plan.commit.parent_commit_id.clone(),
            created_commit_id,
            summary,
            export_map: None,
            error: Some(error),
        }
    }
}

fn summary(
    commit_created: bool,
    ref_updated: bool,
    export_map_written: bool,
) -> GitExportExecutionSummary {
    let mut completed_steps = Vec::new();
    if commit_created {
        completed_steps.push(GitExportExecutionStep::CommitCreated);
    }
    if ref_updated {
        completed_steps.push(GitExportExecutionStep::RefUpdated);
    }
    if export_map_written {
        completed_steps.push(GitExportExecutionStep::ExportMapWritten);
    }

    GitExportExecutionSummary {
        commit_created,
        ref_updated,
        export_map_written,
        completed_steps,
    }
}

impl GitExportRequest {
    pub fn from_checkpoint(checkpoint: &CheckpointRecord) -> Self {
        Self {
            checkpoint: checkpoint.clone(),
            git_remote: None,
            git_ref: FIXTURE_EXPORTED_GIT_REF.to_string(),
            export_shape: policy_approved_single_checkpoint_shape(),
            validation_report_id: FIXTURE_VALIDATION_REPORT_ID.to_string(),
            generated_output_requirements: Vec::new(),
        }
    }
}

pub fn fixture_git_export_request_from_checkpoint(
    checkpoint: &CheckpointRecord,
) -> GitExportRequest {
    GitExportRequest::from_checkpoint(checkpoint)
}

pub fn validate_git_export_request(request: &GitExportRequest) -> GitExportValidationReport {
    let mut failures = Vec::new();

    if !request.checkpoint.conflict_free {
        failures.push(failure(
            GitExportValidationCheck::ConflictGate,
            GitExportValidationFailureCode::CheckpointConflictedView,
            Some("checkpoint.conflict_free"),
            Some(request.checkpoint.conflict_free.to_string()),
            "checkpoint export requires a conflict-free checkpoint",
        ));
    }

    validate_commit_default_privacy(
        &mut failures,
        "checkpoint.privacy_class",
        request.checkpoint.privacy_class,
    );

    if request.export_shape.kind != ExportShapeKind::SingleCheckpointCommit
        || request.export_shape.parent_policy != "base_checkpoint_git_parent"
    {
        failures.push(failure(
            GitExportValidationCheck::ExportShape,
            GitExportValidationFailureCode::ExportPolicyFailed,
            Some("export_shape"),
            Some(request.export_shape.kind.as_str().to_string()),
            "MVP export supports single_checkpoint_commit with base_checkpoint_git_parent",
        ));
    }

    if request.export_shape.include_sunlight_metadata != "policy_approved_manifest_only" {
        failures.push(failure(
            GitExportValidationCheck::PolicyClass,
            GitExportValidationFailureCode::ExportMetadataPolicyFailed,
            Some("export_shape.include_sunlight_metadata"),
            Some(request.export_shape.include_sunlight_metadata.clone()),
            "Git export may include only policy-approved Sunlight manifests",
        ));
    }

    if request.validation_report_id.trim().is_empty() {
        failures.push(failure(
            GitExportValidationCheck::ReportIntegrity,
            GitExportValidationFailureCode::ValidationReportMissing,
            Some("validation_report_id"),
            None,
            "export-map records must reference a validation report",
        ));
    }

    validate_git_ref(&mut failures, &request.git_ref);
    validate_exact_ids(&mut failures, request);
    validate_no_local_only_evidence(&mut failures, &request.checkpoint);
    validate_generated_output_requirements(&mut failures, request);

    let blocked = failures.len() as u32;
    GitExportValidationReport {
        id: request.validation_report_id.clone(),
        policy_id: CONSERVATIVE_EXPORT_POLICY_ID.to_string(),
        checkpoint_id: request.checkpoint.id.clone(),
        resolved_view_id: request.checkpoint.resolved_view_id.clone(),
        tree_identity: request.checkpoint.tree_identity.clone(),
        git_ref: request.git_ref.clone(),
        ok: failures.is_empty(),
        summary: GitExportValidationSummary {
            records_checked: 2 + request.checkpoint.topic_frontier.len() as u32,
            payloads_checked: 0,
            warnings: 0,
            blocked,
        },
        failures,
    }
}

pub fn validate_persisted_git_export(
    input: PersistedGitExportValidationInput<'_>,
) -> GitExportValidationReport {
    let request = input.request;
    let mut report = validate_git_export_request(request);
    let checkpoint = &request.checkpoint;
    let view = input.resolved_view;

    if input.config.git_interop.sunlight_commit_policy != CONSERVATIVE_SUNLIGHT_COMMIT_POLICY {
        report.failures.push(failure(
            GitExportValidationCheck::RepositoryConfig,
            GitExportValidationFailureCode::ExportPolicyFailed,
            Some("git_interop.sunlight_commit_policy"),
            Some(input.config.git_interop.sunlight_commit_policy.clone()),
            "unsupported repository Git interop policy",
        ));
    }
    if input.config.repository_id != checkpoint.repository_id
        || view.repository_id != checkpoint.repository_id
    {
        report.failures.push(failure(
            GitExportValidationCheck::RepositoryConfig,
            GitExportValidationFailureCode::RepositoryScopeMismatch,
            Some("repository_id"),
            Some(input.config.repository_id.clone()),
            "repository config, checkpoint, and resolved view must have the same repository identity",
        ));
    }
    if view.resolved_view_id != checkpoint.resolved_view_id {
        report.failures.push(failure(
            GitExportValidationCheck::Reachability,
            GitExportValidationFailureCode::ResolvedViewMismatch,
            Some("checkpoint.resolved_view_id"),
            Some(checkpoint.resolved_view_id.clone()),
            "persisted checkpoint must resolve to its exact persisted view",
        ));
    }
    if !view.conflict_free() {
        report.failures.push(failure(
            GitExportValidationCheck::ConflictGate,
            GitExportValidationFailureCode::CheckpointConflictedView,
            Some("resolved_view.conflict_ids"),
            None,
            "checkpoint export requires a conflict-free, non-stale resolved view",
        ));
    }
    if view.path_policy_id != POSIX_CASE_SENSITIVE_PATH_POLICY_ID
        || !input.config.path_policy.case_sensitive
    {
        report.failures.push(failure(
            GitExportValidationCheck::RepositoryConfig,
            GitExportValidationFailureCode::PathPolicyMismatch,
            Some("resolved_view.path_policy_id"),
            Some(view.path_policy_id.clone()),
            "local MVP export requires the configured POSIX case-sensitive path policy",
        ));
    }
    let computed_tree_hash = real_tree_hash(input.entries);
    let view_tree_hash = view
        .tree_identity
        .as_ref()
        .map(|tree| tree.tree_hash.as_str());
    if computed_tree_hash != checkpoint.tree_identity.tree_hash
        || view_tree_hash != Some(checkpoint.tree_identity.tree_hash.as_str())
    {
        report.failures.push(failure(
            GitExportValidationCheck::Reachability,
            GitExportValidationFailureCode::TreeIdentityMismatch,
            Some("checkpoint.tree_identity.tree_hash"),
            Some(checkpoint.tree_identity.tree_hash.clone()),
            "persisted checkpoint entries, resolved view, and checkpoint tree identity must match",
        ));
    }

    let reachable_operation_ids = expanded_operation_order(input.state, &view.topic_frontier);
    let path_policy = PathPolicy::posix_case_sensitive();
    for entry in input.entries.iter().filter(|entry| !entry.tombstone) {
        if let Err(error) = path_policy.validate(&entry.path) {
            report.failures.push(failure(
                GitExportValidationCheck::PathScope,
                GitExportValidationFailureCode::ExportPathUnsafe,
                Some("checkpoint.entries[].path"),
                Some(entry.path.clone()),
                &format!("checkpoint entry failed export path policy: {error}"),
            ));
        }
        if real_content_hash(&entry.bytes) != entry.content_hash {
            report.failures.push(failure(
                GitExportValidationCheck::Reachability,
                GitExportValidationFailureCode::ContentHashMismatch,
                Some("checkpoint.entries[].content_hash"),
                Some(entry.path.clone()),
                "persisted checkpoint content bytes do not match their content hash",
            ));
        }
        let secret_reasons = detect_secret_reasons(&entry.path, &entry.bytes);
        if !secret_reasons.is_empty() {
            report.failures.push(failure(
                GitExportValidationCheck::SecretScan,
                GitExportValidationFailureCode::SecretDetected,
                Some("checkpoint.entries[].path"),
                Some(entry.path.clone()),
                &format!(
                    "checkpoint entry contains secret-like content ({})",
                    secret_reasons.join(",")
                ),
            ));
        }

        match entry.classification.as_str() {
            "source" => {}
            "secret" | "local_only" | "local-only" => report.failures.push(failure(
                GitExportValidationCheck::PolicyClass,
                GitExportValidationFailureCode::SecretOrLocalOnlyRecord,
                Some("checkpoint.entries[].classification"),
                Some(entry.path.clone()),
                "secret and local-only artifacts cannot be exported",
            )),
            "cache" | "ignored" | "log" => report.failures.push(failure(
                GitExportValidationCheck::ExecutionRawExclusion,
                GitExportValidationFailureCode::SecretOrLocalOnlyRecord,
                Some("checkpoint.entries[].classification"),
                Some(entry.path.clone()),
                "cache, ignored, and log artifacts are local-only under conservative export policy",
            )),
            "generated" | "generated_artifact" => {
                if !has_promotion_provenance(
                    entry,
                    &reachable_operation_ids,
                    &input.state.operations,
                    &input.state.promotions,
                ) {
                    report.failures.push(failure(
                        GitExportValidationCheck::GeneratedPolicy,
                        GitExportValidationFailureCode::GeneratedOutputRequiresPromotion,
                        Some("checkpoint.entries[].classification"),
                        Some(entry.path.clone()),
                        "generated artifact requires reachable execution-output promotion provenance",
                    ));
                }
            }
            classification => report.failures.push(failure(
                GitExportValidationCheck::PolicyClass,
                GitExportValidationFailureCode::ExportPolicyFailed,
                Some("checkpoint.entries[].classification"),
                Some(entry.path.clone()),
                &format!(
                    "classification `{classification}` is not exportable under conservative policy"
                ),
            )),
        }
    }

    let reachable_operations = reachable_operation_ids.len() as u32;
    report.summary.records_checked =
        2 + checkpoint.topic_frontier.len() as u32 + reachable_operations;
    report.summary.payloads_checked = input
        .entries
        .iter()
        .filter(|entry| !entry.tombstone)
        .count() as u32;
    report.summary.blocked = report.failures.len() as u32;
    report.ok = report.failures.is_empty();
    report.id = persisted_validation_report_id(&report);
    report
}

fn has_promotion_provenance(
    entry: &RealArtifactEntry,
    reachable_operation_ids: &[String],
    operations: &[crate::repo_state::RealOperationRecord],
    promotions: &[crate::repo_state::RealExecutionPromotionSnapshot],
) -> bool {
    let promoted_execution_output = promotions.iter().any(|promotion| {
        promotion.output_path == entry.path
            && promotion.after_hash == entry.content_hash
            && reachable_operation_ids.contains(&promotion.operation_transaction_id)
            && operations.iter().any(|operation| {
                operation.operation_transaction_id == promotion.operation_transaction_id
                    && operation.topic_revision_id == promotion.topic_revision_id
                    && operation.artifact_effects().iter().any(|effect| {
                        !effect.tombstone
                            && effect.path == entry.path
                            && effect.result_content_hash == entry.content_hash
                    })
            })
    });
    let explicit_classification_operation = operations.iter().any(|operation| {
        operation.mutation == "metadata_set"
            && operation.classification == "generated"
            && reachable_operation_ids.contains(&operation.operation_transaction_id)
            && operation.artifact_effects().iter().any(|effect| {
                !effect.tombstone
                    && effect.path == entry.path
                    && effect.result_content_hash == entry.content_hash
                    && effect.classification == "generated"
            })
    });
    promoted_execution_output || explicit_classification_operation
}

fn persisted_validation_report_id(report: &GitExportValidationReport) -> String {
    let mut hasher = Sha256::new();
    for value in [
        report.policy_id.as_str(),
        report.checkpoint_id.as_str(),
        report.resolved_view_id.as_str(),
        report.tree_identity.repository_id.as_str(),
        report.tree_identity.tree_hash.as_str(),
        report.git_ref.as_str(),
    ] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    for failure in &report.failures {
        hasher.update(failure.check.as_str().as_bytes());
        hasher.update([0]);
        hasher.update(failure.code.as_str().as_bytes());
        hasher.update([0]);
        hasher.update(failure.field.as_deref().unwrap_or("").as_bytes());
        hasher.update([0]);
        hasher.update(failure.value.as_deref().unwrap_or("").as_bytes());
        hasher.update([0]);
    }
    format!("validation_sha256_{:x}", hasher.finalize())
}

pub fn git_export_checkpoint(
    request: GitExportRequest,
) -> Result<GitExportResponse, GitExportError> {
    let validation_report = validate_git_export_request(&request);
    if !validation_report.ok {
        return Err(GitExportError {
            code: GitExportErrorCode::ExportPolicyFailed,
            validation_report,
        });
    }

    let git_commit_ids = vec![FIXTURE_GIT_COMMIT_ID.to_string()];
    let export_map = GitExportMapRecord {
        id: FIXTURE_EXPORT_MAP_ID.to_string(),
        repository_id: request.checkpoint.repository_id.clone(),
        checkpoint_id: request.checkpoint.id.clone(),
        tree_identity: request.checkpoint.tree_identity.clone(),
        git_remote: request.git_remote,
        git_ref: request.git_ref.clone(),
        git_commit_ids: git_commit_ids.clone(),
        export_shape: request.export_shape,
        validation_report_id: validation_report.id.clone(),
        exported_at: FIXTURE_CREATED_AT.to_string(),
        privacy_class: PrivacyClass::CommitDefault,
    };

    Ok(GitExportResponse {
        command: "git.export".to_string(),
        checkpoint_id: request.checkpoint.id,
        validation_report,
        git_ref: request.git_ref,
        git_commit_ids,
        export_map,
    })
}

pub fn fixture_git_export_response_from_checkpoint(
    checkpoint: &CheckpointRecord,
) -> Result<GitExportResponse, GitExportError> {
    git_export_checkpoint(fixture_git_export_request_from_checkpoint(checkpoint))
}

fn validate_writer_repository(input: &GitExportWriterInput) -> Result<(), GitExportPlanningError> {
    let checkpoint = &input.request.checkpoint;
    if input.repository.repository_id != checkpoint.repository_id
        || input.repository.git_root.trim().is_empty()
        || input.repository.sunlight_repo_root.trim().is_empty()
        || input.repository.git_root != input.repository.sunlight_repo_root
    {
        return Err(planning_error(
            GitExportErrorCode::ExportRepositoryInvalid,
            input,
            None,
            None,
            "Git repository root must match the configured Sunlight repository scope",
        ));
    }

    validate_git_export_target_ref(&input.request.git_ref)
        .map_err(|error| planning_error_from_validation(error, input, None))?;

    Ok(())
}

fn validate_writer_report(input: &GitExportWriterInput) -> Result<(), GitExportPlanningError> {
    let request = &input.request;
    let report = &input.validation_report;
    if !report.ok
        || report.id != request.validation_report_id
        || report.checkpoint_id != request.checkpoint.id
        || report.git_ref != request.git_ref
    {
        return Err(planning_error(
            GitExportErrorCode::ExportPolicyFailed,
            input,
            None,
            None,
            "export validation report must pass and match checkpoint, target ref, and report ID",
        ));
    }

    Ok(())
}

fn select_base_parent(
    input: &GitExportWriterInput,
) -> Result<GitExportParentPlan, GitExportPlanningError> {
    select_git_export_base_parent(GitExportParentSelectionInput {
        base_checkpoint_ids: input.base_checkpoint_ids.clone(),
        imported_base_commits: input.imported_base_commits.clone(),
        reachable_commit_ids: input.repository.reachable_commit_ids.clone(),
    })
    .map_err(|error| planning_error_from_validation(error, input, None))
}

pub fn validate_git_export_target_ref(
    git_ref: &str,
) -> Result<GitExportTargetRef, GitExportWriterValidationError> {
    if is_moving_selector(git_ref) {
        return Err(GitExportWriterValidationError {
            code: GitExportErrorCode::ExportTargetRefInvalid,
            target_ref: Some(git_ref.to_string()),
            parent_commit_id: None,
            message: "Git export target must be an exact refs/heads/* ref, not a moving selector"
                .to_string(),
        });
    }

    let Some(branch_name) = git_ref.strip_prefix("refs/heads/") else {
        return Err(GitExportWriterValidationError {
            code: GitExportErrorCode::ExportTargetRefInvalid,
            target_ref: Some(git_ref.to_string()),
            parent_commit_id: None,
            message: "Git export target must use refs/heads/<name>".to_string(),
        });
    };

    if !valid_branch_name(branch_name) {
        return Err(GitExportWriterValidationError {
            code: GitExportErrorCode::ExportTargetRefInvalid,
            target_ref: Some(git_ref.to_string()),
            parent_commit_id: None,
            message: "Git export target ref contains an invalid branch name".to_string(),
        });
    }

    Ok(GitExportTargetRef {
        full_name: git_ref.to_string(),
        branch_name: branch_name.to_string(),
    })
}

pub fn select_git_export_base_parent(
    input: GitExportParentSelectionInput,
) -> Result<GitExportParentPlan, GitExportWriterValidationError> {
    let mut candidates = input
        .imported_base_commits
        .iter()
        .filter(|candidate| input.base_checkpoint_ids.contains(&candidate.checkpoint_id))
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.checkpoint_id
            .cmp(&right.checkpoint_id)
            .then(left.git_commit_id.cmp(&right.git_commit_id))
    });

    match candidates.as_slice() {
        [] => Err(GitExportWriterValidationError {
            code: GitExportErrorCode::ExportParentNotFound,
            target_ref: None,
            parent_commit_id: None,
            message: "no imported base checkpoint Git commit is available for this export"
                .to_string(),
        }),
        [candidate] => {
            if !input
                .reachable_commit_ids
                .contains(&candidate.git_commit_id)
            {
                return Err(GitExportWriterValidationError {
                    code: GitExportErrorCode::ExportRepositoryInvalid,
                    target_ref: None,
                    parent_commit_id: Some(candidate.git_commit_id.clone()),
                    message:
                        "selected parent commit is not reachable in the target repository object database"
                            .to_string(),
                });
            }

            Ok(GitExportParentPlan {
                checkpoint_id: candidate.checkpoint_id.clone(),
                commit_id: candidate.git_commit_id.clone(),
            })
        }
        _ => Err(GitExportWriterValidationError {
            code: GitExportErrorCode::ExportParentAmbiguous,
            target_ref: None,
            parent_commit_id: None,
            message: "more than one imported base checkpoint Git commit matches and no policy selected one"
                .to_string(),
        }),
    }
}

fn plan_ref_update(
    input: &GitExportWriterInput,
    parent: &GitExportParentPlan,
) -> Result<GitExportRefUpdatePlan, GitExportPlanningError> {
    let existing_target_ref = input
        .repository
        .refs
        .iter()
        .find(|state| state.git_ref == input.request.git_ref)
        .cloned();

    plan_git_export_target_ref_update(GitExportTargetRefUpdateInput {
        checkpoint_id: input.request.checkpoint.id.clone(),
        target_ref: input.request.git_ref.clone(),
        selected_parent_commit_id: parent.commit_id.clone(),
        planned_commit_id: input.planned_commit_id.clone(),
        existing_target_ref,
        prior_export_maps: input.prior_export_maps.clone(),
    })
    .map_err(|error| planning_error_from_validation(error, input, None))
}

pub fn plan_git_export_target_ref_update(
    input: GitExportTargetRefUpdateInput,
) -> Result<GitExportRefUpdatePlan, GitExportWriterValidationError> {
    validate_git_export_target_ref(&input.target_ref)?;

    let (expected_old_commit_id, allowed_reason) = match input.existing_target_ref.as_ref() {
        None => (None, GitExportRefUpdateReason::CreateRef),
        Some(state) if state.commit_id == input.selected_parent_commit_id => (
            Some(state.commit_id.clone()),
            GitExportRefUpdateReason::ReplaceSelectedParent,
        ),
        Some(state) if prior_export_matches(&input, &state.commit_id) => (
            Some(state.commit_id.clone()),
            GitExportRefUpdateReason::ReplacePriorExportForSameCheckpoint,
        ),
        Some(state) => {
            return Err(GitExportWriterValidationError {
                code: GitExportErrorCode::ExportTargetRefConflict,
                target_ref: Some(input.target_ref.clone()),
                parent_commit_id: Some(input.selected_parent_commit_id.clone()),
                message: format!(
                    "existing target ref points at `{}` instead of the selected parent or a prior export for this checkpoint",
                    state.commit_id
                ),
            });
        }
    };

    Ok(GitExportRefUpdatePlan {
        git_ref: input.target_ref,
        expected_old_commit_id,
        new_commit_id: input.planned_commit_id,
        allowed_reason,
    })
}

fn prior_export_matches(input: &GitExportTargetRefUpdateInput, commit_id: &str) -> bool {
    input.prior_export_maps.iter().any(|export_map| {
        export_map.checkpoint_id == input.checkpoint_id
            && export_map.git_ref == input.target_ref
            && export_map.git_commit_ids.iter().any(|id| id == commit_id)
    })
}

fn commit_message(request: &GitExportRequest, report: &GitExportValidationReport) -> String {
    format!(
        "Export Sunlight checkpoint {}\n\ncheckpoint_id: {}\nresolved_view_id: {}\nvalidation_report_id: {}\nexport_shape: {}",
        request.checkpoint.id,
        request.checkpoint.id,
        request.checkpoint.resolved_view_id,
        report.id,
        request.export_shape.kind.as_str()
    )
}

fn planning_error(
    code: GitExportErrorCode,
    input: &GitExportWriterInput,
    parent_commit_id: Option<String>,
    created_commit_id: Option<String>,
    message: &str,
) -> GitExportPlanningError {
    GitExportPlanningError {
        code,
        checkpoint_id: Some(input.request.checkpoint.id.clone()),
        validation_report_id: Some(input.validation_report.id.clone()),
        target_ref: Some(input.request.git_ref.clone()),
        parent_commit_id,
        created_commit_id,
        message: message.to_string(),
    }
}

fn planning_error_from_validation(
    error: GitExportWriterValidationError,
    input: &GitExportWriterInput,
    created_commit_id: Option<String>,
) -> GitExportPlanningError {
    GitExportPlanningError {
        code: error.code,
        checkpoint_id: Some(input.request.checkpoint.id.clone()),
        validation_report_id: Some(input.validation_report.id.clone()),
        target_ref: error
            .target_ref
            .or_else(|| Some(input.request.git_ref.clone())),
        parent_commit_id: error.parent_commit_id,
        created_commit_id,
        message: error.message,
    }
}

fn policy_approved_single_checkpoint_shape() -> ExportShape {
    ExportShape {
        kind: ExportShapeKind::SingleCheckpointCommit,
        parent_policy: "base_checkpoint_git_parent".to_string(),
        include_sunlight_metadata: "policy_approved_manifest_only".to_string(),
    }
}

fn validate_commit_default_privacy(
    failures: &mut Vec<GitExportValidationFailure>,
    field: &'static str,
    privacy_class: PrivacyClass,
) {
    if privacy_class != PrivacyClass::CommitDefault {
        failures.push(failure(
            GitExportValidationCheck::PolicyClass,
            GitExportValidationFailureCode::SecretOrLocalOnlyRecord,
            Some(field),
            Some(privacy_class.as_str().to_string()),
            "Git export foundation accepts commit_default metadata only",
        ));
    }
}

fn validate_git_ref(failures: &mut Vec<GitExportValidationFailure>, git_ref: &str) {
    if is_moving_selector(git_ref) {
        failures.push(failure(
            GitExportValidationCheck::GitRef,
            GitExportValidationFailureCode::MovingSelector,
            Some("git_ref"),
            Some(git_ref.to_string()),
            "Git export target must be an exact refs/heads/* ref, not a moving selector",
        ));
        return;
    }

    let Some(branch) = git_ref.strip_prefix("refs/heads/") else {
        failures.push(failure(
            GitExportValidationCheck::GitRef,
            GitExportValidationFailureCode::ExportRefInvalid,
            Some("git_ref"),
            Some(git_ref.to_string()),
            "Git export target must use refs/heads/<name>",
        ));
        return;
    };

    if !valid_branch_name(branch) {
        failures.push(failure(
            GitExportValidationCheck::GitRef,
            GitExportValidationFailureCode::ExportRefInvalid,
            Some("git_ref"),
            Some(git_ref.to_string()),
            "Git export branch contains an invalid ref component",
        ));
    }
}

fn validate_exact_ids(failures: &mut Vec<GitExportValidationFailure>, request: &GitExportRequest) {
    for (field, value) in [
        ("checkpoint.id", request.checkpoint.id.as_str()),
        (
            "checkpoint.resolved_view_id",
            request.checkpoint.resolved_view_id.as_str(),
        ),
        (
            "checkpoint.tree_identity.tree_hash",
            request.checkpoint.tree_identity.tree_hash.as_str(),
        ),
        (
            "validation_report_id",
            request.validation_report_id.as_str(),
        ),
    ] {
        validate_exact_ref(failures, field, value);
    }

    for entry in &request.checkpoint.topic_frontier {
        validate_exact_ref(
            failures,
            "checkpoint.topic_frontier.topic_revision_id",
            &entry.topic_revision_id,
        );
    }
}

fn validate_exact_ref(
    failures: &mut Vec<GitExportValidationFailure>,
    field: &'static str,
    value: &str,
) {
    if is_moving_selector(value) {
        failures.push(failure(
            GitExportValidationCheck::UnsafeReference,
            GitExportValidationFailureCode::MovingSelector,
            Some(field),
            Some(value.to_string()),
            "checkpoint and export records must use exact immutable IDs",
        ));
    }
}

fn validate_no_local_only_evidence(
    failures: &mut Vec<GitExportValidationFailure>,
    checkpoint: &CheckpointRecord,
) {
    for evidence in &checkpoint.evidence_refs {
        match evidence {
            crate::checkpoint::EvidenceRef::Execution(execution) => {
                validate_exact_ref(
                    failures,
                    "checkpoint.evidence_refs.execution_id",
                    &execution.execution_id,
                );
                for (field, value) in [
                    (
                        "checkpoint.evidence_refs.execution_id",
                        execution.execution_id.as_str(),
                    ),
                    (
                        "checkpoint.evidence_refs.resolved_view_id",
                        execution.resolved_view_id.as_str(),
                    ),
                ] {
                    if looks_like_local_only_reference(value) {
                        failures.push(failure(
                            GitExportValidationCheck::ExecutionRawExclusion,
                            GitExportValidationFailureCode::LocalOnlyEvidenceReference,
                            Some(field),
                            Some(value.to_string()),
                            "selected evidence must not reference raw logs, sandboxes, caches, or local paths",
                        ));
                    }
                }
            }
        }
    }
}

fn validate_generated_output_requirements(
    failures: &mut Vec<GitExportValidationFailure>,
    request: &GitExportRequest,
) {
    for requirement in &request.generated_output_requirements {
        failures.push(failure(
            GitExportValidationCheck::GeneratedPolicy,
            GitExportValidationFailureCode::GeneratedOutputRequiresPromotion,
            Some("generated_outputs[].path"),
            Some(requirement.path.clone()),
            &format!(
                "generated source output requires promotion provenance before Git export: {}",
                requirement.provenance_requirement
            ),
        ));
    }
}

fn valid_branch_name(branch: &str) -> bool {
    if branch.is_empty()
        || branch.starts_with('/')
        || branch.ends_with('/')
        || branch.contains("//")
        || branch.contains("..")
        || branch.contains("@{")
        || branch.ends_with(".lock")
    {
        return false;
    }

    branch.split('/').all(|part| {
        !part.is_empty()
            && part != "."
            && part != ".."
            && !part.starts_with('.')
            && !part.ends_with('.')
            && !part.ends_with(".lock")
            && !part.chars().any(|ch| {
                ch.is_ascii_control()
                    || matches!(ch, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\')
            })
    })
}

fn is_moving_selector(value: &str) -> bool {
    matches!(value, "main" | "latest")
        || value.ends_with("@head")
        || value.ends_with("@latest")
        || value.contains("@head/")
        || value.contains("@latest/")
}

fn looks_like_local_only_reference(value: &str) -> bool {
    value.starts_with("file://")
        || value.starts_with('/')
        || value.starts_with("~/")
        || value.contains("/.sunlight/local/")
        || value.contains("/.sunlight/cache/")
        || value.contains("/.sunlight/tmp/")
        || value.contains("/.sunlight/quarantine/")
        || value.contains("/sandbox/")
        || value.contains("/raw-log/")
        || value.contains("/raw-logs/")
        || value.contains("raw_log")
        || value.contains("sandbox")
        || value.contains("local_only")
}

fn failure(
    check: GitExportValidationCheck,
    code: GitExportValidationFailureCode,
    field: Option<&str>,
    value: Option<String>,
    reason: &str,
) -> GitExportValidationFailure {
    GitExportValidationFailure {
        check,
        code,
        field: field.map(str::to_string),
        value,
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint::{
        fixture_checkpoint_from_resolved_view, EvidenceRef, ExecutionEvidenceRef,
        FIXTURE_CHECKPOINT_ID,
    };
    use crate::execution::ExecutionStatus;
    use crate::resolver::{
        fixture_auth_revision, fixture_base_entries, fixture_profile_revision,
        fixture_resolver_input, resolve_fixture_view, TopicRevisionSelection,
        FIXTURE_BASE_CHECKPOINT_ID,
    };
    use std::collections::HashSet;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn validates_policy_approved_metadata_only_export() {
        let checkpoint = fixture_checkpoint();
        let request = fixture_git_export_request_from_checkpoint(&checkpoint);

        let report = validate_git_export_request(&request);

        assert!(report.ok);
        assert_eq!(report.id, FIXTURE_VALIDATION_REPORT_ID);
        assert_eq!(report.summary.blocked, 0);
        assert_eq!(report.summary.payloads_checked, 0);
    }

    #[test]
    fn exports_in_memory_response_with_export_map() {
        let checkpoint = fixture_checkpoint();
        let response = fixture_git_export_response_from_checkpoint(&checkpoint).unwrap();

        assert_eq!(response.command, "git.export");
        assert_eq!(response.checkpoint_id, checkpoint.id);
        assert_eq!(response.git_ref, FIXTURE_EXPORTED_GIT_REF);
        assert_eq!(response.git_commit_ids, vec![FIXTURE_GIT_COMMIT_ID]);
        assert_eq!(response.export_map.checkpoint_id, checkpoint.id);
        assert_eq!(response.export_map.tree_identity, checkpoint.tree_identity);
        assert_eq!(
            response.export_map.validation_report_id,
            FIXTURE_VALIDATION_REPORT_ID
        );
        assert_eq!(
            response.export_map.privacy_class,
            PrivacyClass::CommitDefault
        );
    }

    #[test]
    fn rejects_non_conflict_free_checkpoint() {
        let mut request = fixture_git_export_request_from_checkpoint(&fixture_checkpoint());
        request.checkpoint.conflict_free = false;

        let error = git_export_checkpoint(request).unwrap_err();

        assert_eq!(error.code, GitExportErrorCode::ExportPolicyFailed);
        assert!(error.validation_report.failures.iter().any(|failure| {
            failure.check == GitExportValidationCheck::ConflictGate
                && failure.code == GitExportValidationFailureCode::CheckpointConflictedView
        }));
    }

    #[test]
    fn rejects_non_metadata_only_sunlight_export_policy() {
        let mut request = fixture_git_export_request_from_checkpoint(&fixture_checkpoint());
        request.export_shape.include_sunlight_metadata = "include_raw_objects".to_string();

        let report = validate_git_export_request(&request);

        assert!(!report.ok);
        assert!(report.failures.iter().any(|failure| {
            failure.code == GitExportValidationFailureCode::ExportMetadataPolicyFailed
                && failure.field.as_deref() == Some("export_shape.include_sunlight_metadata")
        }));
    }

    #[test]
    fn rejects_invalid_or_moving_git_refs() {
        for git_ref in [
            "main",
            "refs/tags/v1",
            "refs/heads/bad branch",
            "refs/heads/../bad",
        ] {
            let mut request = fixture_git_export_request_from_checkpoint(&fixture_checkpoint());
            request.git_ref = git_ref.to_string();

            let report = validate_git_export_request(&request);

            assert!(!report.ok, "{git_ref} should be rejected");
            assert!(report.failures.iter().any(|failure| {
                failure.check == GitExportValidationCheck::GitRef
                    && matches!(
                        failure.code,
                        GitExportValidationFailureCode::ExportRefInvalid
                            | GitExportValidationFailureCode::MovingSelector
                    )
            }));
        }
    }

    #[test]
    fn rejects_moving_topic_revision_selectors() {
        let mut request = fixture_git_export_request_from_checkpoint(&fixture_checkpoint());
        request.checkpoint.topic_frontier[0].topic_revision_id =
            "topic_auth_nullability@head".to_string();

        let report = validate_git_export_request(&request);

        assert!(!report.ok);
        assert!(report.failures.iter().any(|failure| {
            failure.check == GitExportValidationCheck::UnsafeReference
                && failure.code == GitExportValidationFailureCode::MovingSelector
                && failure.field.as_deref() == Some("checkpoint.topic_frontier.topic_revision_id")
        }));
    }

    #[test]
    fn rejects_local_only_evidence_references() {
        let mut request = fixture_git_export_request_from_checkpoint(&fixture_checkpoint());
        request
            .checkpoint
            .evidence_refs
            .push(EvidenceRef::Execution(ExecutionEvidenceRef {
                execution_id: ".sunlight/executions/exec_1/raw-logs/stdout.log".to_string(),
                result: ExecutionStatus::Pass,
                resolved_view_id: request.checkpoint.resolved_view_id.clone(),
                tree_identity: request.checkpoint.tree_identity.clone(),
            }));

        let report = validate_git_export_request(&request);

        assert!(!report.ok);
        assert!(report.failures.iter().any(|failure| {
            failure.check == GitExportValidationCheck::ExecutionRawExclusion
                && failure.code == GitExportValidationFailureCode::LocalOnlyEvidenceReference
        }));
    }

    #[test]
    fn rejects_generated_output_without_promotion_provenance() {
        let mut request = fixture_git_export_request_from_checkpoint(&fixture_checkpoint());
        request
            .generated_output_requirements
            .push(GeneratedOutputExportRequirement {
                path: "src/generated/auth.generated.ts".to_string(),
                provenance_requirement: "promotion_operation_id".to_string(),
            });

        let report = validate_git_export_request(&request);

        assert!(!report.ok);
        assert!(report.failures.iter().any(|failure| {
            failure.check == GitExportValidationCheck::GeneratedPolicy
                && failure.code == GitExportValidationFailureCode::GeneratedOutputRequiresPromotion
                && failure.field.as_deref() == Some("generated_outputs[].path")
                && failure.value.as_deref() == Some("src/generated/auth.generated.ts")
                && failure.reason.contains("promotion_operation_id")
        }));
    }

    #[test]
    fn stable_error_codes_match_contract_labels() {
        assert_eq!(
            GitExportErrorCode::ExportPolicyFailed.as_str(),
            "export_policy_failed"
        );
        assert_eq!(
            GitExportErrorCode::ExportParentNotFound.as_str(),
            "export_parent_not_found"
        );
        assert_eq!(
            GitExportErrorCode::ExportParentAmbiguous.as_str(),
            "export_parent_ambiguous"
        );
        assert_eq!(
            GitExportErrorCode::ExportTargetRefInvalid.as_str(),
            "export_target_ref_invalid"
        );
        assert_eq!(
            GitExportErrorCode::ExportTargetRefConflict.as_str(),
            "export_target_ref_conflict"
        );
        assert_eq!(
            GitExportErrorCode::ExportRepositoryInvalid.as_str(),
            "export_repository_invalid"
        );
        assert_eq!(
            GitExportErrorCode::ExportGitFailed.as_str(),
            "export_git_failed"
        );
        assert_eq!(
            GitExportErrorCode::ExportRefUpdateFailed.as_str(),
            "export_ref_update_failed"
        );
        assert_eq!(
            GitExportErrorCode::ExportMapWriteFailed.as_str(),
            "export_map_write_failed"
        );
        assert_eq!(
            GitExportValidationFailureCode::MovingSelector.as_str(),
            "moving_selector"
        );
        assert_eq!(
            GitExportValidationFailureCode::GeneratedOutputRequiresPromotion.as_str(),
            "generated_output_requires_promotion"
        );
        assert_eq!(GitExportValidationCheck::GitRef.as_str(), "git_ref");
        assert_eq!(
            GitExportValidationCheck::GeneratedPolicy.as_str(),
            "generated_policy"
        );
        assert_eq!(
            GitExportExecutionLifecycleState::Exported.as_str(),
            "exported"
        );
        assert_eq!(
            GitExportExecutionLifecycleState::Partial.as_str(),
            "partial"
        );
        assert_eq!(GitExportExecutionLifecycleState::Failed.as_str(), "failed");
        assert_eq!(
            GitExportExecutionStep::CommitCreated.as_str(),
            "commit_created"
        );
        assert_eq!(GitExportExecutionStep::RefUpdated.as_str(), "ref_updated");
        assert_eq!(
            GitExportExecutionStep::ExportMapWritten.as_str(),
            "export_map_written"
        );
    }

    #[test]
    fn writer_plan_selects_base_parent_and_commit_and_ref_update_plan() {
        let input = writer_input();

        let plan = plan_git_export_writer(input).unwrap();

        assert_eq!(plan.parent.checkpoint_id, FIXTURE_BASE_CHECKPOINT_ID);
        assert_eq!(plan.parent.commit_id, fixture_base_commit_id());
        assert_eq!(plan.commit.parent_commit_id, fixture_base_commit_id());
        assert_eq!(plan.commit.planned_commit_id, FIXTURE_GIT_COMMIT_ID);
        assert!(plan.commit.message.contains(FIXTURE_VALIDATION_REPORT_ID));
        assert_eq!(
            plan.ref_update.allowed_reason,
            GitExportRefUpdateReason::ReplaceSelectedParent
        );
        assert_eq!(
            plan.ref_update.expected_old_commit_id,
            Some(fixture_base_commit_id())
        );
        assert_eq!(plan.export_map.git_commit_ids, vec![FIXTURE_GIT_COMMIT_ID]);
        assert_eq!(plan.export_map.checkpoint_id, plan.commit.checkpoint_id);
        assert_eq!(plan.export_map.tree_identity, plan.commit.tree_identity);
        assert_eq!(
            plan.export_map.validation_report_id,
            FIXTURE_VALIDATION_REPORT_ID
        );
    }

    #[test]
    fn target_ref_validation_accepts_full_local_branch_ref() {
        let target =
            validate_git_export_target_ref("refs/heads/sunlight/auth-profile-ready").unwrap();

        assert_eq!(target.full_name, "refs/heads/sunlight/auth-profile-ready");
        assert_eq!(target.branch_name, "sunlight/auth-profile-ready");
    }

    #[test]
    fn target_ref_validation_rejects_invalid_ref_without_git_side_effects() {
        for git_ref in [
            "main",
            "refs/tags/v1",
            "refs/heads/feature..bad",
            "refs/heads/feature/bad.lock/child",
            "refs/heads/feature@{1}",
        ] {
            let error = validate_git_export_target_ref(git_ref).unwrap_err();

            assert_eq!(error.code, GitExportErrorCode::ExportTargetRefInvalid);
            assert_eq!(error.target_ref.as_deref(), Some(git_ref));
            assert_eq!(error.parent_commit_id, None);
        }
    }

    #[test]
    fn parent_selection_helper_selects_single_reachable_base_parent() {
        let parent = select_git_export_base_parent(parent_selection_input()).unwrap();

        assert_eq!(parent.checkpoint_id, FIXTURE_BASE_CHECKPOINT_ID);
        assert_eq!(parent.commit_id, fixture_base_commit_id());
    }

    #[test]
    fn parent_selection_helper_missing_parent_reports_policy_error() {
        let mut input = parent_selection_input();
        input.imported_base_commits.clear();

        let error = select_git_export_base_parent(input).unwrap_err();

        assert_eq!(error.code, GitExportErrorCode::ExportParentNotFound);
        assert_eq!(error.parent_commit_id, None);
    }

    #[test]
    fn parent_selection_helper_ambiguous_parent_reports_policy_error() {
        let mut input = parent_selection_input();
        input
            .base_checkpoint_ids
            .push("checkpoint_base_0002".to_string());
        input.imported_base_commits.push(ImportedBaseGitCommit {
            checkpoint_id: "checkpoint_base_0002".to_string(),
            git_commit_id: "git_sha1_base_2".to_string(),
        });
        input
            .reachable_commit_ids
            .push("git_sha1_base_2".to_string());

        let error = select_git_export_base_parent(input).unwrap_err();

        assert_eq!(error.code, GitExportErrorCode::ExportParentAmbiguous);
        assert_eq!(error.parent_commit_id, None);
    }

    #[test]
    fn parent_selection_helper_rejects_parent_outside_repository() {
        let mut input = parent_selection_input();
        input.reachable_commit_ids.clear();

        let error = select_git_export_base_parent(input).unwrap_err();

        assert_eq!(error.code, GitExportErrorCode::ExportRepositoryInvalid);
        assert_eq!(error.parent_commit_id, Some(fixture_base_commit_id()));
    }

    #[test]
    fn target_ref_update_policy_allows_create_and_parent_replacement() {
        let create = plan_git_export_target_ref_update(ref_update_input(None)).unwrap();
        assert_eq!(create.expected_old_commit_id, None);
        assert_eq!(create.allowed_reason, GitExportRefUpdateReason::CreateRef);

        let replace_parent =
            plan_git_export_target_ref_update(ref_update_input(Some(fixture_base_commit_id())))
                .unwrap();
        assert_eq!(
            replace_parent.expected_old_commit_id,
            Some(fixture_base_commit_id())
        );
        assert_eq!(
            replace_parent.allowed_reason,
            GitExportRefUpdateReason::ReplaceSelectedParent
        );
    }

    #[test]
    fn target_ref_update_policy_allows_prior_export_for_same_checkpoint() {
        let mut input = ref_update_input(Some("git_sha1_prior_export".to_string()));
        let checkpoint = fixture_checkpoint();
        input.prior_export_maps = vec![GitExportMapRecord {
            id: "export_map_prior".to_string(),
            repository_id: checkpoint.repository_id,
            checkpoint_id: FIXTURE_CHECKPOINT_ID.to_string(),
            tree_identity: checkpoint.tree_identity,
            git_remote: None,
            git_ref: FIXTURE_EXPORTED_GIT_REF.to_string(),
            git_commit_ids: vec!["git_sha1_prior_export".to_string()],
            export_shape: policy_approved_single_checkpoint_shape(),
            validation_report_id: FIXTURE_VALIDATION_REPORT_ID.to_string(),
            exported_at: FIXTURE_CREATED_AT.to_string(),
            privacy_class: PrivacyClass::CommitDefault,
        }];

        let plan = plan_git_export_target_ref_update(input).unwrap();

        assert_eq!(
            plan.allowed_reason,
            GitExportRefUpdateReason::ReplacePriorExportForSameCheckpoint
        );
    }

    #[test]
    fn target_ref_update_policy_rejects_conflicting_existing_tip() {
        let error = plan_git_export_target_ref_update(ref_update_input(Some(
            "git_sha1_unrelated".to_string(),
        )))
        .unwrap_err();

        assert_eq!(error.code, GitExportErrorCode::ExportTargetRefConflict);
        assert_eq!(error.target_ref.as_deref(), Some(FIXTURE_EXPORTED_GIT_REF));
        assert_eq!(error.parent_commit_id, Some(fixture_base_commit_id()));
    }

    #[test]
    fn writer_plan_can_create_absent_target_ref() {
        let mut input = writer_input();
        input.repository.refs.clear();

        let plan = plan_git_export_writer(input).unwrap();

        assert_eq!(plan.ref_update.expected_old_commit_id, None);
        assert_eq!(
            plan.ref_update.allowed_reason,
            GitExportRefUpdateReason::CreateRef
        );
    }

    #[test]
    fn writer_plan_allows_replacing_prior_export_for_same_checkpoint() {
        let mut input = writer_input();
        input.repository.refs = vec![GitRefState {
            git_ref: FIXTURE_EXPORTED_GIT_REF.to_string(),
            commit_id: "git_sha1_prior_export".to_string(),
        }];
        input.prior_export_maps = vec![GitExportMapRecord {
            id: "export_map_prior".to_string(),
            repository_id: input.request.checkpoint.repository_id.clone(),
            checkpoint_id: input.request.checkpoint.id.clone(),
            tree_identity: input.request.checkpoint.tree_identity.clone(),
            git_remote: None,
            git_ref: FIXTURE_EXPORTED_GIT_REF.to_string(),
            git_commit_ids: vec!["git_sha1_prior_export".to_string()],
            export_shape: input.request.export_shape.clone(),
            validation_report_id: FIXTURE_VALIDATION_REPORT_ID.to_string(),
            exported_at: FIXTURE_CREATED_AT.to_string(),
            privacy_class: PrivacyClass::CommitDefault,
        }];

        let plan = plan_git_export_writer(input).unwrap();

        assert_eq!(
            plan.ref_update.allowed_reason,
            GitExportRefUpdateReason::ReplacePriorExportForSameCheckpoint
        );
        assert_eq!(
            GitExportRefUpdateReason::ReplacePriorExportForSameCheckpoint.as_str(),
            "replace_prior_export_for_same_checkpoint"
        );
    }

    #[test]
    fn writer_plan_missing_parent_fails_before_commit_plan() {
        let mut input = writer_input();
        input.imported_base_commits.clear();

        let error = plan_git_export_writer(input).unwrap_err();

        assert_eq!(error.code, GitExportErrorCode::ExportParentNotFound);
        assert_eq!(error.created_commit_id, None);
        assert_eq!(error.target_ref.as_deref(), Some(FIXTURE_EXPORTED_GIT_REF));
    }

    #[test]
    fn writer_plan_ambiguous_parent_fails() {
        let mut input = writer_input();
        input
            .base_checkpoint_ids
            .push("checkpoint_base_0002".to_string());
        input.imported_base_commits.push(ImportedBaseGitCommit {
            checkpoint_id: "checkpoint_base_0002".to_string(),
            git_commit_id: "git_sha1_base_2".to_string(),
        });
        input
            .repository
            .reachable_commit_ids
            .push("git_sha1_base_2".to_string());

        let error = plan_git_export_writer(input).unwrap_err();

        assert_eq!(error.code, GitExportErrorCode::ExportParentAmbiguous);
        assert_eq!(error.parent_commit_id, None);
    }

    #[test]
    fn writer_plan_rejects_parent_outside_repository_objects() {
        let mut input = writer_input();
        input.repository.reachable_commit_ids.clear();

        let error = plan_git_export_writer(input).unwrap_err();

        assert_eq!(error.code, GitExportErrorCode::ExportRepositoryInvalid);
        assert_eq!(error.parent_commit_id, Some(fixture_base_commit_id()));
    }

    #[test]
    fn writer_plan_ref_conflict_fails_before_update() {
        let mut input = writer_input();
        input.repository.refs = vec![GitRefState {
            git_ref: FIXTURE_EXPORTED_GIT_REF.to_string(),
            commit_id: "git_sha1_unrelated".to_string(),
        }];

        let error = plan_git_export_writer(input).unwrap_err();

        assert_eq!(error.code, GitExportErrorCode::ExportTargetRefConflict);
        assert_eq!(error.parent_commit_id, Some(fixture_base_commit_id()));
        assert_eq!(error.created_commit_id, None);
    }

    #[test]
    fn writer_plan_rejects_invalid_target_ref() {
        let mut input = writer_input();
        input.request.git_ref = "main".to_string();
        input.validation_report.git_ref = "main".to_string();

        let error = plan_git_export_writer(input).unwrap_err();

        assert_eq!(error.code, GitExportErrorCode::ExportTargetRefInvalid);
    }

    #[test]
    fn writer_plan_policy_failure_stops_before_parent_selection() {
        let mut input = writer_input();
        input.validation_report.ok = false;
        input.validation_report.summary.blocked = 1;
        input.validation_report.failures.push(failure(
            GitExportValidationCheck::PolicyClass,
            GitExportValidationFailureCode::ExportPolicyFailed,
            Some("fixture"),
            None,
            "fixture failure",
        ));

        let error = plan_git_export_writer(input).unwrap_err();

        assert_eq!(error.code, GitExportErrorCode::ExportPolicyFailed);
        assert_eq!(error.parent_commit_id, None);
        assert_eq!(error.created_commit_id, None);
    }

    #[test]
    fn writer_plan_rejects_stale_validation_report_target() {
        let mut input = writer_input();
        input.validation_report.git_ref = "refs/heads/other".to_string();

        let error = plan_git_export_writer(input).unwrap_err();

        assert_eq!(error.code, GitExportErrorCode::ExportPolicyFailed);
        assert!(error.message.contains("match checkpoint"));
    }

    #[test]
    fn partial_failure_errors_include_created_commit_context() {
        let map_error = export_map_write_failed_error(
            FIXTURE_CHECKPOINT_ID,
            FIXTURE_VALIDATION_REPORT_ID,
            FIXTURE_EXPORTED_GIT_REF,
            fixture_base_commit_id(),
            FIXTURE_GIT_COMMIT_ID,
        );
        let ref_error = ref_update_failed_error(
            FIXTURE_CHECKPOINT_ID,
            FIXTURE_VALIDATION_REPORT_ID,
            FIXTURE_EXPORTED_GIT_REF,
            fixture_base_commit_id(),
            FIXTURE_GIT_COMMIT_ID,
        );

        assert_eq!(map_error.code, GitExportErrorCode::ExportMapWriteFailed);
        assert_eq!(ref_error.code, GitExportErrorCode::ExportRefUpdateFailed);
        assert_eq!(
            map_error.created_commit_id.as_deref(),
            Some(FIXTURE_GIT_COMMIT_ID)
        );
        assert_eq!(
            ref_error.parent_commit_id.as_deref(),
            Some(fixture_base_commit_id().as_str())
        );
    }

    #[test]
    fn fixture_execution_success_records_commit_ref_and_export_map() {
        let plan = plan_git_export_writer(writer_input()).unwrap();

        let result =
            execute_git_export_writer_plan_fixture(&plan, GitExportExecutionFixture::success());

        assert_eq!(
            result.lifecycle_state,
            GitExportExecutionLifecycleState::Exported
        );
        assert_eq!(
            result.created_commit_id.as_deref(),
            Some(FIXTURE_GIT_COMMIT_ID)
        );
        assert_eq!(result.summary.commit_created, true);
        assert_eq!(result.summary.ref_updated, true);
        assert_eq!(result.summary.export_map_written, true);
        assert_eq!(
            result.summary.completed_steps,
            vec![
                GitExportExecutionStep::CommitCreated,
                GitExportExecutionStep::RefUpdated,
                GitExportExecutionStep::ExportMapWritten,
            ]
        );
        assert_eq!(result.export_map, Some(plan.export_map));
        assert_eq!(result.error, None);
    }

    #[test]
    fn fixture_execution_commit_failure_reports_failed_summary() {
        let plan = plan_git_export_writer(writer_input()).unwrap();
        let mut fixture = GitExportExecutionFixture::success();
        fixture.commit_creation = GitExportExecutionStepFixture::fail("fixture commit failed");

        let result = execute_git_export_writer_plan_fixture(&plan, fixture);

        assert_eq!(
            result.lifecycle_state,
            GitExportExecutionLifecycleState::Failed
        );
        assert_eq!(result.created_commit_id, None);
        assert_eq!(result.summary.commit_created, false);
        assert_eq!(result.summary.ref_updated, false);
        assert_eq!(result.summary.export_map_written, false);
        assert!(result.summary.completed_steps.is_empty());
        let error = result.error.unwrap();
        assert_eq!(error.code, GitExportErrorCode::ExportGitFailed);
        assert_eq!(error.failed_step, GitExportExecutionStep::CommitCreated);
        assert_eq!(error.created_commit_id, None);
        assert_eq!(error.message, "fixture commit failed");
    }

    #[test]
    fn fixture_execution_ref_update_failure_reports_partial_summary() {
        let plan = plan_git_export_writer(writer_input()).unwrap();
        let mut fixture = GitExportExecutionFixture::success();
        fixture.ref_update = GitExportExecutionStepFixture::fail("fixture ref update failed");

        let result = execute_git_export_writer_plan_fixture(&plan, fixture);

        assert_eq!(
            result.lifecycle_state,
            GitExportExecutionLifecycleState::Partial
        );
        assert_eq!(
            result.created_commit_id.as_deref(),
            Some(FIXTURE_GIT_COMMIT_ID)
        );
        assert_eq!(result.summary.commit_created, true);
        assert_eq!(result.summary.ref_updated, false);
        assert_eq!(result.summary.export_map_written, false);
        assert_eq!(
            result.summary.completed_steps,
            vec![GitExportExecutionStep::CommitCreated]
        );
        assert_eq!(result.export_map, None);
        let error = result.error.unwrap();
        assert_eq!(error.code, GitExportErrorCode::ExportRefUpdateFailed);
        assert_eq!(error.failed_step, GitExportExecutionStep::RefUpdated);
        assert_eq!(
            error.created_commit_id.as_deref(),
            Some(FIXTURE_GIT_COMMIT_ID)
        );
        assert_eq!(error.target_ref, FIXTURE_EXPORTED_GIT_REF);
    }

    #[test]
    fn fixture_execution_export_map_failure_reports_partial_summary() {
        let plan = plan_git_export_writer(writer_input()).unwrap();
        let mut fixture = GitExportExecutionFixture::success();
        fixture.export_map_write =
            GitExportExecutionStepFixture::fail("fixture export map write failed");

        let result = execute_git_export_writer_plan_fixture(&plan, fixture);

        assert_eq!(
            result.lifecycle_state,
            GitExportExecutionLifecycleState::Partial
        );
        assert_eq!(
            result.created_commit_id.as_deref(),
            Some(FIXTURE_GIT_COMMIT_ID)
        );
        assert_eq!(result.summary.commit_created, true);
        assert_eq!(result.summary.ref_updated, true);
        assert_eq!(result.summary.export_map_written, false);
        assert_eq!(
            result.summary.completed_steps,
            vec![
                GitExportExecutionStep::CommitCreated,
                GitExportExecutionStep::RefUpdated,
            ]
        );
        assert_eq!(result.export_map, None);
        let error = result.error.unwrap();
        assert_eq!(error.code, GitExportErrorCode::ExportMapWriteFailed);
        assert_eq!(error.failed_step, GitExportExecutionStep::ExportMapWritten);
        assert_eq!(
            error.created_commit_id.as_deref(),
            Some(FIXTURE_GIT_COMMIT_ID)
        );
        assert_eq!(
            error.validation_report_id,
            FIXTURE_VALIDATION_REPORT_ID.to_string()
        );
    }

    #[test]
    fn writer_plan_rejects_git_root_outside_sunlight_scope() {
        let mut input = writer_input();
        input.repository.git_root = "/tmp/other".to_string();

        let error = plan_git_export_writer(input).unwrap_err();

        assert_eq!(error.code, GitExportErrorCode::ExportRepositoryInvalid);
    }

    #[test]
    fn git_export_writes_real_commit_and_persists_map() {
        let repo = LocalGitRepo::new();
        let base_commit_id = repo.create_commit("base", fixture_base_files(), None);
        let unrelated_commit_id = repo.create_commit(
            "unrelated",
            vec![file("unrelated.txt", b"keep\n", false)],
            None,
        );
        repo.update_ref("refs/heads/unrelated", &unrelated_commit_id);
        let mut input = writer_input_for_repo(&repo, &base_commit_id);
        input.repository.refs.clear();
        let mut store = InMemoryGitExportMapStore::default();

        let result =
            execute_local_git_export_writer(input, fixture_checkpoint_files(), &mut store).unwrap();

        assert_eq!(
            result.lifecycle_state,
            GitExportExecutionLifecycleState::Exported
        );
        let commit_id = result.created_commit_id.as_deref().unwrap();
        assert_eq!(repo.rev_parse(FIXTURE_EXPORTED_GIT_REF), commit_id);
        assert_eq!(repo.rev_parse("refs/heads/unrelated"), unrelated_commit_id);
        assert_eq!(store.export_maps.len(), 1);
        assert_eq!(store.export_maps[0].git_commit_ids, vec![commit_id]);
        assert_eq!(result.export_map.unwrap(), store.export_maps[0]);
    }

    #[test]
    fn git_export_commit_tree_matches_checkpoint_files() {
        let repo = LocalGitRepo::new();
        let base_commit_id = repo.create_commit("base", fixture_base_files(), None);
        repo.update_ref(FIXTURE_EXPORTED_GIT_REF, &base_commit_id);
        let input = writer_input_for_repo(&repo, &base_commit_id);
        let mut store = InMemoryGitExportMapStore::default();

        let result =
            execute_local_git_export_writer(input, fixture_checkpoint_files(), &mut store).unwrap();
        let commit_id = result.created_commit_id.unwrap();

        assert_eq!(
            repo.ls_tree(&commit_id),
            vec![
                "100644 .sunlight/export-manifest.json".to_string(),
                "100644 src/auth.rs".to_string(),
                "100644 src/profile.rs".to_string(),
                "100755 bin/run-auth-check".to_string(),
            ]
        );
        assert_eq!(
            repo.cat_file(&commit_id, ".sunlight/export-manifest.json"),
            b"{\"policy\":\"approved_manifest_only\"}\n"
        );
        assert_eq!(
            repo.cat_file(&commit_id, "src/auth.rs"),
            b"pub fn auth() {}\n"
        );
        assert_eq!(
            repo.cat_file(&commit_id, "src/profile.rs"),
            b"pub fn profile() {}\n"
        );
        assert_eq!(
            repo.cat_file(&commit_id, "bin/run-auth-check"),
            b"#!/bin/sh\n"
        );
    }

    #[test]
    fn git_export_ignores_working_tree_and_index_files() {
        let repo = LocalGitRepo::new();
        let base_commit_id = repo.create_commit("base", fixture_base_files(), None);
        repo.update_ref(FIXTURE_EXPORTED_GIT_REF, &base_commit_id);
        repo.update_ref("refs/heads/main", &base_commit_id);
        repo.git(&["symbolic-ref", "HEAD", "refs/heads/main"]);
        repo.git(&["checkout", "--quiet", "HEAD"]);
        repo.write_worktree_file("untracked.txt", b"untracked\n");
        repo.write_worktree_file("staged.txt", b"staged\n");
        repo.git(&["add", "staged.txt"]);
        repo.write_worktree_file("src/base.rs", b"dirty tracked worktree\n");
        let input = writer_input_for_repo(&repo, &base_commit_id);
        let mut store = InMemoryGitExportMapStore::default();

        let result =
            execute_local_git_export_writer(input, fixture_checkpoint_files(), &mut store).unwrap();

        let tree = repo.ls_tree(result.created_commit_id.as_deref().unwrap());
        assert!(!tree.iter().any(|entry| entry.contains("untracked.txt")));
        assert!(!tree.iter().any(|entry| entry.contains("staged.txt")));
        assert!(!tree.iter().any(|entry| entry.contains("src/base.rs")));
    }

    #[test]
    fn temporary_git_index_path_is_unique_for_parallel_writers() {
        let threads = 16;
        let paths_per_thread = 128;
        let barrier = Arc::new(Barrier::new(threads));
        let mut handles = Vec::new();

        for _ in 0..threads {
            let barrier = Arc::clone(&barrier);
            handles.push(thread::spawn(move || {
                barrier.wait();
                (0..paths_per_thread)
                    .map(|_| temporary_git_index_path())
                    .collect::<Vec<_>>()
            }));
        }

        let paths = handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        let unique_paths = paths.iter().collect::<HashSet<_>>();

        assert_eq!(paths.len(), threads * paths_per_thread);
        assert_eq!(unique_paths.len(), paths.len());
    }

    #[test]
    fn git_export_selects_base_parent() {
        let repo = LocalGitRepo::new();
        let base_commit_id = repo.create_commit("base", fixture_base_files(), None);
        repo.update_ref(FIXTURE_EXPORTED_GIT_REF, &base_commit_id);
        let input = writer_input_for_repo(&repo, &base_commit_id);
        let mut store = InMemoryGitExportMapStore::default();

        let result =
            execute_local_git_export_writer(input, fixture_checkpoint_files(), &mut store).unwrap();

        assert_eq!(
            repo.commit_parent(result.created_commit_id.as_deref().unwrap()),
            base_commit_id
        );
    }

    #[test]
    fn git_export_missing_parent_fails_without_ref_update() {
        let repo = LocalGitRepo::new();
        let base_commit_id = repo.create_commit("base", fixture_base_files(), None);
        let mut input = writer_input_for_repo(&repo, &base_commit_id);
        input.imported_base_commits.clear();
        input.repository.refs.clear();
        let mut store = InMemoryGitExportMapStore::default();

        let error = execute_local_git_export_writer(input, fixture_checkpoint_files(), &mut store)
            .unwrap_err();

        assert_eq!(error.code, GitExportErrorCode::ExportParentNotFound);
        assert_eq!(repo.try_rev_parse(FIXTURE_EXPORTED_GIT_REF), None);
        assert!(store.export_maps.is_empty());
    }

    #[test]
    fn git_export_ref_conflict_fails_and_leaves_ref_unchanged() {
        let repo = LocalGitRepo::new();
        let base_commit_id = repo.create_commit("base", fixture_base_files(), None);
        let conflict_commit_id =
            repo.create_commit("conflict", vec![file("other.txt", b"other\n", false)], None);
        repo.update_ref(FIXTURE_EXPORTED_GIT_REF, &conflict_commit_id);
        let mut input = writer_input_for_repo(&repo, &base_commit_id);
        input.repository.refs = vec![GitRefState {
            git_ref: FIXTURE_EXPORTED_GIT_REF.to_string(),
            commit_id: conflict_commit_id.clone(),
        }];
        let mut store = InMemoryGitExportMapStore::default();

        let error = execute_local_git_export_writer(input, fixture_checkpoint_files(), &mut store)
            .unwrap_err();

        assert_eq!(error.code, GitExportErrorCode::ExportTargetRefConflict);
        assert_eq!(repo.rev_parse(FIXTURE_EXPORTED_GIT_REF), conflict_commit_id);
        assert!(store.export_maps.is_empty());
    }

    #[test]
    fn git_export_map_failure_remains_partial() {
        let repo = LocalGitRepo::new();
        let base_commit_id = repo.create_commit("base", fixture_base_files(), None);
        repo.update_ref(FIXTURE_EXPORTED_GIT_REF, &base_commit_id);
        let input = writer_input_for_repo(&repo, &base_commit_id);
        let mut store = FailingGitExportMapStore;

        let result =
            execute_local_git_export_writer(input, fixture_checkpoint_files(), &mut store).unwrap();

        assert_eq!(
            result.lifecycle_state,
            GitExportExecutionLifecycleState::Partial
        );
        assert_eq!(result.summary.commit_created, true);
        assert_eq!(result.summary.ref_updated, true);
        assert_eq!(result.summary.export_map_written, false);
        assert_eq!(result.export_map, None);
        assert_eq!(
            result.error.as_ref().unwrap().code,
            GitExportErrorCode::ExportMapWriteFailed
        );
        assert_eq!(
            repo.rev_parse(FIXTURE_EXPORTED_GIT_REF),
            result.created_commit_id.unwrap()
        );
    }

    fn writer_input() -> GitExportWriterInput {
        let checkpoint = fixture_checkpoint();
        let request = fixture_git_export_request_from_checkpoint(&checkpoint);
        let validation_report = validate_git_export_request(&request);

        GitExportWriterInput {
            request,
            validation_report,
            repository: GitExportRepositoryState {
                repository_id: checkpoint.repository_id.clone(),
                git_root: "/repo/basic-app".to_string(),
                sunlight_repo_root: "/repo/basic-app".to_string(),
                reachable_commit_ids: vec![fixture_base_commit_id()],
                refs: vec![GitRefState {
                    git_ref: FIXTURE_EXPORTED_GIT_REF.to_string(),
                    commit_id: fixture_base_commit_id(),
                }],
            },
            base_checkpoint_ids: vec![FIXTURE_BASE_CHECKPOINT_ID.to_string()],
            imported_base_commits: vec![ImportedBaseGitCommit {
                checkpoint_id: FIXTURE_BASE_CHECKPOINT_ID.to_string(),
                git_commit_id: fixture_base_commit_id(),
            }],
            prior_export_maps: Vec::new(),
            planned_commit_id: FIXTURE_GIT_COMMIT_ID.to_string(),
            export_map_id: FIXTURE_EXPORT_MAP_ID.to_string(),
            exported_at: FIXTURE_CREATED_AT.to_string(),
        }
    }

    fn parent_selection_input() -> GitExportParentSelectionInput {
        GitExportParentSelectionInput {
            base_checkpoint_ids: vec![FIXTURE_BASE_CHECKPOINT_ID.to_string()],
            imported_base_commits: vec![ImportedBaseGitCommit {
                checkpoint_id: FIXTURE_BASE_CHECKPOINT_ID.to_string(),
                git_commit_id: fixture_base_commit_id(),
            }],
            reachable_commit_ids: vec![fixture_base_commit_id()],
        }
    }

    fn ref_update_input(existing_commit_id: Option<String>) -> GitExportTargetRefUpdateInput {
        GitExportTargetRefUpdateInput {
            checkpoint_id: FIXTURE_CHECKPOINT_ID.to_string(),
            target_ref: FIXTURE_EXPORTED_GIT_REF.to_string(),
            selected_parent_commit_id: fixture_base_commit_id(),
            planned_commit_id: FIXTURE_GIT_COMMIT_ID.to_string(),
            existing_target_ref: existing_commit_id.map(|commit_id| GitRefState {
                git_ref: FIXTURE_EXPORTED_GIT_REF.to_string(),
                commit_id,
            }),
            prior_export_maps: Vec::new(),
        }
    }

    fn fixture_base_commit_id() -> String {
        "git_sha1_base_parent_0001".to_string()
    }

    fn writer_input_for_repo(repo: &LocalGitRepo, base_commit_id: &str) -> GitExportWriterInput {
        let mut input = writer_input();
        let root = repo.path().to_string_lossy().to_string();
        input.repository.git_root = root.clone();
        input.repository.sunlight_repo_root = root;
        input.repository.reachable_commit_ids = vec![base_commit_id.to_string()];
        input.repository.refs = vec![GitRefState {
            git_ref: FIXTURE_EXPORTED_GIT_REF.to_string(),
            commit_id: base_commit_id.to_string(),
        }];
        input.imported_base_commits = vec![ImportedBaseGitCommit {
            checkpoint_id: FIXTURE_BASE_CHECKPOINT_ID.to_string(),
            git_commit_id: base_commit_id.to_string(),
        }];
        input.planned_commit_id = "planned_commit_id_replaced_by_real_git".to_string();
        input
    }

    fn fixture_base_files() -> Vec<GitExportContentFile> {
        vec![file("src/base.rs", b"pub fn base() {}\n", false)]
    }

    fn fixture_checkpoint_files() -> Vec<GitExportContentFile> {
        vec![
            file("src/auth.rs", b"pub fn auth() {}\n", false),
            file("src/profile.rs", b"pub fn profile() {}\n", false),
            file("bin/run-auth-check", b"#!/bin/sh\n", true),
            file(
                ".sunlight/export-manifest.json",
                b"{\"policy\":\"approved_manifest_only\"}\n",
                false,
            ),
        ]
    }

    fn file(path: &str, bytes: &[u8], executable: bool) -> GitExportContentFile {
        GitExportContentFile {
            path: path.to_string(),
            bytes: bytes.to_vec(),
            executable,
        }
    }

    struct FailingGitExportMapStore;

    impl GitExportMapStore for FailingGitExportMapStore {
        fn persist_git_export_map(
            &mut self,
            _export_map: GitExportMapRecord,
        ) -> Result<PersistedGitExportMap, String> {
            Err("simulated export-map write failure".to_string())
        }
    }

    struct LocalGitRepo {
        _tempdir: TestTempDir,
        path: PathBuf,
    }

    impl LocalGitRepo {
        fn new() -> Self {
            let tempdir = TestTempDir::new();
            let path = tempdir.path().to_path_buf();
            run_command(&path, &["git", "init", "--quiet"]);
            Self {
                _tempdir: tempdir,
                path,
            }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn create_commit(
            &self,
            message: &str,
            files: Vec<GitExportContentFile>,
            parent: Option<&str>,
        ) -> String {
            let tree_id = write_git_tree(&self.path, &files).unwrap();
            let mut args = vec!["commit-tree", tree_id.as_str()];
            if let Some(parent) = parent {
                args.push("-p");
                args.push(parent);
            }
            let env = [
                ("GIT_AUTHOR_NAME", "Test Author"),
                ("GIT_AUTHOR_EMAIL", "test@example.invalid"),
                ("GIT_AUTHOR_DATE", "2026-07-04T00:00:00Z"),
                ("GIT_COMMITTER_NAME", "Test Author"),
                ("GIT_COMMITTER_EMAIL", "test@example.invalid"),
                ("GIT_COMMITTER_DATE", "2026-07-04T00:00:00Z"),
            ];
            run_git(&self.path, &args, Some(message.as_bytes()), &env)
                .unwrap()
                .trim()
                .to_string()
        }

        fn update_ref(&self, git_ref: &str, commit_id: &str) {
            self.git(&["update-ref", git_ref, commit_id]);
        }

        fn rev_parse(&self, rev: &str) -> String {
            self.git(&["rev-parse", rev]).trim().to_string()
        }

        fn try_rev_parse(&self, rev: &str) -> Option<String> {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(&self.path)
                .args(["rev-parse", rev])
                .output()
                .unwrap();
            output
                .status
                .success()
                .then(|| String::from_utf8(output.stdout).unwrap().trim().to_string())
        }

        fn ls_tree(&self, commit_id: &str) -> Vec<String> {
            let mut entries = self
                .git(&["ls-tree", "-r", "--format=%(objectmode) %(path)", commit_id])
                .lines()
                .map(str::to_string)
                .collect::<Vec<_>>();
            entries.sort();
            entries
        }

        fn cat_file(&self, commit_id: &str, path: &str) -> Vec<u8> {
            std::process::Command::new("git")
                .arg("-C")
                .arg(&self.path)
                .args(["show", &format!("{commit_id}:{path}")])
                .output()
                .unwrap()
                .stdout
        }

        fn commit_parent(&self, commit_id: &str) -> String {
            self.git(&["show", "-s", "--format=%P", commit_id])
                .trim()
                .to_string()
        }

        fn write_worktree_file(&self, path: &str, bytes: &[u8]) {
            let path = self.path.join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(path, bytes).unwrap();
        }

        fn git(&self, args: &[&str]) -> String {
            let output = std::process::Command::new("git")
                .current_dir(&self.path)
                .args(args)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr)
            );
            String::from_utf8(output.stdout).unwrap()
        }
    }

    struct TestTempDir {
        path: PathBuf,
    }

    impl TestTempDir {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "sunlight-git-export-test-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestTempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn run_command(git_root: &Path, args: &[&str]) -> String {
        let output = std::process::Command::new(args[0])
            .current_dir(git_root)
            .args(&args[1..])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }

    fn fixture_checkpoint() -> CheckpointRecord {
        fixture_checkpoint_from_resolved_view(&conflict_free_view(), None).unwrap()
    }

    fn conflict_free_view() -> crate::resolver::ResolvedViewResult {
        let auth = fixture_auth_revision();
        let profile = fixture_profile_revision();
        resolve_fixture_view(
            fixture_resolver_input(vec![
                selection(&auth.topic_id, &auth.revision_id),
                selection(&profile.topic_id, &profile.revision_id),
            ]),
            fixture_base_entries(),
            vec![auth, profile],
        )
    }

    fn selection(topic_id: &str, revision_id: &str) -> TopicRevisionSelection {
        TopicRevisionSelection {
            topic_id: topic_id.to_string(),
            revision_id: revision_id.to_string(),
        }
    }
}

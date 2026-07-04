use crate::checkpoint::{
    CheckpointRecord, ExportShape, ExportShapeKind, GitExportMapRecord, FIXTURE_CREATED_AT,
    FIXTURE_EXPORTED_GIT_REF, FIXTURE_EXPORT_MAP_ID, FIXTURE_GIT_COMMIT_ID,
    FIXTURE_VALIDATION_REPORT_ID,
};
use crate::records::PrivacyClass;
use crate::resolver::SingleRepoTree;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportRequest {
    pub checkpoint: CheckpointRecord,
    pub git_remote: Option<String>,
    pub git_ref: String,
    pub export_shape: ExportShape,
    pub validation_report_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportValidationReport {
    pub id: String,
    pub checkpoint_id: String,
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
    PolicyClass,
    UnsafeReference,
    ExportShape,
    GitRef,
    ExecutionRawExclusion,
    ReportIntegrity,
}

impl GitExportValidationCheck {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ConflictGate => "conflict_gate",
            Self::PolicyClass => "policy_class",
            Self::UnsafeReference => "unsafe_reference",
            Self::ExportShape => "export_shape",
            Self::GitRef => "git_ref",
            Self::ExecutionRawExclusion => "execution_raw_exclusion",
            Self::ReportIntegrity => "report_integrity",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitExportValidationFailureCode {
    CheckpointConflictedView,
    ExportPolicyFailed,
    ExportMetadataPolicyFailed,
    ExportRefInvalid,
    MovingSelector,
    LocalOnlyEvidenceReference,
    SecretOrLocalOnlyRecord,
    ValidationReportMissing,
}

impl GitExportValidationFailureCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CheckpointConflictedView => "checkpoint_conflicted_view",
            Self::ExportPolicyFailed => "export_policy_failed",
            Self::ExportMetadataPolicyFailed => "export_metadata_policy_failed",
            Self::ExportRefInvalid => "export_ref_invalid",
            Self::MovingSelector => "moving_selector",
            Self::LocalOnlyEvidenceReference => "local_only_evidence_reference",
            Self::SecretOrLocalOnlyRecord => "secret_or_local_only_record",
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

    let blocked = failures.len() as u32;
    GitExportValidationReport {
        id: request.validation_report_id.clone(),
        checkpoint_id: request.checkpoint.id.clone(),
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
        assert_eq!(GitExportValidationCheck::GitRef.as_str(), "git_ref");
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
        let target = validate_git_export_target_ref("refs/heads/sunlight/auth-profile-ready")
            .unwrap();

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

        let replace_parent = plan_git_export_target_ref_update(ref_update_input(Some(
            fixture_base_commit_id(),
        )))
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

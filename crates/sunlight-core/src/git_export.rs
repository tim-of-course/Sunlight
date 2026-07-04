use crate::checkpoint::{
    CheckpointRecord, ExportShape, ExportShapeKind, GitExportMapRecord, FIXTURE_CREATED_AT,
    FIXTURE_EXPORTED_GIT_REF, FIXTURE_EXPORT_MAP_ID, FIXTURE_GIT_COMMIT_ID,
    FIXTURE_VALIDATION_REPORT_ID,
};
use crate::records::PrivacyClass;

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
    ExportGitFailed,
    ExportMapWriteFailed,
}

impl GitExportErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExportPolicyFailed => "export_policy_failed",
            Self::ExportParentNotFound => "export_parent_not_found",
            Self::ExportGitFailed => "export_git_failed",
            Self::ExportMapWriteFailed => "export_map_write_failed",
        }
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
        ("validation_report_id", request.validation_report_id.as_str()),
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
    };
    use crate::execution::ExecutionStatus;
    use crate::resolver::{
        fixture_auth_revision, fixture_base_entries, fixture_profile_revision,
        fixture_resolver_input, resolve_fixture_view, TopicRevisionSelection,
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
        assert_eq!(response.export_map.privacy_class, PrivacyClass::CommitDefault);
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
                && failure.field.as_deref()
                    == Some("checkpoint.topic_frontier.topic_revision_id")
        }));
    }

    #[test]
    fn rejects_local_only_evidence_references() {
        let mut request = fixture_git_export_request_from_checkpoint(&fixture_checkpoint());
        request.checkpoint.evidence_refs.push(EvidenceRef::Execution(
            ExecutionEvidenceRef {
                execution_id: ".sunlight/executions/exec_1/raw-logs/stdout.log".to_string(),
                result: ExecutionStatus::Pass,
                resolved_view_id: request.checkpoint.resolved_view_id.clone(),
                tree_identity: request.checkpoint.tree_identity.clone(),
            },
        ));

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
            GitExportValidationFailureCode::MovingSelector.as_str(),
            "moving_selector"
        );
        assert_eq!(GitExportValidationCheck::GitRef.as_str(), "git_ref");
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

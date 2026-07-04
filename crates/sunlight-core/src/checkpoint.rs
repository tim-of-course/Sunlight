use crate::execution::{ExecutionRecord, ExecutionStatus};
use crate::records::PrivacyClass;
use crate::resolver::{ResolvedViewResult, SingleRepoTree};

pub const FIXTURE_CHECKPOINT_ID: &str = "checkpoint_auth_profile_ready_0001";
pub const FIXTURE_VALIDATION_REPORT_ID: &str = "validation_export_auth_profile_ready_0001";
pub const FIXTURE_EXPORT_MAP_ID: &str = "export_map_checkpoint_auth_profile_ready_0001";
pub const FIXTURE_EXPORTED_GIT_REF: &str = "refs/heads/sunlight/auth-profile-ready";
pub const FIXTURE_GIT_COMMIT_ID: &str = "git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub const FIXTURE_CREATED_AT: &str = "2026-07-03T00:00:00Z";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointRecord {
    pub id: String,
    pub repository_id: String,
    pub resolved_view_id: String,
    pub tree_identity: SingleRepoTree,
    pub topic_frontier: Vec<TopicFrontierEntry>,
    pub evidence_refs: Vec<EvidenceRef>,
    pub conflict_free: bool,
    pub created_by: CreatedBy,
    pub created_at: String,
    pub retention_class: RetentionClass,
    pub export_refs: Vec<ExportMapRef>,
    pub privacy_class: PrivacyClass,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TopicFrontierEntry {
    pub topic_id: String,
    pub topic_revision_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvidenceRef {
    Execution(ExecutionEvidenceRef),
}

impl EvidenceRef {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Execution(_) => "execution",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionEvidenceRef {
    pub execution_id: String,
    pub result: ExecutionStatus,
    pub resolved_view_id: String,
    pub tree_identity: SingleRepoTree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedBy {
    pub actor_id: String,
    pub command: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetentionClass {
    Landable,
    LocalCandidate,
}

impl RetentionClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Landable => "landable",
            Self::LocalCandidate => "local_candidate",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportMapRef {
    pub export_map_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExportMapRecord {
    pub id: String,
    pub repository_id: String,
    pub checkpoint_id: String,
    pub tree_identity: SingleRepoTree,
    pub git_remote: Option<String>,
    pub git_ref: String,
    pub git_commit_ids: Vec<String>,
    pub export_shape: ExportShape,
    pub validation_report_id: String,
    pub exported_at: String,
    pub privacy_class: PrivacyClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportShape {
    pub kind: ExportShapeKind,
    pub parent_policy: String,
    pub include_sunlight_metadata: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportShapeKind {
    SingleCheckpointCommit,
}

impl ExportShapeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SingleCheckpointCommit => "single_checkpoint_commit",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointValidationError {
    pub code: CheckpointErrorCode,
    pub resolved_view_id: String,
    pub conflict_ids: Vec<String>,
    pub staleness_ids: Vec<String>,
    pub execution_id: Option<String>,
    pub expected_tree_identity: Option<SingleRepoTree>,
    pub actual_tree_identity: Option<SingleRepoTree>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointErrorCode {
    CheckpointConflictedView,
    CheckpointStaleView,
    CheckpointMissingTree,
    CheckpointEvidenceFailed,
    CheckpointEvidenceViewMismatch,
    CheckpointEvidenceTreeMismatch,
}

impl CheckpointErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CheckpointConflictedView => "checkpoint_conflicted_view",
            Self::CheckpointStaleView => "checkpoint_stale_view",
            Self::CheckpointMissingTree => "checkpoint_missing_tree",
            Self::CheckpointEvidenceFailed => "checkpoint_evidence_failed",
            Self::CheckpointEvidenceViewMismatch => "checkpoint_evidence_view_mismatch",
            Self::CheckpointEvidenceTreeMismatch => "checkpoint_evidence_tree_mismatch",
        }
    }
}

pub fn fixture_checkpoint_from_resolved_view(
    view: &ResolvedViewResult,
    execution_evidence: Option<&ExecutionRecord>,
) -> Result<CheckpointRecord, CheckpointValidationError> {
    let tree_identity = checkpointable_tree_identity(view)?;
    let evidence_refs = match execution_evidence {
        Some(execution) => vec![validated_execution_evidence(
            view,
            &tree_identity,
            execution,
        )?],
        None => Vec::new(),
    };

    Ok(CheckpointRecord {
        id: FIXTURE_CHECKPOINT_ID.to_string(),
        repository_id: view.repository_id.clone(),
        resolved_view_id: view.resolved_view_id.clone(),
        tree_identity,
        topic_frontier: view
            .topic_frontier
            .iter()
            .map(|(topic_id, topic_revision_id)| TopicFrontierEntry {
                topic_id: topic_id.clone(),
                topic_revision_id: topic_revision_id.clone(),
            })
            .collect(),
        evidence_refs,
        conflict_free: true,
        created_by: CreatedBy {
            actor_id: "operator_1".to_string(),
            command: "checkpoint.create".to_string(),
        },
        created_at: FIXTURE_CREATED_AT.to_string(),
        retention_class: RetentionClass::Landable,
        export_refs: Vec::new(),
        privacy_class: PrivacyClass::CommitDefault,
    })
}

pub fn fixture_git_export_map_from_checkpoint(checkpoint: &CheckpointRecord) -> GitExportMapRecord {
    GitExportMapRecord {
        id: FIXTURE_EXPORT_MAP_ID.to_string(),
        repository_id: checkpoint.repository_id.clone(),
        checkpoint_id: checkpoint.id.clone(),
        tree_identity: checkpoint.tree_identity.clone(),
        git_remote: None,
        git_ref: FIXTURE_EXPORTED_GIT_REF.to_string(),
        git_commit_ids: vec![FIXTURE_GIT_COMMIT_ID.to_string()],
        export_shape: ExportShape {
            kind: ExportShapeKind::SingleCheckpointCommit,
            parent_policy: "base_checkpoint_git_parent".to_string(),
            include_sunlight_metadata: "policy_approved_manifest_only".to_string(),
        },
        validation_report_id: FIXTURE_VALIDATION_REPORT_ID.to_string(),
        exported_at: FIXTURE_CREATED_AT.to_string(),
        privacy_class: PrivacyClass::CommitDefault,
    }
}

pub fn checkpoint_with_export_ref(
    checkpoint: &CheckpointRecord,
    export_map: &GitExportMapRecord,
) -> CheckpointRecord {
    let mut checkpoint = checkpoint.clone();
    checkpoint.export_refs.push(ExportMapRef {
        export_map_id: export_map.id.clone(),
    });
    checkpoint
}

fn checkpointable_tree_identity(
    view: &ResolvedViewResult,
) -> Result<SingleRepoTree, CheckpointValidationError> {
    let conflict_ids = view
        .conflicts()
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();
    let staleness_ids = view
        .staleness()
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();

    if !conflict_ids.is_empty() {
        return Err(CheckpointValidationError {
            code: CheckpointErrorCode::CheckpointConflictedView,
            resolved_view_id: view.resolved_view_id.clone(),
            conflict_ids,
            staleness_ids,
            execution_id: None,
            expected_tree_identity: view.tree_identity.clone(),
            actual_tree_identity: None,
        });
    }

    if !staleness_ids.is_empty() {
        return Err(CheckpointValidationError {
            code: CheckpointErrorCode::CheckpointStaleView,
            resolved_view_id: view.resolved_view_id.clone(),
            conflict_ids,
            staleness_ids,
            execution_id: None,
            expected_tree_identity: view.tree_identity.clone(),
            actual_tree_identity: None,
        });
    }

    view.tree_identity
        .clone()
        .ok_or_else(|| CheckpointValidationError {
            code: CheckpointErrorCode::CheckpointMissingTree,
            resolved_view_id: view.resolved_view_id.clone(),
            conflict_ids: Vec::new(),
            staleness_ids: Vec::new(),
            execution_id: None,
            expected_tree_identity: None,
            actual_tree_identity: None,
        })
}

fn validated_execution_evidence(
    view: &ResolvedViewResult,
    tree_identity: &SingleRepoTree,
    execution: &ExecutionRecord,
) -> Result<EvidenceRef, CheckpointValidationError> {
    if execution.resolved_view_id != view.resolved_view_id {
        return Err(CheckpointValidationError {
            code: CheckpointErrorCode::CheckpointEvidenceViewMismatch,
            resolved_view_id: view.resolved_view_id.clone(),
            conflict_ids: Vec::new(),
            staleness_ids: Vec::new(),
            execution_id: Some(execution.id.clone()),
            expected_tree_identity: Some(tree_identity.clone()),
            actual_tree_identity: Some(execution.tree_identity.clone()),
        });
    }

    if execution.tree_identity != *tree_identity {
        return Err(CheckpointValidationError {
            code: CheckpointErrorCode::CheckpointEvidenceTreeMismatch,
            resolved_view_id: view.resolved_view_id.clone(),
            conflict_ids: Vec::new(),
            staleness_ids: Vec::new(),
            execution_id: Some(execution.id.clone()),
            expected_tree_identity: Some(tree_identity.clone()),
            actual_tree_identity: Some(execution.tree_identity.clone()),
        });
    }

    if execution.result.status != ExecutionStatus::Pass {
        return Err(CheckpointValidationError {
            code: CheckpointErrorCode::CheckpointEvidenceFailed,
            resolved_view_id: view.resolved_view_id.clone(),
            conflict_ids: Vec::new(),
            staleness_ids: Vec::new(),
            execution_id: Some(execution.id.clone()),
            expected_tree_identity: Some(tree_identity.clone()),
            actual_tree_identity: Some(execution.tree_identity.clone()),
        });
    }

    Ok(EvidenceRef::Execution(ExecutionEvidenceRef {
        execution_id: execution.id.clone(),
        result: execution.result.status,
        resolved_view_id: execution.resolved_view_id.clone(),
        tree_identity: execution.tree_identity.clone(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::{
        fixture_failing_execution_from_resolved_view, fixture_passing_execution_from_resolved_view,
        FIXTURE_PASSING_EXECUTION_ID,
    };
    use crate::resolver::{
        fixture_auth_revision, fixture_base_entries, fixture_overlapping_auth_revision,
        fixture_profile_revision, fixture_profile_revision_missing_auth_dependency,
        fixture_resolver_input, resolve_fixture_view, TopicRevisionSelection,
    };

    #[test]
    fn checkpoint_freezes_conflict_free_view_with_matching_execution_evidence() {
        let view = conflict_free_view();
        let execution = fixture_passing_execution_from_resolved_view(&view).unwrap();

        let checkpoint = fixture_checkpoint_from_resolved_view(&view, Some(&execution)).unwrap();

        assert_eq!(checkpoint.id, FIXTURE_CHECKPOINT_ID);
        assert_eq!(checkpoint.resolved_view_id, view.resolved_view_id);
        assert_eq!(checkpoint.tree_identity, view.tree_identity.unwrap());
        assert_eq!(
            checkpoint.topic_frontier,
            vec![
                TopicFrontierEntry {
                    topic_id: "topic_auth_nullability".to_string(),
                    topic_revision_id: "rev_auth_nullability_0001".to_string(),
                },
                TopicFrontierEntry {
                    topic_id: "topic_profile_ui".to_string(),
                    topic_revision_id: "rev_profile_ui_0001".to_string(),
                },
            ]
        );
        assert_eq!(checkpoint.evidence_refs.len(), 1);
        assert!(checkpoint.conflict_free);
        assert_eq!(checkpoint.retention_class, RetentionClass::Landable);
        assert_eq!(checkpoint.privacy_class, PrivacyClass::CommitDefault);
    }

    #[test]
    fn checkpoint_can_be_created_without_execution_evidence_for_foundation_fixture() {
        let view = conflict_free_view();

        let checkpoint = fixture_checkpoint_from_resolved_view(&view, None).unwrap();

        assert!(checkpoint.evidence_refs.is_empty());
        assert!(checkpoint.export_refs.is_empty());
    }

    #[test]
    fn checkpoint_rejects_conflicted_view_with_inspectable_ids() {
        let auth = fixture_auth_revision();
        let overlap = fixture_overlapping_auth_revision();
        let view = resolve_fixture_view(
            fixture_resolver_input(vec![
                selection(&auth.topic_id, &auth.revision_id),
                selection(&overlap.topic_id, &overlap.revision_id),
            ]),
            fixture_base_entries(),
            vec![auth, overlap],
        );

        let error = fixture_checkpoint_from_resolved_view(&view, None).unwrap_err();

        assert_eq!(error.code, CheckpointErrorCode::CheckpointConflictedView);
        assert_eq!(error.resolved_view_id, view.resolved_view_id);
        assert_eq!(error.conflict_ids, vec!["conflict_src_auth_ts_0001"]);
        assert!(error.staleness_ids.is_empty());
    }

    #[test]
    fn checkpoint_rejects_stale_view_with_inspectable_ids() {
        let dependent = fixture_profile_revision_missing_auth_dependency();
        let required = fixture_auth_revision();
        let view = resolve_fixture_view(
            fixture_resolver_input(vec![selection(&dependent.topic_id, &dependent.revision_id)]),
            fixture_base_entries(),
            vec![dependent, required],
        );

        let error = fixture_checkpoint_from_resolved_view(&view, None).unwrap_err();

        assert_eq!(error.code, CheckpointErrorCode::CheckpointStaleView);
        assert!(error.conflict_ids.is_empty());
        assert_eq!(
            error.staleness_ids,
            vec!["stale_missing_dependency_rev_auth_nullability_0001"]
        );
    }

    #[test]
    fn checkpoint_rejects_missing_tree() {
        let mut view = conflict_free_view();
        view.tree_identity = None;

        let error = fixture_checkpoint_from_resolved_view(&view, None).unwrap_err();

        assert_eq!(error.code, CheckpointErrorCode::CheckpointMissingTree);
        assert_eq!(error.resolved_view_id, view.resolved_view_id);
        assert!(error.conflict_ids.is_empty());
        assert!(error.staleness_ids.is_empty());
    }

    #[test]
    fn checkpoint_rejects_execution_evidence_for_different_view() {
        let view = conflict_free_view();
        let other_view = profile_only_view();
        let execution = fixture_passing_execution_from_resolved_view(&other_view).unwrap();

        let error = fixture_checkpoint_from_resolved_view(&view, Some(&execution)).unwrap_err();

        assert_eq!(
            error.code,
            CheckpointErrorCode::CheckpointEvidenceViewMismatch
        );
        assert_eq!(error.execution_id, Some(execution.id));
        assert_eq!(error.expected_tree_identity, view.tree_identity);
        assert_eq!(
            error.actual_tree_identity,
            Some(other_view.tree_identity.unwrap())
        );
    }

    #[test]
    fn checkpoint_rejects_execution_evidence_for_different_tree() {
        let view = conflict_free_view();
        let mut execution = fixture_passing_execution_from_resolved_view(&view).unwrap();
        execution.tree_identity.tree_hash = "tree_different".to_string();

        let error = fixture_checkpoint_from_resolved_view(&view, Some(&execution)).unwrap_err();

        assert_eq!(
            error.code,
            CheckpointErrorCode::CheckpointEvidenceTreeMismatch
        );
        assert_eq!(
            error.execution_id,
            Some(FIXTURE_PASSING_EXECUTION_ID.to_string())
        );
        assert_eq!(error.actual_tree_identity, Some(execution.tree_identity));
    }

    #[test]
    fn checkpoint_rejects_failed_execution_evidence() {
        let view = conflict_free_view();
        let execution = fixture_failing_execution_from_resolved_view(&view).unwrap();

        let error = fixture_checkpoint_from_resolved_view(&view, Some(&execution)).unwrap_err();

        assert_eq!(error.code, CheckpointErrorCode::CheckpointEvidenceFailed);
        assert_eq!(error.execution_id, Some(execution.id));
    }

    #[test]
    fn export_map_records_checkpoint_to_git_mapping_without_export_side_effects() {
        let view = conflict_free_view();
        let checkpoint = fixture_checkpoint_from_resolved_view(&view, None).unwrap();

        let export_map = fixture_git_export_map_from_checkpoint(&checkpoint);
        let exported_checkpoint = checkpoint_with_export_ref(&checkpoint, &export_map);

        assert_eq!(export_map.id, FIXTURE_EXPORT_MAP_ID);
        assert_eq!(export_map.checkpoint_id, checkpoint.id);
        assert_eq!(export_map.tree_identity, checkpoint.tree_identity);
        assert_eq!(export_map.git_ref, FIXTURE_EXPORTED_GIT_REF);
        assert_eq!(export_map.git_commit_ids, vec![FIXTURE_GIT_COMMIT_ID]);
        assert_eq!(
            export_map.export_shape.kind,
            ExportShapeKind::SingleCheckpointCommit
        );
        assert_eq!(
            export_map.validation_report_id,
            FIXTURE_VALIDATION_REPORT_ID
        );
        assert_eq!(
            exported_checkpoint.export_refs,
            vec![ExportMapRef {
                export_map_id: FIXTURE_EXPORT_MAP_ID.to_string(),
            }]
        );
    }

    #[test]
    fn stable_error_codes_match_contract_labels() {
        assert_eq!(
            CheckpointErrorCode::CheckpointConflictedView.as_str(),
            "checkpoint_conflicted_view"
        );
        assert_eq!(
            CheckpointErrorCode::CheckpointEvidenceTreeMismatch.as_str(),
            "checkpoint_evidence_tree_mismatch"
        );
        assert_eq!(
            ExportShapeKind::SingleCheckpointCommit.as_str(),
            "single_checkpoint_commit"
        );
    }

    fn conflict_free_view() -> ResolvedViewResult {
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

    fn profile_only_view() -> ResolvedViewResult {
        let profile = fixture_profile_revision();
        resolve_fixture_view(
            fixture_resolver_input(vec![selection(&profile.topic_id, &profile.revision_id)]),
            fixture_base_entries(),
            vec![profile],
        )
    }

    fn selection(topic_id: &str, revision_id: &str) -> TopicRevisionSelection {
        TopicRevisionSelection {
            topic_id: topic_id.to_string(),
            revision_id: revision_id.to_string(),
        }
    }
}

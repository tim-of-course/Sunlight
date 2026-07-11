use crate::artifacts::MutationResponse;
use crate::records::PrivacyClass;
use crate::resolver::{ResolvedViewResult, SingleRepoTree};

pub const FIXTURE_EXECUTION_PROJECTION_ID: &str = "projection_exec_auth_profile_0001";
pub const FIXTURE_PASSING_EXECUTION_ID: &str = "exec_auth_profile_tests_0001";
pub const FIXTURE_FAILING_EXECUTION_ID: &str = "exec_auth_profile_tests_fail_0001";
pub const FIXTURE_ENVIRONMENT_SUMMARY_ID: &str = "env_summary_wsl_rust_0001";
pub const FIXTURE_STARTED_AT: &str = "2026-07-03T00:00:00Z";
pub const FIXTURE_FINISHED_AT: &str = "2026-07-03T00:00:05Z";
pub const FIXTURE_PROMOTION_OPERATION_TRANSACTION_ID: &str = "op_promote_generated_auth_0001";
pub const FIXTURE_PROMOTION_TOPIC_REVISION_ID: &str = "rev_auth_nullability_promotion_0001";
pub const FIXTURE_PROMOTION_SESSION_GENERATION_ID: &str = "gen_agent_a_promotion_0001";
pub const FIXTURE_PROMOTION_RESOLVED_VIEW_ID: &str = "view_agent_a_after_promotion_0001";
pub const FIXTURE_PROMOTION_TREE_HASH: &str = "tree_after_generated_auth_promotion_0001";
pub const FIXTURE_PROMOTION_ARTIFACT_ID: &str = "artifact_src_generated_auth_generated_ts";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionMetadata {
    pub projection_id: String,
    pub resolved_view_id: String,
    pub tree_identity: SingleRepoTree,
    pub purpose: ProjectionPurpose,
    pub strategy: ProjectionStrategy,
    pub root_ref: String,
    pub created_from_content_tree: String,
    pub writable_policy: WritablePolicy,
    pub store_integrity_policy: StoreIntegrityPolicy,
    pub cache_key: String,
    pub privacy_class: PrivacyClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionPurpose {
    Execution,
}

impl ProjectionPurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Execution => "execution",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionStrategy {
    Copy,
    Reflink,
    HardlinkReadonly,
    OverlayCopyup,
}

impl ProjectionStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Copy => "copy",
            Self::Reflink => "reflink",
            Self::HardlinkReadonly => "hardlink_readonly",
            Self::OverlayCopyup => "overlay_copyup",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WritablePolicy {
    ReadOnlySourcePrivateOutputs,
    ManagedProjectionWritableNotIsolated,
}

impl WritablePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnlySourcePrivateOutputs => "read_only_source_private_outputs",
            Self::ManagedProjectionWritableNotIsolated => {
                "managed_projection_writable_not_isolated"
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreIntegrityPolicy {
    VerifyBeforeReuse,
}

impl StoreIntegrityPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::VerifyBeforeReuse => "verify_before_reuse",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvironmentSummary {
    pub id: String,
    pub os: String,
    pub platform_hint: String,
    pub arch: String,
    pub sunlight_build_id: String,
    pub command_runner_version: String,
    pub tool_hints: Vec<ToolHint>,
    pub env_policy: String,
    pub redacted_env_allowlist_digest: String,
    pub network_policy: NetworkPolicy,
    pub sandbox_writable_policy: WritablePolicy,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolHint {
    pub name: String,
    pub version_or_digest: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkPolicy {
    NotEnforced,
    Disabled,
    Allowed,
}

impl NetworkPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotEnforced => "not_enforced",
            Self::Disabled => "disabled",
            Self::Allowed => "allowed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionRecord {
    pub id: String,
    pub repository_id: String,
    pub resolved_view_id: String,
    pub tree_identity: SingleRepoTree,
    pub command: ExecutionCommand,
    pub working_directory: String,
    pub environment_summary: EnvironmentSummary,
    pub projection_id: String,
    pub inputs: ExecutionInputs,
    pub outputs: Vec<OutputSummary>,
    pub promotions: Vec<PromotionCandidateProvenance>,
    pub result: ExecutionResult,
    pub started_at: String,
    pub finished_at: String,
    pub privacy_class: PrivacyClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionCommand {
    pub argv: Vec<String>,
    pub shell: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionInputs {
    pub resolved_view_id: String,
    pub tree_hash: String,
    pub path_policy_id: String,
    pub operation_semantics_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    pub status: ExecutionStatus,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    Pass,
    Fail,
    Timeout,
    Canceled,
    Flaky,
    Unknown,
    PolicyBlocked,
}

impl ExecutionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Timeout => "timeout",
            Self::Canceled => "canceled",
            Self::Flaky => "flaky",
            Self::Unknown => "unknown",
            Self::PolicyBlocked => "policy_blocked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputSummary {
    pub kind: OutputKind,
    pub classification: OutputClassification,
    pub path: Option<String>,
    pub digest: String,
    pub byte_length: u64,
    pub privacy_class: PrivacyClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    StdoutSummary,
    StderrSummary,
    FileDelta,
}

impl OutputKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StdoutSummary => "stdout_summary",
            Self::StderrSummary => "stderr_summary",
            Self::FileDelta => "file_delta",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputClassification {
    SourceLikeDelta,
    GeneratedArtifact,
    Log,
    Cache,
    Coverage,
    Secret,
    Ignored,
}

impl OutputClassification {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SourceLikeDelta => "source_like_delta",
            Self::GeneratedArtifact => "generated_artifact",
            Self::Log => "log",
            Self::Cache => "cache",
            Self::Coverage => "coverage",
            Self::Secret => "secret",
            Self::Ignored => "ignored",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotionCandidateProvenance {
    pub execution_id: String,
    pub projection_id: String,
    pub output_path: String,
    pub target_topic_id: String,
    pub classification: OutputClassification,
    pub before_hash: Option<String>,
    pub after_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionOutputPromotionRecord {
    pub execution_id: String,
    pub projection_id: String,
    pub output_path: String,
    pub target_topic_id: String,
    pub classification: OutputClassification,
    pub before_hash: Option<String>,
    pub after_hash: String,
    pub operation_transaction_id: String,
    pub topic_revision_id: String,
    pub session_generation_id: String,
    pub authored_context_id: String,
    pub provenance_refs: Vec<ExecutionOutputPromotionProvenanceRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionOutputPromotionProvenanceRef {
    pub kind: ExecutionOutputPromotionProvenanceRefKind,
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionOutputPromotionProvenanceRefKind {
    Execution,
    Projection,
    OperationTransaction,
    TopicRevision,
    SessionGeneration,
}

impl ExecutionOutputPromotionProvenanceRefKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Execution => "execution",
            Self::Projection => "projection",
            Self::OperationTransaction => "operation_transaction",
            Self::TopicRevision => "topic_revision",
            Self::SessionGeneration => "session_generation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionFoundationError {
    pub code: ExecutionErrorCode,
    pub resolved_view_id: String,
    pub conflict_ids: Vec<String>,
    pub staleness_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionErrorCode {
    ExecutionConflictedView,
    ExecutionMissingTree,
    ExecutionProjectionFailed,
    ExecutionCommandFailed,
    ExecutionTimeout,
    ExecutionStoreIntegrityFailed,
    ExecutionOutputSecret,
    PromotionNoChanges,
    PromotionPolicyFailed,
    PromotionPreconditionFailed,
    PromotionTopicNotFound,
}

impl ExecutionErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExecutionConflictedView => "execution_conflicted_view",
            Self::ExecutionMissingTree => "execution_missing_tree",
            Self::ExecutionProjectionFailed => "execution_projection_failed",
            Self::ExecutionCommandFailed => "execution_command_failed",
            Self::ExecutionTimeout => "execution_timeout",
            Self::ExecutionStoreIntegrityFailed => "execution_store_integrity_failed",
            Self::ExecutionOutputSecret => "execution_output_secret",
            Self::PromotionNoChanges => "promotion_no_changes",
            Self::PromotionPolicyFailed => "promotion_policy_failed",
            Self::PromotionPreconditionFailed => "promotion_precondition_failed",
            Self::PromotionTopicNotFound => "promotion_topic_not_found",
        }
    }
}

pub fn fixture_projection_from_resolved_view(
    view: &ResolvedViewResult,
) -> Result<ProjectionMetadata, ExecutionFoundationError> {
    let tree_identity = validated_tree_identity(view)?;
    Ok(ProjectionMetadata {
        projection_id: FIXTURE_EXECUTION_PROJECTION_ID.to_string(),
        resolved_view_id: view.resolved_view_id.clone(),
        created_from_content_tree: tree_identity.tree_hash.clone(),
        cache_key: fixture_cache_key(view, ProjectionStrategy::Copy),
        tree_identity,
        purpose: ProjectionPurpose::Execution,
        strategy: ProjectionStrategy::Copy,
        root_ref: "local://.sunlight/projections/projection_exec_auth_profile_0001".to_string(),
        writable_policy: WritablePolicy::ReadOnlySourcePrivateOutputs,
        store_integrity_policy: StoreIntegrityPolicy::VerifyBeforeReuse,
        privacy_class: PrivacyClass::LocalOnly,
    })
}

pub fn fixture_passing_execution_from_resolved_view(
    view: &ResolvedViewResult,
) -> Result<ExecutionRecord, ExecutionFoundationError> {
    fixture_execution_from_resolved_view(
        view,
        FIXTURE_PASSING_EXECUTION_ID,
        ExecutionResult {
            status: ExecutionStatus::Pass,
            exit_code: Some(0),
            timed_out: false,
        },
        vec![fixture_stdout_summary()],
    )
}

pub fn fixture_failing_execution_from_resolved_view(
    view: &ResolvedViewResult,
) -> Result<ExecutionRecord, ExecutionFoundationError> {
    fixture_execution_from_resolved_view(
        view,
        FIXTURE_FAILING_EXECUTION_ID,
        ExecutionResult {
            status: ExecutionStatus::Fail,
            exit_code: Some(101),
            timed_out: false,
        },
        vec![fixture_stdout_summary(), fixture_stderr_summary()],
    )
}

pub fn fixture_promotion_candidate_provenance(
    execution: &ExecutionRecord,
) -> PromotionCandidateProvenance {
    PromotionCandidateProvenance {
        execution_id: execution.id.clone(),
        projection_id: execution.projection_id.clone(),
        output_path: "src/generated/auth.generated.ts".to_string(),
        target_topic_id: "topic_auth_nullability".to_string(),
        classification: OutputClassification::SourceLikeDelta,
        before_hash: None,
        after_hash: "sha256:generated_auth_after".to_string(),
    }
}

pub fn promotion_authored_context_id(candidate: &PromotionCandidateProvenance) -> String {
    format!(
        "execution:{}:{}",
        candidate.execution_id, candidate.output_path
    )
}

pub fn fixture_execution_output_promotion_record(
    candidate: &PromotionCandidateProvenance,
) -> ExecutionOutputPromotionRecord {
    execution_output_promotion_record_from_ids(
        candidate,
        FIXTURE_PROMOTION_OPERATION_TRANSACTION_ID,
        FIXTURE_PROMOTION_TOPIC_REVISION_ID,
        FIXTURE_PROMOTION_SESSION_GENERATION_ID,
    )
}

pub fn execution_output_promotion_record_from_mutation_response(
    candidate: &PromotionCandidateProvenance,
    response: &MutationResponse,
) -> ExecutionOutputPromotionRecord {
    execution_output_promotion_record_from_ids(
        candidate,
        &response.operation.id,
        &response.topic_revision.id,
        &response.session_generation.id,
    )
}

pub fn execution_output_promotion_record_from_ids(
    candidate: &PromotionCandidateProvenance,
    operation_transaction_id: &str,
    topic_revision_id: &str,
    session_generation_id: &str,
) -> ExecutionOutputPromotionRecord {
    let authored_context_id = promotion_authored_context_id(candidate);
    ExecutionOutputPromotionRecord {
        execution_id: candidate.execution_id.clone(),
        projection_id: candidate.projection_id.clone(),
        output_path: candidate.output_path.clone(),
        target_topic_id: candidate.target_topic_id.clone(),
        classification: candidate.classification,
        before_hash: candidate.before_hash.clone(),
        after_hash: candidate.after_hash.clone(),
        operation_transaction_id: operation_transaction_id.to_string(),
        topic_revision_id: topic_revision_id.to_string(),
        session_generation_id: session_generation_id.to_string(),
        authored_context_id,
        provenance_refs: vec![
            ExecutionOutputPromotionProvenanceRef {
                kind: ExecutionOutputPromotionProvenanceRefKind::Execution,
                id: candidate.execution_id.clone(),
            },
            ExecutionOutputPromotionProvenanceRef {
                kind: ExecutionOutputPromotionProvenanceRefKind::Projection,
                id: candidate.projection_id.clone(),
            },
            ExecutionOutputPromotionProvenanceRef {
                kind: ExecutionOutputPromotionProvenanceRefKind::OperationTransaction,
                id: operation_transaction_id.to_string(),
            },
            ExecutionOutputPromotionProvenanceRef {
                kind: ExecutionOutputPromotionProvenanceRefKind::TopicRevision,
                id: topic_revision_id.to_string(),
            },
            ExecutionOutputPromotionProvenanceRef {
                kind: ExecutionOutputPromotionProvenanceRefKind::SessionGeneration,
                id: session_generation_id.to_string(),
            },
        ],
    }
}

fn fixture_execution_from_resolved_view(
    view: &ResolvedViewResult,
    execution_id: &str,
    result: ExecutionResult,
    outputs: Vec<OutputSummary>,
) -> Result<ExecutionRecord, ExecutionFoundationError> {
    let projection = fixture_projection_from_resolved_view(view)?;
    let tree_identity = projection.tree_identity.clone();
    Ok(ExecutionRecord {
        id: execution_id.to_string(),
        repository_id: view.repository_id.clone(),
        resolved_view_id: view.resolved_view_id.clone(),
        tree_identity: tree_identity.clone(),
        command: ExecutionCommand {
            argv: vec!["cargo".to_string(), "test".to_string()],
            shell: None,
        },
        working_directory: ".".to_string(),
        environment_summary: fixture_environment_summary(projection.writable_policy),
        projection_id: projection.projection_id,
        inputs: ExecutionInputs {
            resolved_view_id: view.resolved_view_id.clone(),
            tree_hash: tree_identity.tree_hash,
            path_policy_id: view.path_policy_id.clone(),
            operation_semantics_version: view.operation_semantics_version.clone(),
        },
        outputs,
        promotions: Vec::new(),
        result,
        started_at: FIXTURE_STARTED_AT.to_string(),
        finished_at: FIXTURE_FINISHED_AT.to_string(),
        privacy_class: PrivacyClass::PolicyGated,
    })
}

fn validated_tree_identity(
    view: &ResolvedViewResult,
) -> Result<SingleRepoTree, ExecutionFoundationError> {
    let conflict_ids = view
        .conflicts()
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();
    let staleness_ids = view
        .staleness()
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();

    if !conflict_ids.is_empty() || !staleness_ids.is_empty() {
        return Err(ExecutionFoundationError {
            code: ExecutionErrorCode::ExecutionConflictedView,
            resolved_view_id: view.resolved_view_id.clone(),
            conflict_ids,
            staleness_ids,
        });
    }

    view.tree_identity
        .clone()
        .ok_or_else(|| ExecutionFoundationError {
            code: ExecutionErrorCode::ExecutionMissingTree,
            resolved_view_id: view.resolved_view_id.clone(),
            conflict_ids: Vec::new(),
            staleness_ids: Vec::new(),
        })
}

fn fixture_environment_summary(writable_policy: WritablePolicy) -> EnvironmentSummary {
    EnvironmentSummary {
        id: FIXTURE_ENVIRONMENT_SUMMARY_ID.to_string(),
        os: "linux".to_string(),
        platform_hint: "wsl".to_string(),
        arch: "x86_64".to_string(),
        sunlight_build_id: "sunlight-core-fixture".to_string(),
        command_runner_version: "runner_fixture_v1".to_string(),
        tool_hints: vec![ToolHint {
            name: "cargo".to_string(),
            version_or_digest: "digest-or-version-if-available".to_string(),
        }],
        env_policy: "default_redacted".to_string(),
        redacted_env_allowlist_digest: "sha256:envallowlist".to_string(),
        network_policy: NetworkPolicy::NotEnforced,
        sandbox_writable_policy: writable_policy,
        digest: "sha256:envsummary".to_string(),
    }
}

fn fixture_stdout_summary() -> OutputSummary {
    OutputSummary {
        kind: OutputKind::StdoutSummary,
        classification: OutputClassification::Log,
        path: None,
        digest: "sha256:stdout".to_string(),
        byte_length: 1200,
        privacy_class: PrivacyClass::LocalOnly,
    }
}

fn fixture_stderr_summary() -> OutputSummary {
    OutputSummary {
        kind: OutputKind::StderrSummary,
        classification: OutputClassification::Log,
        path: None,
        digest: "sha256:stderr".to_string(),
        byte_length: 240,
        privacy_class: PrivacyClass::LocalOnly,
    }
}

fn fixture_cache_key(view: &ResolvedViewResult, strategy: ProjectionStrategy) -> String {
    let tree_hash = view
        .tree_identity
        .as_ref()
        .map(|tree| tree.tree_hash.as_str())
        .unwrap_or("missing-tree");
    format!(
        "projection-cache:{}:{}:{}:{}:{}",
        view.repository_id,
        view.resolved_view_id,
        tree_hash,
        view.path_policy_id,
        strategy.as_str()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::{
        ExpectedHash, MutationArtifactView, MutationKind, MutationPayload, MutationPreconditions,
        MutationRefs, OperationTransactionRecord, SessionGenerationMutationRecord, SessionView,
        TopicRevisionRecord, TreeIdentityView, WriteMode, WriteSetEntry, FIXTURE_ACTOR_ID,
        FIXTURE_REPOSITORY_ID, FIXTURE_SESSION_ID,
    };
    use crate::resolver::{
        fixture_auth_revision, fixture_base_entries, fixture_overlapping_auth_revision,
        fixture_profile_revision, fixture_profile_revision_missing_auth_dependency,
        fixture_resolver_input, resolve_fixture_view, TopicRevisionSelection,
    };

    #[test]
    fn projection_records_strategy_policy_and_exact_tree() {
        let view = conflict_free_view();

        let projection = fixture_projection_from_resolved_view(&view).unwrap();

        assert_eq!(projection.projection_id, FIXTURE_EXECUTION_PROJECTION_ID);
        assert_eq!(projection.resolved_view_id, view.resolved_view_id);
        assert_eq!(projection.tree_identity, view.tree_identity.unwrap());
        assert_eq!(projection.purpose, ProjectionPurpose::Execution);
        assert_eq!(projection.strategy, ProjectionStrategy::Copy);
        assert_eq!(projection.privacy_class, PrivacyClass::LocalOnly);
        assert!(projection
            .cache_key
            .contains("path_policy_posix_case_sensitive_v1"));
    }

    #[test]
    fn run_records_pass_result() {
        let view = conflict_free_view();

        let execution = fixture_passing_execution_from_resolved_view(&view).unwrap();

        assert_eq!(execution.id, FIXTURE_PASSING_EXECUTION_ID);
        assert_eq!(execution.resolved_view_id, view.resolved_view_id);
        assert_eq!(execution.command.argv, vec!["cargo", "test"]);
        assert_eq!(execution.result.status, ExecutionStatus::Pass);
        assert_eq!(execution.result.exit_code, Some(0));
        assert_eq!(execution.projection_id, FIXTURE_EXECUTION_PROJECTION_ID);
        assert_eq!(
            execution.environment_summary.id,
            FIXTURE_ENVIRONMENT_SUMMARY_ID
        );
        assert_eq!(execution.outputs[0].kind, OutputKind::StdoutSummary);
    }

    #[test]
    fn run_records_failure_result_without_rejecting_inspection() {
        let view = conflict_free_view();

        let execution = fixture_failing_execution_from_resolved_view(&view).unwrap();

        assert_eq!(execution.id, FIXTURE_FAILING_EXECUTION_ID);
        assert_eq!(execution.result.status, ExecutionStatus::Fail);
        assert_eq!(execution.result.exit_code, Some(101));
        assert_eq!(execution.outputs.len(), 2);
    }

    #[test]
    fn promotion_candidate_provenance_links_execution_projection_and_output() {
        let view = conflict_free_view();
        let execution = fixture_passing_execution_from_resolved_view(&view).unwrap();

        let promotion = fixture_promotion_candidate_provenance(&execution);

        assert_eq!(promotion.execution_id, execution.id);
        assert_eq!(promotion.projection_id, FIXTURE_EXECUTION_PROJECTION_ID);
        assert_eq!(promotion.output_path, "src/generated/auth.generated.ts");
        assert_eq!(promotion.target_topic_id, "topic_auth_nullability");
        assert_eq!(
            promotion.classification,
            OutputClassification::SourceLikeDelta
        );
        assert_eq!(promotion.before_hash, None);
        assert_eq!(promotion.after_hash, "sha256:generated_auth_after");
    }

    #[test]
    fn fixture_promotion_record_links_candidate_to_operation_topic_and_session() {
        let view = conflict_free_view();
        let execution = fixture_passing_execution_from_resolved_view(&view).unwrap();
        let candidate = fixture_promotion_candidate_provenance(&execution);

        let record = fixture_execution_output_promotion_record(&candidate);

        assert_eq!(record.execution_id, execution.id);
        assert_eq!(record.projection_id, FIXTURE_EXECUTION_PROJECTION_ID);
        assert_eq!(record.output_path, "src/generated/auth.generated.ts");
        assert_eq!(record.target_topic_id, "topic_auth_nullability");
        assert_eq!(record.classification, OutputClassification::SourceLikeDelta);
        assert_eq!(record.before_hash, None);
        assert_eq!(record.after_hash, "sha256:generated_auth_after");
        assert_eq!(
            record.operation_transaction_id,
            FIXTURE_PROMOTION_OPERATION_TRANSACTION_ID
        );
        assert_eq!(
            record.topic_revision_id,
            FIXTURE_PROMOTION_TOPIC_REVISION_ID
        );
        assert_eq!(
            record.session_generation_id,
            FIXTURE_PROMOTION_SESSION_GENERATION_ID
        );
        assert_eq!(
            record.authored_context_id,
            "execution:exec_auth_profile_tests_0001:src/generated/auth.generated.ts"
        );
        assert_eq!(
            record
                .provenance_refs
                .iter()
                .map(|reference| (reference.kind.as_str(), reference.id.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("execution", FIXTURE_PASSING_EXECUTION_ID),
                ("projection", FIXTURE_EXECUTION_PROJECTION_ID),
                (
                    "operation_transaction",
                    FIXTURE_PROMOTION_OPERATION_TRANSACTION_ID
                ),
                ("topic_revision", FIXTURE_PROMOTION_TOPIC_REVISION_ID),
                (
                    "session_generation",
                    FIXTURE_PROMOTION_SESSION_GENERATION_ID
                ),
            ]
        );
    }

    #[test]
    fn promotion_record_can_be_derived_from_mutation_response_ids() {
        let view = conflict_free_view();
        let execution = fixture_passing_execution_from_resolved_view(&view).unwrap();
        let candidate = fixture_promotion_candidate_provenance(&execution);
        let response = fixture_promotion_mutation_response(&candidate);

        let record =
            execution_output_promotion_record_from_mutation_response(&candidate, &response);

        assert_eq!(record.operation_transaction_id, response.operation.id);
        assert_eq!(record.topic_revision_id, response.topic_revision.id);
        assert_eq!(record.session_generation_id, response.session_generation.id);
        assert_eq!(
            record.authored_context_id,
            response.operation.authored_context_id
        );
        assert_eq!(record.classification, OutputClassification::SourceLikeDelta);
    }

    #[test]
    fn raw_stdout_stderr_summaries_remain_local_only() {
        let view = conflict_free_view();

        let execution = fixture_failing_execution_from_resolved_view(&view).unwrap();

        let stdout = execution
            .outputs
            .iter()
            .find(|output| output.kind == OutputKind::StdoutSummary)
            .expect("stdout summary fixture");
        let stderr = execution
            .outputs
            .iter()
            .find(|output| output.kind == OutputKind::StderrSummary)
            .expect("stderr summary fixture");
        assert_eq!(stdout.classification, OutputClassification::Log);
        assert_eq!(stdout.privacy_class, PrivacyClass::LocalOnly);
        assert_eq!(stderr.classification, OutputClassification::Log);
        assert_eq!(stderr.privacy_class, PrivacyClass::LocalOnly);
    }

    #[test]
    fn execution_rejects_conflicted_view_with_ids() {
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

        let error = fixture_projection_from_resolved_view(&view).unwrap_err();

        assert_eq!(
            error.code.as_str(),
            ExecutionErrorCode::ExecutionConflictedView.as_str()
        );
        assert_eq!(error.resolved_view_id, view.resolved_view_id);
        assert_eq!(error.conflict_ids, vec!["conflict_src_auth_ts_0001"]);
        assert!(error.staleness_ids.is_empty());
    }

    #[test]
    fn execution_rejects_stale_view_with_ids() {
        let dependent = fixture_profile_revision_missing_auth_dependency();
        let required = fixture_auth_revision();
        let view = resolve_fixture_view(
            fixture_resolver_input(vec![selection(&dependent.topic_id, &dependent.revision_id)]),
            fixture_base_entries(),
            vec![dependent, required],
        );

        let error = fixture_passing_execution_from_resolved_view(&view).unwrap_err();

        assert_eq!(error.code, ExecutionErrorCode::ExecutionConflictedView);
        assert!(error.conflict_ids.is_empty());
        assert_eq!(
            error.staleness_ids,
            vec!["stale_missing_dependency_rev_auth_nullability_0001"]
        );
    }

    #[test]
    fn stable_error_codes_match_contract_labels() {
        assert_eq!(
            ExecutionErrorCode::ExecutionMissingTree.as_str(),
            "execution_missing_tree"
        );
        assert_eq!(
            ExecutionErrorCode::PromotionPreconditionFailed.as_str(),
            "promotion_precondition_failed"
        );
        assert_eq!(
            OutputClassification::SourceLikeDelta.as_str(),
            "source_like_delta"
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

    fn selection(topic_id: &str, revision_id: &str) -> TopicRevisionSelection {
        TopicRevisionSelection {
            topic_id: topic_id.to_string(),
            revision_id: revision_id.to_string(),
        }
    }

    fn fixture_promotion_mutation_response(
        candidate: &PromotionCandidateProvenance,
    ) -> MutationResponse {
        let tree_identity = TreeIdentityView {
            kind: "SingleRepoTree".to_string(),
            repository_id: FIXTURE_REPOSITORY_ID.to_string(),
            tree_hash: FIXTURE_PROMOTION_TREE_HASH.to_string(),
        };
        MutationResponse {
            command: "write",
            repository_id: FIXTURE_REPOSITORY_ID.to_string(),
            session_id: FIXTURE_SESSION_ID.to_string(),
            view: SessionView {
                resolved_view_id: FIXTURE_PROMOTION_RESOLVED_VIEW_ID.to_string(),
                session_generation_id: FIXTURE_PROMOTION_SESSION_GENERATION_ID.to_string(),
                tree_identity: tree_identity.clone(),
            },
            artifact: MutationArtifactView {
                artifact_id: FIXTURE_PROMOTION_ARTIFACT_ID.to_string(),
                path: candidate.output_path.clone(),
                kind: crate::artifacts::ArtifactKind::File,
                before_hash: candidate.before_hash.clone(),
                after_hash: candidate.after_hash.clone(),
                classification: "source".to_string(),
                executable: false,
            },
            operation: OperationTransactionRecord {
                id: FIXTURE_PROMOTION_OPERATION_TRANSACTION_ID.to_string(),
                repository_id: FIXTURE_REPOSITORY_ID.to_string(),
                topic_id: candidate.target_topic_id.clone(),
                session_id: FIXTURE_SESSION_ID.to_string(),
                session_generation_id: FIXTURE_PROMOTION_SESSION_GENERATION_ID.to_string(),
                actor_id: FIXTURE_ACTOR_ID.to_string(),
                authored_context_id: promotion_authored_context_id(candidate),
                preconditions: MutationPreconditions {
                    resolved_view_id: "view_base_0001".to_string(),
                    session_generation_id: "gen_agent_a_0001".to_string(),
                    write_topic_id: candidate.target_topic_id.clone(),
                    parent_topic_revision_id: None,
                    path_policy_id: "path_policy_posix_case_sensitive_v1".to_string(),
                    operation_semantics_version: "file_ops_v1".to_string(),
                    expected_path: candidate.output_path.clone(),
                    expected_hash: ExpectedHash::New,
                },
                read_set: "full_authored_context".to_string(),
                write_set: vec![WriteSetEntry {
                    artifact_id: FIXTURE_PROMOTION_ARTIFACT_ID.to_string(),
                    path: candidate.output_path.clone(),
                    mutation: MutationKind::Write,
                }],
                mutation_payload: MutationPayload::Write {
                    write_mode: WriteMode::Create,
                    content_hash: candidate.after_hash.clone(),
                    byte_length: 43,
                    media_type: "text/typescript; charset=utf-8".to_string(),
                    executable: false,
                    classification: "source".to_string(),
                },
                before_refs: MutationRefs {
                    artifacts: Vec::new(),
                    tree_identity: tree_identity.clone(),
                },
                after_refs: MutationRefs {
                    artifacts: Vec::new(),
                    tree_identity: tree_identity.clone(),
                },
                classification: "source".to_string(),
                parent_topic_revision_id: None,
                next_topic_revision_number: 1,
                parents: Vec::new(),
            },
            topic_revision: TopicRevisionRecord {
                id: FIXTURE_PROMOTION_TOPIC_REVISION_ID.to_string(),
                repository_id: FIXTURE_REPOSITORY_ID.to_string(),
                topic_id: candidate.target_topic_id.clone(),
                revision_number: 1,
                parent_revision_id: None,
                operation_transaction_id: FIXTURE_PROMOTION_OPERATION_TRANSACTION_ID.to_string(),
                tree_delta_ref: "delta_promote_generated_auth_0001".to_string(),
                dependency_revision_ids: Vec::new(),
            },
            session_generation: SessionGenerationMutationRecord {
                id: FIXTURE_PROMOTION_SESSION_GENERATION_ID.to_string(),
                repository_id: FIXTURE_REPOSITORY_ID.to_string(),
                session_id: FIXTURE_SESSION_ID.to_string(),
                write_topic_id: candidate.target_topic_id.clone(),
                base_resolved_view_id: "view_base_0001".to_string(),
                resolved_view_id: FIXTURE_PROMOTION_RESOLVED_VIEW_ID.to_string(),
                topic_frontier: Default::default(),
                generation_number: 2,
                refresh_policy: "fixture_refresh".to_string(),
                created_by_operation_id: FIXTURE_PROMOTION_OPERATION_TRANSACTION_ID.to_string(),
            },
        }
    }
}

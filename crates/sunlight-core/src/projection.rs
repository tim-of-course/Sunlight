use crate::records::{PrivacyClass, RECORD_SCHEMA_VERSION};
use crate::resolver::{ResolvedViewResult, SingleRepoTree};

pub const FIXTURE_EXECUTION_PROJECTION_ID: &str = "projection_exec_auth_profile_0001";
pub const FIXTURE_COMPATIBILITY_PROJECTION_ID: &str = "projection_compat_agent_a_0001";
pub const FIXTURE_INSPECTION_PROJECTION_ID: &str = "projection_inspect_auth_profile_0001";
pub const FIXTURE_EXPORT_PROJECTION_ID: &str = "projection_export_auth_profile_0001";
pub const FIXTURE_CREATED_AT: &str = "2026-07-03T00:00:00Z";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionRecord {
    pub schema_version: u32,
    pub record_type: &'static str,
    pub id: String,
    pub repository_id: String,
    pub resolved_view_id: String,
    pub session_generation_id: Option<String>,
    pub tree_identity: SingleRepoTree,
    pub path_policy_id: String,
    pub operation_semantics_version: String,
    pub purpose: ProjectionPurpose,
    pub strategy: ProjectionStrategy,
    pub root_ref: ProjectionRootRef,
    pub created_from_content_tree: String,
    pub baseline_manifest_ref: Option<String>,
    pub writable_policy: WritablePolicy,
    pub store_integrity_policy: StoreIntegrityPolicy,
    pub cache_key: ProjectionCacheKey,
    pub retention_state: ProjectionRetentionState,
    pub privacy_class: PrivacyClass,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionPurpose {
    Execution,
    Compatibility,
    Inspection,
    Export,
}

impl ProjectionPurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Execution => "execution",
            Self::Compatibility => "compatibility",
            Self::Inspection => "inspection",
            Self::Export => "export",
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
    ReadOnly,
    ReadOnlySourcePrivateOutputs,
    WritableWithExplicitImport,
    ExportMaterializationOnly,
}

impl WritablePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::ReadOnlySourcePrivateOutputs => "read_only_source_private_outputs",
            Self::WritableWithExplicitImport => "writable_with_explicit_import",
            Self::ExportMaterializationOnly => "export_materialization_only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreIntegrityPolicy {
    VerifyBeforeReuse,
    VerifyOnImport,
    VerifyBeforeExport,
    VerifyForInspection,
}

impl StoreIntegrityPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::VerifyBeforeReuse => "verify_before_reuse",
            Self::VerifyOnImport => "verify_on_import",
            Self::VerifyBeforeExport => "verify_before_export",
            Self::VerifyForInspection => "verify_for_inspection",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionCacheKey {
    pub repository_id: String,
    pub resolved_view_id: String,
    pub tree_hash: String,
    pub path_policy_id: String,
    pub operation_semantics_version: String,
    pub purpose: ProjectionPurpose,
    pub strategy: ProjectionStrategy,
    pub writable_policy: WritablePolicy,
}

impl ProjectionCacheKey {
    pub fn stable_string(&self) -> String {
        format!(
            "projection-cache:{}:{}:{}:{}:{}:{}:{}:{}",
            self.repository_id,
            self.resolved_view_id,
            self.tree_hash,
            self.path_policy_id,
            self.operation_semantics_version,
            self.purpose.as_str(),
            self.strategy.as_str(),
            self.writable_policy.as_str()
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionRootRef {
    pub value: String,
    pub privacy: RootRefPrivacy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootRefPrivacy {
    LocalOnlyPath,
    LocalOnlyOpaqueHandle,
}

impl RootRefPrivacy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalOnlyPath => "local_only_path",
            Self::LocalOnlyOpaqueHandle => "local_only_opaque_handle",
        }
    }

    pub fn privacy_class(self) -> PrivacyClass {
        PrivacyClass::LocalOnly
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionRetentionState {
    Active,
    ReusableCache,
    Quarantined,
    Released,
}

impl ProjectionRetentionState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::ReusableCache => "reusable_cache",
            Self::Quarantined => "quarantined",
            Self::Released => "released",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionValidationError {
    pub code: ProjectionValidationErrorCode,
    pub resolved_view_id: String,
    pub conflict_ids: Vec<String>,
    pub staleness_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionValidationErrorCode {
    ConflictedView,
    StaleView,
    ConflictedAndStaleView,
    MissingTree,
}

impl ProjectionValidationErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ConflictedView => "projection_conflicted_view",
            Self::StaleView => "projection_stale_view",
            Self::ConflictedAndStaleView => "projection_conflicted_and_stale_view",
            Self::MissingTree => "projection_missing_tree",
        }
    }
}

pub fn fixture_execution_projection_from_resolved_view(
    view: &ResolvedViewResult,
) -> Result<ProjectionRecord, ProjectionValidationError> {
    fixture_projection_from_resolved_view(
        view,
        ProjectionPurpose::Execution,
        FIXTURE_EXECUTION_PROJECTION_ID,
        ProjectionStrategy::Copy,
        None,
    )
}

pub fn fixture_compatibility_projection_from_resolved_view(
    view: &ResolvedViewResult,
    session_generation_id: impl Into<String>,
) -> Result<ProjectionRecord, ProjectionValidationError> {
    fixture_projection_from_resolved_view(
        view,
        ProjectionPurpose::Compatibility,
        FIXTURE_COMPATIBILITY_PROJECTION_ID,
        ProjectionStrategy::Copy,
        Some(session_generation_id.into()),
    )
}

pub fn fixture_inspection_projection_from_resolved_view(
    view: &ResolvedViewResult,
) -> Result<ProjectionRecord, ProjectionValidationError> {
    fixture_projection_from_resolved_view(
        view,
        ProjectionPurpose::Inspection,
        FIXTURE_INSPECTION_PROJECTION_ID,
        ProjectionStrategy::HardlinkReadonly,
        None,
    )
}

pub fn fixture_export_projection_from_resolved_view(
    view: &ResolvedViewResult,
) -> Result<ProjectionRecord, ProjectionValidationError> {
    fixture_projection_from_resolved_view(
        view,
        ProjectionPurpose::Export,
        FIXTURE_EXPORT_PROJECTION_ID,
        ProjectionStrategy::Copy,
        None,
    )
}

pub fn fixture_projection_from_resolved_view(
    view: &ResolvedViewResult,
    purpose: ProjectionPurpose,
    projection_id: &str,
    strategy: ProjectionStrategy,
    session_generation_id: Option<String>,
) -> Result<ProjectionRecord, ProjectionValidationError> {
    let tree_identity = validated_tree_identity(view)?;
    let writable_policy = default_writable_policy(purpose);
    let cache_key = ProjectionCacheKey {
        repository_id: view.repository_id.clone(),
        resolved_view_id: view.resolved_view_id.clone(),
        tree_hash: tree_identity.tree_hash.clone(),
        path_policy_id: view.path_policy_id.clone(),
        operation_semantics_version: view.operation_semantics_version.clone(),
        purpose,
        strategy,
        writable_policy,
    };
    let baseline_manifest_ref =
        (purpose == ProjectionPurpose::Compatibility).then(|| baseline_manifest_ref(view));

    Ok(ProjectionRecord {
        schema_version: RECORD_SCHEMA_VERSION,
        record_type: "projection",
        id: projection_id.to_string(),
        repository_id: view.repository_id.clone(),
        resolved_view_id: view.resolved_view_id.clone(),
        session_generation_id,
        created_from_content_tree: tree_identity.tree_hash.clone(),
        tree_identity,
        path_policy_id: view.path_policy_id.clone(),
        operation_semantics_version: view.operation_semantics_version.clone(),
        purpose,
        strategy,
        root_ref: fixture_root_ref(purpose, projection_id),
        baseline_manifest_ref,
        writable_policy,
        store_integrity_policy: default_store_integrity_policy(purpose),
        cache_key,
        retention_state: ProjectionRetentionState::Active,
        privacy_class: PrivacyClass::LocalOnly,
        created_at: FIXTURE_CREATED_AT.to_string(),
    })
}

pub fn validate_projectable_view(
    view: &ResolvedViewResult,
) -> Result<SingleRepoTree, ProjectionValidationError> {
    validated_tree_identity(view)
}

fn validated_tree_identity(
    view: &ResolvedViewResult,
) -> Result<SingleRepoTree, ProjectionValidationError> {
    let conflict_ids = view
        .conflicts()
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();
    let staleness_ids = view
        .staleness()
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();

    if !conflict_ids.is_empty() || !staleness_ids.is_empty() {
        let code = match (conflict_ids.is_empty(), staleness_ids.is_empty()) {
            (false, true) => ProjectionValidationErrorCode::ConflictedView,
            (true, false) => ProjectionValidationErrorCode::StaleView,
            (false, false) => ProjectionValidationErrorCode::ConflictedAndStaleView,
            (true, true) => unreachable!("checked above"),
        };
        return Err(ProjectionValidationError {
            code,
            resolved_view_id: view.resolved_view_id.clone(),
            conflict_ids,
            staleness_ids,
        });
    }

    view.tree_identity
        .clone()
        .ok_or_else(|| ProjectionValidationError {
            code: ProjectionValidationErrorCode::MissingTree,
            resolved_view_id: view.resolved_view_id.clone(),
            conflict_ids: Vec::new(),
            staleness_ids: Vec::new(),
        })
}

fn default_writable_policy(purpose: ProjectionPurpose) -> WritablePolicy {
    match purpose {
        ProjectionPurpose::Execution => WritablePolicy::ReadOnlySourcePrivateOutputs,
        ProjectionPurpose::Compatibility => WritablePolicy::WritableWithExplicitImport,
        ProjectionPurpose::Inspection => WritablePolicy::ReadOnly,
        ProjectionPurpose::Export => WritablePolicy::ExportMaterializationOnly,
    }
}

fn default_store_integrity_policy(purpose: ProjectionPurpose) -> StoreIntegrityPolicy {
    match purpose {
        ProjectionPurpose::Execution => StoreIntegrityPolicy::VerifyBeforeReuse,
        ProjectionPurpose::Compatibility => StoreIntegrityPolicy::VerifyOnImport,
        ProjectionPurpose::Inspection => StoreIntegrityPolicy::VerifyForInspection,
        ProjectionPurpose::Export => StoreIntegrityPolicy::VerifyBeforeExport,
    }
}

fn fixture_root_ref(purpose: ProjectionPurpose, projection_id: &str) -> ProjectionRootRef {
    ProjectionRootRef {
        value: format!(
            "local://.sunlight/projections/{}/{}",
            purpose.as_str(),
            projection_id
        ),
        privacy: RootRefPrivacy::LocalOnlyPath,
    }
}

fn baseline_manifest_ref(view: &ResolvedViewResult) -> String {
    format!(
        "objects/projection-baselines/{}/{}",
        view.repository_id, view.resolved_view_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::{
        fixture_auth_revision, fixture_base_entries, fixture_overlapping_auth_revision,
        fixture_profile_revision, fixture_profile_revision_missing_auth_dependency,
        fixture_resolver_input, resolve_fixture_view, TopicRevisionSelection,
    };

    #[test]
    fn execution_projection_records_exact_view_policy_and_local_root() {
        let view = conflict_free_view();

        let projection = fixture_execution_projection_from_resolved_view(&view).unwrap();

        assert_eq!(projection.record_type, "projection");
        assert_eq!(projection.schema_version, RECORD_SCHEMA_VERSION);
        assert_eq!(projection.id, FIXTURE_EXECUTION_PROJECTION_ID);
        assert_eq!(projection.resolved_view_id, view.resolved_view_id);
        assert_eq!(projection.tree_identity, view.tree_identity.unwrap());
        assert_eq!(projection.purpose, ProjectionPurpose::Execution);
        assert_eq!(
            projection.writable_policy,
            WritablePolicy::ReadOnlySourcePrivateOutputs
        );
        assert_eq!(
            projection.store_integrity_policy,
            StoreIntegrityPolicy::VerifyBeforeReuse
        );
        assert_eq!(projection.root_ref.privacy.privacy_class(), PrivacyClass::LocalOnly);
        assert_eq!(projection.privacy_class, PrivacyClass::LocalOnly);
    }

    #[test]
    fn compatibility_projection_carries_import_baseline_and_session_generation() {
        let view = conflict_free_view();

        let projection =
            fixture_compatibility_projection_from_resolved_view(&view, "gen_agent_a_0001")
                .unwrap();

        assert_eq!(projection.purpose, ProjectionPurpose::Compatibility);
        assert_eq!(
            projection.session_generation_id.as_deref(),
            Some("gen_agent_a_0001")
        );
        assert_eq!(
            projection.writable_policy,
            WritablePolicy::WritableWithExplicitImport
        );
        assert_eq!(
            projection.store_integrity_policy,
            StoreIntegrityPolicy::VerifyOnImport
        );
        let expected_manifest_ref = format!(
            "objects/projection-baselines/{}/{}",
            view.repository_id, view.resolved_view_id
        );
        assert_eq!(
            projection.baseline_manifest_ref.as_deref(),
            Some(expected_manifest_ref.as_str())
        );
    }

    #[test]
    fn inspection_and_export_purposes_have_distinct_policies_and_cache_keys() {
        let view = conflict_free_view();

        let inspection = fixture_inspection_projection_from_resolved_view(&view).unwrap();
        let export = fixture_export_projection_from_resolved_view(&view).unwrap();

        assert_eq!(inspection.purpose, ProjectionPurpose::Inspection);
        assert_eq!(inspection.strategy, ProjectionStrategy::HardlinkReadonly);
        assert_eq!(inspection.writable_policy, WritablePolicy::ReadOnly);
        assert_eq!(export.purpose, ProjectionPurpose::Export);
        assert_eq!(
            export.writable_policy,
            WritablePolicy::ExportMaterializationOnly
        );
        assert_ne!(
            inspection.cache_key.stable_string(),
            export.cache_key.stable_string()
        );
        assert!(export
            .cache_key
            .stable_string()
            .contains(":export:copy:export_materialization_only"));
    }

    #[test]
    fn projection_rejects_conflicted_view_with_inspectable_ids() {
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

        let error = fixture_execution_projection_from_resolved_view(&view).unwrap_err();

        assert_eq!(error.code, ProjectionValidationErrorCode::ConflictedView);
        assert_eq!(error.resolved_view_id, view.resolved_view_id);
        assert_eq!(error.conflict_ids, vec!["conflict_src_auth_ts_0001"]);
        assert!(error.staleness_ids.is_empty());
    }

    #[test]
    fn projection_rejects_stale_view_with_inspectable_ids() {
        let dependent = fixture_profile_revision_missing_auth_dependency();
        let required = fixture_auth_revision();
        let view = resolve_fixture_view(
            fixture_resolver_input(vec![selection(&dependent.topic_id, &dependent.revision_id)]),
            fixture_base_entries(),
            vec![dependent, required],
        );

        let error = fixture_execution_projection_from_resolved_view(&view).unwrap_err();

        assert_eq!(error.code, ProjectionValidationErrorCode::StaleView);
        assert!(error.conflict_ids.is_empty());
        assert_eq!(
            error.staleness_ids,
            vec!["stale_missing_dependency_rev_auth_nullability_0001"]
        );
    }

    #[test]
    fn projection_rejects_missing_tree_without_synthesizing_materialization() {
        let mut view = conflict_free_view();
        view.tree_identity = None;

        let error = fixture_export_projection_from_resolved_view(&view).unwrap_err();

        assert_eq!(error.code, ProjectionValidationErrorCode::MissingTree);
        assert_eq!(error.resolved_view_id, view.resolved_view_id);
        assert!(error.conflict_ids.is_empty());
        assert!(error.staleness_ids.is_empty());
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
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionMaterializationRequest {
    pub purpose: ProjectionPurpose,
    pub projection_id: String,
    pub session_generation_id: Option<String>,
    pub strategy_preference: Vec<ProjectionStrategy>,
    pub fallback_to_copy: bool,
    pub capabilities: ProjectionMaterializationCapabilities,
}

impl ProjectionMaterializationRequest {
    pub fn fixture_execution(capabilities: ProjectionMaterializationCapabilities) -> Self {
        Self {
            purpose: ProjectionPurpose::Execution,
            projection_id: FIXTURE_EXECUTION_PROJECTION_ID.to_string(),
            session_generation_id: None,
            strategy_preference: vec![
                ProjectionStrategy::Reflink,
                ProjectionStrategy::OverlayCopyup,
                ProjectionStrategy::Copy,
            ],
            fallback_to_copy: true,
            capabilities,
        }
    }

    pub fn fixture_inspection(capabilities: ProjectionMaterializationCapabilities) -> Self {
        Self {
            purpose: ProjectionPurpose::Inspection,
            projection_id: FIXTURE_INSPECTION_PROJECTION_ID.to_string(),
            session_generation_id: None,
            strategy_preference: vec![
                ProjectionStrategy::HardlinkReadonly,
                ProjectionStrategy::Reflink,
                ProjectionStrategy::Copy,
            ],
            fallback_to_copy: true,
            capabilities,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionMaterializationCapabilities {
    pub copy_supported: bool,
    pub reflink_supported: bool,
    pub reflink_writes_are_private: bool,
    pub hardlink_supported: bool,
    pub hardlink_readonly_enforced: bool,
    pub hardlink_store_mutation_protected: bool,
    pub overlay_supported: bool,
    pub overlay_copyup_writes_are_private: bool,
    pub preserves_path_policy: bool,
    pub preserves_executable_metadata: bool,
}

impl ProjectionMaterializationCapabilities {
    pub fn copy_only() -> Self {
        Self {
            copy_supported: true,
            reflink_supported: false,
            reflink_writes_are_private: false,
            hardlink_supported: false,
            hardlink_readonly_enforced: false,
            hardlink_store_mutation_protected: false,
            overlay_supported: false,
            overlay_copyup_writes_are_private: false,
            preserves_path_policy: true,
            preserves_executable_metadata: true,
        }
    }

    pub fn all_supported() -> Self {
        Self {
            copy_supported: true,
            reflink_supported: true,
            reflink_writes_are_private: true,
            hardlink_supported: true,
            hardlink_readonly_enforced: true,
            hardlink_store_mutation_protected: true,
            overlay_supported: true,
            overlay_copyup_writes_are_private: true,
            preserves_path_policy: true,
            preserves_executable_metadata: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionMaterializationPlan {
    pub projection: ProjectionRecord,
    pub source: ProjectionMaterializationSource,
    pub local_metadata: ProjectionMaterializationLocalMetadata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionMaterializationSource {
    ResolvedContentTree,
}

impl ProjectionMaterializationSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ResolvedContentTree => "resolved_content_tree",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionMaterializationLocalMetadata {
    pub projection_id: String,
    pub resolved_view_id: String,
    pub tree_identity: SingleRepoTree,
    pub strategy: ProjectionStrategy,
    pub cache_key: String,
    pub root_ref: ProjectionRootRef,
    pub writable_policy: WritablePolicy,
    pub store_integrity_policy: StoreIntegrityPolicy,
    pub source: ProjectionMaterializationSource,
    pub privacy_class: PrivacyClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionMaterializationError {
    pub code: ProjectionMaterializationErrorCode,
    pub resolved_view_id: String,
    pub strategy: Option<ProjectionStrategy>,
    pub validation_error: Option<ProjectionValidationError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionMaterializationErrorCode {
    ProjectionValidationFailed,
    CopyFallbackUnavailable,
    ReflinkUnsupported,
    ReflinkUnsafeForWrites,
    HardlinkReadonlyUnsupported,
    HardlinkReadonlyRequiresReadOnlyPolicy,
    HardlinkReadonlyUnsafeForStore,
    OverlayCopyupUnsupported,
    OverlayCopyupUnsafeForWrites,
    MetadataPolicyUnsupported,
    NoEligibleStrategy,
}

impl ProjectionMaterializationErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProjectionValidationFailed => "projection_materialization_validation_failed",
            Self::CopyFallbackUnavailable => "projection_materialization_copy_unavailable",
            Self::ReflinkUnsupported => "projection_materialization_reflink_unsupported",
            Self::ReflinkUnsafeForWrites => "projection_materialization_reflink_unsafe_for_writes",
            Self::HardlinkReadonlyUnsupported => {
                "projection_materialization_hardlink_readonly_unsupported"
            }
            Self::HardlinkReadonlyRequiresReadOnlyPolicy => {
                "projection_materialization_hardlink_readonly_requires_read_only_policy"
            }
            Self::HardlinkReadonlyUnsafeForStore => {
                "projection_materialization_hardlink_readonly_unsafe_for_store"
            }
            Self::OverlayCopyupUnsupported => {
                "projection_materialization_overlay_copyup_unsupported"
            }
            Self::OverlayCopyupUnsafeForWrites => {
                "projection_materialization_overlay_copyup_unsafe_for_writes"
            }
            Self::MetadataPolicyUnsupported => {
                "projection_materialization_metadata_policy_unsupported"
            }
            Self::NoEligibleStrategy => "projection_materialization_no_eligible_strategy",
        }
    }
}

pub fn plan_fixture_projection_materialization(
    view: &ResolvedViewResult,
    request: ProjectionMaterializationRequest,
) -> Result<ProjectionMaterializationPlan, ProjectionMaterializationError> {
    let tree_identity = validate_projectable_view(view).map_err(|error| {
        ProjectionMaterializationError {
            code: ProjectionMaterializationErrorCode::ProjectionValidationFailed,
            resolved_view_id: view.resolved_view_id.clone(),
            strategy: None,
            validation_error: Some(error),
        }
    })?;
    let writable_policy = default_writable_policy(request.purpose);
    let strategy = select_projection_materialization_strategy(
        &request.strategy_preference,
        request.fallback_to_copy,
        writable_policy,
        &request.capabilities,
    )
    .map_err(|mut error| {
        error.resolved_view_id = view.resolved_view_id.clone();
        error
    })?;
    let projection = fixture_projection_from_resolved_view(
        view,
        request.purpose,
        &request.projection_id,
        strategy,
        request.session_generation_id,
    )
    .map_err(|error| ProjectionMaterializationError {
        code: ProjectionMaterializationErrorCode::ProjectionValidationFailed,
        resolved_view_id: view.resolved_view_id.clone(),
        strategy: Some(strategy),
        validation_error: Some(error),
    })?;
    let local_metadata = ProjectionMaterializationLocalMetadata {
        projection_id: projection.id.clone(),
        resolved_view_id: projection.resolved_view_id.clone(),
        tree_identity,
        strategy: projection.strategy,
        cache_key: projection.cache_key.stable_string(),
        root_ref: projection.root_ref.clone(),
        writable_policy: projection.writable_policy,
        store_integrity_policy: projection.store_integrity_policy,
        source: ProjectionMaterializationSource::ResolvedContentTree,
        privacy_class: PrivacyClass::LocalOnly,
    };

    Ok(ProjectionMaterializationPlan {
        projection,
        source: ProjectionMaterializationSource::ResolvedContentTree,
        local_metadata,
    })
}

pub fn select_projection_materialization_strategy(
    strategy_preference: &[ProjectionStrategy],
    fallback_to_copy: bool,
    writable_policy: WritablePolicy,
    capabilities: &ProjectionMaterializationCapabilities,
) -> Result<ProjectionStrategy, ProjectionMaterializationError> {
    let candidates = if strategy_preference.is_empty() {
        vec![ProjectionStrategy::Copy]
    } else {
        strategy_preference.to_vec()
    };
    let mut first_error = None;

    for strategy in candidates {
        match materialization_strategy_error(strategy, writable_policy, capabilities) {
            None => return Ok(strategy),
            Some(code) => {
                if first_error.is_none() {
                    first_error = Some((strategy, code));
                }
            }
        }
    }

    if fallback_to_copy
        && !strategy_preference.contains(&ProjectionStrategy::Copy)
        && materialization_strategy_error(ProjectionStrategy::Copy, writable_policy, capabilities)
            .is_none()
    {
        return Ok(ProjectionStrategy::Copy);
    }

    let (strategy, code) = first_error.unwrap_or((
        ProjectionStrategy::Copy,
        ProjectionMaterializationErrorCode::NoEligibleStrategy,
    ));
    Err(ProjectionMaterializationError {
        code,
        resolved_view_id: String::new(),
        strategy: Some(strategy),
        validation_error: None,
    })
}

fn materialization_strategy_error(
    strategy: ProjectionStrategy,
    writable_policy: WritablePolicy,
    capabilities: &ProjectionMaterializationCapabilities,
) -> Option<ProjectionMaterializationErrorCode> {
    if !capabilities.preserves_path_policy || !capabilities.preserves_executable_metadata {
        return Some(ProjectionMaterializationErrorCode::MetadataPolicyUnsupported);
    }

    match strategy {
        ProjectionStrategy::Copy => (!capabilities.copy_supported)
            .then_some(ProjectionMaterializationErrorCode::CopyFallbackUnavailable),
        ProjectionStrategy::Reflink => {
            if !capabilities.reflink_supported {
                Some(ProjectionMaterializationErrorCode::ReflinkUnsupported)
            } else if !capabilities.reflink_writes_are_private {
                Some(ProjectionMaterializationErrorCode::ReflinkUnsafeForWrites)
            } else {
                None
            }
        }
        ProjectionStrategy::HardlinkReadonly => {
            if writable_policy != WritablePolicy::ReadOnly {
                Some(ProjectionMaterializationErrorCode::HardlinkReadonlyRequiresReadOnlyPolicy)
            } else if !capabilities.hardlink_supported || !capabilities.hardlink_readonly_enforced {
                Some(ProjectionMaterializationErrorCode::HardlinkReadonlyUnsupported)
            } else if !capabilities.hardlink_store_mutation_protected {
                Some(ProjectionMaterializationErrorCode::HardlinkReadonlyUnsafeForStore)
            } else {
                None
            }
        }
        ProjectionStrategy::OverlayCopyup => {
            if !capabilities.overlay_supported {
                Some(ProjectionMaterializationErrorCode::OverlayCopyupUnsupported)
            } else if !capabilities.overlay_copyup_writes_are_private {
                Some(ProjectionMaterializationErrorCode::OverlayCopyupUnsafeForWrites)
            } else {
                None
            }
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
        assert_eq!(
            projection.root_ref.privacy.privacy_class(),
            PrivacyClass::LocalOnly
        );
        assert_eq!(projection.privacy_class, PrivacyClass::LocalOnly);
    }

    #[test]
    fn compatibility_projection_carries_import_baseline_and_session_generation() {
        let view = conflict_free_view();

        let projection =
            fixture_compatibility_projection_from_resolved_view(&view, "gen_agent_a_0001").unwrap();

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

    #[test]
    fn materialization_plan_prefers_safe_reflink_and_records_local_only_metadata() {
        let view = conflict_free_view();

        let plan = plan_fixture_projection_materialization(
            &view,
            ProjectionMaterializationRequest::fixture_execution(
                ProjectionMaterializationCapabilities::all_supported(),
            ),
        )
        .unwrap();

        assert_eq!(plan.source, ProjectionMaterializationSource::ResolvedContentTree);
        assert_eq!(plan.projection.strategy, ProjectionStrategy::Reflink);
        assert_eq!(
            plan.projection.created_from_content_tree,
            view.tree_identity.as_ref().unwrap().tree_hash
        );
        assert_eq!(plan.local_metadata.strategy, ProjectionStrategy::Reflink);
        assert_eq!(plan.local_metadata.privacy_class, PrivacyClass::LocalOnly);
        assert_eq!(
            plan.local_metadata.root_ref.privacy,
            RootRefPrivacy::LocalOnlyPath
        );
        assert!(plan.local_metadata.cache_key.contains(":execution:reflink:"));
    }

    #[test]
    fn materialization_plan_falls_back_to_copy_when_fast_paths_are_unavailable() {
        let view = conflict_free_view();

        let plan = plan_fixture_projection_materialization(
            &view,
            ProjectionMaterializationRequest::fixture_execution(
                ProjectionMaterializationCapabilities::copy_only(),
            ),
        )
        .unwrap();

        assert_eq!(plan.projection.strategy, ProjectionStrategy::Copy);
        assert!(plan.local_metadata.cache_key.contains(":execution:copy:"));
    }

    #[test]
    fn materialization_plan_reports_stable_error_for_required_unsupported_reflink() {
        let view = conflict_free_view();
        let mut request = ProjectionMaterializationRequest::fixture_execution(
            ProjectionMaterializationCapabilities::copy_only(),
        );
        request.strategy_preference = vec![ProjectionStrategy::Reflink];
        request.fallback_to_copy = false;

        let error = plan_fixture_projection_materialization(&view, request).unwrap_err();

        assert_eq!(
            error.code,
            ProjectionMaterializationErrorCode::ReflinkUnsupported
        );
        assert_eq!(
            error.code.as_str(),
            "projection_materialization_reflink_unsupported"
        );
        assert_eq!(error.strategy, Some(ProjectionStrategy::Reflink));
        assert_eq!(error.resolved_view_id, view.resolved_view_id);
        assert!(error.validation_error.is_none());
    }

    #[test]
    fn materialization_plan_rejects_reflink_without_private_write_isolation() {
        let mut capabilities = ProjectionMaterializationCapabilities::all_supported();
        capabilities.reflink_writes_are_private = false;
        let error = select_projection_materialization_strategy(
            &[ProjectionStrategy::Reflink],
            false,
            WritablePolicy::ReadOnlySourcePrivateOutputs,
            &capabilities,
        )
        .unwrap_err();

        assert_eq!(
            error.code,
            ProjectionMaterializationErrorCode::ReflinkUnsafeForWrites
        );
    }

    #[test]
    fn materialization_plan_allows_hardlink_only_for_read_only_protected_views() {
        let view = conflict_free_view();

        let plan = plan_fixture_projection_materialization(
            &view,
            ProjectionMaterializationRequest::fixture_inspection(
                ProjectionMaterializationCapabilities::all_supported(),
            ),
        )
        .unwrap();

        assert_eq!(plan.projection.strategy, ProjectionStrategy::HardlinkReadonly);
        assert_eq!(plan.projection.writable_policy, WritablePolicy::ReadOnly);

        let error = select_projection_materialization_strategy(
            &[ProjectionStrategy::HardlinkReadonly],
            false,
            WritablePolicy::ReadOnlySourcePrivateOutputs,
            &ProjectionMaterializationCapabilities::all_supported(),
        )
        .unwrap_err();
        assert_eq!(
            error.code,
            ProjectionMaterializationErrorCode::HardlinkReadonlyRequiresReadOnlyPolicy
        );
    }

    #[test]
    fn materialization_plan_rejects_unprotected_hardlink_store_mutation_risk() {
        let mut capabilities = ProjectionMaterializationCapabilities::all_supported();
        capabilities.hardlink_store_mutation_protected = false;

        let error = select_projection_materialization_strategy(
            &[ProjectionStrategy::HardlinkReadonly],
            false,
            WritablePolicy::ReadOnly,
            &capabilities,
        )
        .unwrap_err();

        assert_eq!(
            error.code,
            ProjectionMaterializationErrorCode::HardlinkReadonlyUnsafeForStore
        );
    }

    #[test]
    fn materialization_plan_can_select_overlay_copyup_for_private_writable_outputs() {
        let mut capabilities = ProjectionMaterializationCapabilities::copy_only();
        capabilities.overlay_supported = true;
        capabilities.overlay_copyup_writes_are_private = true;

        let strategy = select_projection_materialization_strategy(
            &[ProjectionStrategy::OverlayCopyup, ProjectionStrategy::Copy],
            true,
            WritablePolicy::WritableWithExplicitImport,
            &capabilities,
        )
        .unwrap();

        assert_eq!(strategy, ProjectionStrategy::OverlayCopyup);
    }

    #[test]
    fn materialization_plan_rejects_metadata_policy_loss_before_strategy_choice() {
        let mut capabilities = ProjectionMaterializationCapabilities::all_supported();
        capabilities.preserves_executable_metadata = false;

        let error = select_projection_materialization_strategy(
            &[ProjectionStrategy::Reflink, ProjectionStrategy::Copy],
            true,
            WritablePolicy::ReadOnlySourcePrivateOutputs,
            &capabilities,
        )
        .unwrap_err();

        assert_eq!(
            error.code,
            ProjectionMaterializationErrorCode::MetadataPolicyUnsupported
        );
    }

    #[test]
    fn materialization_plan_rejects_conflicted_view_before_strategy_selection() {
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

        let error = plan_fixture_projection_materialization(
            &view,
            ProjectionMaterializationRequest::fixture_execution(
                ProjectionMaterializationCapabilities::all_supported(),
            ),
        )
        .unwrap_err();

        assert_eq!(
            error.code,
            ProjectionMaterializationErrorCode::ProjectionValidationFailed
        );
        assert_eq!(
            error.validation_error.unwrap().code,
            ProjectionValidationErrorCode::ConflictedView
        );
        assert!(error.strategy.is_none());
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

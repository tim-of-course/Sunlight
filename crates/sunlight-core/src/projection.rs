use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest as ShaDigest, Sha256};

use crate::artifacts::{ArtifactKind, ContentBlob, ContentTree, PathPolicy};
use crate::records::{canonical_json_bytes, JsonValue, PrivacyClass, RECORD_SCHEMA_VERSION};
use crate::resolver::{ResolvedViewResult, SingleRepoTree};

pub const FIXTURE_EXECUTION_PROJECTION_ID: &str = "projection_exec_auth_profile_0001";
pub const FIXTURE_COMPATIBILITY_PROJECTION_ID: &str = "projection_compat_agent_a_0001";
pub const FIXTURE_INSPECTION_PROJECTION_ID: &str = "projection_inspect_auth_profile_0001";
pub const FIXTURE_EXPORT_PROJECTION_ID: &str = "projection_export_auth_profile_0001";
pub const FIXTURE_CREATED_AT: &str = "2026-07-03T00:00:00Z";
pub const FIXTURE_MANIFEST_MATERIALIZATION_GENERATION: u64 = 1;
pub const PROJECTION_LOCAL_METADATA_DIR: &str = ".sunlight/projections";
pub const PROJECTION_MANIFEST_LOCAL_RECORD_FILE: &str = "projection-manifest-local.json";
pub const PROJECTION_QUARANTINE_LOCAL_METADATA_DIR: &str = ".sunlight/quarantine/projections";

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
pub struct ProjectionStoreIntegrityResult {
    pub privacy_class: PrivacyClass,
    pub integrity_status: ProjectionStoreIntegrityStatus,
    pub policy: StoreIntegrityPolicy,
    pub reason_code: Option<ProjectionStoreIntegrityReasonCode>,
    pub projection_id: String,
    pub resolved_view_id: String,
    pub tree_identity: SingleRepoTree,
    pub root_ref: ProjectionRootRef,
    pub cache_key: String,
    pub manifest_ref: Option<String>,
    pub manifest_digest: Option<String>,
    pub source_truth: ProjectionStoreIntegritySourceTruth,
    pub local_filesystem_source_truth: bool,
    pub quarantine: Option<ProjectionQuarantineResult>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionStoreIntegrityStatus {
    NotChecked,
    Verified,
    Failed,
}

impl ProjectionStoreIntegrityStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotChecked => "not_checked",
            Self::Verified => "verified",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionStoreIntegrityReasonCode {
    ExecutionStoreIntegrityFailed,
}

impl ProjectionStoreIntegrityReasonCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExecutionStoreIntegrityFailed => "execution_store_integrity_failed",
        }
    }

    pub fn reason(self) -> &'static str {
        match self {
            Self::ExecutionStoreIntegrityFailed => "store_integrity_mismatch",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionStoreIntegritySourceTruth {
    NotChecked,
    ImmutableStoreManifest,
}

impl ProjectionStoreIntegritySourceTruth {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotChecked => "not_checked",
            Self::ImmutableStoreManifest => "immutable_store_manifest",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionQuarantineResult {
    pub privacy_class: PrivacyClass,
    pub state: ProjectionRetentionState,
    pub reason_code: ProjectionStoreIntegrityReasonCode,
    pub projection_id: String,
    pub resolved_view_id: String,
    pub root_ref: ProjectionRootRef,
    pub cache_key: String,
    pub manifest_ref: Option<String>,
    pub manifest_digest: Option<String>,
    pub quarantine_refs: ProjectionQuarantineRefs,
    pub provenance: ProjectionQuarantineProvenance,
    pub source_truth: ProjectionStoreIntegritySourceTruth,
    pub local_filesystem_source_truth: bool,
    pub durable_record: Option<String>,
    pub cache_reuse_allowed: bool,
    pub cache_invalidation_reason: ProjectionStoreIntegrityReasonCode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionQuarantineRefs {
    pub projection: String,
    pub cache: String,
    pub native_error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionQuarantineProvenance {
    pub repository_id: String,
    pub resolved_view_id: String,
    pub tree_identity: SingleRepoTree,
    pub created_from_content_tree: String,
    pub store_integrity_policy: StoreIntegrityPolicy,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionFilesystemMaterialization {
    pub plan: ProjectionMaterializationPlan,
    pub projection_root: PathBuf,
    pub local_manifest_record_path: PathBuf,
    pub files_written: usize,
    pub directories_created: usize,
    pub bytes_written: u64,
    pub executable_files: usize,
    pub cleanup: ProjectionCleanupCheck,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionCleanupCheck {
    pub projection_root: PathBuf,
    pub exists: bool,
    pub local_only: bool,
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
pub struct ProjectionManifestRecord {
    pub schema_version: u32,
    pub record_type: &'static str,
    pub id: String,
    pub manifest_digest: String,
    pub projection_id: String,
    pub repository_id: String,
    pub purpose: ProjectionPurpose,
    pub strategy: ProjectionStrategy,
    pub resolved_view_id: String,
    pub session_generation_id: Option<String>,
    pub tree_identity: SingleRepoTree,
    pub path_policy_id: String,
    pub operation_semantics_version: String,
    pub materialization_generation: u64,
    pub root_ref: ProjectionRootRef,
    pub entries: Vec<ProjectionManifestEntry>,
    pub summary: ProjectionManifestSummary,
    pub privacy_class: PrivacyClass,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionManifestLocalRecord {
    pub manifest: ProjectionManifestRecord,
    pub root_binding: ProjectionManifestRootBinding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionManifestRootBinding {
    pub normalized_root_ref: ProjectionRootRef,
    pub normalization: ProjectionRootNormalization,
    pub privacy_class: PrivacyClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionRootNormalization {
    LocalUriRelativeV1,
}

impl ProjectionRootNormalization {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalUriRelativeV1 => "local_uri_relative_v1",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionManifestEntry {
    pub path: String,
    pub kind: ArtifactKind,
    pub artifact_id: String,
    pub content_hash: String,
    pub byte_length: u64,
    pub executable: bool,
    pub tombstone: bool,
    pub classification: String,
    pub path_policy_result: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionManifestSummary {
    pub directories: usize,
    pub files: usize,
    pub bytes: u64,
    pub executable_files: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionManifestIdentityInputs {
    pub projection_id: String,
    pub purpose: ProjectionPurpose,
    pub strategy: ProjectionStrategy,
    pub repository_id: String,
    pub resolved_view_id: String,
    pub session_generation_id: Option<String>,
    pub tree_identity: SingleRepoTree,
    pub path_policy_id: String,
    pub operation_semantics_version: String,
    pub materialization_generation: u64,
}

impl ProjectionManifestRecord {
    pub fn identity_inputs(&self) -> ProjectionManifestIdentityInputs {
        ProjectionManifestIdentityInputs {
            projection_id: self.projection_id.clone(),
            purpose: self.purpose,
            strategy: self.strategy,
            repository_id: self.repository_id.clone(),
            resolved_view_id: self.resolved_view_id.clone(),
            session_generation_id: self.session_generation_id.clone(),
            tree_identity: self.tree_identity.clone(),
            path_policy_id: self.path_policy_id.clone(),
            operation_semantics_version: self.operation_semantics_version.clone(),
            materialization_generation: self.materialization_generation,
        }
    }

    pub fn digest_payload_json(&self) -> JsonValue {
        projection_manifest_digest_payload(&self.identity_inputs(), &self.entries, &self.summary)
    }

    pub fn digest(&self) -> Result<String, ProjectionManifestDigestError> {
        Ok(sha256_digest(&canonical_json_bytes(
            &self.digest_payload_json(),
        )?))
    }

    pub fn to_json_value(&self) -> JsonValue {
        let mut object = BTreeMap::new();
        object.insert(
            "schema_version".to_string(),
            JsonValue::Number(self.schema_version.to_string()),
        );
        object.insert(
            "record_type".to_string(),
            JsonValue::String(self.record_type.to_string()),
        );
        object.insert("id".to_string(), JsonValue::String(self.id.clone()));
        object.insert(
            "manifest_digest".to_string(),
            JsonValue::String(self.manifest_digest.clone()),
        );
        object.insert(
            "projection_id".to_string(),
            JsonValue::String(self.projection_id.clone()),
        );
        object.insert(
            "repository_id".to_string(),
            JsonValue::String(self.repository_id.clone()),
        );
        object.insert(
            "purpose".to_string(),
            JsonValue::String(self.purpose.as_str().to_string()),
        );
        object.insert(
            "strategy".to_string(),
            JsonValue::String(self.strategy.as_str().to_string()),
        );
        object.insert(
            "resolved_view_id".to_string(),
            JsonValue::String(self.resolved_view_id.clone()),
        );
        object.insert(
            "session_generation_id".to_string(),
            optional_string_json(self.session_generation_id.as_deref()),
        );
        object.insert(
            "tree_identity".to_string(),
            tree_identity_json(&self.tree_identity),
        );
        object.insert(
            "path_policy_id".to_string(),
            JsonValue::String(self.path_policy_id.clone()),
        );
        object.insert(
            "operation_semantics_version".to_string(),
            JsonValue::String(self.operation_semantics_version.clone()),
        );
        object.insert(
            "materialization_generation".to_string(),
            JsonValue::Number(self.materialization_generation.to_string()),
        );
        object.insert("root_ref".to_string(), root_ref_json(&self.root_ref));
        object.insert(
            "entries".to_string(),
            JsonValue::Array(
                self.entries
                    .iter()
                    .map(projection_manifest_entry_json)
                    .collect(),
            ),
        );
        object.insert("summary".to_string(), manifest_summary_json(&self.summary));
        object.insert(
            "privacy_class".to_string(),
            JsonValue::String(self.privacy_class.as_str().to_string()),
        );
        object.insert(
            "created_at".to_string(),
            JsonValue::String(self.created_at.clone()),
        );
        JsonValue::Object(object)
    }
}

impl ProjectionManifestLocalRecord {
    pub fn digest(&self) -> Result<String, ProjectionManifestDigestError> {
        self.manifest.digest()
    }

    pub fn to_json_value(&self) -> JsonValue {
        let mut object = BTreeMap::new();
        object.insert("manifest".to_string(), self.manifest.to_json_value());
        object.insert(
            "root_binding".to_string(),
            manifest_root_binding_json(&self.root_binding),
        );
        object.insert(
            "privacy_class".to_string(),
            JsonValue::String(PrivacyClass::LocalOnly.as_str().to_string()),
        );
        JsonValue::Object(object)
    }
}

impl ProjectionQuarantineResult {
    pub fn to_json_value(&self) -> JsonValue {
        let mut object = BTreeMap::new();
        object.insert(
            "privacy_class".to_string(),
            JsonValue::String(self.privacy_class.as_str().to_string()),
        );
        object.insert(
            "state".to_string(),
            JsonValue::String(self.state.as_str().to_string()),
        );
        object.insert(
            "reason".to_string(),
            JsonValue::String(self.reason_code.reason().to_string()),
        );
        object.insert(
            "reason_code".to_string(),
            JsonValue::String(self.reason_code.as_str().to_string()),
        );
        object.insert(
            "projection_id".to_string(),
            JsonValue::String(self.projection_id.clone()),
        );
        object.insert(
            "resolved_view_id".to_string(),
            JsonValue::String(self.resolved_view_id.clone()),
        );
        object.insert("root_ref".to_string(), root_ref_json(&self.root_ref));
        object.insert(
            "cache_key".to_string(),
            JsonValue::String(self.cache_key.clone()),
        );
        object.insert(
            "manifest_ref".to_string(),
            optional_string_json(self.manifest_ref.as_deref()),
        );
        object.insert(
            "manifest_digest".to_string(),
            optional_string_json(self.manifest_digest.as_deref()),
        );
        object.insert(
            "quarantine_refs".to_string(),
            quarantine_refs_json(&self.quarantine_refs),
        );
        object.insert(
            "provenance".to_string(),
            quarantine_provenance_json(&self.provenance),
        );
        object.insert(
            "source_truth".to_string(),
            JsonValue::String(self.source_truth.as_str().to_string()),
        );
        object.insert(
            "local_filesystem_source_truth".to_string(),
            JsonValue::Bool(self.local_filesystem_source_truth),
        );
        object.insert(
            "durable_record".to_string(),
            optional_string_json(self.durable_record.as_deref()),
        );
        object.insert(
            "cache_reuse_allowed".to_string(),
            JsonValue::Bool(self.cache_reuse_allowed),
        );
        object.insert(
            "cache_invalidation_reason".to_string(),
            JsonValue::String(self.cache_invalidation_reason.as_str().to_string()),
        );
        JsonValue::Object(object)
    }
}

impl ProjectionManifestRootBinding {
    pub fn from_normalized_root_ref(normalized_root_ref: ProjectionRootRef) -> Self {
        Self {
            normalized_root_ref,
            normalization: ProjectionRootNormalization::LocalUriRelativeV1,
            privacy_class: PrivacyClass::LocalOnly,
        }
    }
}

impl ProjectionManifestIdentityInputs {
    pub fn from_projection(projection: &ProjectionRecord, materialization_generation: u64) -> Self {
        Self {
            projection_id: projection.id.clone(),
            purpose: projection.purpose,
            strategy: projection.strategy,
            repository_id: projection.repository_id.clone(),
            resolved_view_id: projection.resolved_view_id.clone(),
            session_generation_id: projection.session_generation_id.clone(),
            tree_identity: projection.tree_identity.clone(),
            path_policy_id: projection.path_policy_id.clone(),
            operation_semantics_version: projection.operation_semantics_version.clone(),
            materialization_generation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionManifestDigestError;

impl From<crate::records::RecordError> for ProjectionManifestDigestError {
    fn from(_: crate::records::RecordError) -> Self {
        Self
    }
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
    UnsupportedFilesystemStrategy,
    ContentTreeMismatch,
    MissingContentBlob,
    UnsupportedContentEntryKind,
    ProjectionRootUnavailable,
    ProjectionWriteFailed,
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
            Self::UnsupportedFilesystemStrategy => {
                "projection_materialization_unsupported_filesystem_strategy"
            }
            Self::ContentTreeMismatch => "projection_materialization_content_tree_mismatch",
            Self::MissingContentBlob => "projection_materialization_missing_content_blob",
            Self::UnsupportedContentEntryKind => {
                "projection_materialization_unsupported_content_entry_kind"
            }
            Self::ProjectionRootUnavailable => {
                "projection_materialization_projection_root_unavailable"
            }
            Self::ProjectionWriteFailed => "projection_materialization_write_failed",
        }
    }
}

impl Display for ProjectionMaterializationError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code.as_str())
    }
}

impl Error for ProjectionMaterializationError {}

pub fn materialize_fixture_projection_copy(
    view: &ResolvedViewResult,
    request: ProjectionMaterializationRequest,
    content_tree: &ContentTree,
    blobs: &BTreeMap<String, ContentBlob>,
    projection_root: impl AsRef<Path>,
) -> Result<ProjectionFilesystemMaterialization, ProjectionMaterializationError> {
    let plan = plan_fixture_projection_materialization(view, request)?;
    materialize_projection_plan_copy(&plan, view, content_tree, blobs, projection_root)
}

pub fn materialize_projection_plan_copy(
    plan: &ProjectionMaterializationPlan,
    view: &ResolvedViewResult,
    content_tree: &ContentTree,
    blobs: &BTreeMap<String, ContentBlob>,
    projection_root: impl AsRef<Path>,
) -> Result<ProjectionFilesystemMaterialization, ProjectionMaterializationError> {
    if plan.projection.strategy != ProjectionStrategy::Copy {
        return Err(materialization_error(
            ProjectionMaterializationErrorCode::UnsupportedFilesystemStrategy,
            view,
            Some(plan.projection.strategy),
        ));
    }

    validate_content_tree_matches_view(view, content_tree)?;
    let root = projection_root.as_ref();
    prepare_projection_root(root, view)?;

    let mut files_written = 0;
    let mut directories = Vec::new();
    let mut bytes_written = 0;
    let mut executable_files = 0;
    let path_policy = PathPolicy {
        id: content_tree.path_policy_id.clone(),
    };
    let mut entries = content_tree.entries.clone();
    entries.sort_by(|left, right| left.path.cmp(&right.path));

    for entry in entries.iter().filter(|entry| !entry.tombstone) {
        let relative_path = path_policy.validate(&entry.path).map_err(|_| {
            materialization_error(
                ProjectionMaterializationErrorCode::ContentTreeMismatch,
                view,
                Some(plan.projection.strategy),
            )
        })?;
        let destination = root.join(&relative_path);

        match entry.kind {
            ArtifactKind::File => {
                let blob = blobs.get(&entry.content_ref).ok_or_else(|| {
                    materialization_error(
                        ProjectionMaterializationErrorCode::MissingContentBlob,
                        view,
                        Some(plan.projection.strategy),
                    )
                })?;
                if let Some(parent) = destination.parent() {
                    if !parent.exists() {
                        fs::create_dir_all(parent).map_err(|_| {
                            materialization_error(
                                ProjectionMaterializationErrorCode::ProjectionWriteFailed,
                                view,
                                Some(plan.projection.strategy),
                            )
                        })?;
                    }
                }
                fs::write(&destination, &blob.bytes).map_err(|_| {
                    materialization_error(
                        ProjectionMaterializationErrorCode::ProjectionWriteFailed,
                        view,
                        Some(plan.projection.strategy),
                    )
                })?;
                set_file_executable(&destination, entry.executable).map_err(|_| {
                    materialization_error(
                        ProjectionMaterializationErrorCode::ProjectionWriteFailed,
                        view,
                        Some(plan.projection.strategy),
                    )
                })?;
                files_written += 1;
                bytes_written += blob.bytes.len() as u64;
                executable_files += usize::from(entry.executable);
            }
            ArtifactKind::Directory => directories.push(destination),
            ArtifactKind::Symlink => {
                return Err(materialization_error(
                    ProjectionMaterializationErrorCode::UnsupportedContentEntryKind,
                    view,
                    Some(plan.projection.strategy),
                ));
            }
        }
    }

    for directory in directories {
        fs::create_dir_all(&directory).map_err(|_| {
            materialization_error(
                ProjectionMaterializationErrorCode::ProjectionWriteFailed,
                view,
                Some(plan.projection.strategy),
            )
        })?;
    }

    let manifest = projection_manifest_from_content_tree(
        &plan.projection,
        view,
        content_tree,
        blobs,
        FIXTURE_MANIFEST_MATERIALIZATION_GENERATION,
        FIXTURE_CREATED_AT,
    )?;
    let local_record = ProjectionManifestLocalRecord {
        manifest,
        root_binding: ProjectionManifestRootBinding::from_normalized_root_ref(
            plan.local_metadata.root_ref.clone(),
        ),
    };
    let local_manifest_record_path =
        persist_projection_manifest_local_record(root, &plan.projection, &local_record, view)?;

    Ok(ProjectionFilesystemMaterialization {
        plan: plan.clone(),
        projection_root: root.to_path_buf(),
        local_manifest_record_path,
        files_written,
        directories_created: count_materialized_directories(root).map_err(|_| {
            materialization_error(
                ProjectionMaterializationErrorCode::ProjectionWriteFailed,
                view,
                Some(plan.projection.strategy),
            )
        })?,
        bytes_written,
        executable_files,
        cleanup: projection_cleanup_check(root),
    })
}

pub fn plan_fixture_projection_materialization(
    view: &ResolvedViewResult,
    request: ProjectionMaterializationRequest,
) -> Result<ProjectionMaterializationPlan, ProjectionMaterializationError> {
    let tree_identity =
        validate_projectable_view(view).map_err(|error| ProjectionMaterializationError {
            code: ProjectionMaterializationErrorCode::ProjectionValidationFailed,
            resolved_view_id: view.resolved_view_id.clone(),
            strategy: None,
            validation_error: Some(error),
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

pub fn fixture_projection_manifest_from_content_tree(
    projection: &ProjectionRecord,
    view: &ResolvedViewResult,
    content_tree: &ContentTree,
    blobs: &BTreeMap<String, ContentBlob>,
) -> Result<ProjectionManifestRecord, ProjectionMaterializationError> {
    projection_manifest_from_content_tree(
        projection,
        view,
        content_tree,
        blobs,
        FIXTURE_MANIFEST_MATERIALIZATION_GENERATION,
        FIXTURE_CREATED_AT,
    )
}

pub fn projection_manifest_ref(manifest: &ProjectionManifestRecord) -> String {
    format!(
        "objects/projection-manifests/sha256/{}",
        manifest
            .manifest_digest
            .strip_prefix("sha256:")
            .unwrap_or(&manifest.manifest_digest)
    )
}

pub fn projection_store_integrity_not_checked(
    projection: &ProjectionRecord,
) -> ProjectionStoreIntegrityResult {
    ProjectionStoreIntegrityResult {
        privacy_class: PrivacyClass::LocalOnly,
        integrity_status: ProjectionStoreIntegrityStatus::NotChecked,
        policy: projection.store_integrity_policy,
        reason_code: None,
        projection_id: projection.id.clone(),
        resolved_view_id: projection.resolved_view_id.clone(),
        tree_identity: projection.tree_identity.clone(),
        root_ref: projection.root_ref.clone(),
        cache_key: projection.cache_key.stable_string(),
        manifest_ref: None,
        manifest_digest: None,
        source_truth: ProjectionStoreIntegritySourceTruth::NotChecked,
        local_filesystem_source_truth: false,
        quarantine: None,
    }
}

pub fn projection_store_integrity_verified(
    projection: &ProjectionRecord,
    manifest: &ProjectionManifestRecord,
) -> ProjectionStoreIntegrityResult {
    ProjectionStoreIntegrityResult {
        privacy_class: PrivacyClass::LocalOnly,
        integrity_status: ProjectionStoreIntegrityStatus::Verified,
        policy: projection.store_integrity_policy,
        reason_code: None,
        projection_id: projection.id.clone(),
        resolved_view_id: projection.resolved_view_id.clone(),
        tree_identity: projection.tree_identity.clone(),
        root_ref: projection.root_ref.clone(),
        cache_key: projection.cache_key.stable_string(),
        manifest_ref: Some(projection_manifest_ref(manifest)),
        manifest_digest: Some(manifest.manifest_digest.clone()),
        source_truth: ProjectionStoreIntegritySourceTruth::ImmutableStoreManifest,
        local_filesystem_source_truth: false,
        quarantine: None,
    }
}

pub fn projection_store_integrity_failed_quarantined(
    projection: &ProjectionRecord,
    manifest: &ProjectionManifestRecord,
    reason_code: ProjectionStoreIntegrityReasonCode,
) -> ProjectionStoreIntegrityResult {
    let cache_key = projection.cache_key.stable_string();
    let manifest_ref = projection_manifest_ref(manifest);
    let quarantine = ProjectionQuarantineResult {
        privacy_class: PrivacyClass::LocalOnly,
        state: ProjectionRetentionState::Quarantined,
        reason_code,
        projection_id: projection.id.clone(),
        resolved_view_id: projection.resolved_view_id.clone(),
        root_ref: projection.root_ref.clone(),
        cache_key: cache_key.clone(),
        manifest_ref: Some(manifest_ref.clone()),
        manifest_digest: Some(manifest.manifest_digest.clone()),
        quarantine_refs: ProjectionQuarantineRefs {
            projection: format!("projection:{}", projection.id),
            cache: cache_key.clone(),
            native_error: format!("native-error:{}:{}", reason_code.as_str(), projection.id),
        },
        provenance: ProjectionQuarantineProvenance {
            repository_id: projection.repository_id.clone(),
            resolved_view_id: projection.resolved_view_id.clone(),
            tree_identity: projection.tree_identity.clone(),
            created_from_content_tree: projection.created_from_content_tree.clone(),
            store_integrity_policy: projection.store_integrity_policy,
        },
        source_truth: ProjectionStoreIntegritySourceTruth::ImmutableStoreManifest,
        local_filesystem_source_truth: false,
        durable_record: Some(projection_quarantine_durable_record_ref(
            &projection.id,
            reason_code,
        )),
        cache_reuse_allowed: false,
        cache_invalidation_reason: reason_code,
    };

    ProjectionStoreIntegrityResult {
        privacy_class: PrivacyClass::LocalOnly,
        integrity_status: ProjectionStoreIntegrityStatus::Failed,
        policy: projection.store_integrity_policy,
        reason_code: Some(reason_code),
        projection_id: projection.id.clone(),
        resolved_view_id: projection.resolved_view_id.clone(),
        tree_identity: projection.tree_identity.clone(),
        root_ref: projection.root_ref.clone(),
        cache_key,
        manifest_ref: Some(manifest_ref),
        manifest_digest: Some(manifest.manifest_digest.clone()),
        source_truth: ProjectionStoreIntegritySourceTruth::ImmutableStoreManifest,
        local_filesystem_source_truth: false,
        quarantine: Some(quarantine),
    }
}

fn projection_quarantine_durable_record_ref(
    projection_id: &str,
    reason_code: ProjectionStoreIntegrityReasonCode,
) -> String {
    format!(
        "local://.sunlight/quarantine/projections/{}/{}.json",
        projection_id,
        reason_code.as_str()
    )
}

pub fn projection_store_integrity_from_manifest_scan(
    projection: &ProjectionRecord,
    manifest: &ProjectionManifestRecord,
    blobs: &BTreeMap<String, ContentBlob>,
) -> ProjectionStoreIntegrityResult {
    let integrity_failed = || {
        projection_store_integrity_failed_quarantined(
            projection,
            manifest,
            ProjectionStoreIntegrityReasonCode::ExecutionStoreIntegrityFailed,
        )
    };

    for entry in manifest.entries.iter().filter(|entry| !entry.tombstone) {
        if entry.kind != ArtifactKind::File {
            return integrity_failed();
        }

        let Some(blob) = blobs.get(&entry.content_hash) else {
            return integrity_failed();
        };

        if blob.digest != entry.content_hash {
            return integrity_failed();
        }
        if blob.byte_length() as u64 != entry.byte_length {
            return integrity_failed();
        }
        if blob.classification != entry.classification {
            return integrity_failed();
        }
    }

    projection_store_integrity_verified(projection, manifest)
}

pub fn projection_manifest_from_content_tree(
    projection: &ProjectionRecord,
    view: &ResolvedViewResult,
    content_tree: &ContentTree,
    blobs: &BTreeMap<String, ContentBlob>,
    materialization_generation: u64,
    created_at: impl Into<String>,
) -> Result<ProjectionManifestRecord, ProjectionMaterializationError> {
    validate_projection_matches_view(projection, view)?;
    validate_content_tree_matches_view(view, content_tree)?;

    let path_policy = PathPolicy {
        id: projection.path_policy_id.clone(),
    };
    let mut source_entries = content_tree.entries.clone();
    source_entries.sort_by(|left, right| left.path.cmp(&right.path));

    let mut manifest_entries = Vec::new();
    let mut directories = BTreeSet::new();
    let mut files = 0;
    let mut bytes = 0;
    let mut executable_files = 0;

    for entry in source_entries.iter().filter(|entry| !entry.tombstone) {
        let path = path_policy.validate(&entry.path).map_err(|_| {
            materialization_error(
                ProjectionMaterializationErrorCode::ContentTreeMismatch,
                view,
                Some(projection.strategy),
            )
        })?;

        match entry.kind {
            ArtifactKind::File => {
                let blob = blobs.get(&entry.content_ref).ok_or_else(|| {
                    materialization_error(
                        ProjectionMaterializationErrorCode::MissingContentBlob,
                        view,
                        Some(projection.strategy),
                    )
                })?;
                record_parent_directories(&path, &mut directories);
                files += 1;
                bytes += blob.byte_length() as u64;
                executable_files += usize::from(entry.executable);
                manifest_entries.push(ProjectionManifestEntry {
                    path,
                    kind: ArtifactKind::File,
                    artifact_id: entry.artifact_id.clone(),
                    content_hash: blob.digest.clone(),
                    byte_length: blob.byte_length() as u64,
                    executable: entry.executable,
                    tombstone: false,
                    classification: blob.classification.clone(),
                    path_policy_result: "accepted".to_string(),
                });
            }
            ArtifactKind::Directory => {
                directories.insert(path);
            }
            ArtifactKind::Symlink => {
                return Err(materialization_error(
                    ProjectionMaterializationErrorCode::UnsupportedContentEntryKind,
                    view,
                    Some(projection.strategy),
                ));
            }
        }
    }

    let summary = ProjectionManifestSummary {
        directories: directories.len(),
        files,
        bytes,
        executable_files,
    };
    let identity_inputs =
        ProjectionManifestIdentityInputs::from_projection(projection, materialization_generation);
    let digest_payload =
        projection_manifest_digest_payload(&identity_inputs, &manifest_entries, &summary);
    let manifest_digest = sha256_digest(&canonical_json_bytes(&digest_payload).map_err(|_| {
        materialization_error(
            ProjectionMaterializationErrorCode::ProjectionWriteFailed,
            view,
            Some(projection.strategy),
        )
    })?);

    Ok(ProjectionManifestRecord {
        schema_version: RECORD_SCHEMA_VERSION,
        record_type: "projection_manifest",
        id: fixture_projection_manifest_id(projection, materialization_generation),
        manifest_digest,
        projection_id: projection.id.clone(),
        repository_id: projection.repository_id.clone(),
        purpose: projection.purpose,
        strategy: projection.strategy,
        resolved_view_id: projection.resolved_view_id.clone(),
        session_generation_id: projection.session_generation_id.clone(),
        tree_identity: projection.tree_identity.clone(),
        path_policy_id: projection.path_policy_id.clone(),
        operation_semantics_version: projection.operation_semantics_version.clone(),
        materialization_generation,
        root_ref: projection.root_ref.clone(),
        entries: manifest_entries,
        summary,
        privacy_class: PrivacyClass::LocalOnly,
        created_at: created_at.into(),
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

fn fixture_projection_manifest_id(
    projection: &ProjectionRecord,
    materialization_generation: u64,
) -> String {
    if materialization_generation == FIXTURE_MANIFEST_MATERIALIZATION_GENERATION {
        format!(
            "projection_manifest_{}",
            projection
                .id
                .strip_prefix("projection_")
                .unwrap_or(projection.id.as_str())
        )
    } else {
        format!(
            "projection_manifest_{}_gen_{}",
            projection
                .id
                .strip_prefix("projection_")
                .unwrap_or(projection.id.as_str()),
            materialization_generation
        )
    }
}

fn validate_projection_matches_view(
    projection: &ProjectionRecord,
    view: &ResolvedViewResult,
) -> Result<(), ProjectionMaterializationError> {
    let tree_identity =
        validate_projectable_view(view).map_err(|error| ProjectionMaterializationError {
            code: ProjectionMaterializationErrorCode::ProjectionValidationFailed,
            resolved_view_id: view.resolved_view_id.clone(),
            strategy: Some(projection.strategy),
            validation_error: Some(error),
        })?;

    if projection.repository_id != view.repository_id
        || projection.resolved_view_id != view.resolved_view_id
        || projection.tree_identity != tree_identity
        || projection.path_policy_id != view.path_policy_id
        || projection.operation_semantics_version != view.operation_semantics_version
    {
        return Err(materialization_error(
            ProjectionMaterializationErrorCode::ContentTreeMismatch,
            view,
            Some(projection.strategy),
        ));
    }

    Ok(())
}

fn record_parent_directories(path: &str, directories: &mut BTreeSet<String>) {
    let mut parts = path.split('/').collect::<Vec<_>>();
    parts.pop();
    let mut current = String::new();
    for part in parts {
        if part.is_empty() {
            continue;
        }
        if !current.is_empty() {
            current.push('/');
        }
        current.push_str(part);
        directories.insert(current.clone());
    }
}

fn projection_manifest_digest_payload(
    identity_inputs: &ProjectionManifestIdentityInputs,
    entries: &[ProjectionManifestEntry],
    summary: &ProjectionManifestSummary,
) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "schema_version".to_string(),
        JsonValue::Number(RECORD_SCHEMA_VERSION.to_string()),
    );
    object.insert(
        "record_type".to_string(),
        JsonValue::String("projection_manifest_digest_payload".to_string()),
    );
    object.insert(
        "identity_inputs".to_string(),
        manifest_identity_inputs_json(identity_inputs),
    );
    object.insert(
        "entries".to_string(),
        JsonValue::Array(entries.iter().map(projection_manifest_entry_json).collect()),
    );
    object.insert("summary".to_string(), manifest_summary_json(summary));
    object.insert(
        "privacy_class".to_string(),
        JsonValue::String(PrivacyClass::LocalOnly.as_str().to_string()),
    );
    JsonValue::Object(object)
}

fn manifest_identity_inputs_json(inputs: &ProjectionManifestIdentityInputs) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "projection_id".to_string(),
        JsonValue::String(inputs.projection_id.clone()),
    );
    object.insert(
        "purpose".to_string(),
        JsonValue::String(inputs.purpose.as_str().to_string()),
    );
    object.insert(
        "strategy".to_string(),
        JsonValue::String(inputs.strategy.as_str().to_string()),
    );
    object.insert(
        "repository_id".to_string(),
        JsonValue::String(inputs.repository_id.clone()),
    );
    object.insert(
        "resolved_view_id".to_string(),
        JsonValue::String(inputs.resolved_view_id.clone()),
    );
    object.insert(
        "session_generation_id".to_string(),
        optional_string_json(inputs.session_generation_id.as_deref()),
    );
    object.insert(
        "tree_identity".to_string(),
        tree_identity_json(&inputs.tree_identity),
    );
    object.insert(
        "path_policy_id".to_string(),
        JsonValue::String(inputs.path_policy_id.clone()),
    );
    object.insert(
        "operation_semantics_version".to_string(),
        JsonValue::String(inputs.operation_semantics_version.clone()),
    );
    object.insert(
        "materialization_generation".to_string(),
        JsonValue::Number(inputs.materialization_generation.to_string()),
    );
    JsonValue::Object(object)
}

fn projection_manifest_entry_json(entry: &ProjectionManifestEntry) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert("path".to_string(), JsonValue::String(entry.path.clone()));
    object.insert(
        "kind".to_string(),
        JsonValue::String(entry.kind.as_str().to_string()),
    );
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
        JsonValue::Number(entry.byte_length.to_string()),
    );
    object.insert("executable".to_string(), JsonValue::Bool(entry.executable));
    object.insert("tombstone".to_string(), JsonValue::Bool(entry.tombstone));
    object.insert(
        "classification".to_string(),
        JsonValue::String(entry.classification.clone()),
    );
    object.insert(
        "path_policy_result".to_string(),
        JsonValue::String(entry.path_policy_result.clone()),
    );
    JsonValue::Object(object)
}

fn manifest_summary_json(summary: &ProjectionManifestSummary) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "directories".to_string(),
        JsonValue::Number(summary.directories.to_string()),
    );
    object.insert(
        "files".to_string(),
        JsonValue::Number(summary.files.to_string()),
    );
    object.insert(
        "bytes".to_string(),
        JsonValue::Number(summary.bytes.to_string()),
    );
    object.insert(
        "executable_files".to_string(),
        JsonValue::Number(summary.executable_files.to_string()),
    );
    JsonValue::Object(object)
}

fn tree_identity_json(tree_identity: &SingleRepoTree) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "kind".to_string(),
        JsonValue::String("SingleRepoTree".to_string()),
    );
    object.insert(
        "repository_id".to_string(),
        JsonValue::String(tree_identity.repository_id.clone()),
    );
    object.insert(
        "tree_hash".to_string(),
        JsonValue::String(tree_identity.tree_hash.clone()),
    );
    JsonValue::Object(object)
}

fn quarantine_refs_json(refs: &ProjectionQuarantineRefs) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "projection".to_string(),
        JsonValue::String(refs.projection.clone()),
    );
    object.insert("cache".to_string(), JsonValue::String(refs.cache.clone()));
    object.insert(
        "native_error".to_string(),
        JsonValue::String(refs.native_error.clone()),
    );
    JsonValue::Object(object)
}

fn quarantine_provenance_json(provenance: &ProjectionQuarantineProvenance) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "repository_id".to_string(),
        JsonValue::String(provenance.repository_id.clone()),
    );
    object.insert(
        "resolved_view_id".to_string(),
        JsonValue::String(provenance.resolved_view_id.clone()),
    );
    object.insert(
        "tree_identity".to_string(),
        tree_identity_json(&provenance.tree_identity),
    );
    object.insert(
        "created_from_content_tree".to_string(),
        JsonValue::String(provenance.created_from_content_tree.clone()),
    );
    object.insert(
        "store_integrity_policy".to_string(),
        JsonValue::String(provenance.store_integrity_policy.as_str().to_string()),
    );
    JsonValue::Object(object)
}

fn root_ref_json(root_ref: &ProjectionRootRef) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "value".to_string(),
        JsonValue::String(root_ref.value.clone()),
    );
    object.insert(
        "privacy".to_string(),
        JsonValue::String(root_ref.privacy.as_str().to_string()),
    );
    object.insert(
        "privacy_class".to_string(),
        JsonValue::String(root_ref.privacy.privacy_class().as_str().to_string()),
    );
    JsonValue::Object(object)
}

fn manifest_root_binding_json(root_binding: &ProjectionManifestRootBinding) -> JsonValue {
    let mut object = BTreeMap::new();
    object.insert(
        "normalized_root_ref".to_string(),
        root_ref_json(&root_binding.normalized_root_ref),
    );
    object.insert(
        "normalization".to_string(),
        JsonValue::String(root_binding.normalization.as_str().to_string()),
    );
    object.insert(
        "privacy_class".to_string(),
        JsonValue::String(root_binding.privacy_class.as_str().to_string()),
    );
    JsonValue::Object(object)
}

fn optional_string_json(value: Option<&str>) -> JsonValue {
    value
        .map(|value| JsonValue::String(value.to_string()))
        .unwrap_or(JsonValue::Null)
}

fn sha256_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity("sha256:".len() + digest.len() * 2);
    output.push_str("sha256:");
    for byte in digest {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn validate_content_tree_matches_view(
    view: &ResolvedViewResult,
    content_tree: &ContentTree,
) -> Result<(), ProjectionMaterializationError> {
    let tree_identity =
        validate_projectable_view(view).map_err(|error| ProjectionMaterializationError {
            code: ProjectionMaterializationErrorCode::ProjectionValidationFailed,
            resolved_view_id: view.resolved_view_id.clone(),
            strategy: Some(ProjectionStrategy::Copy),
            validation_error: Some(error),
        })?;

    if content_tree.repository_id != view.repository_id
        || content_tree.repository_id != tree_identity.repository_id
        || content_tree.tree_hash != tree_identity.tree_hash
        || content_tree.path_policy_id != view.path_policy_id
    {
        return Err(materialization_error(
            ProjectionMaterializationErrorCode::ContentTreeMismatch,
            view,
            Some(ProjectionStrategy::Copy),
        ));
    }

    let active_entries = content_tree
        .entries
        .iter()
        .filter(|entry| !entry.tombstone)
        .map(|entry| (entry.path.clone(), entry))
        .collect::<BTreeMap<_, _>>();

    if active_entries.len() != view.tree_entries.len() {
        return Err(materialization_error(
            ProjectionMaterializationErrorCode::ContentTreeMismatch,
            view,
            Some(ProjectionStrategy::Copy),
        ));
    }

    for (path, view_entry) in &view.tree_entries {
        let Some(tree_entry) = active_entries.get(path) else {
            return Err(materialization_error(
                ProjectionMaterializationErrorCode::ContentTreeMismatch,
                view,
                Some(ProjectionStrategy::Copy),
            ));
        };
        if tree_entry.artifact_id != view_entry.artifact_id
            || tree_entry.content_ref != view_entry.content_hash
        {
            return Err(materialization_error(
                ProjectionMaterializationErrorCode::ContentTreeMismatch,
                view,
                Some(ProjectionStrategy::Copy),
            ));
        }
    }

    Ok(())
}

fn prepare_projection_root(
    root: &Path,
    view: &ResolvedViewResult,
) -> Result<(), ProjectionMaterializationError> {
    if root.exists() {
        if !root.is_dir()
            || root
                .read_dir()
                .map_or(true, |mut entries| entries.next().is_some())
        {
            return Err(materialization_error(
                ProjectionMaterializationErrorCode::ProjectionRootUnavailable,
                view,
                Some(ProjectionStrategy::Copy),
            ));
        }
        return Ok(());
    }

    fs::create_dir_all(root).map_err(|_| {
        materialization_error(
            ProjectionMaterializationErrorCode::ProjectionRootUnavailable,
            view,
            Some(ProjectionStrategy::Copy),
        )
    })
}

fn projection_cleanup_check(root: &Path) -> ProjectionCleanupCheck {
    ProjectionCleanupCheck {
        projection_root: root.to_path_buf(),
        exists: root.exists(),
        local_only: true,
    }
}

pub fn projection_manifest_local_record_path(
    projection_root: impl AsRef<Path>,
    projection: &ProjectionRecord,
) -> PathBuf {
    projection_root
        .as_ref()
        .join(PROJECTION_LOCAL_METADATA_DIR)
        .join(projection.purpose.as_str())
        .join(&projection.id)
        .join(PROJECTION_MANIFEST_LOCAL_RECORD_FILE)
}

pub fn projection_quarantine_local_record_path(
    projection_root: impl AsRef<Path>,
    quarantine: &ProjectionQuarantineResult,
) -> PathBuf {
    projection_root
        .as_ref()
        .join(PROJECTION_QUARANTINE_LOCAL_METADATA_DIR)
        .join(&quarantine.projection_id)
        .join(format!("{}.json", quarantine.reason_code.as_str()))
}

pub fn persist_projection_quarantine_local_record(
    projection_root: impl AsRef<Path>,
    quarantine: &ProjectionQuarantineResult,
) -> std::io::Result<PathBuf> {
    let path = projection_quarantine_local_record_path(projection_root, quarantine);
    let Some(parent) = path.parent() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "projection quarantine record path has no parent",
        ));
    };
    fs::create_dir_all(parent)?;
    let bytes = canonical_json_bytes(&quarantine.to_json_value())
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    fs::write(&path, bytes)?;
    Ok(path)
}

fn persist_projection_manifest_local_record(
    projection_root: &Path,
    projection: &ProjectionRecord,
    local_record: &ProjectionManifestLocalRecord,
    view: &ResolvedViewResult,
) -> Result<PathBuf, ProjectionMaterializationError> {
    let path = projection_manifest_local_record_path(projection_root, projection);
    let Some(parent) = path.parent() else {
        return Err(materialization_error(
            ProjectionMaterializationErrorCode::ProjectionWriteFailed,
            view,
            Some(projection.strategy),
        ));
    };
    fs::create_dir_all(parent).map_err(|_| {
        materialization_error(
            ProjectionMaterializationErrorCode::ProjectionWriteFailed,
            view,
            Some(projection.strategy),
        )
    })?;
    let bytes = canonical_json_bytes(&local_record.to_json_value()).map_err(|_| {
        materialization_error(
            ProjectionMaterializationErrorCode::ProjectionWriteFailed,
            view,
            Some(projection.strategy),
        )
    })?;
    fs::write(&path, bytes).map_err(|_| {
        materialization_error(
            ProjectionMaterializationErrorCode::ProjectionWriteFailed,
            view,
            Some(projection.strategy),
        )
    })?;
    Ok(path)
}

fn count_materialized_directories(root: &Path) -> std::io::Result<usize> {
    let mut count = usize::from(root.is_dir());
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if is_projection_local_metadata_path(root, &path) {
            continue;
        }
        if path.is_dir() {
            count += count_materialized_directories(&path)?;
        }
    }
    Ok(count)
}

pub fn is_projection_local_metadata_path(root: &Path, path: &Path) -> bool {
    path.strip_prefix(root)
        .ok()
        .map(|relative| {
            let mut components = relative.components();
            matches!(
                (components.next(), components.next()),
                (Some(first), Some(second))
                    if first.as_os_str() == ".sunlight"
                        && (second.as_os_str() == "projections"
                            || second.as_os_str() == "quarantine")
            )
        })
        .unwrap_or(false)
}

#[cfg(unix)]
fn set_file_executable(path: &Path, executable: bool) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mode = if executable { 0o755 } else { 0o644 };
    let permissions = fs::Permissions::from_mode(mode);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn set_file_executable(_path: &Path, _executable: bool) -> std::io::Result<()> {
    Ok(())
}

fn materialization_error(
    code: ProjectionMaterializationErrorCode,
    view: &ResolvedViewResult,
    strategy: Option<ProjectionStrategy>,
) -> ProjectionMaterializationError {
    ProjectionMaterializationError {
        code,
        resolved_view_id: view.resolved_view_id.clone(),
        strategy,
        validation_error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::{InMemoryArtifactStore, FILE_OPERATION_SEMANTICS_VERSION};
    use crate::records::parse_json_record;
    use crate::resolver::{
        fixture_auth_revision, fixture_base_entries, fixture_overlapping_auth_revision,
        fixture_profile_revision, fixture_profile_revision_missing_auth_dependency,
        fixture_resolver_input, resolve_fixture_view, DependencyClosure,
        DeterministicResolverOrder, TopicRevisionSelection, TreeEntryState,
        FIXTURE_BASE_CHECKPOINT_ID, FIXTURE_BASE_RESOLVED_VIEW_ID,
    };
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

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

        assert_eq!(
            plan.source,
            ProjectionMaterializationSource::ResolvedContentTree
        );
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
        assert!(plan
            .local_metadata
            .cache_key
            .contains(":execution:reflink:"));
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

        assert_eq!(
            plan.projection.strategy,
            ProjectionStrategy::HardlinkReadonly
        );
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

    #[test]
    fn fixture_projection_manifest_records_basic_app_entries_and_summary() {
        let store = InMemoryArtifactStore::fixture_basic_app();
        let view = view_for_store(&store);
        let projection = fixture_execution_projection_from_resolved_view(&view).unwrap();

        let manifest = fixture_projection_manifest_from_content_tree(
            &projection,
            &view,
            store.tree(),
            store.content_blobs(),
        )
        .unwrap();

        assert_eq!(manifest.record_type, "projection_manifest");
        assert_eq!(manifest.schema_version, RECORD_SCHEMA_VERSION);
        assert_eq!(manifest.id, "projection_manifest_exec_auth_profile_0001");
        assert_eq!(manifest.projection_id, FIXTURE_EXECUTION_PROJECTION_ID);
        assert_eq!(manifest.repository_id, store.tree().repository_id);
        assert_eq!(manifest.purpose, ProjectionPurpose::Execution);
        assert_eq!(manifest.strategy, ProjectionStrategy::Copy);
        assert_eq!(manifest.privacy_class, PrivacyClass::LocalOnly);
        assert_eq!(
            manifest.identity_inputs(),
            ProjectionManifestIdentityInputs::from_projection(
                &projection,
                FIXTURE_MANIFEST_MATERIALIZATION_GENERATION
            )
        );
        assert_eq!(
            manifest
                .entries
                .iter()
                .map(|entry| entry.path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "README.md",
                "docs/guide.md",
                "scripts/build.sh",
                "src/auth.ts",
                "src/profile.ts"
            ]
        );
        assert_eq!(
            manifest.summary,
            ProjectionManifestSummary {
                directories: 3,
                files: 5,
                bytes: 222,
                executable_files: 1,
            }
        );
        assert_eq!(manifest.entries[2].artifact_id, "artifact_scripts_build_sh");
        assert_eq!(manifest.entries[2].content_hash, "sha256:build_base");
        assert_eq!(manifest.entries[2].byte_length, 29);
        assert!(manifest.entries[2].executable);
        assert_eq!(manifest.entries[2].classification, "source");
        assert_eq!(manifest.entries[2].path_policy_result, "accepted");
        assert_eq!(manifest.digest().unwrap(), manifest.manifest_digest);
    }

    #[test]
    fn projection_manifest_entry_ordering_and_digest_are_deterministic() {
        let store = InMemoryArtifactStore::fixture_basic_app();
        let view = view_for_store(&store);
        let projection = fixture_execution_projection_from_resolved_view(&view).unwrap();
        let mut reordered_tree = store.tree().clone();
        reordered_tree.entries.reverse();

        let manifest = fixture_projection_manifest_from_content_tree(
            &projection,
            &view,
            store.tree(),
            store.content_blobs(),
        )
        .unwrap();
        let reordered_manifest = fixture_projection_manifest_from_content_tree(
            &projection,
            &view,
            &reordered_tree,
            store.content_blobs(),
        )
        .unwrap();

        assert_eq!(reordered_manifest.entries, manifest.entries);
        assert_eq!(reordered_manifest.summary, manifest.summary);
        assert_eq!(reordered_manifest.manifest_digest, manifest.manifest_digest);
        assert_eq!(
            canonical_json_bytes(&reordered_manifest.digest_payload_json()).unwrap(),
            canonical_json_bytes(&manifest.digest_payload_json()).unwrap()
        );
    }

    #[test]
    fn projection_store_integrity_verified_result_uses_fixture_manifest_context() {
        let store = InMemoryArtifactStore::fixture_basic_app();
        let view = view_for_store(&store);
        let projection = fixture_execution_projection_from_resolved_view(&view).unwrap();
        let manifest = fixture_projection_manifest_from_content_tree(
            &projection,
            &view,
            store.tree(),
            store.content_blobs(),
        )
        .unwrap();

        let result = projection_store_integrity_verified(&projection, &manifest);

        assert_eq!(
            result.integrity_status,
            ProjectionStoreIntegrityStatus::Verified
        );
        assert_eq!(result.privacy_class, PrivacyClass::LocalOnly);
        assert_eq!(result.reason_code, None);
        assert_eq!(result.projection_id, projection.id);
        assert_eq!(result.root_ref, projection.root_ref);
        assert_eq!(result.cache_key, projection.cache_key.stable_string());
        assert_eq!(
            result.manifest_ref.as_deref(),
            Some(projection_manifest_ref(&manifest).as_str())
        );
        assert_eq!(
            result.manifest_digest.as_deref(),
            Some(manifest.manifest_digest.as_str())
        );
        assert_eq!(
            result.source_truth,
            ProjectionStoreIntegritySourceTruth::ImmutableStoreManifest
        );
        assert!(!result.local_filesystem_source_truth);
        assert!(result.quarantine.is_none());
    }

    #[test]
    fn projection_store_integrity_manifest_scan_verifies_fixture_blobs() {
        let store = InMemoryArtifactStore::fixture_basic_app();
        let view = view_for_store(&store);
        let projection = fixture_execution_projection_from_resolved_view(&view).unwrap();
        let manifest = fixture_projection_manifest_from_content_tree(
            &projection,
            &view,
            store.tree(),
            store.content_blobs(),
        )
        .unwrap();

        let result = projection_store_integrity_from_manifest_scan(
            &projection,
            &manifest,
            store.content_blobs(),
        );

        assert_eq!(
            result.integrity_status,
            ProjectionStoreIntegrityStatus::Verified
        );
        assert_eq!(result.reason_code, None);
        assert_eq!(result.privacy_class, PrivacyClass::LocalOnly);
        assert_eq!(
            result.source_truth,
            ProjectionStoreIntegritySourceTruth::ImmutableStoreManifest
        );
        assert!(!result.local_filesystem_source_truth);
        assert!(result.quarantine.is_none());
    }

    #[test]
    fn projection_store_integrity_manifest_scan_quarantines_missing_blob() {
        let store = InMemoryArtifactStore::fixture_basic_app();
        let view = view_for_store(&store);
        let projection = fixture_execution_projection_from_resolved_view(&view).unwrap();
        let manifest = fixture_projection_manifest_from_content_tree(
            &projection,
            &view,
            store.tree(),
            store.content_blobs(),
        )
        .unwrap();
        let mut blobs = store.content_blobs().clone();
        blobs.remove(&manifest.entries[0].content_hash);

        let result = projection_store_integrity_from_manifest_scan(&projection, &manifest, &blobs);

        assert_eq!(
            result.integrity_status,
            ProjectionStoreIntegrityStatus::Failed
        );
        assert_eq!(
            result.reason_code,
            Some(ProjectionStoreIntegrityReasonCode::ExecutionStoreIntegrityFailed)
        );
        assert_eq!(
            result.source_truth,
            ProjectionStoreIntegritySourceTruth::ImmutableStoreManifest
        );
        assert!(!result.local_filesystem_source_truth);
        assert!(result.quarantine.is_some());
    }

    #[test]
    fn projection_store_integrity_manifest_scan_quarantines_digest_mismatch() {
        let store = InMemoryArtifactStore::fixture_basic_app();
        let view = view_for_store(&store);
        let projection = fixture_execution_projection_from_resolved_view(&view).unwrap();
        let manifest = fixture_projection_manifest_from_content_tree(
            &projection,
            &view,
            store.tree(),
            store.content_blobs(),
        )
        .unwrap();
        let mut blobs = store.content_blobs().clone();
        let content_hash = manifest.entries[0].content_hash.clone();
        blobs.get_mut(&content_hash).unwrap().digest = "sha256:tampered".to_string();

        let result = projection_store_integrity_from_manifest_scan(&projection, &manifest, &blobs);

        assert_eq!(
            result.integrity_status,
            ProjectionStoreIntegrityStatus::Failed
        );
        assert_eq!(
            result.reason_code,
            Some(ProjectionStoreIntegrityReasonCode::ExecutionStoreIntegrityFailed)
        );
        assert!(result.quarantine.is_some());
    }

    #[test]
    fn projection_store_integrity_manifest_scan_quarantines_byte_length_mismatch() {
        let store = InMemoryArtifactStore::fixture_basic_app();
        let view = view_for_store(&store);
        let projection = fixture_execution_projection_from_resolved_view(&view).unwrap();
        let manifest = fixture_projection_manifest_from_content_tree(
            &projection,
            &view,
            store.tree(),
            store.content_blobs(),
        )
        .unwrap();
        let mut blobs = store.content_blobs().clone();
        let content_hash = manifest.entries[0].content_hash.clone();
        blobs.get_mut(&content_hash).unwrap().bytes.push(b'!');

        let result = projection_store_integrity_from_manifest_scan(&projection, &manifest, &blobs);

        assert_eq!(
            result.integrity_status,
            ProjectionStoreIntegrityStatus::Failed
        );
        assert_eq!(
            result.reason_code,
            Some(ProjectionStoreIntegrityReasonCode::ExecutionStoreIntegrityFailed)
        );
        assert!(result.quarantine.is_some());
    }

    #[test]
    fn projection_store_integrity_mismatch_result_quarantines_projection_cache_context() {
        let store = InMemoryArtifactStore::fixture_basic_app();
        let view = view_for_store(&store);
        let projection = fixture_execution_projection_from_resolved_view(&view).unwrap();
        let manifest = fixture_projection_manifest_from_content_tree(
            &projection,
            &view,
            store.tree(),
            store.content_blobs(),
        )
        .unwrap();

        let result = projection_store_integrity_failed_quarantined(
            &projection,
            &manifest,
            ProjectionStoreIntegrityReasonCode::ExecutionStoreIntegrityFailed,
        );

        assert_eq!(
            result.integrity_status,
            ProjectionStoreIntegrityStatus::Failed
        );
        assert_eq!(
            result.reason_code,
            Some(ProjectionStoreIntegrityReasonCode::ExecutionStoreIntegrityFailed)
        );
        assert_eq!(result.projection_id, FIXTURE_EXECUTION_PROJECTION_ID);
        assert_eq!(result.resolved_view_id, view.resolved_view_id);
        assert_eq!(result.root_ref, projection.root_ref);
        assert_eq!(result.cache_key, projection.cache_key.stable_string());
        assert_eq!(
            result.manifest_digest.as_deref(),
            Some(manifest.manifest_digest.as_str())
        );

        let quarantine = result.quarantine.as_ref().unwrap();
        assert_eq!(quarantine.privacy_class, PrivacyClass::LocalOnly);
        assert_eq!(quarantine.state, ProjectionRetentionState::Quarantined);
        assert_eq!(quarantine.reason_code.reason(), "store_integrity_mismatch");
        assert_eq!(
            quarantine.quarantine_refs.projection,
            "projection:projection_exec_auth_profile_0001"
        );
        assert_eq!(quarantine.quarantine_refs.cache, result.cache_key);
        assert_eq!(
            quarantine.quarantine_refs.native_error,
            "native-error:execution_store_integrity_failed:projection_exec_auth_profile_0001"
        );
        assert_eq!(
            quarantine.provenance.store_integrity_policy,
            StoreIntegrityPolicy::VerifyBeforeReuse
        );
        assert_eq!(
            quarantine.source_truth,
            ProjectionStoreIntegritySourceTruth::ImmutableStoreManifest
        );
        assert!(!quarantine.local_filesystem_source_truth);
        assert_eq!(
            quarantine.durable_record.as_deref(),
            Some(
                "local://.sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json"
            )
        );
        assert!(!quarantine.cache_reuse_allowed);
        assert_eq!(
            quarantine.cache_invalidation_reason,
            ProjectionStoreIntegrityReasonCode::ExecutionStoreIntegrityFailed
        );
    }

    #[test]
    fn projection_quarantine_local_record_persists_canonical_json_under_durable_record_path() {
        let store = InMemoryArtifactStore::fixture_basic_app();
        let view = view_for_store(&store);
        let projection = fixture_execution_projection_from_resolved_view(&view).unwrap();
        let manifest = fixture_projection_manifest_from_content_tree(
            &projection,
            &view,
            store.tree(),
            store.content_blobs(),
        )
        .unwrap();
        let result = projection_store_integrity_failed_quarantined(
            &projection,
            &manifest,
            ProjectionStoreIntegrityReasonCode::ExecutionStoreIntegrityFailed,
        );
        let quarantine = result.quarantine.as_ref().unwrap();
        let root = temp_projection_root("quarantine-local-record");

        let path = persist_projection_quarantine_local_record(&root, quarantine).unwrap();

        assert_eq!(
            path,
            root.join(".sunlight")
                .join("quarantine")
                .join("projections")
                .join(FIXTURE_EXECUTION_PROJECTION_ID)
                .join("execution_store_integrity_failed.json")
        );
        let bytes = fs::read(&path).unwrap();
        assert_eq!(bytes, canonical_json_bytes(&quarantine.to_json_value()).unwrap());
        let parsed = parse_json_record(&bytes).unwrap();
        let JsonValue::Object(record) = parsed else {
            panic!("quarantine record should be a JSON object");
        };
        assert_eq!(
            record.get("privacy_class"),
            Some(&JsonValue::String("local_only".to_string()))
        );
        assert_eq!(
            record.get("reason_code"),
            Some(&JsonValue::String(
                "execution_store_integrity_failed".to_string()
            ))
        );
        assert_eq!(
            record.get("durable_record"),
            Some(&JsonValue::String(
                "local://.sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json".to_string()
            ))
        );
    }

    #[test]
    fn projection_local_metadata_path_excludes_projection_and_quarantine_metadata_only() {
        let root = Path::new("/tmp/projection-root");

        assert!(is_projection_local_metadata_path(
            root,
            &root.join(".sunlight/projections/execution/projection_exec_auth_profile_0001/projection-manifest-local.json")
        ));
        assert!(is_projection_local_metadata_path(
            root,
            &root.join(".sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json")
        ));
        assert!(!is_projection_local_metadata_path(
            root,
            &root.join(".sunlight/other/local.txt")
        ));
        assert!(!is_projection_local_metadata_path(
            root,
            &root.join("src/auth.ts")
        ));
    }

    #[test]
    fn projection_manifest_digest_tracks_identity_inputs_not_storage_metadata() {
        let store = InMemoryArtifactStore::fixture_basic_app();
        let view = view_for_store(&store);
        let projection = fixture_execution_projection_from_resolved_view(&view).unwrap();
        let manifest = fixture_projection_manifest_from_content_tree(
            &projection,
            &view,
            store.tree(),
            store.content_blobs(),
        )
        .unwrap();

        let mut moved_manifest = manifest.clone();
        moved_manifest.root_ref.value =
            "local://.sunlight/projections/execution/moved-root".to_string();
        moved_manifest.created_at = "2026-07-04T00:00:00Z".to_string();
        assert_eq!(moved_manifest.digest().unwrap(), manifest.manifest_digest);

        let local_record = ProjectionManifestLocalRecord {
            manifest: manifest.clone(),
            root_binding: ProjectionManifestRootBinding::from_normalized_root_ref(
                manifest.root_ref.clone(),
            ),
        };
        let mut rebound_local_record = local_record.clone();
        rebound_local_record.root_binding.normalized_root_ref.value =
            "local://.sunlight/projections/execution/rebound-root".to_string();
        assert_eq!(local_record.digest().unwrap(), manifest.manifest_digest);
        assert_eq!(
            rebound_local_record.digest().unwrap(),
            manifest.manifest_digest
        );
        assert_ne!(rebound_local_record.root_binding, local_record.root_binding);
        let local_record_json =
            String::from_utf8(canonical_json_bytes(&local_record.to_json_value()).unwrap())
                .unwrap();
        assert!(local_record_json.contains("\"root_binding\""));
        assert!(local_record_json.contains("\"normalization\":\"local_uri_relative_v1\""));

        let next_generation_manifest = projection_manifest_from_content_tree(
            &projection,
            &view,
            store.tree(),
            store.content_blobs(),
            2,
            FIXTURE_CREATED_AT,
        )
        .unwrap();
        assert_eq!(
            next_generation_manifest
                .identity_inputs()
                .materialization_generation,
            2
        );
        assert_ne!(
            next_generation_manifest.manifest_digest,
            manifest.manifest_digest
        );
    }

    #[test]
    fn filesystem_copy_materializes_basic_app_resolved_content_tree() {
        let store = InMemoryArtifactStore::fixture_basic_app();
        let view = view_for_store(&store);
        let root = temp_projection_root("copy-materializes-basic-app");

        let materialization = materialize_fixture_projection_copy(
            &view,
            copy_request(),
            store.tree(),
            store.content_blobs(),
            &root,
        )
        .unwrap();

        assert_eq!(
            materialization.plan.projection.strategy,
            ProjectionStrategy::Copy
        );
        assert_eq!(materialization.files_written, 5);
        assert_eq!(materialization.executable_files, 1);
        assert!(materialization.cleanup.local_only);
        assert_eq!(
            fs::read_to_string(root.join("README.md")).unwrap(),
            "# Fixture Basic App\n\nUses User.email for login.\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("src/auth.ts")).unwrap(),
            "export function login(email: string) {\n  return email.trim().toLowerCase();\n}\n"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let script_mode = fs::metadata(root.join("scripts/build.sh"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111;
            let source_mode = fs::metadata(root.join("src/auth.ts"))
                .unwrap()
                .permissions()
                .mode()
                & 0o111;
            assert_ne!(script_mode, 0);
            assert_eq!(source_mode, 0);
        }
    }

    #[test]
    fn filesystem_copy_persists_local_projection_manifest_root_binding() {
        let store = InMemoryArtifactStore::fixture_basic_app();
        let view = view_for_store(&store);
        let root = temp_projection_root("copy-persists-local-root-binding");

        let materialization = materialize_fixture_projection_copy(
            &view,
            copy_request(),
            store.tree(),
            store.content_blobs(),
            &root,
        )
        .unwrap();
        let expected_path =
            projection_manifest_local_record_path(&root, &materialization.plan.projection);

        assert_eq!(materialization.local_manifest_record_path, expected_path);
        assert!(expected_path.is_file());
        assert_eq!(materialization.directories_created, 4);
        let bytes = fs::read(&expected_path).unwrap();
        let parsed = parse_json_record(&bytes).unwrap();
        let JsonValue::Object(envelope) = parsed else {
            panic!("local manifest record should be a JSON object");
        };
        assert_eq!(
            envelope.get("privacy_class"),
            Some(&JsonValue::String("local_only".to_string()))
        );

        let JsonValue::Object(manifest) = envelope.get("manifest").unwrap() else {
            panic!("local manifest record should include manifest object");
        };
        let manifest_digest = manifest.get("manifest_digest").unwrap();
        assert_eq!(
            manifest.get("id"),
            Some(&JsonValue::String(
                "projection_manifest_exec_auth_profile_0001".to_string()
            ))
        );
        assert_eq!(
            manifest.get("root_ref").and_then(|value| match value {
                JsonValue::Object(root_ref) => root_ref.get("value"),
                _ => None,
            }),
            Some(&JsonValue::String(
                "local://.sunlight/projections/execution/projection_exec_auth_profile_0001"
                    .to_string()
            ))
        );

        let JsonValue::Object(root_binding) = envelope.get("root_binding").unwrap() else {
            panic!("local manifest record should include root_binding object");
        };
        assert_eq!(
            root_binding.get("normalization"),
            Some(&JsonValue::String("local_uri_relative_v1".to_string()))
        );
        assert_eq!(
            root_binding.get("privacy_class"),
            Some(&JsonValue::String("local_only".to_string()))
        );
        assert_eq!(
            root_binding
                .get("normalized_root_ref")
                .and_then(|value| match value {
                    JsonValue::Object(root_ref) => root_ref.get("value"),
                    _ => None,
                }),
            Some(&JsonValue::String(
                "local://.sunlight/projections/execution/projection_exec_auth_profile_0001"
                    .to_string()
            ))
        );

        let persisted = String::from_utf8(bytes).unwrap();
        assert!(!persisted.contains(&root.display().to_string()));
        assert!(
            matches!(manifest_digest, JsonValue::String(value) if value.starts_with("sha256:"))
        );
    }

    #[test]
    fn filesystem_copy_does_not_read_git_working_tree_source() {
        let store = InMemoryArtifactStore::fixture_basic_app();
        let view = view_for_store(&store);
        let root = temp_projection_root("copy-ignores-working-tree");
        let working_tree = temp_projection_root("working-tree-source");
        fs::create_dir_all(&working_tree).unwrap();
        fs::write(working_tree.join("README.md"), "mutable working tree\n").unwrap();

        materialize_fixture_projection_copy(
            &view,
            copy_request(),
            store.tree(),
            store.content_blobs(),
            &root,
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(root.join("README.md")).unwrap(),
            "# Fixture Basic App\n\nUses User.email for login.\n"
        );
    }

    #[test]
    fn filesystem_copy_rejects_conflicted_view_before_creating_root() {
        let store = InMemoryArtifactStore::fixture_basic_app();
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
        let root = temp_projection_root("copy-conflicted-rejected");

        let error = materialize_fixture_projection_copy(
            &view,
            copy_request(),
            store.tree(),
            store.content_blobs(),
            &root,
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
        assert!(!root.exists());
    }

    #[test]
    fn filesystem_copy_rejects_stale_view_before_creating_root() {
        let store = InMemoryArtifactStore::fixture_basic_app();
        let dependent = fixture_profile_revision_missing_auth_dependency();
        let required = fixture_auth_revision();
        let view = resolve_fixture_view(
            fixture_resolver_input(vec![selection(&dependent.topic_id, &dependent.revision_id)]),
            fixture_base_entries(),
            vec![dependent, required],
        );
        let root = temp_projection_root("copy-stale-rejected");

        let error = materialize_fixture_projection_copy(
            &view,
            copy_request(),
            store.tree(),
            store.content_blobs(),
            &root,
        )
        .unwrap_err();

        assert_eq!(
            error.code,
            ProjectionMaterializationErrorCode::ProjectionValidationFailed
        );
        assert_eq!(
            error.validation_error.unwrap().code,
            ProjectionValidationErrorCode::StaleView
        );
        assert!(!root.exists());
    }

    #[test]
    fn filesystem_copy_rejects_mismatched_content_tree_before_creating_root() {
        let store = InMemoryArtifactStore::fixture_basic_app();
        let view = view_for_store(&store);
        let mut content_tree = store.tree().clone();
        content_tree.tree_hash = "tree_fixture_wrong_0001".to_string();
        let root = temp_projection_root("copy-tree-mismatch-rejected");

        let error = materialize_fixture_projection_copy(
            &view,
            copy_request(),
            &content_tree,
            store.content_blobs(),
            &root,
        )
        .unwrap_err();

        assert_eq!(
            error.code,
            ProjectionMaterializationErrorCode::ContentTreeMismatch
        );
        assert!(!root.exists());
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

    fn view_for_store(store: &InMemoryArtifactStore) -> ResolvedViewResult {
        let tree_identity = SingleRepoTree {
            repository_id: store.tree().repository_id.clone(),
            tree_hash: store.tree().tree_hash.clone(),
        };
        let tree_entries = store
            .tree()
            .entries
            .iter()
            .filter(|entry| !entry.tombstone)
            .map(|entry| {
                (
                    entry.path.clone(),
                    TreeEntryState {
                        artifact_id: entry.artifact_id.clone(),
                        path: entry.path.clone(),
                        content_hash: entry.content_ref.clone(),
                    },
                )
            })
            .collect();

        ResolvedViewResult {
            resolved_view_id: FIXTURE_BASE_RESOLVED_VIEW_ID.to_string(),
            repository_id: store.tree().repository_id.clone(),
            base_checkpoint_ids: vec![FIXTURE_BASE_CHECKPOINT_ID.to_string()],
            topic_frontier: BTreeMap::new(),
            dependency_closure: DependencyClosure {
                revision_ids: Vec::new(),
            },
            operation_semantics_version: FILE_OPERATION_SEMANTICS_VERSION.to_string(),
            path_policy_id: store.tree().path_policy_id.clone(),
            resolver_order: DeterministicResolverOrder {
                operation_ids: Vec::new(),
            },
            tree_identity: Some(tree_identity),
            records: Vec::new(),
            tree_entries,
        }
    }

    fn copy_request() -> ProjectionMaterializationRequest {
        ProjectionMaterializationRequest {
            purpose: ProjectionPurpose::Execution,
            projection_id: FIXTURE_EXECUTION_PROJECTION_ID.to_string(),
            session_generation_id: None,
            strategy_preference: vec![ProjectionStrategy::Copy],
            fallback_to_copy: true,
            capabilities: ProjectionMaterializationCapabilities::copy_only(),
        }
    }

    fn temp_projection_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "sunlight-projection-{label}-{}-{unique}",
            std::process::id()
        ))
    }
}

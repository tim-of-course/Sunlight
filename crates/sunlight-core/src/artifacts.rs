use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

pub const FIXTURE_REPOSITORY_ID: &str = "repo_fixture_basic_app";
pub const FIXTURE_SESSION_ID: &str = "session_agent_a";
pub const FIXTURE_RESOLVED_VIEW_ID: &str = "view_base_0001";
pub const FIXTURE_SESSION_GENERATION_ID: &str = "gen_agent_a_0001";
pub const FIXTURE_TREE_HASH: &str = "tree_fixture_base_0001";
pub const POSIX_CASE_SENSITIVE_PATH_POLICY_ID: &str = "path_policy_posix_case_sensitive_v1";
pub const FIXTURE_WRITE_TOPIC_ID: &str = "topic_auth_nullability";
pub const FIXTURE_ACTOR_ID: &str = "agent_a";
pub const FILE_OPERATION_SEMANTICS_VERSION: &str = "file_ops_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    File,
    Directory,
    Symlink,
}

impl ArtifactKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathBindingState {
    Active,
    Tombstone,
}

impl PathBindingState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Tombstone => "tombstone",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathBinding {
    pub path: String,
    pub state: PathBindingState,
    pub introduced_by_operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactMetadata {
    pub executable: bool,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactRecord {
    pub id: String,
    pub repository_id: String,
    pub kind: ArtifactKind,
    pub path_bindings: Vec<PathBinding>,
    pub current_content_ref: String,
    pub metadata: ArtifactMetadata,
    pub classification: String,
    pub created_by_operation_id: String,
    pub privacy_class: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentBlob {
    pub id: String,
    pub repository_id: String,
    pub digest: String,
    pub bytes: Vec<u8>,
    pub media_type: String,
    pub classification: String,
    pub storage_ref: String,
    pub privacy_class: String,
    pub created_at: String,
}

impl ContentBlob {
    pub fn byte_length(&self) -> usize {
        self.bytes.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentTree {
    pub id: String,
    pub repository_id: String,
    pub tree_hash: String,
    pub path_policy_id: String,
    pub entries: Vec<TreeEntry>,
    pub privacy_class: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub path: String,
    pub artifact_id: String,
    pub content_ref: String,
    pub kind: ArtifactKind,
    pub executable: bool,
    pub tombstone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeIdentityView {
    pub kind: String,
    pub repository_id: String,
    pub tree_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionVisibleArtifactView {
    pub artifact_id: String,
    pub path: String,
    pub kind: ArtifactKind,
    pub content_hash: String,
    pub byte_length: usize,
    pub classification: String,
    pub executable: bool,
    pub tombstone: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionView {
    pub resolved_view_id: String,
    pub session_generation_id: String,
    pub tree_identity: TreeIdentityView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadResponse {
    pub command: &'static str,
    pub repository_id: String,
    pub session_id: String,
    pub view: SessionView,
    pub artifact: SessionVisibleArtifactView,
    pub content: ContentView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentView {
    pub encoding: String,
    pub bytes: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListResponse {
    pub command: &'static str,
    pub repository_id: String,
    pub session_id: String,
    pub view: SessionView,
    pub artifacts: Vec<SessionVisibleArtifactView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResponse {
    pub command: &'static str,
    pub repository_id: String,
    pub session_id: String,
    pub view: SessionView,
    pub matches: Vec<SearchMatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchMatch {
    pub artifact_id: String,
    pub path: String,
    pub content_hash: String,
    pub line: usize,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationPreconditions {
    pub resolved_view_id: String,
    pub session_generation_id: String,
    pub write_topic_id: String,
    pub parent_topic_revision_id: Option<String>,
    pub path_policy_id: String,
    pub operation_semantics_version: String,
    pub expected_path: String,
    pub expected_hash: ExpectedHash,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpectedHash {
    Existing(String),
    New,
}

impl ExpectedHash {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Existing(hash) => hash,
            Self::New => "new",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchRequest {
    pub session_id: String,
    pub path: String,
    pub expected_hash: String,
    pub patch: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteRequest {
    pub session_id: String,
    pub path: String,
    pub expected_hash: ExpectedHash,
    pub content: Vec<u8>,
    pub classification: String,
    pub executable: bool,
    pub media_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveRequest {
    pub session_id: String,
    pub source_path: String,
    pub target_path: String,
    pub expected_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteRequest {
    pub session_id: String,
    pub path: String,
    pub expected_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataSetRequest {
    pub session_id: String,
    pub path: String,
    pub expected_hash: String,
    pub classification: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationKind {
    Patch,
    Write,
    Move,
    Delete,
    MetadataSet,
}

impl MutationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Patch => "patch",
            Self::Write => "write",
            Self::Move => "move",
            Self::Delete => "delete",
            Self::MetadataSet => "metadata_set",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WriteMode {
    Create,
    Replace,
}

impl WriteMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Replace => "replace",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MutationPayload {
    Patch {
        format: String,
        patch_digest: String,
        base_content_hash: String,
        result_content_hash: String,
        hunk_count: usize,
        byte_delta: isize,
    },
    Write {
        write_mode: WriteMode,
        content_hash: String,
        byte_length: usize,
        media_type: String,
        executable: bool,
        classification: String,
    },
    Move {
        source_path: String,
        target_path: String,
        artifact_id: String,
        content_hash: String,
        source_path_state: String,
        target_path_state: String,
    },
    Delete {
        path: String,
        artifact_id: String,
        content_hash: String,
        path_state: String,
    },
    MetadataSet {
        path: String,
        artifact_id: String,
        content_hash: String,
        classification_before: String,
        classification_after: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationArtifactRef {
    pub artifact_id: Option<String>,
    pub path: String,
    pub path_state: String,
    pub content_hash: Option<String>,
    pub executable: Option<bool>,
    pub classification: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationRefs {
    pub artifacts: Vec<MutationArtifactRef>,
    pub tree_identity: TreeIdentityView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteSetEntry {
    pub artifact_id: String,
    pub path: String,
    pub mutation: MutationKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationTransactionRecord {
    pub id: String,
    pub repository_id: String,
    pub topic_id: String,
    pub session_id: String,
    pub session_generation_id: String,
    pub actor_id: String,
    pub authored_context_id: String,
    pub preconditions: MutationPreconditions,
    pub read_set: String,
    pub write_set: Vec<WriteSetEntry>,
    pub mutation_payload: MutationPayload,
    pub before_refs: MutationRefs,
    pub after_refs: MutationRefs,
    pub classification: String,
    pub parent_topic_revision_id: Option<String>,
    pub next_topic_revision_number: u64,
    pub parents: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicRevisionRecord {
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
pub struct SessionGenerationMutationRecord {
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
pub struct MutationResponse {
    pub command: &'static str,
    pub repository_id: String,
    pub session_id: String,
    pub view: SessionView,
    pub artifact: MutationArtifactView,
    pub operation: OperationTransactionRecord,
    pub topic_revision: TopicRevisionRecord,
    pub session_generation: SessionGenerationMutationRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationArtifactView {
    pub artifact_id: String,
    pub path: String,
    pub kind: ArtifactKind,
    pub before_hash: Option<String>,
    pub after_hash: String,
    pub classification: String,
    pub executable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathPolicy {
    pub id: String,
}

impl PathPolicy {
    pub fn posix_case_sensitive() -> Self {
        Self {
            id: POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string(),
        }
    }

    pub fn validate(&self, path: &str) -> Result<String, ArtifactIoError> {
        validate_repo_path(path, &self.id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathPolicyViolationReason {
    AbsolutePath,
    EscapesRepository,
    NonNormalizedPath,
    PlatformInvalidSeparator,
    InvalidCharacter,
    ReservedPath,
}

impl PathPolicyViolationReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AbsolutePath => "absolute_path",
            Self::EscapesRepository => "escapes_repository",
            Self::NonNormalizedPath => "non_normalized_path",
            Self::PlatformInvalidSeparator => "platform_invalid_separator",
            Self::InvalidCharacter => "invalid_character",
            Self::ReservedPath => "reserved_path",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtifactIoError {
    PathPolicyViolation {
        path: String,
        policy_id: String,
        reason: PathPolicyViolationReason,
        session_generation_id: String,
    },
    PathNotFound {
        path: String,
        session_generation_id: String,
    },
    SessionNotFound {
        session_id: String,
    },
    MissingContent {
        content_ref: String,
    },
    NonUtf8Content {
        path: String,
    },
    PreconditionFailed {
        failed_precondition: String,
        path: String,
        artifact_id: Option<String>,
        expected: String,
        actual: Option<String>,
        session_generation_id: String,
        resolved_view_id: String,
    },
    PatchApplyFailed {
        path: String,
        artifact_id: String,
        content_hash: String,
        failed_hunk: usize,
        session_generation_id: String,
        resolved_view_id: String,
    },
}

impl ArtifactIoError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::PathPolicyViolation { .. } => "path_policy_violation",
            Self::PathNotFound { .. } => "path_not_found",
            Self::SessionNotFound { .. } => "session_not_found",
            Self::MissingContent { .. } => "missing_content",
            Self::NonUtf8Content { .. } => "invalid_content_encoding",
            Self::PreconditionFailed { .. } => "precondition_failed",
            Self::PatchApplyFailed { .. } => "patch_apply_failed",
        }
    }
}

impl Display for ArtifactIoError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PathPolicyViolation {
                path,
                policy_id,
                reason,
                ..
            } => write!(
                f,
                "path `{path}` is rejected by repository path policy `{policy_id}`: {}",
                reason.as_str()
            ),
            Self::PathNotFound { path, .. } => write!(f, "path `{path}` was not found"),
            Self::SessionNotFound { session_id } => {
                write!(f, "session `{session_id}` was not found")
            }
            Self::MissingContent { content_ref } => {
                write!(f, "content blob `{content_ref}` was not found")
            }
            Self::NonUtf8Content { path } => write!(f, "path `{path}` is not UTF-8 text"),
            Self::PreconditionFailed {
                failed_precondition,
                ..
            } => write!(f, "mutation precondition failed: {failed_precondition}"),
            Self::PatchApplyFailed { path, .. } => {
                write!(f, "patch did not apply to expected content at `{path}`")
            }
        }
    }
}

impl Error for ArtifactIoError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InMemoryArtifactStore {
    repository_id: String,
    session_id: String,
    view: SessionView,
    path_policy: PathPolicy,
    write_topic_id: String,
    actor_id: String,
    parent_topic_revision_id: Option<String>,
    generation_number: u64,
    revision_number: u64,
    artifacts: BTreeMap<String, ArtifactRecord>,
    blobs: BTreeMap<String, ContentBlob>,
    tree: ContentTree,
    operations: Vec<OperationTransactionRecord>,
    topic_revisions: Vec<TopicRevisionRecord>,
    session_generations: Vec<SessionGenerationMutationRecord>,
}

impl InMemoryArtifactStore {
    pub fn fixture_basic_app() -> Self {
        let repository_id = FIXTURE_REPOSITORY_ID.to_string();
        let session_id = FIXTURE_SESSION_ID.to_string();
        let path_policy = PathPolicy::posix_case_sensitive();
        let view = SessionView {
            resolved_view_id: FIXTURE_RESOLVED_VIEW_ID.to_string(),
            session_generation_id: FIXTURE_SESSION_GENERATION_ID.to_string(),
            tree_identity: TreeIdentityView {
                kind: "SingleRepoTree".to_string(),
                repository_id: repository_id.clone(),
                tree_hash: FIXTURE_TREE_HASH.to_string(),
            },
        };

        let fixtures = [
            FixtureFile {
                path: "README.md",
                artifact_id: "artifact_readme_md",
                blob_id: "blob_readme_base",
                digest: "sha256:readme_base",
                bytes: b"# Fixture Basic App\n\nUses User.email for login.\n",
                media_type: "text/markdown; charset=utf-8",
                language: "markdown",
                executable: false,
            },
            FixtureFile {
                path: "docs/guide.md",
                artifact_id: "artifact_docs_guide_md",
                blob_id: "blob_guide_base",
                digest: "sha256:guide_base",
                bytes: b"Search token: User.email\n",
                media_type: "text/markdown; charset=utf-8",
                language: "markdown",
                executable: false,
            },
            FixtureFile {
                path: "scripts/build.sh",
                artifact_id: "artifact_scripts_build_sh",
                blob_id: "blob_build_base",
                digest: "sha256:build_base",
                bytes: b"#!/usr/bin/env sh\necho build\n",
                media_type: "text/x-shellscript; charset=utf-8",
                language: "shell",
                executable: true,
            },
            FixtureFile {
                path: "src/auth.ts",
                artifact_id: "artifact_src_auth_ts",
                blob_id: "blob_auth_base",
                digest: "sha256:auth_base",
                bytes: b"export function login(email: string) {\n  return email.trim().toLowerCase();\n}\n",
                media_type: "text/typescript; charset=utf-8",
                language: "typescript",
                executable: false,
            },
            FixtureFile {
                path: "src/profile.ts",
                artifact_id: "artifact_src_profile_ts",
                blob_id: "blob_profile_base",
                digest: "sha256:profile_base",
                bytes: b"export const profileLabel = \"User.email\";\n",
                media_type: "text/typescript; charset=utf-8",
                language: "typescript",
                executable: false,
            },
        ];

        let mut artifacts = BTreeMap::new();
        let mut blobs = BTreeMap::new();
        let mut entries = Vec::new();

        for fixture in fixtures {
            artifacts.insert(
                fixture.artifact_id.to_string(),
                ArtifactRecord {
                    id: fixture.artifact_id.to_string(),
                    repository_id: repository_id.clone(),
                    kind: ArtifactKind::File,
                    path_bindings: vec![PathBinding {
                        path: fixture.path.to_string(),
                        state: PathBindingState::Active,
                        introduced_by_operation_id: "op_import_base_0001".to_string(),
                    }],
                    current_content_ref: fixture.digest.to_string(),
                    metadata: ArtifactMetadata {
                        executable: fixture.executable,
                        language: Some(fixture.language.to_string()),
                    },
                    classification: "source".to_string(),
                    created_by_operation_id: "op_import_base_0001".to_string(),
                    privacy_class: "commit_default".to_string(),
                    created_at: "2026-07-03T00:00:00Z".to_string(),
                },
            );
            blobs.insert(
                fixture.digest.to_string(),
                ContentBlob {
                    id: fixture.blob_id.to_string(),
                    repository_id: repository_id.clone(),
                    digest: fixture.digest.to_string(),
                    bytes: fixture.bytes.to_vec(),
                    media_type: fixture.media_type.to_string(),
                    classification: "source".to_string(),
                    storage_ref: fixture.storage_ref(),
                    privacy_class: "policy_gated".to_string(),
                    created_at: "2026-07-03T00:00:00Z".to_string(),
                },
            );
            entries.push(TreeEntry {
                path: fixture.path.to_string(),
                artifact_id: fixture.artifact_id.to_string(),
                content_ref: fixture.digest.to_string(),
                kind: ArtifactKind::File,
                executable: fixture.executable,
                tombstone: false,
            });
        }

        entries.sort_by(|left, right| left.path.cmp(&right.path));

        Self {
            repository_id: repository_id.clone(),
            session_id,
            view,
            path_policy: path_policy.clone(),
            write_topic_id: FIXTURE_WRITE_TOPIC_ID.to_string(),
            actor_id: FIXTURE_ACTOR_ID.to_string(),
            parent_topic_revision_id: None,
            generation_number: 1,
            revision_number: 0,
            artifacts,
            blobs,
            tree: ContentTree {
                id: FIXTURE_TREE_HASH.to_string(),
                repository_id,
                tree_hash: FIXTURE_TREE_HASH.to_string(),
                path_policy_id: path_policy.id,
                entries,
                privacy_class: "policy_gated".to_string(),
                created_at: "2026-07-03T00:00:00Z".to_string(),
            },
            operations: Vec::new(),
            topic_revisions: Vec::new(),
            session_generations: Vec::new(),
        }
    }

    pub fn read(&self, session_id: &str, path: &str) -> Result<ReadResponse, ArtifactIoError> {
        self.ensure_session(session_id)?;
        let path = self.path_policy.validate(path)?;
        let entry = self.entry_for_path(&path)?;
        let artifact = self.artifact_view(entry)?;
        let blob = self.blob_for_entry(entry)?;
        let bytes = std::str::from_utf8(&blob.bytes)
            .map_err(|_| ArtifactIoError::NonUtf8Content { path: path.clone() })?;

        Ok(ReadResponse {
            command: "artifact.read",
            repository_id: self.repository_id.clone(),
            session_id: session_id.to_string(),
            view: self.view.clone(),
            artifact,
            content: ContentView {
                encoding: "utf-8".to_string(),
                bytes: bytes.to_string(),
            },
        })
    }

    pub fn list(&self, session_id: &str, prefix: &str) -> Result<ListResponse, ArtifactIoError> {
        self.ensure_session(session_id)?;
        let prefix = if prefix.is_empty() {
            String::new()
        } else {
            self.path_policy.validate(prefix)?
        };

        let artifacts = self
            .tree
            .entries
            .iter()
            .filter(|entry| is_under_prefix(&entry.path, &prefix))
            .map(|entry| self.artifact_view(entry))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(ListResponse {
            command: "artifact.list",
            repository_id: self.repository_id.clone(),
            session_id: session_id.to_string(),
            view: self.view.clone(),
            artifacts,
        })
    }

    pub fn search(&self, session_id: &str, query: &str) -> Result<SearchResponse, ArtifactIoError> {
        self.ensure_session(session_id)?;
        let mut matches = Vec::new();

        if !query.is_empty() {
            for entry in &self.tree.entries {
                if entry.tombstone {
                    continue;
                }
                let blob = self.blob_for_entry(entry)?;
                let text = std::str::from_utf8(&blob.bytes).map_err(|_| {
                    ArtifactIoError::NonUtf8Content {
                        path: entry.path.clone(),
                    }
                })?;
                for (line_index, line) in text.lines().enumerate() {
                    if line.contains(query) {
                        matches.push(SearchMatch {
                            artifact_id: entry.artifact_id.clone(),
                            path: entry.path.clone(),
                            content_hash: entry.content_ref.clone(),
                            line: line_index + 1,
                            snippet: line.to_string(),
                        });
                    }
                }
            }
        }

        Ok(SearchResponse {
            command: "artifact.search",
            repository_id: self.repository_id.clone(),
            session_id: session_id.to_string(),
            view: self.view.clone(),
            matches,
        })
    }

    pub fn patch(&mut self, request: PatchRequest) -> Result<MutationResponse, ArtifactIoError> {
        self.ensure_session(&request.session_id)?;
        let path = self.path_policy.validate(&request.path)?;
        let entry = self.entry_for_path(&path)?.clone();
        let before_blob = self.blob_for_entry(&entry)?.clone();

        if entry.content_ref != request.expected_hash {
            return Err(ArtifactIoError::PreconditionFailed {
                failed_precondition: "expected_hash".to_string(),
                path,
                artifact_id: Some(entry.artifact_id),
                expected: request.expected_hash,
                actual: Some(entry.content_ref),
                session_generation_id: self.view.session_generation_id.clone(),
                resolved_view_id: self.view.resolved_view_id.clone(),
            });
        }

        let before_text = std::str::from_utf8(&before_blob.bytes)
            .map_err(|_| ArtifactIoError::NonUtf8Content { path: path.clone() })?;
        let (after_text, hunk_count) =
            apply_fixture_patch(before_text, &request.patch).map_err(|failed_hunk| {
                ArtifactIoError::PatchApplyFailed {
                    path: path.clone(),
                    artifact_id: entry.artifact_id.clone(),
                    content_hash: entry.content_ref.clone(),
                    failed_hunk,
                    session_generation_id: self.view.session_generation_id.clone(),
                    resolved_view_id: self.view.resolved_view_id.clone(),
                }
            })?;
        let after_bytes = after_text.into_bytes();
        let after_hash = fixture_content_hash(&path, &after_bytes, self.revision_number + 1);
        let patch_digest = fixture_patch_digest(&request.patch);
        let byte_delta = after_bytes.len() as isize - before_blob.bytes.len() as isize;

        self.accept_mutation(AcceptMutation {
            session_id: request.session_id,
            path,
            artifact_id: entry.artifact_id,
            before_hash: Some(entry.content_ref),
            after_hash: after_hash.clone(),
            after_bytes,
            media_type: before_blob.media_type,
            classification: before_blob.classification,
            executable: entry.executable,
            kind: MutationKind::Patch,
            payload: MutationPayload::Patch {
                format: "unified_diff".to_string(),
                patch_digest,
                base_content_hash: before_blob.digest,
                result_content_hash: after_hash.clone(),
                hunk_count,
                byte_delta,
            },
            expected_hash: ExpectedHash::Existing(request.expected_hash),
        })
    }

    pub fn write(&mut self, request: WriteRequest) -> Result<MutationResponse, ArtifactIoError> {
        self.ensure_session(&request.session_id)?;
        let path = self.path_policy.validate(&request.path)?;
        let existing_entry = self
            .tree
            .entries
            .iter()
            .find(|entry| entry.path == path && !entry.tombstone)
            .cloned();

        let (artifact_id, before_hash, write_mode) = match (&request.expected_hash, existing_entry)
        {
            (ExpectedHash::New, Some(entry)) => {
                return Err(ArtifactIoError::PreconditionFailed {
                    failed_precondition: "expected_hash".to_string(),
                    path,
                    artifact_id: Some(entry.artifact_id),
                    expected: "new".to_string(),
                    actual: Some(entry.content_ref),
                    session_generation_id: self.view.session_generation_id.clone(),
                    resolved_view_id: self.view.resolved_view_id.clone(),
                });
            }
            (ExpectedHash::New, None) => {
                (fixture_artifact_id_for_path(&path), None, WriteMode::Create)
            }
            (ExpectedHash::Existing(expected), Some(entry)) if entry.content_ref == *expected => (
                entry.artifact_id,
                Some(expected.clone()),
                WriteMode::Replace,
            ),
            (ExpectedHash::Existing(expected), Some(entry)) => {
                return Err(ArtifactIoError::PreconditionFailed {
                    failed_precondition: "expected_hash".to_string(),
                    path,
                    artifact_id: Some(entry.artifact_id),
                    expected: expected.clone(),
                    actual: Some(entry.content_ref),
                    session_generation_id: self.view.session_generation_id.clone(),
                    resolved_view_id: self.view.resolved_view_id.clone(),
                });
            }
            (ExpectedHash::Existing(expected), None) => {
                return Err(ArtifactIoError::PreconditionFailed {
                    failed_precondition: "expected_hash".to_string(),
                    path,
                    artifact_id: None,
                    expected: expected.clone(),
                    actual: None,
                    session_generation_id: self.view.session_generation_id.clone(),
                    resolved_view_id: self.view.resolved_view_id.clone(),
                });
            }
        };

        let after_hash = fixture_content_hash(&path, &request.content, self.revision_number + 1);
        self.accept_mutation(AcceptMutation {
            session_id: request.session_id,
            path,
            artifact_id,
            before_hash,
            after_hash: after_hash.clone(),
            after_bytes: request.content.clone(),
            media_type: request.media_type.clone(),
            classification: request.classification.clone(),
            executable: request.executable,
            kind: MutationKind::Write,
            payload: MutationPayload::Write {
                write_mode,
                content_hash: after_hash,
                byte_length: request.content.len(),
                media_type: request.media_type,
                executable: request.executable,
                classification: request.classification,
            },
            expected_hash: request.expected_hash,
        })
    }

    pub fn move_path(&mut self, request: MoveRequest) -> Result<MutationResponse, ArtifactIoError> {
        self.ensure_session(&request.session_id)?;
        let source_path = self.path_policy.validate(&request.source_path)?;
        let target_path = self.path_policy.validate(&request.target_path)?;
        let entry = self.entry_for_path(&source_path)?.clone();
        if self
            .tree
            .entries
            .iter()
            .any(|entry| entry.path == target_path && !entry.tombstone)
        {
            return Err(ArtifactIoError::PreconditionFailed {
                failed_precondition: "expected_target_absent".to_string(),
                path: target_path,
                artifact_id: Some(entry.artifact_id),
                expected: "absent".to_string(),
                actual: Some("active".to_string()),
                session_generation_id: self.view.session_generation_id.clone(),
                resolved_view_id: self.view.resolved_view_id.clone(),
            });
        }
        if entry.content_ref != request.expected_hash {
            return Err(ArtifactIoError::PreconditionFailed {
                failed_precondition: "expected_hash".to_string(),
                path: source_path,
                artifact_id: Some(entry.artifact_id),
                expected: request.expected_hash,
                actual: Some(entry.content_ref),
                session_generation_id: self.view.session_generation_id.clone(),
                resolved_view_id: self.view.resolved_view_id.clone(),
            });
        }

        self.accept_structural_mutation(StructuralMutation {
            session_id: request.session_id,
            kind: MutationKind::Move,
            path: target_path.clone(),
            artifact_id: entry.artifact_id.clone(),
            before_hash: Some(entry.content_ref.clone()),
            after_hash: entry.content_ref.clone(),
            classification: self.artifact_classification(&entry.artifact_id)?,
            executable: entry.executable,
            expected_path: source_path.clone(),
            expected_hash: ExpectedHash::Existing(request.expected_hash),
            payload: MutationPayload::Move {
                source_path: source_path.clone(),
                target_path: target_path.clone(),
                artifact_id: entry.artifact_id.clone(),
                content_hash: entry.content_ref.clone(),
                source_path_state: "tombstone".to_string(),
                target_path_state: "active".to_string(),
            },
            before_refs: vec![MutationArtifactRef {
                artifact_id: Some(entry.artifact_id.clone()),
                path: source_path.clone(),
                path_state: "active".to_string(),
                content_hash: Some(entry.content_ref.clone()),
                executable: Some(entry.executable),
                classification: Some(self.artifact_classification(&entry.artifact_id)?),
            }],
            after_refs: vec![
                MutationArtifactRef {
                    artifact_id: Some(entry.artifact_id.clone()),
                    path: source_path.clone(),
                    path_state: "tombstone".to_string(),
                    content_hash: Some(entry.content_ref.clone()),
                    executable: Some(entry.executable),
                    classification: Some(self.artifact_classification(&entry.artifact_id)?),
                },
                MutationArtifactRef {
                    artifact_id: Some(entry.artifact_id.clone()),
                    path: target_path.clone(),
                    path_state: "active".to_string(),
                    content_hash: Some(entry.content_ref.clone()),
                    executable: Some(entry.executable),
                    classification: Some(self.artifact_classification(&entry.artifact_id)?),
                },
            ],
        })
    }

    pub fn delete_path(
        &mut self,
        request: DeleteRequest,
    ) -> Result<MutationResponse, ArtifactIoError> {
        self.ensure_session(&request.session_id)?;
        let path = self.path_policy.validate(&request.path)?;
        let entry = self.entry_for_path(&path)?.clone();
        if entry.content_ref != request.expected_hash {
            return Err(ArtifactIoError::PreconditionFailed {
                failed_precondition: "expected_hash".to_string(),
                path,
                artifact_id: Some(entry.artifact_id),
                expected: request.expected_hash,
                actual: Some(entry.content_ref),
                session_generation_id: self.view.session_generation_id.clone(),
                resolved_view_id: self.view.resolved_view_id.clone(),
            });
        }
        let classification = self.artifact_classification(&entry.artifact_id)?;

        self.accept_structural_mutation(StructuralMutation {
            session_id: request.session_id,
            kind: MutationKind::Delete,
            path: path.clone(),
            artifact_id: entry.artifact_id.clone(),
            before_hash: Some(entry.content_ref.clone()),
            after_hash: entry.content_ref.clone(),
            classification: classification.clone(),
            executable: entry.executable,
            expected_path: path.clone(),
            expected_hash: ExpectedHash::Existing(request.expected_hash),
            payload: MutationPayload::Delete {
                path: path.clone(),
                artifact_id: entry.artifact_id.clone(),
                content_hash: entry.content_ref.clone(),
                path_state: "tombstone".to_string(),
            },
            before_refs: vec![MutationArtifactRef {
                artifact_id: Some(entry.artifact_id.clone()),
                path: path.clone(),
                path_state: "active".to_string(),
                content_hash: Some(entry.content_ref.clone()),
                executable: Some(entry.executable),
                classification: Some(classification.clone()),
            }],
            after_refs: vec![MutationArtifactRef {
                artifact_id: Some(entry.artifact_id.clone()),
                path: path.clone(),
                path_state: "tombstone".to_string(),
                content_hash: Some(entry.content_ref.clone()),
                executable: Some(entry.executable),
                classification: Some(classification),
            }],
        })
    }

    pub fn metadata_set(
        &mut self,
        request: MetadataSetRequest,
    ) -> Result<MutationResponse, ArtifactIoError> {
        self.ensure_session(&request.session_id)?;
        let path = self.path_policy.validate(&request.path)?;
        let entry = self.entry_for_path(&path)?.clone();
        if entry.content_ref != request.expected_hash {
            return Err(ArtifactIoError::PreconditionFailed {
                failed_precondition: "expected_hash".to_string(),
                path,
                artifact_id: Some(entry.artifact_id),
                expected: request.expected_hash,
                actual: Some(entry.content_ref),
                session_generation_id: self.view.session_generation_id.clone(),
                resolved_view_id: self.view.resolved_view_id.clone(),
            });
        }
        let classification_before = self.artifact_classification(&entry.artifact_id)?;

        self.accept_structural_mutation(StructuralMutation {
            session_id: request.session_id,
            kind: MutationKind::MetadataSet,
            path: path.clone(),
            artifact_id: entry.artifact_id.clone(),
            before_hash: Some(entry.content_ref.clone()),
            after_hash: entry.content_ref.clone(),
            classification: request.classification.clone(),
            executable: entry.executable,
            expected_path: path.clone(),
            expected_hash: ExpectedHash::Existing(request.expected_hash),
            payload: MutationPayload::MetadataSet {
                path: path.clone(),
                artifact_id: entry.artifact_id.clone(),
                content_hash: entry.content_ref.clone(),
                classification_before: classification_before.clone(),
                classification_after: request.classification.clone(),
            },
            before_refs: vec![MutationArtifactRef {
                artifact_id: Some(entry.artifact_id.clone()),
                path: path.clone(),
                path_state: "active".to_string(),
                content_hash: Some(entry.content_ref.clone()),
                executable: Some(entry.executable),
                classification: Some(classification_before),
            }],
            after_refs: vec![MutationArtifactRef {
                artifact_id: Some(entry.artifact_id.clone()),
                path: path.clone(),
                path_state: "active".to_string(),
                content_hash: Some(entry.content_ref.clone()),
                executable: Some(entry.executable),
                classification: Some(request.classification),
            }],
        })
    }

    pub fn operations(&self) -> &[OperationTransactionRecord] {
        &self.operations
    }

    pub fn topic_revisions(&self) -> &[TopicRevisionRecord] {
        &self.topic_revisions
    }

    pub fn session_generations(&self) -> &[SessionGenerationMutationRecord] {
        &self.session_generations
    }

    pub fn tree(&self) -> &ContentTree {
        &self.tree
    }

    pub fn content_blob(&self, content_ref: &str) -> Option<&ContentBlob> {
        self.blobs.get(content_ref)
    }

    pub fn content_blobs(&self) -> &BTreeMap<String, ContentBlob> {
        &self.blobs
    }

    fn accept_mutation(
        &mut self,
        mutation: AcceptMutation,
    ) -> Result<MutationResponse, ArtifactIoError> {
        let prior_view = self.view.clone();
        let prior_tree_identity = self.view.tree_identity.clone();
        let next_revision_number = self.revision_number + 1;
        let parent_revision_id = self.parent_topic_revision_id.clone();
        let tree_hash = fixture_tree_hash(&mutation.kind, next_revision_number);
        let resolved_view_id = fixture_resolved_view_id(&mutation.kind, next_revision_number);
        let session_generation_id = format!("gen_agent_a_{:04}", self.generation_number + 1);
        let operation_id =
            fixture_operation_id(&mutation.kind, &mutation.path, next_revision_number);
        let topic_revision_id = format!("rev_auth_nullability_{next_revision_number:04}");

        let after_tree_identity = TreeIdentityView {
            kind: "SingleRepoTree".to_string(),
            repository_id: self.repository_id.clone(),
            tree_hash: tree_hash.clone(),
        };
        let before_ref = MutationArtifactRef {
            artifact_id: mutation
                .before_hash
                .as_ref()
                .map(|_| mutation.artifact_id.clone()),
            path: mutation.path.clone(),
            path_state: if mutation.before_hash.is_some() {
                "active".to_string()
            } else {
                "absent".to_string()
            },
            content_hash: mutation.before_hash.clone(),
            executable: mutation.before_hash.as_ref().map(|_| mutation.executable),
            classification: mutation
                .before_hash
                .as_ref()
                .map(|_| mutation.classification.clone()),
        };
        let after_ref = MutationArtifactRef {
            artifact_id: Some(mutation.artifact_id.clone()),
            path: mutation.path.clone(),
            path_state: "active".to_string(),
            content_hash: Some(mutation.after_hash.clone()),
            executable: Some(mutation.executable),
            classification: Some(mutation.classification.clone()),
        };

        let preconditions = MutationPreconditions {
            resolved_view_id: prior_view.resolved_view_id.clone(),
            session_generation_id: prior_view.session_generation_id.clone(),
            write_topic_id: self.write_topic_id.clone(),
            parent_topic_revision_id: parent_revision_id.clone(),
            path_policy_id: self.path_policy.id.clone(),
            operation_semantics_version: FILE_OPERATION_SEMANTICS_VERSION.to_string(),
            expected_path: mutation.path.clone(),
            expected_hash: mutation.expected_hash.clone(),
        };
        let write_set = vec![WriteSetEntry {
            artifact_id: mutation.artifact_id.clone(),
            path: mutation.path.clone(),
            mutation: mutation.kind.clone(),
        }];
        let operation = OperationTransactionRecord {
            id: operation_id.clone(),
            repository_id: self.repository_id.clone(),
            topic_id: self.write_topic_id.clone(),
            session_id: mutation.session_id.clone(),
            session_generation_id: prior_view.session_generation_id.clone(),
            actor_id: self.actor_id.clone(),
            authored_context_id: format!("ctx_agent_a_gen_{:04}", self.generation_number),
            preconditions,
            read_set: "full_authored_context".to_string(),
            write_set: write_set.clone(),
            mutation_payload: mutation.payload,
            before_refs: MutationRefs {
                artifacts: vec![before_ref],
                tree_identity: prior_tree_identity,
            },
            after_refs: MutationRefs {
                artifacts: vec![after_ref],
                tree_identity: after_tree_identity.clone(),
            },
            classification: mutation.classification.clone(),
            parent_topic_revision_id: parent_revision_id.clone(),
            next_topic_revision_number: next_revision_number,
            parents: parent_revision_id.iter().cloned().collect(),
        };
        let topic_revision = TopicRevisionRecord {
            id: topic_revision_id.clone(),
            repository_id: self.repository_id.clone(),
            topic_id: self.write_topic_id.clone(),
            revision_number: next_revision_number,
            parent_revision_id,
            operation_transaction_id: operation_id.clone(),
            tree_delta_ref: format!("delta_mutation_{next_revision_number:04}"),
            dependency_revision_ids: Vec::new(),
        };
        let session_generation = SessionGenerationMutationRecord {
            id: session_generation_id.clone(),
            repository_id: self.repository_id.clone(),
            session_id: mutation.session_id.clone(),
            write_topic_id: self.write_topic_id.clone(),
            base_resolved_view_id: FIXTURE_RESOLVED_VIEW_ID.to_string(),
            resolved_view_id: resolved_view_id.clone(),
            topic_frontier: BTreeMap::from([(
                self.write_topic_id.clone(),
                topic_revision_id.clone(),
            )]),
            generation_number: self.generation_number + 1,
            refresh_policy: "pinned_except_own_topic".to_string(),
            created_by_operation_id: operation_id,
        };

        self.blobs.insert(
            mutation.after_hash.clone(),
            ContentBlob {
                id: fixture_blob_id(&mutation.after_hash),
                repository_id: self.repository_id.clone(),
                digest: mutation.after_hash.clone(),
                bytes: mutation.after_bytes.clone(),
                media_type: mutation.media_type,
                classification: mutation.classification.clone(),
                storage_ref: storage_ref_for_digest(&mutation.after_hash),
                privacy_class: "policy_gated".to_string(),
                created_at: "2026-07-03T00:00:00Z".to_string(),
            },
        );
        self.artifacts
            .entry(mutation.artifact_id.clone())
            .and_modify(|artifact| {
                artifact.current_content_ref = mutation.after_hash.clone();
                artifact.classification = mutation.classification.clone();
                artifact.metadata.executable = mutation.executable;
            })
            .or_insert_with(|| ArtifactRecord {
                id: mutation.artifact_id.clone(),
                repository_id: self.repository_id.clone(),
                kind: ArtifactKind::File,
                path_bindings: vec![PathBinding {
                    path: mutation.path.clone(),
                    state: PathBindingState::Active,
                    introduced_by_operation_id: operation.id.clone(),
                }],
                current_content_ref: mutation.after_hash.clone(),
                metadata: ArtifactMetadata {
                    executable: mutation.executable,
                    language: language_for_path(&mutation.path).map(str::to_string),
                },
                classification: mutation.classification.clone(),
                created_by_operation_id: operation.id.clone(),
                privacy_class: "commit_default".to_string(),
                created_at: "2026-07-03T00:00:00Z".to_string(),
            });
        match self
            .tree
            .entries
            .iter_mut()
            .find(|entry| entry.path == mutation.path && !entry.tombstone)
        {
            Some(entry) => {
                entry.content_ref = mutation.after_hash.clone();
                entry.executable = mutation.executable;
            }
            None => self.tree.entries.push(TreeEntry {
                path: mutation.path.clone(),
                artifact_id: mutation.artifact_id.clone(),
                content_ref: mutation.after_hash.clone(),
                kind: ArtifactKind::File,
                executable: mutation.executable,
                tombstone: false,
            }),
        }
        self.tree
            .entries
            .sort_by(|left, right| left.path.cmp(&right.path));
        self.tree.tree_hash = tree_hash.clone();
        self.tree.id = tree_hash;
        self.view = SessionView {
            resolved_view_id,
            session_generation_id,
            tree_identity: after_tree_identity,
        };
        self.generation_number += 1;
        self.revision_number = next_revision_number;
        self.parent_topic_revision_id = Some(topic_revision_id);
        self.operations.push(operation.clone());
        self.topic_revisions.push(topic_revision.clone());
        self.session_generations.push(session_generation.clone());

        Ok(MutationResponse {
            command: match mutation.kind {
                MutationKind::Patch => "artifact.patch",
                MutationKind::Write => "artifact.write",
                MutationKind::Move => "artifact.move",
                MutationKind::Delete => "artifact.delete",
                MutationKind::MetadataSet => "artifact.metadata_set",
            },
            repository_id: self.repository_id.clone(),
            session_id: mutation.session_id,
            view: self.view.clone(),
            artifact: MutationArtifactView {
                artifact_id: mutation.artifact_id,
                path: mutation.path,
                kind: ArtifactKind::File,
                before_hash: mutation.before_hash,
                after_hash: mutation.after_hash,
                classification: mutation.classification,
                executable: mutation.executable,
            },
            operation,
            topic_revision,
            session_generation,
        })
    }

    fn accept_structural_mutation(
        &mut self,
        mutation: StructuralMutation,
    ) -> Result<MutationResponse, ArtifactIoError> {
        let prior_view = self.view.clone();
        let prior_tree_identity = self.view.tree_identity.clone();
        let next_revision_number = self.revision_number + 1;
        let parent_revision_id = self.parent_topic_revision_id.clone();
        let tree_hash = fixture_tree_hash(&mutation.kind, next_revision_number);
        let resolved_view_id = fixture_resolved_view_id(&mutation.kind, next_revision_number);
        let session_generation_id = format!("gen_agent_a_{:04}", self.generation_number + 1);
        let operation_id =
            fixture_operation_id(&mutation.kind, &mutation.path, next_revision_number);
        let topic_revision_id = format!("rev_auth_nullability_{next_revision_number:04}");
        let after_tree_identity = TreeIdentityView {
            kind: "SingleRepoTree".to_string(),
            repository_id: self.repository_id.clone(),
            tree_hash: tree_hash.clone(),
        };
        let preconditions = MutationPreconditions {
            resolved_view_id: prior_view.resolved_view_id.clone(),
            session_generation_id: prior_view.session_generation_id.clone(),
            write_topic_id: self.write_topic_id.clone(),
            parent_topic_revision_id: parent_revision_id.clone(),
            path_policy_id: self.path_policy.id.clone(),
            operation_semantics_version: FILE_OPERATION_SEMANTICS_VERSION.to_string(),
            expected_path: mutation.expected_path.clone(),
            expected_hash: mutation.expected_hash.clone(),
        };
        let write_set = vec![WriteSetEntry {
            artifact_id: mutation.artifact_id.clone(),
            path: mutation.path.clone(),
            mutation: mutation.kind.clone(),
        }];
        let operation = OperationTransactionRecord {
            id: operation_id.clone(),
            repository_id: self.repository_id.clone(),
            topic_id: self.write_topic_id.clone(),
            session_id: mutation.session_id.clone(),
            session_generation_id: prior_view.session_generation_id.clone(),
            actor_id: self.actor_id.clone(),
            authored_context_id: format!("ctx_agent_a_gen_{:04}", self.generation_number),
            preconditions,
            read_set: "full_authored_context".to_string(),
            write_set: write_set.clone(),
            mutation_payload: mutation.payload.clone(),
            before_refs: MutationRefs {
                artifacts: mutation.before_refs.clone(),
                tree_identity: prior_tree_identity,
            },
            after_refs: MutationRefs {
                artifacts: mutation.after_refs.clone(),
                tree_identity: after_tree_identity.clone(),
            },
            classification: mutation.classification.clone(),
            parent_topic_revision_id: parent_revision_id.clone(),
            next_topic_revision_number: next_revision_number,
            parents: parent_revision_id.iter().cloned().collect(),
        };
        let topic_revision = TopicRevisionRecord {
            id: topic_revision_id.clone(),
            repository_id: self.repository_id.clone(),
            topic_id: self.write_topic_id.clone(),
            revision_number: next_revision_number,
            parent_revision_id,
            operation_transaction_id: operation_id.clone(),
            tree_delta_ref: format!("delta_mutation_{next_revision_number:04}"),
            dependency_revision_ids: Vec::new(),
        };
        let session_generation = SessionGenerationMutationRecord {
            id: session_generation_id.clone(),
            repository_id: self.repository_id.clone(),
            session_id: mutation.session_id.clone(),
            write_topic_id: self.write_topic_id.clone(),
            base_resolved_view_id: FIXTURE_RESOLVED_VIEW_ID.to_string(),
            resolved_view_id: resolved_view_id.clone(),
            topic_frontier: BTreeMap::from([(
                self.write_topic_id.clone(),
                topic_revision_id.clone(),
            )]),
            generation_number: self.generation_number + 1,
            refresh_policy: "pinned_except_own_topic".to_string(),
            created_by_operation_id: operation_id,
        };

        match &mutation.payload {
            MutationPayload::Move {
                source_path,
                target_path,
                ..
            } => {
                if let Some(entry) = self
                    .tree
                    .entries
                    .iter_mut()
                    .find(|entry| entry.path == *source_path && !entry.tombstone)
                {
                    entry.tombstone = true;
                }
                self.tree.entries.push(TreeEntry {
                    path: target_path.clone(),
                    artifact_id: mutation.artifact_id.clone(),
                    content_ref: mutation.after_hash.clone(),
                    kind: ArtifactKind::File,
                    executable: mutation.executable,
                    tombstone: false,
                });
                if let Some(artifact) = self.artifacts.get_mut(&mutation.artifact_id) {
                    artifact.path_bindings.push(PathBinding {
                        path: source_path.clone(),
                        state: PathBindingState::Tombstone,
                        introduced_by_operation_id: operation.id.clone(),
                    });
                    artifact.path_bindings.push(PathBinding {
                        path: target_path.clone(),
                        state: PathBindingState::Active,
                        introduced_by_operation_id: operation.id.clone(),
                    });
                }
            }
            MutationPayload::Delete { path, .. } => {
                if let Some(entry) = self
                    .tree
                    .entries
                    .iter_mut()
                    .find(|entry| entry.path == *path && !entry.tombstone)
                {
                    entry.tombstone = true;
                }
                if let Some(artifact) = self.artifacts.get_mut(&mutation.artifact_id) {
                    artifact.path_bindings.push(PathBinding {
                        path: path.clone(),
                        state: PathBindingState::Tombstone,
                        introduced_by_operation_id: operation.id.clone(),
                    });
                }
            }
            MutationPayload::MetadataSet { .. } => {
                if let Some(artifact) = self.artifacts.get_mut(&mutation.artifact_id) {
                    artifact.classification = mutation.classification.clone();
                }
                if let Some(blob) = self.blobs.get_mut(&mutation.after_hash) {
                    blob.classification = mutation.classification.clone();
                }
            }
            MutationPayload::Patch { .. } | MutationPayload::Write { .. } => {}
        }

        self.tree
            .entries
            .sort_by(|left, right| left.path.cmp(&right.path));
        self.tree.tree_hash = tree_hash.clone();
        self.tree.id = tree_hash;
        self.view = SessionView {
            resolved_view_id,
            session_generation_id,
            tree_identity: after_tree_identity,
        };
        self.generation_number += 1;
        self.revision_number = next_revision_number;
        self.parent_topic_revision_id = Some(topic_revision_id);
        self.operations.push(operation.clone());
        self.topic_revisions.push(topic_revision.clone());
        self.session_generations.push(session_generation.clone());

        Ok(MutationResponse {
            command: match mutation.kind {
                MutationKind::Patch => "artifact.patch",
                MutationKind::Write => "artifact.write",
                MutationKind::Move => "artifact.move",
                MutationKind::Delete => "artifact.delete",
                MutationKind::MetadataSet => "artifact.metadata_set",
            },
            repository_id: self.repository_id.clone(),
            session_id: mutation.session_id,
            view: self.view.clone(),
            artifact: MutationArtifactView {
                artifact_id: mutation.artifact_id,
                path: mutation.path,
                kind: ArtifactKind::File,
                before_hash: mutation.before_hash,
                after_hash: mutation.after_hash,
                classification: mutation.classification,
                executable: mutation.executable,
            },
            operation,
            topic_revision,
            session_generation,
        })
    }

    fn artifact_classification(&self, artifact_id: &str) -> Result<String, ArtifactIoError> {
        self.artifacts
            .get(artifact_id)
            .map(|artifact| artifact.classification.clone())
            .ok_or_else(|| ArtifactIoError::PathNotFound {
                path: artifact_id.to_string(),
                session_generation_id: self.view.session_generation_id.clone(),
            })
    }

    fn ensure_session(&self, session_id: &str) -> Result<(), ArtifactIoError> {
        if session_id == self.session_id {
            Ok(())
        } else {
            Err(ArtifactIoError::SessionNotFound {
                session_id: session_id.to_string(),
            })
        }
    }

    fn entry_for_path(&self, path: &str) -> Result<&TreeEntry, ArtifactIoError> {
        self.tree
            .entries
            .iter()
            .find(|entry| entry.path == path && !entry.tombstone)
            .ok_or_else(|| ArtifactIoError::PathNotFound {
                path: path.to_string(),
                session_generation_id: self.view.session_generation_id.clone(),
            })
    }

    fn artifact_view(
        &self,
        entry: &TreeEntry,
    ) -> Result<SessionVisibleArtifactView, ArtifactIoError> {
        let artifact = self.artifacts.get(&entry.artifact_id).ok_or_else(|| {
            ArtifactIoError::PathNotFound {
                path: entry.path.clone(),
                session_generation_id: self.view.session_generation_id.clone(),
            }
        })?;
        let blob = self.blob_for_entry(entry)?;

        Ok(SessionVisibleArtifactView {
            artifact_id: entry.artifact_id.clone(),
            path: entry.path.clone(),
            kind: entry.kind,
            content_hash: entry.content_ref.clone(),
            byte_length: blob.byte_length(),
            classification: artifact.classification.clone(),
            executable: entry.executable,
            tombstone: entry.tombstone,
        })
    }

    fn blob_for_entry(&self, entry: &TreeEntry) -> Result<&ContentBlob, ArtifactIoError> {
        self.blobs
            .get(&entry.content_ref)
            .ok_or_else(|| ArtifactIoError::MissingContent {
                content_ref: entry.content_ref.clone(),
            })
    }
}

fn validate_repo_path(path: &str, policy_id: &str) -> Result<String, ArtifactIoError> {
    let violation = |reason| ArtifactIoError::PathPolicyViolation {
        path: path.to_string(),
        policy_id: policy_id.to_string(),
        reason,
        session_generation_id: FIXTURE_SESSION_GENERATION_ID.to_string(),
    };

    if path.starts_with('/') || path.as_bytes().get(1) == Some(&b':') {
        return Err(violation(PathPolicyViolationReason::AbsolutePath));
    }
    if path.contains('\\') {
        return Err(violation(
            PathPolicyViolationReason::PlatformInvalidSeparator,
        ));
    }
    if path.as_bytes().contains(&0) {
        return Err(violation(PathPolicyViolationReason::InvalidCharacter));
    }
    if path == ".." || path.starts_with("../") {
        return Err(violation(PathPolicyViolationReason::EscapesRepository));
    }
    if path.is_empty() || path == "." || path.starts_with("./") || path.ends_with('/') {
        return Err(violation(PathPolicyViolationReason::NonNormalizedPath));
    }

    let parts = path.split('/').collect::<Vec<_>>();
    if parts
        .iter()
        .any(|part| part.is_empty() || *part == "." || *part == "..")
    {
        return Err(violation(PathPolicyViolationReason::NonNormalizedPath));
    }
    if matches!(parts.first(), Some(&".git" | &".sunlight")) {
        return Err(violation(PathPolicyViolationReason::ReservedPath));
    }

    Ok(path.to_string())
}

fn is_under_prefix(path: &str, prefix: &str) -> bool {
    prefix.is_empty()
        || path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

struct AcceptMutation {
    session_id: String,
    path: String,
    artifact_id: String,
    before_hash: Option<String>,
    after_hash: String,
    after_bytes: Vec<u8>,
    media_type: String,
    classification: String,
    executable: bool,
    kind: MutationKind,
    payload: MutationPayload,
    expected_hash: ExpectedHash,
}

struct StructuralMutation {
    session_id: String,
    kind: MutationKind,
    path: String,
    artifact_id: String,
    before_hash: Option<String>,
    after_hash: String,
    classification: String,
    executable: bool,
    expected_path: String,
    expected_hash: ExpectedHash,
    payload: MutationPayload,
    before_refs: Vec<MutationArtifactRef>,
    after_refs: Vec<MutationArtifactRef>,
}

#[derive(Clone, Copy)]
struct FixtureFile {
    path: &'static str,
    artifact_id: &'static str,
    blob_id: &'static str,
    digest: &'static str,
    bytes: &'static [u8],
    media_type: &'static str,
    language: &'static str,
    executable: bool,
}

impl FixtureFile {
    fn storage_ref(self) -> String {
        let digest = self.digest.strip_prefix("sha256:").unwrap_or(self.digest);
        format!("objects/blobs/sha256/{digest}")
    }
}

fn apply_fixture_patch(before: &str, patch: &str) -> Result<(String, usize), usize> {
    let mut removed = Vec::new();
    let mut added = Vec::new();
    let mut hunk_count = 0;

    for line in patch.lines() {
        if line.starts_with("@@") {
            hunk_count += 1;
            continue;
        }
        if line.starts_with("---") || line.starts_with("+++") || line.starts_with("diff ") {
            continue;
        }
        if let Some(rest) = line.strip_prefix('-') {
            removed.push(format!("{rest}\n"));
        } else if let Some(rest) = line.strip_prefix('+') {
            added.push(format!("{rest}\n"));
        }
    }

    if hunk_count == 0 || removed.is_empty() {
        return Err(1);
    }

    let before_block = removed.concat();
    let after_block = added.concat();
    before
        .find(&before_block)
        .map(|start| {
            let mut output =
                String::with_capacity(before.len() - before_block.len() + after_block.len());
            output.push_str(&before[..start]);
            output.push_str(&after_block);
            output.push_str(&before[start + before_block.len()..]);
            (output, hunk_count)
        })
        .ok_or(1)
}

fn fixture_content_hash(path: &str, bytes: &[u8], revision_number: u64) -> String {
    if path == "src/auth.ts"
        && bytes
            == b"export function login(email: string) {\n  const normalized = email.trim().toLowerCase();\n  return normalized;\n}\n"
    {
        "sha256:auth_trim_guard".to_string()
    } else if path == "src/session.ts" {
        "sha256:session_new".to_string()
    } else {
        format!(
            "sha256:{}_mutation_{revision_number:04}",
            path.replace(['/', '.'], "_")
        )
    }
}

fn fixture_patch_digest(patch: &str) -> String {
    if patch.contains("const normalized = email.trim().toLowerCase();") {
        "sha256:auth_trim_guard_patch".to_string()
    } else {
        format!("sha256:fixture_patch_{}", patch.len())
    }
}

fn fixture_tree_hash(kind: &MutationKind, revision_number: u64) -> String {
    match (kind, revision_number) {
        (MutationKind::Patch, 1) => "tree_after_auth_patch_0001".to_string(),
        (MutationKind::Move, 1) => "tree_after_auth_move_0001".to_string(),
        (MutationKind::Delete, 1) => "tree_after_auth_delete_0001".to_string(),
        (MutationKind::MetadataSet, 1) => "tree_after_auth_metadata_0001".to_string(),
        (MutationKind::Write, 2) => "tree_after_session_write_0002".to_string(),
        (MutationKind::Write, 1) => "tree_after_session_write_0001".to_string(),
        _ => format!("tree_after_mutation_{revision_number:04}"),
    }
}

fn fixture_resolved_view_id(kind: &MutationKind, revision_number: u64) -> String {
    match (kind, revision_number) {
        (MutationKind::Patch, 1) => "view_agent_a_after_patch_0001".to_string(),
        (MutationKind::Move, 1) => "view_agent_a_after_move_0001".to_string(),
        (MutationKind::Delete, 1) => "view_agent_a_after_delete_0001".to_string(),
        (MutationKind::MetadataSet, 1) => "view_agent_a_after_metadata_0001".to_string(),
        (MutationKind::Write, 2) => "view_agent_a_after_write_0002".to_string(),
        (MutationKind::Write, 1) => "view_agent_a_after_write_0001".to_string(),
        _ => format!("view_agent_a_after_mutation_{revision_number:04}"),
    }
}

fn fixture_operation_id(kind: &MutationKind, path: &str, revision_number: u64) -> String {
    match (kind, path, revision_number) {
        (MutationKind::Patch, "src/auth.ts", 1) => "op_auth_trim_guard_0001".to_string(),
        (MutationKind::Move, "src/auth.renamed.ts", 1) => "op_auth_move_0001".to_string(),
        (MutationKind::Delete, "src/auth.ts", 1) => "op_auth_delete_0001".to_string(),
        (MutationKind::MetadataSet, "src/auth.ts", 1) => "op_auth_metadata_0001".to_string(),
        (MutationKind::Write, "src/session.ts", 2) => "op_write_session_ts_0001".to_string(),
        (MutationKind::Write, "src/session.ts", 1) => "op_write_session_ts_0001".to_string(),
        _ => format!("op_mutation_{revision_number:04}"),
    }
}

fn fixture_artifact_id_for_path(path: &str) -> String {
    match path {
        "src/session.ts" => "artifact_src_session_ts".to_string(),
        _ => format!("artifact_{}", path.replace(['/', '.'], "_")),
    }
}

fn fixture_blob_id(content_hash: &str) -> String {
    format!(
        "blob_{}",
        content_hash
            .strip_prefix("sha256:")
            .unwrap_or(content_hash)
            .replace('-', "_")
    )
}

fn storage_ref_for_digest(content_hash: &str) -> String {
    let digest = content_hash.strip_prefix("sha256:").unwrap_or(content_hash);
    format!("objects/blobs/sha256/{digest}")
}

fn language_for_path(path: &str) -> Option<&'static str> {
    if path.ends_with(".ts") {
        Some("typescript")
    } else if path.ends_with(".md") {
        Some("markdown")
    } else if path.ends_with(".sh") {
        Some("shell")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_fixture_file_with_artifact_metadata_and_inline_text() {
        let store = InMemoryArtifactStore::fixture_basic_app();

        let response = store.read(FIXTURE_SESSION_ID, "src/auth.ts").unwrap();

        assert_eq!(response.command, "artifact.read");
        assert_eq!(response.repository_id, FIXTURE_REPOSITORY_ID);
        assert_eq!(response.view.resolved_view_id, FIXTURE_RESOLVED_VIEW_ID);
        assert_eq!(
            response.view.session_generation_id,
            FIXTURE_SESSION_GENERATION_ID
        );
        assert_eq!(response.view.tree_identity.kind, "SingleRepoTree");
        assert_eq!(response.artifact.artifact_id, "artifact_src_auth_ts");
        assert_eq!(response.artifact.path, "src/auth.ts");
        assert_eq!(response.artifact.kind, ArtifactKind::File);
        assert_eq!(response.artifact.content_hash, "sha256:auth_base");
        assert_eq!(response.artifact.byte_length, 78);
        assert_eq!(response.artifact.classification, "source");
        assert!(!response.artifact.executable);
        assert_eq!(
            response.content.bytes,
            "export function login(email: string) {\n  return email.trim().toLowerCase();\n}\n"
        );
    }

    #[test]
    fn lists_path_prefix_in_stable_order() {
        let store = InMemoryArtifactStore::fixture_basic_app();

        let response = store.list(FIXTURE_SESSION_ID, "src").unwrap();

        let paths = response
            .artifacts
            .iter()
            .map(|artifact| artifact.path.as_str())
            .collect::<Vec<_>>();
        assert_eq!(paths, vec!["src/auth.ts", "src/profile.ts"]);
        assert_eq!(response.artifacts[0].artifact_id, "artifact_src_auth_ts");
        assert_eq!(response.artifacts[1].artifact_id, "artifact_src_profile_ts");
        assert_eq!(response.artifacts[1].content_hash, "sha256:profile_base");
        assert_eq!(response.artifacts[1].byte_length, 42);
        assert!(response.artifacts.iter().all(|entry| !entry.tombstone));
    }

    #[test]
    fn searches_literal_text_by_path_then_line_order() {
        let store = InMemoryArtifactStore::fixture_basic_app();

        let response = store.search(FIXTURE_SESSION_ID, "User.email").unwrap();

        let matches = response
            .matches
            .iter()
            .map(|item| {
                (
                    item.path.as_str(),
                    item.artifact_id.as_str(),
                    item.content_hash.as_str(),
                    item.line,
                    item.snippet.as_str(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            matches,
            vec![
                (
                    "README.md",
                    "artifact_readme_md",
                    "sha256:readme_base",
                    3,
                    "Uses User.email for login."
                ),
                (
                    "docs/guide.md",
                    "artifact_docs_guide_md",
                    "sha256:guide_base",
                    1,
                    "Search token: User.email"
                ),
                (
                    "src/profile.ts",
                    "artifact_src_profile_ts",
                    "sha256:profile_base",
                    1,
                    "export const profileLabel = \"User.email\";"
                ),
            ]
        );
    }

    #[test]
    fn path_policy_failures_report_fixture_reasons() {
        let store = InMemoryArtifactStore::fixture_basic_app();

        for (path, reason) in [
            ("../README.md", PathPolicyViolationReason::EscapesRepository),
            ("/tmp/README.md", PathPolicyViolationReason::AbsolutePath),
            ("src//auth.ts", PathPolicyViolationReason::NonNormalizedPath),
            (
                "./src/auth.ts",
                PathPolicyViolationReason::NonNormalizedPath,
            ),
            (
                "src/../README.md",
                PathPolicyViolationReason::NonNormalizedPath,
            ),
            (
                "src\\auth.ts",
                PathPolicyViolationReason::PlatformInvalidSeparator,
            ),
            (
                "src/auth.ts\0.md",
                PathPolicyViolationReason::InvalidCharacter,
            ),
            (".git/config", PathPolicyViolationReason::ReservedPath),
            (
                ".sunlight/config.toml",
                PathPolicyViolationReason::ReservedPath,
            ),
        ] {
            let error = store.read(FIXTURE_SESSION_ID, path).unwrap_err();
            assert_eq!(error.code(), "path_policy_violation");
            assert_eq!(
                error,
                ArtifactIoError::PathPolicyViolation {
                    path: path.to_string(),
                    policy_id: POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string(),
                    reason,
                    session_generation_id: FIXTURE_SESSION_GENERATION_ID.to_string(),
                }
            );
        }
    }

    #[test]
    fn missing_path_is_not_a_policy_violation() {
        let store = InMemoryArtifactStore::fixture_basic_app();

        let error = store
            .read(FIXTURE_SESSION_ID, "src/missing.ts")
            .unwrap_err();

        assert_eq!(error.code(), "path_not_found");
        assert_eq!(
            error,
            ArtifactIoError::PathNotFound {
                path: "src/missing.ts".to_string(),
                session_generation_id: FIXTURE_SESSION_GENERATION_ID.to_string(),
            }
        );
    }

    #[test]
    fn successful_patch_records_operation_shape_and_advances_session() {
        let mut store = InMemoryArtifactStore::fixture_basic_app();

        let response = store
            .patch(PatchRequest {
                session_id: FIXTURE_SESSION_ID.to_string(),
                path: "src/auth.ts".to_string(),
                expected_hash: "sha256:auth_base".to_string(),
                patch: auth_trim_guard_patch(),
            })
            .unwrap();

        assert_eq!(response.command, "artifact.patch");
        assert_eq!(response.artifact.artifact_id, "artifact_src_auth_ts");
        assert_eq!(response.operation.id, "op_auth_trim_guard_0001".to_string());
        assert_eq!(
            response.topic_revision.id,
            "rev_auth_nullability_0001".to_string()
        );
        assert_eq!(response.view.session_generation_id, "gen_agent_a_0002");
        assert_eq!(
            response.view.resolved_view_id,
            "view_agent_a_after_patch_0001"
        );
        assert_eq!(
            response.view.tree_identity.tree_hash,
            "tree_after_auth_patch_0001"
        );
        assert_eq!(
            response.operation.session_generation_id,
            FIXTURE_SESSION_GENERATION_ID
        );
        assert_eq!(
            response.operation.preconditions.expected_hash,
            ExpectedHash::Existing("sha256:auth_base".to_string())
        );
        assert_eq!(
            response.operation.before_refs.artifacts[0].content_hash,
            Some("sha256:auth_base".to_string())
        );
        assert_eq!(
            response.operation.after_refs.artifacts[0].content_hash,
            Some("sha256:auth_trim_guard".to_string())
        );
        assert_eq!(
            response.operation.write_set[0].mutation,
            MutationKind::Patch
        );
        assert_eq!(store.operations().len(), 1);
        assert_eq!(store.topic_revisions().len(), 1);
        assert_eq!(store.session_generations().len(), 1);

        let read = store.read(FIXTURE_SESSION_ID, "src/auth.ts").unwrap();
        assert_eq!(read.view.session_generation_id, "gen_agent_a_0002");
        assert_eq!(read.artifact.content_hash, "sha256:auth_trim_guard");
        assert!(read.content.bytes.contains("const normalized = email"));
    }

    #[test]
    fn successful_new_file_write_records_create_shape_and_is_readable() {
        let mut store = InMemoryArtifactStore::fixture_basic_app();

        let response = store
            .write(WriteRequest {
                session_id: FIXTURE_SESSION_ID.to_string(),
                path: "src/session.ts".to_string(),
                expected_hash: ExpectedHash::New,
                content: session_file_bytes(),
                classification: "source".to_string(),
                executable: false,
                media_type: "text/typescript; charset=utf-8".to_string(),
            })
            .unwrap();

        assert_eq!(response.command, "artifact.write");
        assert_eq!(response.artifact.artifact_id, "artifact_src_session_ts");
        assert_eq!(response.artifact.before_hash, None);
        assert_eq!(response.artifact.after_hash, "sha256:session_new");
        assert_eq!(response.view.session_generation_id, "gen_agent_a_0002");
        assert_eq!(
            response.operation.before_refs.artifacts[0].path_state,
            "absent"
        );
        assert_eq!(
            response.operation.after_refs.artifacts[0].content_hash,
            Some("sha256:session_new".to_string())
        );
        assert_eq!(
            response.operation.write_set[0].mutation,
            MutationKind::Write
        );
        assert!(matches!(
            response.operation.mutation_payload,
            MutationPayload::Write {
                write_mode: WriteMode::Create,
                ..
            }
        ));

        let read = store.read(FIXTURE_SESSION_ID, "src/session.ts").unwrap();
        assert_eq!(read.view.session_generation_id, "gen_agent_a_0002");
        assert_eq!(read.artifact.content_hash, "sha256:session_new");
        assert!(read.content.bytes.contains("SessionStore"));
    }

    #[test]
    fn stale_hash_precondition_failure_creates_no_records() {
        let mut store = InMemoryArtifactStore::fixture_basic_app();

        let error = store
            .patch(PatchRequest {
                session_id: FIXTURE_SESSION_ID.to_string(),
                path: "src/auth.ts".to_string(),
                expected_hash: "sha256:stale".to_string(),
                patch: auth_trim_guard_patch(),
            })
            .unwrap_err();

        assert_eq!(error.code(), "precondition_failed");
        assert_eq!(
            error,
            ArtifactIoError::PreconditionFailed {
                failed_precondition: "expected_hash".to_string(),
                path: "src/auth.ts".to_string(),
                artifact_id: Some("artifact_src_auth_ts".to_string()),
                expected: "sha256:stale".to_string(),
                actual: Some("sha256:auth_base".to_string()),
                session_generation_id: "gen_agent_a_0001".to_string(),
                resolved_view_id: "view_base_0001".to_string(),
            }
        );
        assert!(store.operations().is_empty());
        assert!(store.topic_revisions().is_empty());
        assert!(store.session_generations().is_empty());
        assert_eq!(
            store
                .read(FIXTURE_SESSION_ID, "src/auth.ts")
                .unwrap()
                .view
                .session_generation_id,
            "gen_agent_a_0001"
        );
    }

    #[test]
    fn write_existing_with_new_precondition_fails_without_records() {
        let mut store = InMemoryArtifactStore::fixture_basic_app();

        let error = store
            .write(WriteRequest {
                session_id: FIXTURE_SESSION_ID.to_string(),
                path: "src/auth.ts".to_string(),
                expected_hash: ExpectedHash::New,
                content: session_file_bytes(),
                classification: "source".to_string(),
                executable: false,
                media_type: "text/typescript; charset=utf-8".to_string(),
            })
            .unwrap_err();

        assert_eq!(error.code(), "precondition_failed");
        assert_eq!(
            error,
            ArtifactIoError::PreconditionFailed {
                failed_precondition: "expected_hash".to_string(),
                path: "src/auth.ts".to_string(),
                artifact_id: Some("artifact_src_auth_ts".to_string()),
                expected: "new".to_string(),
                actual: Some("sha256:auth_base".to_string()),
                session_generation_id: "gen_agent_a_0001".to_string(),
                resolved_view_id: "view_base_0001".to_string(),
            }
        );
        assert!(store.operations().is_empty());
        assert!(store.topic_revisions().is_empty());
        assert!(store.session_generations().is_empty());
    }

    #[test]
    fn failed_patch_apply_leaves_generation_unchanged() {
        let mut store = InMemoryArtifactStore::fixture_basic_app();

        let error = store
            .patch(PatchRequest {
                session_id: FIXTURE_SESSION_ID.to_string(),
                path: "src/auth.ts".to_string(),
                expected_hash: "sha256:auth_base".to_string(),
                patch: bad_auth_patch(),
            })
            .unwrap_err();

        assert_eq!(error.code(), "patch_apply_failed");
        assert_eq!(
            error,
            ArtifactIoError::PatchApplyFailed {
                path: "src/auth.ts".to_string(),
                artifact_id: "artifact_src_auth_ts".to_string(),
                content_hash: "sha256:auth_base".to_string(),
                failed_hunk: 1,
                session_generation_id: "gen_agent_a_0001".to_string(),
                resolved_view_id: "view_base_0001".to_string(),
            }
        );
        assert!(store.operations().is_empty());
        assert_eq!(
            store
                .read(FIXTURE_SESSION_ID, "src/auth.ts")
                .unwrap()
                .view
                .session_generation_id,
            "gen_agent_a_0001"
        );
    }

    fn auth_trim_guard_patch() -> String {
        "--- a/src/auth.ts\n+++ b/src/auth.ts\n@@\n-  return email.trim().toLowerCase();\n+  const normalized = email.trim().toLowerCase();\n+  return normalized;\n"
            .to_string()
    }

    fn bad_auth_patch() -> String {
        "--- a/src/auth.ts\n+++ b/src/auth.ts\n@@\n-  return missing.trim();\n+  return email.trim();\n"
            .to_string()
    }

    fn session_file_bytes() -> Vec<u8> {
        b"export class SessionStore {\n  readonly active = true;\n}\n".to_vec()
    }
}

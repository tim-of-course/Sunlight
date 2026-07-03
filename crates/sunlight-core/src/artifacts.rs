use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

pub const FIXTURE_REPOSITORY_ID: &str = "repo_fixture_basic_app";
pub const FIXTURE_SESSION_ID: &str = "session_agent_a";
pub const FIXTURE_RESOLVED_VIEW_ID: &str = "view_base_0001";
pub const FIXTURE_SESSION_GENERATION_ID: &str = "gen_agent_a_0001";
pub const FIXTURE_TREE_HASH: &str = "tree_fixture_base_0001";
pub const POSIX_CASE_SENSITIVE_PATH_POLICY_ID: &str = "path_policy_posix_case_sensitive_v1";

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
}

impl ArtifactIoError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::PathPolicyViolation { .. } => "path_policy_violation",
            Self::PathNotFound { .. } => "path_not_found",
            Self::SessionNotFound { .. } => "session_not_found",
            Self::MissingContent { .. } => "missing_content",
            Self::NonUtf8Content { .. } => "invalid_content_encoding",
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
    artifacts: BTreeMap<String, ArtifactRecord>,
    blobs: BTreeMap<String, ContentBlob>,
    tree: ContentTree,
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

    pub fn tree(&self) -> &ContentTree {
        &self.tree
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
}

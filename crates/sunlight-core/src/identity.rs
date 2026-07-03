use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use sha2::{Digest as ShaDigest, Sha256};

pub const CANONICAL_HASH_SCHEMA_VERSION: u32 = 1;
pub const TREE_IDENTITY_SCHEMA_VERSION: u32 = 1;

const HASH_ALGORITHM: &str = "sha256";
const HASH_BYTES: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdentityParseError {
    EmptyRepoId,
    InvalidRepoId { value: String },
    UnknownObjectKind { value: String },
    InvalidDigestLength { actual: usize },
    InvalidDigestHex { value: String },
    InvalidObjectId { value: String },
}

impl Display for IdentityParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyRepoId => write!(f, "repository id cannot be empty"),
            Self::InvalidRepoId { value } => write!(f, "invalid repository id `{value}`"),
            Self::UnknownObjectKind { value } => write!(f, "unknown object kind `{value}`"),
            Self::InvalidDigestLength { actual } => {
                write!(
                    f,
                    "invalid digest length: expected {} hex characters, got {actual}",
                    HASH_BYTES * 2
                )
            }
            Self::InvalidDigestHex { value } => write!(f, "invalid digest hex `{value}`"),
            Self::InvalidObjectId { value } => write!(f, "invalid object id `{value}`"),
        }
    }
}

impl Error for IdentityParseError {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RepoId(String);

impl RepoId {
    pub fn new(value: impl Into<String>) -> Result<Self, IdentityParseError> {
        let value = value.into();
        if value.is_empty() {
            return Err(IdentityParseError::EmptyRepoId);
        }

        if !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        {
            return Err(IdentityParseError::InvalidRepoId { value });
        }

        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for RepoId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for RepoId {
    type Err = IdentityParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjectKind {
    Blob,
    Tree,
    Artifact,
    Operation,
    Topic,
    TopicRevision,
    Session,
    SessionGeneration,
    View,
    Conflict,
    Execution,
    Checkpoint,
    ExportMap,
    TreeIdentity,
}

impl ObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Blob => "blob",
            Self::Tree => "tree",
            Self::Artifact => "artifact",
            Self::Operation => "operation",
            Self::Topic => "topic",
            Self::TopicRevision => "topic-revision",
            Self::Session => "session",
            Self::SessionGeneration => "session-generation",
            Self::View => "view",
            Self::Conflict => "conflict",
            Self::Execution => "execution",
            Self::Checkpoint => "checkpoint",
            Self::ExportMap => "export-map",
            Self::TreeIdentity => "tree-identity",
        }
    }
}

impl Display for ObjectKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ObjectKind {
    type Err = IdentityParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "blob" => Ok(Self::Blob),
            "tree" => Ok(Self::Tree),
            "artifact" => Ok(Self::Artifact),
            "operation" => Ok(Self::Operation),
            "topic" => Ok(Self::Topic),
            "topic-revision" => Ok(Self::TopicRevision),
            "session" => Ok(Self::Session),
            "session-generation" => Ok(Self::SessionGeneration),
            "view" => Ok(Self::View),
            "conflict" => Ok(Self::Conflict),
            "execution" => Ok(Self::Execution),
            "checkpoint" => Ok(Self::Checkpoint),
            "export-map" => Ok(Self::ExportMap),
            "tree-identity" => Ok(Self::TreeIdentity),
            _ => Err(IdentityParseError::UnknownObjectKind {
                value: value.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectHash([u8; HASH_BYTES]);

impl ObjectHash {
    pub fn from_bytes(bytes: [u8; HASH_BYTES]) -> Self {
        Self(bytes)
    }

    pub fn canonical(kind: ObjectKind, schema_version: u32, canonical_bytes: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"sunlight\0canonical-object\0");
        hasher.update(schema_version.to_be_bytes());
        hasher.update(b"\0");
        hasher.update(kind.as_str().as_bytes());
        hasher.update(b"\0");
        hasher.update((canonical_bytes.len() as u64).to_be_bytes());
        hasher.update(b"\0");
        hasher.update(canonical_bytes);
        Self(hasher.finalize().into())
    }

    pub fn as_bytes(&self) -> &[u8; HASH_BYTES] {
        &self.0
    }

    pub fn to_hex(self) -> String {
        let mut output = String::with_capacity(HASH_BYTES * 2);
        for byte in self.0 {
            output.push_str(&format!("{byte:02x}"));
        }
        output
    }
}

impl Display for ObjectHash {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", HASH_ALGORITHM, self.to_hex())
    }
}

impl FromStr for ObjectHash {
    type Err = IdentityParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let hex = value.strip_prefix("sha256:").unwrap_or(value);
        if hex.len() != HASH_BYTES * 2 {
            return Err(IdentityParseError::InvalidDigestLength { actual: hex.len() });
        }

        let mut bytes = [0_u8; HASH_BYTES];
        for (index, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
            let pair =
                std::str::from_utf8(chunk).map_err(|_| IdentityParseError::InvalidDigestHex {
                    value: value.to_string(),
                })?;
            bytes[index] =
                u8::from_str_radix(pair, 16).map_err(|_| IdentityParseError::InvalidDigestHex {
                    value: value.to_string(),
                })?;
        }

        Ok(Self(bytes))
    }
}

pub type Digest = ObjectHash;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ObjectId {
    repo_id: RepoId,
    kind: ObjectKind,
    hash: ObjectHash,
}

impl ObjectId {
    pub fn new(repo_id: RepoId, kind: ObjectKind, hash: ObjectHash) -> Self {
        Self {
            repo_id,
            kind,
            hash,
        }
    }

    pub fn from_canonical_bytes(
        repo_id: RepoId,
        kind: ObjectKind,
        schema_version: u32,
        canonical_bytes: &[u8],
    ) -> Self {
        Self::new(
            repo_id,
            kind,
            ObjectHash::canonical(kind, schema_version, canonical_bytes),
        )
    }

    pub fn repo_id(&self) -> &RepoId {
        &self.repo_id
    }

    pub fn kind(&self) -> ObjectKind {
        self.kind
    }

    pub fn hash(&self) -> ObjectHash {
        self.hash
    }
}

impl Display for ObjectId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.repo_id, self.kind, self.hash)
    }
}

impl FromStr for ObjectId {
    type Err = IdentityParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = value.split(':').collect();
        if parts.len() != 4 || parts[2] != HASH_ALGORITHM {
            return Err(IdentityParseError::InvalidObjectId {
                value: value.to_string(),
            });
        }

        Ok(Self {
            repo_id: parts[0].parse()?,
            kind: parts[1].parse()?,
            hash: format!("{}:{}", parts[2], parts[3]).parse()?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeIdentity {
    SingleRepoTree {
        repo_id: RepoId,
        tree_hash: ObjectHash,
    },
    RepoTreeMap(BTreeMap<RepoId, ObjectHash>),
}

impl TreeIdentity {
    pub fn single_repo(repo_id: RepoId, tree_hash: ObjectHash) -> Self {
        Self::SingleRepoTree { repo_id, tree_hash }
    }

    pub fn repo_tree_map<I>(entries: I) -> Self
    where
        I: IntoIterator<Item = (RepoId, ObjectHash)>,
    {
        Self::RepoTreeMap(entries.into_iter().collect())
    }

    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();
        match self {
            Self::SingleRepoTree { repo_id, tree_hash } => {
                bytes.extend_from_slice(b"single-repo-tree\0");
                append_field(&mut bytes, repo_id.as_str().as_bytes());
                append_field(&mut bytes, tree_hash.as_bytes());
            }
            Self::RepoTreeMap(entries) => {
                bytes.extend_from_slice(b"repo-tree-map\0");
                bytes.extend_from_slice(&(entries.len() as u64).to_be_bytes());
                bytes.push(0);
                for (repo_id, tree_hash) in entries {
                    append_field(&mut bytes, repo_id.as_str().as_bytes());
                    append_field(&mut bytes, tree_hash.as_bytes());
                }
            }
        }
        bytes
    }

    pub fn hash(&self) -> ObjectHash {
        ObjectHash::canonical(
            ObjectKind::TreeIdentity,
            TREE_IDENTITY_SCHEMA_VERSION,
            &self.canonical_bytes(),
        )
    }
}

impl Display for TreeIdentity {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SingleRepoTree { repo_id, tree_hash } => {
                write!(f, "single-repo:{repo_id}:{tree_hash}")
            }
            Self::RepoTreeMap(entries) => {
                f.write_str("repo-map:")?;
                for (index, (repo_id, tree_hash)) in entries.iter().enumerate() {
                    if index > 0 {
                        f.write_str(",")?;
                    }
                    write!(f, "{repo_id}={tree_hash}")?;
                }
                Ok(())
            }
        }
    }
}

fn append_field(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
    bytes.push(0);
    bytes.extend_from_slice(value);
    bytes.push(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_hashes_are_stable_for_identical_bytes() {
        let first = ObjectHash::canonical(ObjectKind::Artifact, 1, br#"{"path":"src/lib.rs"}"#);
        let second = ObjectHash::canonical(ObjectKind::Artifact, 1, br#"{"path":"src/lib.rs"}"#);

        assert_eq!(first, second);
        assert_eq!(
            first.to_hex(),
            "c6421e0777c1c27ab27b0877b1811e43a0198f24fd0755a21c045a5e28a7678f"
        );
    }

    #[test]
    fn canonical_hashes_change_with_version_kind_or_content() {
        let base = ObjectHash::canonical(ObjectKind::Artifact, 1, b"same logical bytes");

        assert_ne!(
            base,
            ObjectHash::canonical(ObjectKind::Artifact, 2, b"same logical bytes")
        );
        assert_ne!(
            base,
            ObjectHash::canonical(ObjectKind::Blob, 1, b"same logical bytes")
        );
        assert_ne!(
            base,
            ObjectHash::canonical(ObjectKind::Artifact, 1, b"different bytes")
        );
    }

    #[test]
    fn repo_hash_and_object_id_round_trip_as_strings() {
        let repo_id: RepoId = "repo-alpha_1".parse().unwrap();
        let hash = ObjectHash::canonical(ObjectKind::Blob, CANONICAL_HASH_SCHEMA_VERSION, b"hello");
        let object_id = ObjectId::new(repo_id, ObjectKind::Blob, hash);
        let encoded = object_id.to_string();

        assert_eq!(encoded.parse::<ObjectId>().unwrap(), object_id);
        assert_eq!(hash.to_string().parse::<ObjectHash>().unwrap(), hash);
    }

    #[test]
    fn repo_tree_map_identity_is_deterministic_by_repo_id() {
        let repo_a: RepoId = "app".parse().unwrap();
        let repo_b: RepoId = "shared".parse().unwrap();
        let tree_a = ObjectHash::canonical(ObjectKind::Tree, 1, b"tree-a");
        let tree_b = ObjectHash::canonical(ObjectKind::Tree, 1, b"tree-b");

        let first =
            TreeIdentity::repo_tree_map([(repo_b.clone(), tree_b), (repo_a.clone(), tree_a)]);
        let second = TreeIdentity::repo_tree_map([(repo_a, tree_a), (repo_b, tree_b)]);

        assert_eq!(first, second);
        assert_eq!(first.canonical_bytes(), second.canonical_bytes());
        assert_eq!(first.hash(), second.hash());
        assert!(first.to_string().starts_with("repo-map:app=sha256:"));
    }

    #[test]
    fn single_repo_tree_keeps_mvp_shape_distinct_from_map_shape() {
        let repo_id: RepoId = "app".parse().unwrap();
        let tree_hash = ObjectHash::canonical(ObjectKind::Tree, 1, b"tree");

        let single = TreeIdentity::single_repo(repo_id.clone(), tree_hash);
        let map = TreeIdentity::repo_tree_map([(repo_id, tree_hash)]);

        assert_ne!(single.canonical_bytes(), map.canonical_bytes());
        assert_ne!(single.hash(), map.hash());
        assert_eq!(single.to_string(), format!("single-repo:app:{tree_hash}"));
    }
}

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::identity::{ObjectId, RepoId};
use crate::records::{
    canonical_record_id, JsonValue, PrivacyClass, RecordError, RecordKind, RECORD_SCHEMA_VERSION,
};

const TOPIC_ID_SEED: &str = "topic_creation_v1";
const SESSION_ID_SEED: &str = "session_start_v1";
const GENERATION_ID_SEED: &str = "session_generation_v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicSlug(String);

impl TopicSlug {
    pub fn new(value: impl Into<String>) -> Result<Self, TopicSessionError> {
        let value = value.into();
        if !is_valid_topic_slug(&value) {
            return Err(TopicSessionError::InvalidTopicSlug { value });
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for TopicSlug {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicStatus {
    Open,
}

impl TopicStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicVisibility {
    Private,
    Shared,
}

impl TopicVisibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Shared => "shared",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRefreshPolicy {
    PinnedExceptOwnTopic,
}

impl SessionRefreshPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PinnedExceptOwnTopic => "pinned_except_own_topic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SessionCapability {
    Read,
    List,
    Search,
    Inspect,
    Patch,
    Write,
    Move,
    Delete,
    Metadata,
}

impl SessionCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::List => "list",
            Self::Search => "search",
            Self::Inspect => "inspect",
            Self::Patch => "patch",
            Self::Write => "write",
            Self::Move => "move",
            Self::Delete => "delete",
            Self::Metadata => "metadata",
        }
    }
}

pub const PHASE1_SESSION_CAPABILITIES: &[SessionCapability] = &[
    SessionCapability::Read,
    SessionCapability::List,
    SessionCapability::Search,
    SessionCapability::Inspect,
    SessionCapability::Patch,
    SessionCapability::Write,
    SessionCapability::Move,
    SessionCapability::Delete,
    SessionCapability::Metadata,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicBindingMode {
    Write,
    PinnedRead,
}

impl TopicBindingMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::PinnedRead => "pinned_read",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicFrontierEntry {
    pub topic_id: ObjectId,
    pub revision_id: Option<ObjectId>,
    pub mode: TopicBindingMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopicCreateInput {
    pub repository_id: RepoId,
    pub slug: TopicSlug,
    pub display_name: String,
    pub owner_actor_id: String,
    pub base_checkpoint_id: ObjectId,
    pub created_at: String,
    pub visibility: TopicVisibility,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TopicRecord {
    pub id: ObjectId,
    pub record: JsonValue,
    pub slug: TopicSlug,
    pub display_name: String,
    pub owner_actor_id: String,
    pub base_checkpoint_id: ObjectId,
    pub status: TopicStatus,
    pub head_revision_id: Option<ObjectId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStartInput {
    pub repository_id: RepoId,
    pub actor_id: String,
    pub base_resolved_view_id: ObjectId,
    pub resolved_view_id: ObjectId,
    pub topic_frontier: Vec<TopicFrontierEntry>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionRecord {
    pub id: ObjectId,
    pub record: JsonValue,
    pub actor_id: String,
    pub write_topic_id: ObjectId,
    pub pinned_resolved_view_id: ObjectId,
    pub current_generation_id: ObjectId,
    pub capabilities: Vec<SessionCapability>,
    pub refresh_policy: SessionRefreshPolicy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionGenerationRecord {
    pub id: ObjectId,
    pub record: JsonValue,
    pub session_id: ObjectId,
    pub write_topic_id: ObjectId,
    pub generation_number: u64,
    pub base_resolved_view_id: ObjectId,
    pub resolved_view_id: ObjectId,
    pub topic_frontier: Vec<TopicFrontierEntry>,
    pub refresh_policy: SessionRefreshPolicy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionStartRecords {
    pub session: SessionRecord,
    pub generation: SessionGenerationRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopicSessionError {
    InvalidTopicSlug { value: String },
    MissingWriteTopic,
    MultipleWriteTopics { count: usize },
    EmptyActorId,
    Record(RecordError),
}

impl Display for TopicSessionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTopicSlug { value } => {
                write!(f, "invalid topic slug `{value}`")
            }
            Self::MissingWriteTopic => f.write_str("session must have exactly one write topic"),
            Self::MultipleWriteTopics { count } => {
                write!(f, "session must have exactly one write topic, got {count}")
            }
            Self::EmptyActorId => f.write_str("actor_id cannot be empty"),
            Self::Record(error) => Display::fmt(error, f),
        }
    }
}

impl Error for TopicSessionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Record(error) => Some(error),
            _ => None,
        }
    }
}

impl From<RecordError> for TopicSessionError {
    fn from(error: RecordError) -> Self {
        Self::Record(error)
    }
}

pub fn create_topic_record(input: TopicCreateInput) -> Result<TopicRecord, TopicSessionError> {
    let record = topic_record_json(&input);
    let id = canonical_record_id(input.repository_id.clone(), RecordKind::Topic, &record)?;

    Ok(TopicRecord {
        id,
        record,
        slug: input.slug,
        display_name: input.display_name,
        owner_actor_id: input.owner_actor_id,
        base_checkpoint_id: input.base_checkpoint_id,
        status: TopicStatus::Open,
        head_revision_id: None,
    })
}

pub fn start_session_records(
    input: SessionStartInput,
) -> Result<SessionStartRecords, TopicSessionError> {
    if input.actor_id.is_empty() {
        return Err(TopicSessionError::EmptyActorId);
    }

    let write_topic_id = exactly_one_write_topic(&input.topic_frontier)?;
    let session_record = session_record_json(&input, &write_topic_id);
    let session_id = canonical_record_id(
        input.repository_id.clone(),
        RecordKind::Session,
        &session_record,
    )?;
    let generation_record = session_generation_record_json(&input, &write_topic_id, &session_id);
    let generation_id = canonical_record_id(
        input.repository_id,
        RecordKind::SessionGeneration,
        &generation_record,
    )?;

    Ok(SessionStartRecords {
        session: SessionRecord {
            id: session_id.clone(),
            record: session_record,
            actor_id: input.actor_id.clone(),
            write_topic_id: write_topic_id.clone(),
            pinned_resolved_view_id: input.resolved_view_id.clone(),
            current_generation_id: generation_id.clone(),
            capabilities: PHASE1_SESSION_CAPABILITIES.to_vec(),
            refresh_policy: SessionRefreshPolicy::PinnedExceptOwnTopic,
        },
        generation: SessionGenerationRecord {
            id: generation_id,
            record: generation_record,
            session_id,
            write_topic_id,
            generation_number: 0,
            base_resolved_view_id: input.base_resolved_view_id,
            resolved_view_id: input.resolved_view_id,
            topic_frontier: input.topic_frontier,
            refresh_policy: SessionRefreshPolicy::PinnedExceptOwnTopic,
        },
    })
}

fn is_valid_topic_slug(value: &str) -> bool {
    if value.is_empty() || value.len() > 64 {
        return false;
    }

    let bytes = value.as_bytes();
    if bytes.first() == Some(&b'-') || bytes.last() == Some(&b'-') {
        return false;
    }

    bytes
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
        && !value.contains("--")
}

fn exactly_one_write_topic(frontier: &[TopicFrontierEntry]) -> Result<ObjectId, TopicSessionError> {
    let write_topics: Vec<ObjectId> = frontier
        .iter()
        .filter(|entry| entry.mode == TopicBindingMode::Write)
        .map(|entry| entry.topic_id.clone())
        .collect();

    match write_topics.as_slice() {
        [] => Err(TopicSessionError::MissingWriteTopic),
        [topic_id] => Ok(topic_id.clone()),
        topics => Err(TopicSessionError::MultipleWriteTopics {
            count: topics.len(),
        }),
    }
}

fn topic_record_json(input: &TopicCreateInput) -> JsonValue {
    object([
        ("schema_version", number(RECORD_SCHEMA_VERSION)),
        ("record_type", string(RecordKind::Topic.as_str())),
        (
            "id",
            string(topic_logical_id(&input.repository_id, &input.slug)),
        ),
        ("repository_id", string(input.repository_id.as_str())),
        ("created_at", string(&input.created_at)),
        (
            "privacy_class",
            string(PrivacyClass::CommitDefault.as_str()),
        ),
        ("slug", string(input.slug.as_str())),
        ("display_name", string(&input.display_name)),
        ("owner_actor_id", string(&input.owner_actor_id)),
        (
            "base_checkpoint_id",
            string(input.base_checkpoint_id.to_string()),
        ),
        ("visibility", string(input.visibility.as_str())),
        ("status", string(TopicStatus::Open.as_str())),
        ("head_revision_id", JsonValue::Null),
    ])
}

fn session_record_json(input: &SessionStartInput, write_topic_id: &ObjectId) -> JsonValue {
    object([
        ("schema_version", number(RECORD_SCHEMA_VERSION)),
        ("record_type", string(RecordKind::Session.as_str())),
        ("id", string(session_logical_id(input))),
        ("repository_id", string(input.repository_id.as_str())),
        ("created_at", string(&input.created_at)),
        ("privacy_class", string(PrivacyClass::LocalOnly.as_str())),
        ("actor_id", string(&input.actor_id)),
        ("write_topic_id", string(write_topic_id.to_string())),
        (
            "pinned_resolved_view_id",
            string(input.resolved_view_id.to_string()),
        ),
        (
            "refresh_policy",
            string(SessionRefreshPolicy::PinnedExceptOwnTopic.as_str()),
        ),
        ("capabilities", capabilities_json()),
    ])
}

fn session_generation_record_json(
    input: &SessionStartInput,
    write_topic_id: &ObjectId,
    session_id: &ObjectId,
) -> JsonValue {
    object([
        ("schema_version", number(RECORD_SCHEMA_VERSION)),
        (
            "record_type",
            string(RecordKind::SessionGeneration.as_str()),
        ),
        ("id", string(generation_logical_id(input))),
        ("repository_id", string(input.repository_id.as_str())),
        ("created_at", string(&input.created_at)),
        ("privacy_class", string(PrivacyClass::LocalOnly.as_str())),
        ("session_id", string(session_id.to_string())),
        ("write_topic_id", string(write_topic_id.to_string())),
        (
            "base_resolved_view_id",
            string(input.base_resolved_view_id.to_string()),
        ),
        (
            "resolved_view_id",
            string(input.resolved_view_id.to_string()),
        ),
        ("topic_frontier", topic_frontier_json(&input.topic_frontier)),
        ("generation_number", JsonValue::Number("0".to_string())),
        (
            "refresh_policy",
            string(SessionRefreshPolicy::PinnedExceptOwnTopic.as_str()),
        ),
        ("created_by", string(&input.actor_id)),
    ])
}

fn topic_frontier_json(frontier: &[TopicFrontierEntry]) -> JsonValue {
    JsonValue::Array(
        frontier
            .iter()
            .map(|entry| {
                object([
                    ("topic_id", string(entry.topic_id.to_string())),
                    (
                        "revision_id",
                        entry
                            .revision_id
                            .as_ref()
                            .map(|id| string(id.to_string()))
                            .unwrap_or(JsonValue::Null),
                    ),
                    ("mode", string(entry.mode.as_str())),
                ])
            })
            .collect(),
    )
}

fn capabilities_json() -> JsonValue {
    JsonValue::Array(
        PHASE1_SESSION_CAPABILITIES
            .iter()
            .map(|capability| string(capability.as_str()))
            .collect(),
    )
}

fn topic_logical_id(repo_id: &RepoId, slug: &TopicSlug) -> String {
    format!("{TOPIC_ID_SEED}:{}:{}", repo_id.as_str(), slug.as_str())
}

fn session_logical_id(input: &SessionStartInput) -> String {
    format!(
        "{SESSION_ID_SEED}:{}:{}:{}",
        input.repository_id, input.actor_id, input.resolved_view_id
    )
}

fn generation_logical_id(input: &SessionStartInput) -> String {
    format!("{GENERATION_ID_SEED}:{}:0", session_logical_id(input))
}

fn object<const N: usize>(entries: [(&str, JsonValue); N]) -> JsonValue {
    JsonValue::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.to_string(), value))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn string(value: impl Into<String>) -> JsonValue {
    JsonValue::String(value.into())
}

fn number(value: u32) -> JsonValue {
    JsonValue::Number(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{ObjectHash, ObjectKind};
    use crate::records::{canonical_json_bytes, canonical_record_hash};

    #[test]
    fn topic_creation_record_has_phase1_shape() {
        let input = topic_input("auth-nullability");

        let topic = create_topic_record(input).unwrap();

        assert_eq!(topic.id.kind(), ObjectKind::Topic);
        assert_eq!(topic.slug.as_str(), "auth-nullability");
        assert_eq!(topic.status, TopicStatus::Open);
        assert_eq!(topic.head_revision_id, None);

        let record = match &topic.record {
            JsonValue::Object(record) => record,
            _ => panic!("topic record should be an object"),
        };
        assert_eq!(record.get("record_type"), Some(&string("topic")));
        assert_eq!(record.get("slug"), Some(&string("auth-nullability")));
        assert_eq!(
            record.get("display_name"),
            Some(&string("Auth nullability"))
        );
        assert_eq!(record.get("status"), Some(&string("open")));
        assert_eq!(record.get("head_revision_id"), Some(&JsonValue::Null));
        assert_eq!(
            record.get("base_checkpoint_id"),
            Some(&string(topic.base_checkpoint_id.to_string()))
        );
    }

    #[test]
    fn topic_slugs_are_narrow_and_agent_friendly() {
        assert!(TopicSlug::new("profile-ui-2").is_ok());

        for value in [
            "",
            "Profile",
            "profile_ui",
            "-profile",
            "profile-",
            "profile--ui",
        ] {
            assert_eq!(
                TopicSlug::new(value).unwrap_err(),
                TopicSessionError::InvalidTopicSlug {
                    value: value.to_string()
                }
            );
        }
    }

    #[test]
    fn session_start_records_read_and_write_capabilities() {
        let write_topic = object_id(ObjectKind::Topic, b"write-topic");
        let view = object_id(ObjectKind::View, b"view");
        let records = start_session_records(SessionStartInput {
            repository_id: repo_id(),
            actor_id: "agent-a".to_string(),
            base_resolved_view_id: view.clone(),
            resolved_view_id: view.clone(),
            topic_frontier: vec![TopicFrontierEntry {
                topic_id: write_topic.clone(),
                revision_id: None,
                mode: TopicBindingMode::Write,
            }],
            created_at: "2026-07-03T00:00:00Z".to_string(),
        })
        .unwrap();

        assert_eq!(records.session.id.kind(), ObjectKind::Session);
        assert_eq!(records.generation.id.kind(), ObjectKind::SessionGeneration);
        assert_eq!(records.session.actor_id, "agent-a");
        assert_eq!(records.session.write_topic_id, write_topic);
        assert_eq!(
            records.session.capabilities,
            PHASE1_SESSION_CAPABILITIES.to_vec()
        );
        assert!(records
            .session
            .capabilities
            .contains(&SessionCapability::Read));
        assert!(records
            .session
            .capabilities
            .contains(&SessionCapability::Write));
        assert!(records
            .session
            .capabilities
            .contains(&SessionCapability::Patch));
        assert_eq!(records.generation.generation_number, 0);
        assert_eq!(records.generation.base_resolved_view_id, view);
    }

    #[test]
    fn session_start_requires_exactly_one_write_topic() {
        let view = object_id(ObjectKind::View, b"view");
        let topic_a = object_id(ObjectKind::Topic, b"topic-a");
        let topic_b = object_id(ObjectKind::Topic, b"topic-b");

        let no_write = SessionStartInput {
            repository_id: repo_id(),
            actor_id: "agent-a".to_string(),
            base_resolved_view_id: view.clone(),
            resolved_view_id: view.clone(),
            topic_frontier: vec![TopicFrontierEntry {
                topic_id: topic_a.clone(),
                revision_id: None,
                mode: TopicBindingMode::PinnedRead,
            }],
            created_at: "2026-07-03T00:00:00Z".to_string(),
        };
        assert_eq!(
            start_session_records(no_write).unwrap_err(),
            TopicSessionError::MissingWriteTopic
        );

        let two_writes = SessionStartInput {
            topic_frontier: vec![
                TopicFrontierEntry {
                    topic_id: topic_a,
                    revision_id: None,
                    mode: TopicBindingMode::Write,
                },
                TopicFrontierEntry {
                    topic_id: topic_b,
                    revision_id: None,
                    mode: TopicBindingMode::Write,
                },
            ],
            ..session_input()
        };
        assert_eq!(
            start_session_records(two_writes).unwrap_err(),
            TopicSessionError::MultipleWriteTopics { count: 2 }
        );
    }

    #[test]
    fn pinned_refresh_policy_keeps_other_topic_frontier_fixed() {
        let input = SessionStartInput {
            topic_frontier: vec![
                TopicFrontierEntry {
                    topic_id: object_id(ObjectKind::Topic, b"write-topic"),
                    revision_id: Some(object_id(ObjectKind::TopicRevision, b"write-r1")),
                    mode: TopicBindingMode::Write,
                },
                TopicFrontierEntry {
                    topic_id: object_id(ObjectKind::Topic, b"read-topic"),
                    revision_id: Some(object_id(ObjectKind::TopicRevision, b"read-r7")),
                    mode: TopicBindingMode::PinnedRead,
                },
            ],
            ..session_input()
        };

        let records = start_session_records(input.clone()).unwrap();

        assert_eq!(
            records.session.refresh_policy,
            SessionRefreshPolicy::PinnedExceptOwnTopic
        );
        assert_eq!(
            records.generation.refresh_policy,
            SessionRefreshPolicy::PinnedExceptOwnTopic
        );
        assert_eq!(records.generation.topic_frontier, input.topic_frontier);
    }

    #[test]
    fn canonical_identity_is_stable_for_topic_and_session_records() {
        let first_topic = create_topic_record(topic_input("auth-nullability")).unwrap();
        let second_topic = create_topic_record(topic_input("auth-nullability")).unwrap();
        let different_topic = create_topic_record(topic_input("profile-ui")).unwrap();

        assert_eq!(first_topic.id, second_topic.id);
        assert_eq!(
            canonical_json_bytes(&first_topic.record).unwrap(),
            canonical_json_bytes(&second_topic.record).unwrap()
        );
        assert_eq!(
            canonical_record_hash(RecordKind::Topic, &first_topic.record).unwrap(),
            first_topic.id.hash()
        );
        assert_ne!(first_topic.id, different_topic.id);

        let first_session = start_session_records(session_input()).unwrap();
        let second_session = start_session_records(session_input()).unwrap();

        assert_eq!(first_session.session.id, second_session.session.id);
        assert_eq!(first_session.generation.id, second_session.generation.id);
        assert_eq!(
            canonical_record_hash(RecordKind::Session, &first_session.session.record).unwrap(),
            first_session.session.id.hash()
        );
        assert_eq!(
            canonical_record_hash(
                RecordKind::SessionGeneration,
                &first_session.generation.record
            )
            .unwrap(),
            first_session.generation.id.hash()
        );
    }

    fn topic_input(slug: &str) -> TopicCreateInput {
        TopicCreateInput {
            repository_id: repo_id(),
            slug: TopicSlug::new(slug).unwrap(),
            display_name: match slug {
                "auth-nullability" => "Auth nullability".to_string(),
                _ => slug.to_string(),
            },
            owner_actor_id: "agent-a".to_string(),
            base_checkpoint_id: object_id(ObjectKind::Checkpoint, b"base-checkpoint"),
            created_at: "2026-07-03T00:00:00Z".to_string(),
            visibility: TopicVisibility::Private,
        }
    }

    fn session_input() -> SessionStartInput {
        let view = object_id(ObjectKind::View, b"view");
        SessionStartInput {
            repository_id: repo_id(),
            actor_id: "agent-a".to_string(),
            base_resolved_view_id: view.clone(),
            resolved_view_id: view,
            topic_frontier: vec![TopicFrontierEntry {
                topic_id: object_id(ObjectKind::Topic, b"write-topic"),
                revision_id: None,
                mode: TopicBindingMode::Write,
            }],
            created_at: "2026-07-03T00:00:00Z".to_string(),
        }
    }

    fn repo_id() -> RepoId {
        "repo-a".parse().unwrap()
    }

    fn object_id(kind: ObjectKind, bytes: &[u8]) -> ObjectId {
        ObjectId::new(
            repo_id(),
            kind,
            ObjectHash::canonical(kind, RECORD_SCHEMA_VERSION, bytes),
        )
    }
}

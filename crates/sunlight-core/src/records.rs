use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::str::FromStr;

use crate::identity::{ObjectHash, ObjectId, ObjectKind, RepoId, CANONICAL_HASH_SCHEMA_VERSION};

pub const RECORD_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<JsonValue>),
    Object(BTreeMap<String, JsonValue>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RecordKind {
    Repository,
    Artifact,
    ContentBlob,
    ContentTree,
    OperationTransaction,
    Topic,
    TopicRevision,
    Session,
    SessionGeneration,
    ResolvedView,
    ConflictStaleness,
    Execution,
    Checkpoint,
    GitExportMap,
}

impl RecordKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Repository => "repository",
            Self::Artifact => "artifact",
            Self::ContentBlob => "content_blob",
            Self::ContentTree => "content_tree",
            Self::OperationTransaction => "operation_transaction",
            Self::Topic => "topic",
            Self::TopicRevision => "topic_revision",
            Self::Session => "session",
            Self::SessionGeneration => "session_generation",
            Self::ResolvedView => "resolved_view",
            Self::ConflictStaleness => "conflict_staleness",
            Self::Execution => "execution",
            Self::Checkpoint => "checkpoint",
            Self::GitExportMap => "git_export_map",
        }
    }

    pub fn object_kind(self) -> ObjectKind {
        match self {
            Self::Repository => ObjectKind::Repository,
            Self::Artifact => ObjectKind::Artifact,
            Self::ContentBlob => ObjectKind::Blob,
            Self::ContentTree => ObjectKind::Tree,
            Self::OperationTransaction => ObjectKind::Operation,
            Self::Topic => ObjectKind::Topic,
            Self::TopicRevision => ObjectKind::TopicRevision,
            Self::Session => ObjectKind::Session,
            Self::SessionGeneration => ObjectKind::SessionGeneration,
            Self::ResolvedView => ObjectKind::View,
            Self::ConflictStaleness => ObjectKind::Conflict,
            Self::Execution => ObjectKind::Execution,
            Self::Checkpoint => ObjectKind::Checkpoint,
            Self::GitExportMap => ObjectKind::ExportMap,
        }
    }
}

impl Display for RecordKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RecordKind {
    type Err = RecordError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "repository" => Ok(Self::Repository),
            "artifact" => Ok(Self::Artifact),
            "content_blob" => Ok(Self::ContentBlob),
            "content_tree" => Ok(Self::ContentTree),
            "operation_transaction" => Ok(Self::OperationTransaction),
            "topic" => Ok(Self::Topic),
            "topic_revision" => Ok(Self::TopicRevision),
            "session" => Ok(Self::Session),
            "session_generation" => Ok(Self::SessionGeneration),
            "resolved_view" => Ok(Self::ResolvedView),
            "conflict_staleness" => Ok(Self::ConflictStaleness),
            "execution" => Ok(Self::Execution),
            "checkpoint" => Ok(Self::Checkpoint),
            "git_export_map" => Ok(Self::GitExportMap),
            _ => Err(RecordError::UnknownRecordKind {
                value: value.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PrivacyClass {
    CommitDefault,
    PolicyGated,
    LocalOnly,
    Secret,
}

impl PrivacyClass {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CommitDefault => "commit_default",
            Self::PolicyGated => "policy_gated",
            Self::LocalOnly => "local_only",
            Self::Secret => "secret",
        }
    }
}

impl Display for PrivacyClass {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for PrivacyClass {
    type Err = RecordError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "commit_default" => Ok(Self::CommitDefault),
            "policy_gated" => Ok(Self::PolicyGated),
            "local_only" => Ok(Self::LocalOnly),
            "secret" => Ok(Self::Secret),
            _ => Err(RecordError::UnknownPrivacyClass {
                value: value.to_string(),
            }),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryScope {
    RepositoryId(String),
    RepositoryScope(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordEnvelope {
    pub schema_version: u32,
    pub record_type: RecordKind,
    pub id: String,
    pub repository_scope: RepositoryScope,
    pub created_at: String,
    pub privacy_class: PrivacyClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordError {
    Json(String),
    ExpectedObject,
    MissingField(&'static str),
    InvalidField {
        field: &'static str,
        expected: &'static str,
    },
    UnsupportedSchemaVersion(u64),
    UnknownRecordKind {
        value: String,
    },
    UnknownPrivacyClass {
        value: String,
    },
}

impl Display for RecordError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(message) => write!(f, "invalid JSON record: {message}"),
            Self::ExpectedObject => f.write_str("canonical record must be a JSON object"),
            Self::MissingField(field) => write!(f, "missing required record field `{field}`"),
            Self::InvalidField { field, expected } => {
                write!(f, "invalid record field `{field}`: expected {expected}")
            }
            Self::UnsupportedSchemaVersion(version) => {
                write!(f, "unsupported record schema_version `{version}`")
            }
            Self::UnknownRecordKind { value } => write!(f, "unknown record_type `{value}`"),
            Self::UnknownPrivacyClass { value } => {
                write!(f, "unknown privacy_class `{value}`")
            }
        }
    }
}

impl Error for RecordError {}

pub fn parse_json_record(input: &[u8]) -> Result<JsonValue, RecordError> {
    JsonParser::new(input).parse()
}

pub fn canonical_json_bytes(value: &JsonValue) -> Result<Vec<u8>, RecordError> {
    let mut output = Vec::new();
    write_canonical_json(value, &mut output);
    Ok(output)
}

pub fn canonical_json_bytes_from_slice(input: &[u8]) -> Result<Vec<u8>, RecordError> {
    canonical_json_bytes(&parse_json_record(input)?)
}

pub fn parse_record_envelope(value: &JsonValue) -> Result<RecordEnvelope, RecordError> {
    let object = match value {
        JsonValue::Object(object) => object,
        _ => return Err(RecordError::ExpectedObject),
    };
    let schema_version = required_u64(value, "schema_version")?;
    if schema_version != u64::from(RECORD_SCHEMA_VERSION) {
        return Err(RecordError::UnsupportedSchemaVersion(schema_version));
    }

    let repository_scope = match (object.get("repository_id"), object.get("repository_scope")) {
        (Some(_), Some(_)) => {
            return Err(RecordError::InvalidField {
                field: "repository_id",
                expected: "only one repository scope field",
            })
        }
        (Some(_), None) => RepositoryScope::RepositoryId(required_string(value, "repository_id")?),
        (None, Some(_)) => {
            RepositoryScope::RepositoryScope(required_string(value, "repository_scope")?)
        }
        (None, None) => return Err(RecordError::MissingField("repository_id")),
    };

    Ok(RecordEnvelope {
        schema_version: RECORD_SCHEMA_VERSION,
        record_type: required_string(value, "record_type")?.parse()?,
        id: required_string(value, "id")?,
        repository_scope,
        created_at: required_string(value, "created_at")?,
        privacy_class: required_string(value, "privacy_class")?.parse()?,
    })
}

pub fn canonical_record_hash(
    kind: RecordKind,
    record: &JsonValue,
) -> Result<ObjectHash, RecordError> {
    let envelope = parse_record_envelope(record)?;
    if envelope.record_type != kind {
        return Err(RecordError::InvalidField {
            field: "record_type",
            expected: kind.as_str(),
        });
    }

    Ok(ObjectHash::canonical(
        kind.object_kind(),
        CANONICAL_HASH_SCHEMA_VERSION,
        &canonical_json_bytes(record)?,
    ))
}

pub fn canonical_record_id(
    repo_id: RepoId,
    kind: RecordKind,
    record: &JsonValue,
) -> Result<ObjectId, RecordError> {
    Ok(ObjectId::new(
        repo_id,
        kind.object_kind(),
        canonical_record_hash(kind, record)?,
    ))
}

fn required_string(value: &JsonValue, field: &'static str) -> Result<String, RecordError> {
    match get_field(value, field)? {
        JsonValue::String(value) => Ok(value.clone()),
        _ => Err(RecordError::InvalidField {
            field,
            expected: "string",
        }),
    }
}

fn required_u64(value: &JsonValue, field: &'static str) -> Result<u64, RecordError> {
    match get_field(value, field)? {
        JsonValue::Number(value) => value.parse::<u64>().map_err(|_| RecordError::InvalidField {
            field,
            expected: "positive integer",
        }),
        _ => Err(RecordError::InvalidField {
            field,
            expected: "positive integer",
        }),
    }
}

fn get_field<'a>(value: &'a JsonValue, field: &'static str) -> Result<&'a JsonValue, RecordError> {
    match value {
        JsonValue::Object(object) => object.get(field).ok_or(RecordError::MissingField(field)),
        _ => Err(RecordError::ExpectedObject),
    }
}

fn write_canonical_json(value: &JsonValue, output: &mut Vec<u8>) {
    match value {
        JsonValue::Null => output.extend_from_slice(b"null"),
        JsonValue::Bool(true) => output.extend_from_slice(b"true"),
        JsonValue::Bool(false) => output.extend_from_slice(b"false"),
        JsonValue::Number(value) => output.extend_from_slice(value.as_bytes()),
        JsonValue::String(value) => write_json_string(value, output),
        JsonValue::Array(values) => {
            output.push(b'[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                write_canonical_json(value, output);
            }
            output.push(b']');
        }
        JsonValue::Object(entries) => {
            output.push(b'{');
            for (index, (key, value)) in entries.iter().enumerate() {
                if index > 0 {
                    output.push(b',');
                }
                write_json_string(key, output);
                output.push(b':');
                write_canonical_json(value, output);
            }
            output.push(b'}');
        }
    }
}

fn write_json_string(value: &str, output: &mut Vec<u8>) {
    output.push(b'"');
    for character in value.chars() {
        match character {
            '"' => output.extend_from_slice(br#"\""#),
            '\\' => output.extend_from_slice(br#"\\"#),
            '\u{08}' => output.extend_from_slice(br#"\b"#),
            '\u{0c}' => output.extend_from_slice(br#"\f"#),
            '\n' => output.extend_from_slice(br#"\n"#),
            '\r' => output.extend_from_slice(br#"\r"#),
            '\t' => output.extend_from_slice(br#"\t"#),
            character if character < ' ' => {
                output.extend_from_slice(format!("\\u{:04x}", character as u32).as_bytes());
            }
            character => {
                let mut buffer = [0_u8; 4];
                output.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
            }
        }
    }
    output.push(b'"');
}

struct JsonParser<'a> {
    input: &'a [u8],
    position: usize,
}

impl<'a> JsonParser<'a> {
    fn new(input: &'a [u8]) -> Self {
        Self { input, position: 0 }
    }

    fn parse(mut self) -> Result<JsonValue, RecordError> {
        let value = self.parse_value()?;
        self.skip_whitespace();
        if self.position != self.input.len() {
            return self.error("trailing bytes after JSON value");
        }
        Ok(value)
    }

    fn parse_value(&mut self) -> Result<JsonValue, RecordError> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'n') => self.parse_literal(b"null", JsonValue::Null),
            Some(b't') => self.parse_literal(b"true", JsonValue::Bool(true)),
            Some(b'f') => self.parse_literal(b"false", JsonValue::Bool(false)),
            Some(b'"') => self.parse_string().map(JsonValue::String),
            Some(b'[') => self.parse_array(),
            Some(b'{') => self.parse_object(),
            Some(b'-' | b'0'..=b'9') => self.parse_number(),
            Some(_) => self.error("unexpected JSON value"),
            None => self.error("unexpected end of JSON input"),
        }
    }

    fn parse_literal(
        &mut self,
        literal: &[u8],
        value: JsonValue,
    ) -> Result<JsonValue, RecordError> {
        if self.input[self.position..].starts_with(literal) {
            self.position += literal.len();
            Ok(value)
        } else {
            self.error("invalid JSON literal")
        }
    }

    fn parse_array(&mut self) -> Result<JsonValue, RecordError> {
        self.expect(b'[')?;
        let mut values = Vec::new();
        loop {
            self.skip_whitespace();
            if self.consume(b']') {
                break;
            }
            values.push(self.parse_value()?);
            self.skip_whitespace();
            if self.consume(b']') {
                break;
            }
            self.expect(b',')?;
        }
        Ok(JsonValue::Array(values))
    }

    fn parse_object(&mut self) -> Result<JsonValue, RecordError> {
        self.expect(b'{')?;
        let mut entries = BTreeMap::new();
        loop {
            self.skip_whitespace();
            if self.consume(b'}') {
                break;
            }
            let key = self.parse_string()?;
            self.skip_whitespace();
            self.expect(b':')?;
            let value = self.parse_value()?;
            entries.insert(key, value);
            self.skip_whitespace();
            if self.consume(b'}') {
                break;
            }
            self.expect(b',')?;
        }
        Ok(JsonValue::Object(entries))
    }

    fn parse_string(&mut self) -> Result<String, RecordError> {
        self.expect(b'"')?;
        let mut output = String::new();
        while let Some(byte) = self.next() {
            match byte {
                b'"' => return Ok(output),
                b'\\' => output.push(self.parse_escape()?),
                byte if byte < 0x20 => return self.error("unescaped control character in string"),
                byte if byte < 0x80 => output.push(byte as char),
                byte => {
                    let start = self.position - 1;
                    let width = utf8_width(byte)?;
                    let end = start + width;
                    if end > self.input.len() {
                        return self.error("truncated UTF-8 sequence in string");
                    }
                    let text = std::str::from_utf8(&self.input[start..end])
                        .map_err(|error| RecordError::Json(error.to_string()))?;
                    output.push_str(text);
                    self.position = end;
                }
            }
        }
        self.error("unterminated string")
    }

    fn parse_escape(&mut self) -> Result<char, RecordError> {
        match self.next() {
            Some(b'"') => Ok('"'),
            Some(b'\\') => Ok('\\'),
            Some(b'/') => Ok('/'),
            Some(b'b') => Ok('\u{08}'),
            Some(b'f') => Ok('\u{0c}'),
            Some(b'n') => Ok('\n'),
            Some(b'r') => Ok('\r'),
            Some(b't') => Ok('\t'),
            Some(b'u') => self.parse_unicode_escape(),
            _ => self.error("invalid string escape"),
        }
    }

    fn parse_unicode_escape(&mut self) -> Result<char, RecordError> {
        let value = self.parse_hex_quad()?;
        if (0xd800..=0xdbff).contains(&value) {
            let position = self.position;
            if self.next() != Some(b'\\') || self.next() != Some(b'u') {
                self.position = position;
                return self.error("missing low surrogate after high surrogate");
            }
            let low = self.parse_hex_quad()?;
            if !(0xdc00..=0xdfff).contains(&low) {
                return self.error("invalid low surrogate");
            }
            let combined = 0x10000 + (((value - 0xd800) as u32) << 10) + ((low - 0xdc00) as u32);
            char::from_u32(combined)
                .ok_or_else(|| RecordError::Json("invalid unicode escape".to_string()))
        } else if (0xdc00..=0xdfff).contains(&value) {
            self.error("low surrogate without high surrogate")
        } else {
            char::from_u32(value as u32)
                .ok_or_else(|| RecordError::Json("invalid unicode escape".to_string()))
        }
    }

    fn parse_hex_quad(&mut self) -> Result<u16, RecordError> {
        let mut value = 0_u16;
        for _ in 0..4 {
            value <<= 4;
            value |= match self.next() {
                Some(byte @ b'0'..=b'9') => u16::from(byte - b'0'),
                Some(byte @ b'a'..=b'f') => u16::from(byte - b'a' + 10),
                Some(byte @ b'A'..=b'F') => u16::from(byte - b'A' + 10),
                _ => return self.error("invalid unicode escape"),
            };
        }
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<JsonValue, RecordError> {
        let start = self.position;
        self.consume(b'-');
        match self.peek() {
            Some(b'0') => {
                self.position += 1;
            }
            Some(b'1'..=b'9') => {
                self.position += 1;
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.position += 1;
                }
            }
            _ => return self.error("invalid number"),
        }
        if self.consume(b'.') {
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return self.error("invalid number fraction");
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.position += 1;
            }
        }
        if self.consume(b'e') || self.consume(b'E') {
            let _ = self.consume(b'+') || self.consume(b'-');
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return self.error("invalid number exponent");
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.position += 1;
            }
        }
        let value = std::str::from_utf8(&self.input[start..self.position])
            .map_err(|error| RecordError::Json(error.to_string()))?;
        Ok(JsonValue::Number(value.to_string()))
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.position += 1;
        }
    }

    fn expect(&mut self, expected: u8) -> Result<(), RecordError> {
        match self.next() {
            Some(actual) if actual == expected => Ok(()),
            _ => self.error("unexpected JSON byte"),
        }
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.position).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.position += 1;
        Some(byte)
    }

    fn error<T>(&self, message: &str) -> Result<T, RecordError> {
        Err(RecordError::Json(format!(
            "{message} at byte {}",
            self.position
        )))
    }
}

fn utf8_width(byte: u8) -> Result<usize, RecordError> {
    match byte {
        0xc2..=0xdf => Ok(2),
        0xe0..=0xef => Ok(3),
        0xf0..=0xf4 => Ok(4),
        _ => Err(RecordError::Json("invalid UTF-8 sequence".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_json_bytes_ignore_key_order_and_formatting() {
        let compact = br#"{"schema_version":1,"record_type":"artifact","id":"artifact_1","repository_id":"repo_a","created_at":"2026-07-03T00:00:00Z","privacy_class":"commit_default","path_bindings":[{"path":"src/lib.rs","state":"active"}],"metadata":{"language":"rust","generated":false}}"#;
        let pretty = br#"{
            "privacy_class": "commit_default",
            "created_at": "2026-07-03T00:00:00Z",
            "repository_id": "repo_a",
            "id": "artifact_1",
            "metadata": {
                "generated": false,
                "language": "rust"
            },
            "path_bindings": [
                {
                    "state": "active",
                    "path": "src/lib.rs"
                }
            ],
            "record_type": "artifact",
            "schema_version": 1
        }"#;

        let compact = parse_json_record(compact).unwrap();
        let pretty = parse_json_record(pretty).unwrap();

        assert_eq!(
            canonical_json_bytes(&compact).unwrap(),
            canonical_json_bytes(&pretty).unwrap()
        );
        assert_eq!(
            canonical_record_hash(RecordKind::Artifact, &compact).unwrap(),
            canonical_record_hash(RecordKind::Artifact, &pretty).unwrap()
        );
    }

    #[test]
    fn privacy_class_round_trips_contract_names() {
        let cases = [
            ("commit_default", PrivacyClass::CommitDefault),
            ("policy_gated", PrivacyClass::PolicyGated),
            ("local_only", PrivacyClass::LocalOnly),
            ("secret", PrivacyClass::Secret),
        ];

        for (name, privacy_class) in cases {
            assert_eq!(name.parse::<PrivacyClass>().unwrap(), privacy_class);
            assert_eq!(privacy_class.to_string(), name);
        }

        assert!("private".parse::<PrivacyClass>().is_err());
    }

    #[test]
    fn record_envelope_requires_common_contract_fields() {
        let record = parse_json_record(
            br#"{
                "schema_version": 1,
                "record_type": "resolved_view",
                "id": "view_1",
                "repository_scope": "repo_a",
                "created_at": "2026-07-03T00:00:00Z",
                "privacy_class": "commit_default"
            }"#,
        )
        .unwrap();

        let envelope = parse_record_envelope(&record).unwrap();

        assert_eq!(envelope.schema_version, 1);
        assert_eq!(envelope.record_type, RecordKind::ResolvedView);
        assert_eq!(
            envelope.repository_scope,
            RepositoryScope::RepositoryScope("repo_a".to_string())
        );
        assert_eq!(envelope.privacy_class, PrivacyClass::CommitDefault);

        let missing_schema = parse_json_record(
            br#"{
                "record_type": "artifact",
                "id": "artifact_1",
                "repository_id": "repo_a",
                "created_at": "2026-07-03T00:00:00Z",
                "privacy_class": "commit_default"
            }"#,
        )
        .unwrap();

        assert_eq!(
            parse_record_envelope(&missing_schema).unwrap_err(),
            RecordError::MissingField("schema_version")
        );
    }

    #[test]
    fn canonical_record_hash_integrates_with_object_identity() {
        let record = parse_json_record(
            br#"{
                "privacy_class": "policy_gated",
                "created_at": "2026-07-03T00:00:00Z",
                "repository_id": "repo_a",
                "id": "op_sha256_example",
                "record_type": "operation_transaction",
                "schema_version": 1,
                "topic_id": "topic_auth",
                "session_id": "session_a",
                "session_generation_id": "gen_1",
                "actor_id": "agent_a",
                "authored_context_id": "ctx_1",
                "preconditions": {},
                "read_set": {},
                "write_set": [],
                "mutation_payload": {"kind": "patch"},
                "before_refs": {},
                "after_refs": {},
                "classification": "source",
                "logical_time": {},
                "parents": []
            }"#,
        )
        .unwrap();
        let repo_id: RepoId = "repo_a".parse().unwrap();

        let hash = canonical_record_hash(RecordKind::OperationTransaction, &record).unwrap();
        let object_id =
            canonical_record_id(repo_id.clone(), RecordKind::OperationTransaction, &record)
                .unwrap();
        let direct_object_id = ObjectId::from_canonical_bytes(
            repo_id,
            ObjectKind::Operation,
            CANONICAL_HASH_SCHEMA_VERSION,
            &canonical_json_bytes(&record).unwrap(),
        );

        assert_eq!(object_id.hash(), hash);
        assert_eq!(object_id, direct_object_id);
        assert_eq!(object_id.kind(), ObjectKind::Operation);
        assert!(object_id
            .to_string()
            .starts_with("repo_a:operation:sha256:"));
    }
}

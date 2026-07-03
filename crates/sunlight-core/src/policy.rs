use crate::records::PrivacyClass;

pub const MANAGED_IGNORE_BEGIN: &str = "# BEGIN SUNLIGHT MANAGED IGNORE";
pub const MANAGED_IGNORE_END: &str = "# END SUNLIGHT MANAGED IGNORE";
pub const MANAGED_IGNORE_COMMENTS: &[&str] = &[
    "# Sunlight local/cache state. Do not commit directly.",
    "# Object payloads are policy-validated before commit or export.",
];
pub const REQUIRED_MANAGED_IGNORE_ENTRIES: &[&str] = &[
    ".sunlight/local/",
    ".sunlight/cache/",
    ".sunlight/projections/",
    ".sunlight/tmp/",
    ".sunlight/quarantine/",
    ".sunlight/index.sqlite",
    ".sunlight/executions/**/sandbox/",
    ".sunlight/executions/**/raw-logs/",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PolicyCheck {
    IgnorePolicy,
    PathScope,
    PolicyClass,
    ExecutionRawExclusion,
}

impl PolicyCheck {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::IgnorePolicy => "ignore_policy",
            Self::PathScope => "path_scope",
            Self::PolicyClass => "policy_class",
            Self::ExecutionRawExclusion => "execution_raw_exclusion",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PolicyFailureCode {
    ManagedIgnoreBlockMissing,
    ManagedIgnoreEntryMissing,
    PathTraversal,
    AbsolutePath,
    OutsideSunlight,
    BlockedLocalPath,
    RawExecutionPath,
}

impl PolicyFailureCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ManagedIgnoreBlockMissing => "managed_ignore_block_missing",
            Self::ManagedIgnoreEntryMissing => "managed_ignore_entry_missing",
            Self::PathTraversal => "path_traversal",
            Self::AbsolutePath => "absolute_path",
            Self::OutsideSunlight => "outside_sunlight",
            Self::BlockedLocalPath => "blocked_local_path",
            Self::RawExecutionPath => "raw_execution_path",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CandidatePathClass {
    CommitDefaultMetadata,
    PolicyGatedPayload,
    LocalOnly,
    RawExecution,
}

impl CandidatePathClass {
    pub fn effective_privacy_class(self) -> PrivacyClass {
        match self {
            Self::CommitDefaultMetadata => PrivacyClass::CommitDefault,
            Self::PolicyGatedPayload => PrivacyClass::PolicyGated,
            Self::LocalOnly | Self::RawExecution => PrivacyClass::LocalOnly,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationFailure {
    pub check: PolicyCheck,
    pub code: PolicyFailureCode,
    pub path: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationReport {
    pub ok: bool,
    pub failures: Vec<ValidationFailure>,
}

impl ValidationReport {
    pub fn success() -> Self {
        Self {
            ok: true,
            failures: Vec::new(),
        }
    }

    pub fn from_failures(failures: Vec<ValidationFailure>) -> Self {
        Self {
            ok: failures.is_empty(),
            failures,
        }
    }
}

pub fn managed_ignore_block() -> String {
    let mut block = String::new();
    block.push_str(MANAGED_IGNORE_BEGIN);
    block.push('\n');
    for comment in MANAGED_IGNORE_COMMENTS {
        block.push_str(comment);
        block.push('\n');
    }
    for entry in REQUIRED_MANAGED_IGNORE_ENTRIES {
        block.push_str(entry);
        block.push('\n');
    }
    block.push_str(MANAGED_IGNORE_END);
    block.push('\n');
    block
}

pub fn validate_managed_ignore_block(gitignore: &str) -> ValidationReport {
    let Some(block) = managed_block_body(gitignore) else {
        return ValidationReport::from_failures(vec![ValidationFailure {
            check: PolicyCheck::IgnorePolicy,
            code: PolicyFailureCode::ManagedIgnoreBlockMissing,
            path: Some(".gitignore".to_string()),
            reason: "Sunlight managed ignore block is missing".to_string(),
        }]);
    };

    let entries = block
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    let failures = REQUIRED_MANAGED_IGNORE_ENTRIES
        .iter()
        .filter(|required| !entries.contains(required))
        .map(|required| ValidationFailure {
            check: PolicyCheck::IgnorePolicy,
            code: PolicyFailureCode::ManagedIgnoreEntryMissing,
            path: Some((*required).to_string()),
            reason: format!("required Sunlight ignore entry `{required}` is missing"),
        })
        .collect();

    ValidationReport::from_failures(failures)
}

pub fn classify_candidate_path(path: &str) -> Result<CandidatePathClass, ValidationFailure> {
    let normalized = normalize_candidate_path(path)?;
    let parts = normalized.split('/').collect::<Vec<_>>();
    if parts.first() != Some(&".sunlight") {
        return Err(ValidationFailure {
            check: PolicyCheck::PathScope,
            code: PolicyFailureCode::OutsideSunlight,
            path: Some(normalized),
            reason: "policy candidate path is outside .sunlight".to_string(),
        });
    }

    if normalized == ".sunlight/config.toml"
        || has_prefix(&parts, &[".sunlight", "records"])
        || has_prefix(&parts, &[".sunlight", "topics"])
        || has_prefix(&parts, &[".sunlight", "views"])
        || has_prefix(&parts, &[".sunlight", "checkpoints"])
        || has_prefix(&parts, &[".sunlight", "conflicts"])
        || has_prefix(&parts, &[".sunlight", "export-map"])
    {
        return Ok(CandidatePathClass::CommitDefaultMetadata);
    }

    if normalized == ".sunlight/index.sqlite"
        || has_prefix(&parts, &[".sunlight", "local"])
        || has_prefix(&parts, &[".sunlight", "cache"])
        || has_prefix(&parts, &[".sunlight", "projection"])
        || has_prefix(&parts, &[".sunlight", "projections"])
        || has_prefix(&parts, &[".sunlight", "temp"])
        || has_prefix(&parts, &[".sunlight", "tmp"])
        || has_prefix(&parts, &[".sunlight", "quarantine"])
    {
        return Ok(CandidatePathClass::LocalOnly);
    }

    if has_execution_component(&parts, "sandbox")
        || has_execution_component(&parts, "raw-log")
        || has_execution_component(&parts, "raw-logs")
    {
        return Ok(CandidatePathClass::RawExecution);
    }

    if has_prefix(&parts, &[".sunlight", "objects"])
        || has_prefix(&parts, &[".sunlight", "operations"])
        || has_prefix(&parts, &[".sunlight", "executions"])
    {
        return Ok(CandidatePathClass::PolicyGatedPayload);
    }

    Ok(CandidatePathClass::PolicyGatedPayload)
}

pub fn validate_candidate_paths<'a>(paths: impl IntoIterator<Item = &'a str>) -> ValidationReport {
    let failures = paths
        .into_iter()
        .filter_map(|path| match classify_candidate_path(path) {
            Ok(
                CandidatePathClass::CommitDefaultMetadata | CandidatePathClass::PolicyGatedPayload,
            ) => None,
            Ok(CandidatePathClass::LocalOnly) => Some(ValidationFailure {
                check: PolicyCheck::PolicyClass,
                code: PolicyFailureCode::BlockedLocalPath,
                path: Some(path.to_string()),
                reason: "local_only Sunlight paths cannot be committed or exported".to_string(),
            }),
            Ok(CandidatePathClass::RawExecution) => Some(ValidationFailure {
                check: PolicyCheck::ExecutionRawExclusion,
                code: PolicyFailureCode::RawExecutionPath,
                path: Some(path.to_string()),
                reason: "raw execution sandboxes and logs are local_only".to_string(),
            }),
            Err(failure) => Some(failure),
        })
        .collect();

    ValidationReport::from_failures(failures)
}

fn managed_block_body(gitignore: &str) -> Option<&str> {
    let start = gitignore.find(MANAGED_IGNORE_BEGIN)? + MANAGED_IGNORE_BEGIN.len();
    let tail = &gitignore[start..];
    let end = tail.find(MANAGED_IGNORE_END)?;
    Some(&tail[..end])
}

fn normalize_candidate_path(path: &str) -> Result<String, ValidationFailure> {
    let path = path.trim();
    if path.starts_with('/') || path.as_bytes().get(1) == Some(&b':') {
        return Err(ValidationFailure {
            check: PolicyCheck::PathScope,
            code: PolicyFailureCode::AbsolutePath,
            path: Some(path.to_string()),
            reason: "candidate path must be repository-relative".to_string(),
        });
    }

    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                return Err(ValidationFailure {
                    check: PolicyCheck::PathScope,
                    code: PolicyFailureCode::PathTraversal,
                    path: Some(path.to_string()),
                    reason: "candidate path must not traverse outside the repository".to_string(),
                });
            }
            part => parts.push(part),
        }
    }

    Ok(parts.join("/"))
}

fn has_prefix(parts: &[&str], prefix: &[&str]) -> bool {
    parts.len() >= prefix.len() && parts[..prefix.len()] == *prefix
}

fn has_execution_component(parts: &[&str], component: &str) -> bool {
    parts.len() >= 4 && parts[0] == ".sunlight" && parts[1] == "executions" && parts[3] == component
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_ignore_block_requires_all_policy_entries() {
        let block = managed_ignore_block();
        let report = validate_managed_ignore_block(&block);

        assert!(report.ok);
        for entry in REQUIRED_MANAGED_IGNORE_ENTRIES {
            assert!(block.contains(entry));
        }
        assert!(block.contains(MANAGED_IGNORE_BEGIN));
        assert!(block.contains(MANAGED_IGNORE_END));
    }

    #[test]
    fn missing_required_ignore_entry_reports_structured_code() {
        let gitignore = format!("{MANAGED_IGNORE_BEGIN}\n.sunlight/local/\n{MANAGED_IGNORE_END}\n");
        let report = validate_managed_ignore_block(&gitignore);

        assert!(!report.ok);
        assert!(report.failures.iter().any(|failure| {
            failure.check == PolicyCheck::IgnorePolicy
                && failure.code == PolicyFailureCode::ManagedIgnoreEntryMissing
                && failure.path.as_deref() == Some(".sunlight/cache/")
        }));
    }

    #[test]
    fn candidate_classification_allows_commit_default_metadata_paths() {
        for path in [
            ".sunlight/config.toml",
            ".sunlight/records/repository.json",
            ".sunlight/topics/topic_1/meta.json",
            ".sunlight/views/view_1.json",
            ".sunlight/checkpoints/checkpoint_1.json",
            ".sunlight/conflicts/conflict_1.json",
            ".sunlight/export-map/map_1.json",
        ] {
            let class = classify_candidate_path(path).unwrap();
            assert_eq!(class, CandidatePathClass::CommitDefaultMetadata);
            assert_eq!(class.effective_privacy_class(), PrivacyClass::CommitDefault);
        }
    }

    #[test]
    fn local_and_cache_paths_are_rejected() {
        let report = validate_candidate_paths([
            ".sunlight/local/lease.json",
            ".sunlight/cache/blob.tmp",
            ".sunlight/projection/view_1/src/lib.rs",
            ".sunlight/projections/view_1/src/lib.rs",
            ".sunlight/temp/journal",
            ".sunlight/tmp/journal",
            ".sunlight/quarantine/payload",
            ".sunlight/index.sqlite",
        ]);

        assert!(!report.ok);
        assert_eq!(report.failures.len(), 8);
        assert!(report.failures.iter().all(|failure| {
            failure.check == PolicyCheck::PolicyClass
                && failure.code == PolicyFailureCode::BlockedLocalPath
        }));
    }

    #[test]
    fn raw_execution_and_sandbox_paths_are_rejected() {
        let report = validate_candidate_paths([
            ".sunlight/executions/exec_1/raw-log/stderr.log",
            ".sunlight/executions/exec_1/raw-logs/stdout.log",
            ".sunlight/executions/exec_1/sandbox/src/lib.rs",
        ]);

        assert!(!report.ok);
        assert_eq!(report.failures.len(), 3);
        assert!(report.failures.iter().all(|failure| {
            failure.check == PolicyCheck::ExecutionRawExclusion
                && failure.code == PolicyFailureCode::RawExecutionPath
        }));
    }

    #[test]
    fn path_scope_failures_use_structured_codes() {
        let report =
            validate_candidate_paths(["../.sunlight/config.toml", "/tmp/raw.log", "src/lib.rs"]);

        assert!(!report.ok);
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.code == PolicyFailureCode::PathTraversal));
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.code == PolicyFailureCode::AbsolutePath));
        assert!(report
            .failures
            .iter()
            .any(|failure| failure.code == PolicyFailureCode::OutsideSunlight));
    }
}

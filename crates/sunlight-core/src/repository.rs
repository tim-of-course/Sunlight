use std::collections::hash_map::DefaultHasher;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::policy::managed_ignore_block;

pub const CURRENT_CONFIG_SCHEMA_VERSION: u32 = 1;
pub const PRIVATE_PROJECTION_FILESYSTEM_WRITE_POLICY: &str = "private_projection_isolated";
pub const CURRENT_STORAGE_SCHEMA_VERSION: u32 = 1;
pub const CONSERVATIVE_SUNLIGHT_COMMIT_POLICY: &str = "conservative";
pub const SUPPORTED_UNICODE_NORMALIZATION: &str = "preserve";
pub const SUPPORTED_SYMLINK_POLICY: &str = "preserve";
pub const SUPPORTED_EXECUTABLE_BITS_POLICY: &str = "preserve";
pub const DEFAULT_EXECUTION_TIMEOUT_MS: u64 = 300_000;
pub const DEFAULT_EXECUTION_STDOUT_LIMIT_BYTES: u64 = 1_048_576;
pub const DEFAULT_EXECUTION_STDERR_LIMIT_BYTES: u64 = 1_048_576;
pub const DEFAULT_EXECUTION_PROCESS_MEMORY_LIMIT_BYTES: u64 = 2_147_483_648;
pub const DEFAULT_EXECUTION_JOB_MEMORY_LIMIT_BYTES: u64 = 4_294_967_296;
pub const DEFAULT_EXECUTION_CPU_TIME_LIMIT_MS: u64 = 300_000;
pub const DEFAULT_EXECUTION_ACTIVE_PROCESS_LIMIT: u32 = 32;
pub const CONSERVATIVE_ENVIRONMENT_INHERITANCE: &str = "minimal_os_allowlist";
pub const NOT_ENFORCED_NETWORK_POLICY: &str = "not_enforced";
pub const DISABLED_NETWORK_POLICY: &str = "disabled";

pub const fn default_local_network_policy() -> &'static str {
    NOT_ENFORCED_NETWORK_POLICY
}

const MAX_EXECUTION_TIMEOUT_MS: u64 = 86_400_000;
const MAX_EXECUTION_OUTPUT_LIMIT_BYTES: u64 = 67_108_864;
const MIN_EXECUTION_MEMORY_LIMIT_BYTES: u64 = 16_777_216;
const MAX_EXECUTION_MEMORY_LIMIT_BYTES: u64 = 1_099_511_627_776;
const MAX_EXECUTION_CPU_TIME_LIMIT_MS: u64 = 86_400_000;
const MAX_EXECUTION_ACTIVE_PROCESS_LIMIT: u32 = 1_024;

const SUNLIGHT_DIR: &str = ".sunlight";
const CONFIG_FILE: &str = "config.toml";
const GITIGNORE_FILE: &str = ".gitignore";

const AUTHORITATIVE_DIRS: &[&str] = &[
    "objects",
    "records",
    "topics",
    "operations",
    "views",
    "checkpoints",
    "executions",
    "conflicts",
    "export-map",
];

const LOCAL_ONLY_DIRS: &[&str] = &["local", "cache", "projections", "tmp", "quarantine"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryConfig {
    pub repository_id: String,
    pub config_schema_version: u32,
    pub storage_schema_version: u32,
    pub path_policy: PathPolicy,
    pub projection_policy: ProjectionPolicy,
    pub execution_policy: ExecutionPolicy,
    pub git_interop: GitInteropPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathPolicy {
    pub case_sensitive: bool,
    pub unicode_normalization: String,
    pub symlinks: String,
    pub executable_bits: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionPolicy {
    pub default_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPolicy {
    pub timeout_ms: u64,
    pub stdout_limit_bytes: u64,
    pub stderr_limit_bytes: u64,
    pub process_memory_limit_bytes: u64,
    pub job_memory_limit_bytes: u64,
    pub cpu_time_limit_ms: u64,
    pub active_process_limit: u32,
    pub environment_inheritance: String,
    pub network_policy: String,
    pub filesystem_write_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedProjectionPolicy {
    pub managed_root: PathBuf,
    pub managed_root_relative: PathBuf,
}

impl ResolvedProjectionPolicy {
    pub fn compatibility_root(&self, projection_id: &str) -> PathBuf {
        self.managed_root
            .join("compat")
            .join(projection_id)
            .join("root")
    }

    pub fn execution_root(&self, projection_id: &str) -> PathBuf {
        self.managed_root.join(projection_id).join("root")
    }

    pub fn projection_root(&self, purpose: &str, projection_id: &str) -> PathBuf {
        match purpose {
            "compatibility" => self.compatibility_root(projection_id),
            "execution" => self.execution_root(projection_id),
            purpose => self
                .managed_root
                .join(purpose)
                .join(projection_id)
                .join("root"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitInteropPolicy {
    pub sunlight_commit_policy: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitReport {
    pub repo_root: PathBuf,
    pub sunlight_dir: PathBuf,
    pub repository_id: String,
    pub created_config: bool,
    pub created_gitignore: bool,
    pub created_directories: Vec<PathBuf>,
}

#[derive(Debug)]
pub enum RepositoryError {
    Io { path: PathBuf, source: io::Error },
    InvalidConfig { path: PathBuf, message: String },
}

impl Display for RepositoryError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "failed to access {}: {}", path.display(), source)
            }
            Self::InvalidConfig { path, message } => {
                write!(
                    f,
                    "invalid Sunlight config at {}: {}",
                    path.display(),
                    message
                )
            }
        }
    }
}

impl Error for RepositoryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::InvalidConfig { .. } => None,
        }
    }
}

impl RepositoryConfig {
    pub fn new(repository_id: String) -> Self {
        Self {
            repository_id,
            config_schema_version: CURRENT_CONFIG_SCHEMA_VERSION,
            storage_schema_version: CURRENT_STORAGE_SCHEMA_VERSION,
            path_policy: PathPolicy {
                case_sensitive: true,
                unicode_normalization: "preserve".to_string(),
                symlinks: "preserve".to_string(),
                executable_bits: "preserve".to_string(),
            },
            projection_policy: ProjectionPolicy {
                default_root: ".sunlight/projections".to_string(),
            },
            execution_policy: ExecutionPolicy {
                timeout_ms: DEFAULT_EXECUTION_TIMEOUT_MS,
                stdout_limit_bytes: DEFAULT_EXECUTION_STDOUT_LIMIT_BYTES,
                stderr_limit_bytes: DEFAULT_EXECUTION_STDERR_LIMIT_BYTES,
                process_memory_limit_bytes: DEFAULT_EXECUTION_PROCESS_MEMORY_LIMIT_BYTES,
                job_memory_limit_bytes: DEFAULT_EXECUTION_JOB_MEMORY_LIMIT_BYTES,
                cpu_time_limit_ms: DEFAULT_EXECUTION_CPU_TIME_LIMIT_MS,
                active_process_limit: DEFAULT_EXECUTION_ACTIVE_PROCESS_LIMIT,
                environment_inheritance: CONSERVATIVE_ENVIRONMENT_INHERITANCE.to_string(),
                network_policy: default_local_network_policy().to_string(),
                filesystem_write_policy: PRIVATE_PROJECTION_FILESYSTEM_WRITE_POLICY.to_string(),
            },
            git_interop: GitInteropPolicy {
                sunlight_commit_policy: "conservative".to_string(),
            },
        }
    }

    pub fn to_toml(&self) -> String {
        format!(
            "\
repository_id = \"{}\"
config_schema_version = {}
storage_schema_version = {}

[path_policy]
case_sensitive = {}
unicode_normalization = \"{}\"
symlinks = \"{}\"
executable_bits = \"{}\"

[projection_policy]
default_root = \"{}\"

[execution_policy]
timeout_ms = {}
stdout_limit_bytes = {}
stderr_limit_bytes = {}
process_memory_limit_bytes = {}
job_memory_limit_bytes = {}
cpu_time_limit_ms = {}
active_process_limit = {}
environment_inheritance = \"{}\"
network_policy = \"{}\"
filesystem_write_policy = \"{}\"

[git_interop]
sunlight_commit_policy = \"{}\"
",
            escape_toml(&self.repository_id),
            self.config_schema_version,
            self.storage_schema_version,
            self.path_policy.case_sensitive,
            escape_toml(&self.path_policy.unicode_normalization),
            escape_toml(&self.path_policy.symlinks),
            escape_toml(&self.path_policy.executable_bits),
            escape_toml(&self.projection_policy.default_root),
            self.execution_policy.timeout_ms,
            self.execution_policy.stdout_limit_bytes,
            self.execution_policy.stderr_limit_bytes,
            self.execution_policy.process_memory_limit_bytes,
            self.execution_policy.job_memory_limit_bytes,
            self.execution_policy.cpu_time_limit_ms,
            self.execution_policy.active_process_limit,
            escape_toml(&self.execution_policy.environment_inheritance),
            escape_toml(&self.execution_policy.network_policy),
            escape_toml(&self.execution_policy.filesystem_write_policy),
            escape_toml(&self.git_interop.sunlight_commit_policy),
        )
    }

    pub fn from_toml(input: &str, path: impl Into<PathBuf>) -> Result<Self, RepositoryError> {
        let path = path.into();
        let repository_id = parse_string_key(input, "repository_id").ok_or_else(|| {
            RepositoryError::InvalidConfig {
                path: path.clone(),
                message: "missing repository_id".to_string(),
            }
        })?;

        let sunlight_commit_policy =
            parse_string_key(input, "sunlight_commit_policy").ok_or_else(|| {
                RepositoryError::InvalidConfig {
                    path: path.clone(),
                    message: "missing git_interop.sunlight_commit_policy".to_string(),
                }
            })?;
        let case_sensitive = parse_bool_key(input, "case_sensitive").ok_or_else(|| {
            RepositoryError::InvalidConfig {
                path: path.clone(),
                message: "missing path_policy.case_sensitive".to_string(),
            }
        })?;
        let unicode_normalization =
            parse_string_key(input, "unicode_normalization").ok_or_else(|| {
                RepositoryError::InvalidConfig {
                    path: path.clone(),
                    message: "missing path_policy.unicode_normalization".to_string(),
                }
            })?;
        let symlinks =
            parse_string_key(input, "symlinks").ok_or_else(|| RepositoryError::InvalidConfig {
                path: path.clone(),
                message: "missing path_policy.symlinks".to_string(),
            })?;
        let executable_bits = parse_string_key(input, "executable_bits").ok_or_else(|| {
            RepositoryError::InvalidConfig {
                path: path.clone(),
                message: "missing path_policy.executable_bits".to_string(),
            }
        })?;
        let default_root = parse_string_key(input, "default_root").ok_or_else(|| {
            RepositoryError::InvalidConfig {
                path: path.clone(),
                message: "missing projection_policy.default_root".to_string(),
            }
        })?;
        let config = Self {
            repository_id,
            config_schema_version: parse_u32_key(input, "config_schema_version").unwrap_or(1),
            storage_schema_version: parse_u32_key(input, "storage_schema_version").unwrap_or(1),
            path_policy: PathPolicy {
                case_sensitive,
                unicode_normalization,
                symlinks,
                executable_bits,
            },
            projection_policy: ProjectionPolicy { default_root },
            execution_policy: ExecutionPolicy {
                timeout_ms: parse_u64_key_or_default(
                    input,
                    "timeout_ms",
                    DEFAULT_EXECUTION_TIMEOUT_MS,
                    &path,
                )?,
                stdout_limit_bytes: parse_u64_key_or_default(
                    input,
                    "stdout_limit_bytes",
                    DEFAULT_EXECUTION_STDOUT_LIMIT_BYTES,
                    &path,
                )?,
                stderr_limit_bytes: parse_u64_key_or_default(
                    input,
                    "stderr_limit_bytes",
                    DEFAULT_EXECUTION_STDERR_LIMIT_BYTES,
                    &path,
                )?,
                process_memory_limit_bytes: parse_u64_key_or_default(
                    input,
                    "process_memory_limit_bytes",
                    DEFAULT_EXECUTION_PROCESS_MEMORY_LIMIT_BYTES,
                    &path,
                )?,
                job_memory_limit_bytes: parse_u64_key_or_default(
                    input,
                    "job_memory_limit_bytes",
                    DEFAULT_EXECUTION_JOB_MEMORY_LIMIT_BYTES,
                    &path,
                )?,
                cpu_time_limit_ms: parse_u64_key_or_default(
                    input,
                    "cpu_time_limit_ms",
                    DEFAULT_EXECUTION_CPU_TIME_LIMIT_MS,
                    &path,
                )?,
                active_process_limit: parse_u32_key_or_default(
                    input,
                    "active_process_limit",
                    DEFAULT_EXECUTION_ACTIVE_PROCESS_LIMIT,
                    &path,
                )?,
                environment_inheritance: parse_string_key_or_default(
                    input,
                    "environment_inheritance",
                    CONSERVATIVE_ENVIRONMENT_INHERITANCE,
                    &path,
                )?,
                network_policy: parse_string_key_or_default(
                    input,
                    "network_policy",
                    default_local_network_policy(),
                    &path,
                )?,
                filesystem_write_policy: parse_string_key_or_default(
                    input,
                    "filesystem_write_policy",
                    PRIVATE_PROJECTION_FILESYSTEM_WRITE_POLICY,
                    &path,
                )?,
            },
            git_interop: GitInteropPolicy {
                sunlight_commit_policy,
            },
        };
        config.validate(&path)?;
        Ok(config)
    }

    pub fn validate(&self, path: impl Into<PathBuf>) -> Result<(), RepositoryError> {
        let path = path.into();
        if self.config_schema_version != CURRENT_CONFIG_SCHEMA_VERSION {
            return Err(RepositoryError::InvalidConfig {
                path,
                message: format!(
                    "unsupported config_schema_version `{}`; expected `{}`",
                    self.config_schema_version, CURRENT_CONFIG_SCHEMA_VERSION
                ),
            });
        }
        if self.storage_schema_version != CURRENT_STORAGE_SCHEMA_VERSION {
            return Err(RepositoryError::InvalidConfig {
                path,
                message: format!(
                    "unsupported storage_schema_version `{}`; expected `{}`",
                    self.storage_schema_version, CURRENT_STORAGE_SCHEMA_VERSION
                ),
            });
        }
        if self.git_interop.sunlight_commit_policy != CONSERVATIVE_SUNLIGHT_COMMIT_POLICY {
            return Err(RepositoryError::InvalidConfig {
                path,
                message: format!(
                    "unsupported git_interop.sunlight_commit_policy `{}`; supported value is `{}`",
                    self.git_interop.sunlight_commit_policy, CONSERVATIVE_SUNLIGHT_COMMIT_POLICY
                ),
            });
        }
        if self.execution_policy.timeout_ms == 0
            || self.execution_policy.timeout_ms > MAX_EXECUTION_TIMEOUT_MS
        {
            return Err(RepositoryError::InvalidConfig {
                path,
                message: format!(
                    "unsafe execution_policy.timeout_ms `{}`; expected 1..={MAX_EXECUTION_TIMEOUT_MS}",
                    self.execution_policy.timeout_ms
                ),
            });
        }
        for (field, value) in [
            (
                "stdout_limit_bytes",
                self.execution_policy.stdout_limit_bytes,
            ),
            (
                "stderr_limit_bytes",
                self.execution_policy.stderr_limit_bytes,
            ),
        ] {
            if value == 0 || value > MAX_EXECUTION_OUTPUT_LIMIT_BYTES {
                return Err(RepositoryError::InvalidConfig {
                    path,
                    message: format!(
                        "unsafe execution_policy.{field} `{value}`; expected 1..={MAX_EXECUTION_OUTPUT_LIMIT_BYTES}"
                    ),
                });
            }
        }
        for (field, value) in [
            (
                "process_memory_limit_bytes",
                self.execution_policy.process_memory_limit_bytes,
            ),
            (
                "job_memory_limit_bytes",
                self.execution_policy.job_memory_limit_bytes,
            ),
        ] {
            if !(MIN_EXECUTION_MEMORY_LIMIT_BYTES..=MAX_EXECUTION_MEMORY_LIMIT_BYTES)
                .contains(&value)
            {
                return Err(RepositoryError::InvalidConfig {
                    path: path.clone(),
                    message: format!(
                        "unsafe execution_policy.{field} `{value}`; expected {MIN_EXECUTION_MEMORY_LIMIT_BYTES}..={MAX_EXECUTION_MEMORY_LIMIT_BYTES}"
                    ),
                });
            }
        }
        if self.execution_policy.job_memory_limit_bytes
            < self.execution_policy.process_memory_limit_bytes
        {
            return Err(RepositoryError::InvalidConfig {
                path: path.clone(),
                message: "unsafe execution_policy.job_memory_limit_bytes; expected a value greater than or equal to process_memory_limit_bytes".to_string(),
            });
        }
        if self.execution_policy.cpu_time_limit_ms == 0
            || self.execution_policy.cpu_time_limit_ms > MAX_EXECUTION_CPU_TIME_LIMIT_MS
        {
            return Err(RepositoryError::InvalidConfig {
                path: path.clone(),
                message: format!(
                    "unsafe execution_policy.cpu_time_limit_ms `{}`; expected 1..={MAX_EXECUTION_CPU_TIME_LIMIT_MS}",
                    self.execution_policy.cpu_time_limit_ms
                ),
            });
        }
        if self.execution_policy.active_process_limit == 0
            || self.execution_policy.active_process_limit > MAX_EXECUTION_ACTIVE_PROCESS_LIMIT
        {
            return Err(RepositoryError::InvalidConfig {
                path: path.clone(),
                message: format!(
                    "unsafe execution_policy.active_process_limit `{}`; expected 1..={MAX_EXECUTION_ACTIVE_PROCESS_LIMIT}",
                    self.execution_policy.active_process_limit
                ),
            });
        }
        if self.execution_policy.environment_inheritance != CONSERVATIVE_ENVIRONMENT_INHERITANCE {
            return Err(RepositoryError::InvalidConfig {
                path,
                message: format!(
                    "unsupported execution_policy.environment_inheritance `{}`; supported value is `{CONSERVATIVE_ENVIRONMENT_INHERITANCE}`",
                    self.execution_policy.environment_inheritance
                ),
            });
        }
        if ![NOT_ENFORCED_NETWORK_POLICY, DISABLED_NETWORK_POLICY]
            .contains(&self.execution_policy.network_policy.as_str())
        {
            return Err(RepositoryError::InvalidConfig {
                path,
                message: format!(
                    "unsupported execution_policy.network_policy `{}`; supported values are `{NOT_ENFORCED_NETWORK_POLICY}` and `{DISABLED_NETWORK_POLICY}`",
                    self.execution_policy.network_policy,
                ),
            });
        }
        if self.execution_policy.filesystem_write_policy
            != PRIVATE_PROJECTION_FILESYSTEM_WRITE_POLICY
        {
            return Err(RepositoryError::InvalidConfig {
                path,
                message: format!(
                    "unsupported execution_policy.filesystem_write_policy `{}`; supported value is `{PRIVATE_PROJECTION_FILESYSTEM_WRITE_POLICY}`",
                    self.execution_policy.filesystem_write_policy
                ),
            });
        }
        if !self.path_policy.case_sensitive {
            return Err(RepositoryError::InvalidConfig {
                path,
                message:
                    "unsupported path_policy.case_sensitive `false`; the local MVP requires `true`"
                        .to_string(),
            });
        }
        for (field, actual, supported) in [
            (
                "unicode_normalization",
                self.path_policy.unicode_normalization.as_str(),
                SUPPORTED_UNICODE_NORMALIZATION,
            ),
            (
                "symlinks",
                self.path_policy.symlinks.as_str(),
                SUPPORTED_SYMLINK_POLICY,
            ),
            (
                "executable_bits",
                self.path_policy.executable_bits.as_str(),
                SUPPORTED_EXECUTABLE_BITS_POLICY,
            ),
        ] {
            if actual != supported {
                return Err(RepositoryError::InvalidConfig {
                    path,
                    message: format!(
                        "unsupported path_policy.{field} `{actual}`; supported value is `{supported}`"
                    ),
                });
            }
        }
        Ok(())
    }
}

pub fn resolve_projection_policy(
    repo_root: impl AsRef<Path>,
    config: &RepositoryConfig,
) -> Result<ResolvedProjectionPolicy, RepositoryError> {
    let repo_root = repo_root.as_ref();
    let config_path = repo_root.join(SUNLIGHT_DIR).join(CONFIG_FILE);
    config.validate(&config_path)?;
    let configured = Path::new(&config.projection_policy.default_root);
    let configured_is_absolute = configured.is_absolute();
    let mut normalized = PathBuf::new();
    for component in configured.components() {
        match component {
            Component::Normal(value) => normalized.push(value),
            Component::CurDir => {
                return Err(invalid_projection_root(
                    &config_path,
                    &config.projection_policy.default_root,
                    "must be normalized without `.` components",
                ));
            }
            Component::ParentDir => {
                return Err(invalid_projection_root(
                    &config_path,
                    &config.projection_policy.default_root,
                    "must not contain parent traversal",
                ));
            }
            Component::RootDir | Component::Prefix(_) if configured_is_absolute => {
                normalized.push(component.as_os_str());
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(invalid_projection_root(
                    &config_path,
                    &config.projection_policy.default_root,
                    "contains an invalid path prefix",
                ));
            }
        }
    }
    let components = normalized
        .iter()
        .map(|value| value.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    if components.is_empty() {
        return Err(invalid_projection_root(
            &config_path,
            &config.projection_policy.default_root,
            "must not be empty",
        ));
    }
    let repo_root = fs::canonicalize(repo_root).map_err(|source| RepositoryError::Io {
        path: repo_root.to_path_buf(),
        source,
    })?;
    let managed_root = if configured_is_absolute {
        reject_reparse_path_components(&normalized, &config_path)?;
        let managed_root = fs::canonicalize(&normalized).map_err(|source| RepositoryError::Io {
            path: normalized.clone(),
            source,
        })?;
        if managed_root == repo_root
            || managed_root.starts_with(&repo_root)
            || repo_root.starts_with(&managed_root)
        {
            return Err(invalid_projection_root(
                &config_path,
                &config.projection_policy.default_root,
                "absolute managed root must be disjoint from the repository source tree",
            ));
        }
        managed_root
    } else {
        if components
            .iter()
            .any(|component| component.eq_ignore_ascii_case(".git"))
        {
            return Err(invalid_projection_root(
                &config_path,
                &config.projection_policy.default_root,
                "must not overlap `.git`",
            ));
        }
        if components.first().map(String::as_str) != Some(SUNLIGHT_DIR) || components.len() < 2 {
            return Err(invalid_projection_root(
                &config_path,
                &config.projection_policy.default_root,
                "must be a descendant of `.sunlight` and must not overlap the source tree",
            ));
        }
        let protected = AUTHORITATIVE_DIRS.iter().copied().chain([
            "local",
            "quarantine",
            CONFIG_FILE,
            GITIGNORE_FILE,
            "index.sqlite",
        ]);
        if protected.into_iter().any(|name| components[1] == name) {
            return Err(invalid_projection_root(
                &config_path,
                &config.projection_policy.default_root,
                "must not overlap authoritative or quarantine state",
            ));
        }
        reject_symlinked_managed_components(&repo_root, &normalized, &config_path)?;
        repo_root.join(&normalized)
    };
    Ok(ResolvedProjectionPolicy {
        managed_root,
        managed_root_relative: normalized,
    })
}

fn reject_reparse_path_components(path: &Path, config_path: &Path) -> Result<(), RepositoryError> {
    for current in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
        let metadata = fs::symlink_metadata(current).map_err(|source| RepositoryError::Io {
            path: current.to_path_buf(),
            source,
        })?;
        if metadata_is_reparse_point(&metadata) {
            return Err(RepositoryError::InvalidConfig {
                path: config_path.to_path_buf(),
                message: format!(
                    "unsafe projection_policy.default_root: managed path component `{}` is a reparse point",
                    current.display()
                ),
            });
        }
        if !metadata.is_dir() {
            return Err(RepositoryError::InvalidConfig {
                path: config_path.to_path_buf(),
                message: format!(
                    "unsafe projection_policy.default_root: managed path component `{}` is not a directory",
                    current.display()
                ),
            });
        }
    }
    Ok(())
}

#[cfg(windows)]
fn metadata_is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;

    metadata.file_attributes() & 0x0000_0400 != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

fn reject_symlinked_managed_components(
    repo_root: &Path,
    relative: &Path,
    config_path: &Path,
) -> Result<(), RepositoryError> {
    let mut current = repo_root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(RepositoryError::InvalidConfig {
                    path: config_path.to_path_buf(),
                    message: format!(
                        "unsafe projection_policy.default_root: managed path component `{}` is a symlink",
                        current.display()
                    ),
                });
            }
            Ok(metadata) if !metadata.is_dir() => {
                return Err(RepositoryError::InvalidConfig {
                    path: config_path.to_path_buf(),
                    message: format!(
                        "unsafe projection_policy.default_root: managed path component `{}` is not a directory",
                        current.display()
                    ),
                });
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(source) => {
                return Err(RepositoryError::Io {
                    path: current,
                    source,
                });
            }
        }
    }
    Ok(())
}

fn invalid_projection_root(path: &Path, value: &str, reason: &str) -> RepositoryError {
    RepositoryError::InvalidConfig {
        path: path.to_path_buf(),
        message: format!("unsafe projection_policy.default_root `{value}`: {reason}"),
    }
}

pub fn init_repository(repo_root: impl AsRef<Path>) -> Result<InitReport, RepositoryError> {
    let repo_root = repo_root.as_ref().to_path_buf();
    let sunlight_dir = repo_root.join(SUNLIGHT_DIR);
    ensure_dir(&sunlight_dir)?;

    let mut created_directories = Vec::new();
    for child in AUTHORITATIVE_DIRS.iter().chain(LOCAL_ONLY_DIRS.iter()) {
        let path = sunlight_dir.join(child);
        if ensure_dir(&path)? {
            created_directories.push(path);
        }
    }

    let config_path = sunlight_dir.join(CONFIG_FILE);
    let (config, created_config) = if config_path.exists() {
        let body = read_to_string(&config_path)?;
        (
            RepositoryConfig::from_toml(&body, config_path.clone())?,
            false,
        )
    } else {
        let config = RepositoryConfig::new(generate_repository_id(&repo_root));
        write_new_file(&config_path, config.to_toml().as_bytes())?;
        (config, true)
    };

    let gitignore_path = sunlight_dir.join(GITIGNORE_FILE);
    let created_gitignore = if gitignore_path.exists() {
        false
    } else {
        let generated_gitignore = managed_ignore_block();
        write_new_file(&gitignore_path, generated_gitignore.as_bytes())?;
        true
    };

    Ok(InitReport {
        repo_root,
        sunlight_dir,
        repository_id: config.repository_id,
        created_config,
        created_gitignore,
        created_directories,
    })
}

fn ensure_dir(path: &Path) -> Result<bool, RepositoryError> {
    if path.is_dir() {
        return Ok(false);
    }

    fs::create_dir_all(path).map_err(|source| RepositoryError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(true)
}

fn read_to_string(path: &Path) -> Result<String, RepositoryError> {
    fs::read_to_string(path).map_err(|source| RepositoryError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<(), RepositoryError> {
    fs::write(path, bytes).map_err(|source| RepositoryError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn generate_repository_id(repo_root: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    repo_root.hash(&mut hasher);
    std::process::id().hash(&mut hasher);
    now_nanos().hash(&mut hasher);
    format!("repo-{:016x}", hasher.finish())
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn escape_toml(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn parse_string_key(input: &str, key: &str) -> Option<String> {
    let prefix = format!("{} = ", key);
    input.lines().find_map(|line| {
        let value = line.trim().strip_prefix(&prefix)?;
        let value = value.strip_prefix('"')?.strip_suffix('"')?;
        Some(value.replace("\\\"", "\"").replace("\\\\", "\\"))
    })
}

fn parse_u32_key(input: &str, key: &str) -> Option<u32> {
    let prefix = format!("{} = ", key);
    input.lines().find_map(|line| {
        line.trim()
            .strip_prefix(&prefix)
            .and_then(|value| value.parse::<u32>().ok())
    })
}

fn parse_u64_key_or_default(
    input: &str,
    key: &str,
    default: u64,
    path: &Path,
) -> Result<u64, RepositoryError> {
    let prefix = format!("{} = ", key);
    let Some(value) = input
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
    else {
        return Ok(default);
    };
    value
        .parse::<u64>()
        .map_err(|_| RepositoryError::InvalidConfig {
            path: path.to_path_buf(),
            message: format!("invalid execution_policy.{key} `{value}`; expected an integer"),
        })
}

fn parse_u32_key_or_default(
    input: &str,
    key: &str,
    default: u32,
    path: &Path,
) -> Result<u32, RepositoryError> {
    let prefix = format!("{} = ", key);
    let Some(value) = input
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
    else {
        return Ok(default);
    };
    value
        .parse::<u32>()
        .map_err(|_| RepositoryError::InvalidConfig {
            path: path.to_path_buf(),
            message: format!("invalid execution_policy.{key} `{value}`; expected an integer"),
        })
}

fn parse_string_key_or_default(
    input: &str,
    key: &str,
    default: &str,
    path: &Path,
) -> Result<String, RepositoryError> {
    let prefix = format!("{} = ", key);
    let Some(value) = input
        .lines()
        .find_map(|line| line.trim().strip_prefix(&prefix))
    else {
        return Ok(default.to_string());
    };
    let Some(value) = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
    else {
        return Err(RepositoryError::InvalidConfig {
            path: path.to_path_buf(),
            message: format!("invalid execution_policy.{key}; expected a quoted string"),
        });
    };
    Ok(value.replace("\\\"", "\"").replace("\\\\", "\\"))
}

fn parse_bool_key(input: &str, key: &str) -> Option<bool> {
    let prefix = format!("{} = ", key);
    input.lines().find_map(|line| {
        line.trim()
            .strip_prefix(&prefix)
            .and_then(|value| value.parse::<bool>().ok())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_architecture_aligned_layout() {
        let repo = TestRepo::new("layout");

        let report = init_repository(repo.path()).expect("init should succeed");

        assert!(report.created_config);
        assert!(report.created_gitignore);
        assert!(repo.path().join(".sunlight/config.toml").is_file());
        assert!(repo.path().join(".sunlight/.gitignore").is_file());

        for child in AUTHORITATIVE_DIRS.iter().chain(LOCAL_ONLY_DIRS.iter()) {
            assert!(
                repo.path().join(".sunlight").join(child).is_dir(),
                "missing .sunlight/{child}"
            );
        }

        let config = fs::read_to_string(repo.path().join(".sunlight/config.toml")).unwrap();
        assert!(config.contains("repository_id = \"repo-"));
        assert!(config.contains("sunlight_commit_policy = \"conservative\""));
        assert!(config.contains("[execution_policy]"));
        assert!(config.contains("timeout_ms = 300000"));
        assert!(config.contains("process_memory_limit_bytes = 2147483648"));
        assert!(config.contains("job_memory_limit_bytes = 4294967296"));
        assert!(config.contains("cpu_time_limit_ms = 300000"));
        assert!(config.contains("active_process_limit = 32"));
        assert!(config.contains("environment_inheritance = \"minimal_os_allowlist\""));
        assert!(config.contains(&format!(
            "network_policy = \"{}\"",
            default_local_network_policy()
        )));

        let gitignore = fs::read_to_string(repo.path().join(".sunlight/.gitignore")).unwrap();
        assert!(gitignore.contains("/local/"));
        assert!(gitignore.contains("/cache/"));
        assert!(gitignore.contains("/executions/**/raw-logs/"));
    }

    #[test]
    fn init_is_idempotent_and_preserves_identity_and_policy_files() {
        let repo = TestRepo::new("idempotent");

        let first = init_repository(repo.path()).expect("first init should succeed");
        let gitignore_path = repo.path().join(".sunlight/.gitignore");
        fs::write(&gitignore_path, "# user managed\n/local/\n").unwrap();

        let second = init_repository(repo.path()).expect("second init should succeed");

        assert!(!second.created_config);
        assert!(!second.created_gitignore);
        assert!(second.created_directories.is_empty());
        assert_eq!(first.repository_id, second.repository_id);
        assert_eq!(
            fs::read_to_string(gitignore_path).unwrap(),
            "# user managed\n/local/\n"
        );
    }

    #[test]
    fn config_rejects_unknown_git_interop_policy() {
        let path = PathBuf::from(".sunlight/config.toml");
        let input = RepositoryConfig::new("repo_test".to_string())
            .to_toml()
            .replace(
                "sunlight_commit_policy = \"conservative\"",
                "sunlight_commit_policy = \"permissive\"",
            );

        let error = RepositoryConfig::from_toml(&input, path).unwrap_err();
        assert!(error
            .to_string()
            .contains("unsupported git_interop.sunlight_commit_policy `permissive`"));
    }

    #[test]
    fn old_config_without_execution_keys_loads_conservative_defaults() {
        let input = RepositoryConfig::new("repo_test".to_string())
            .to_toml()
            .lines()
            .filter(|line| {
                !matches!(
                    line.trim(),
                    "[execution_policy]"
                        | "timeout_ms = 300000"
                        | "stdout_limit_bytes = 1048576"
                        | "stderr_limit_bytes = 1048576"
                        | "process_memory_limit_bytes = 2147483648"
                        | "job_memory_limit_bytes = 4294967296"
                        | "cpu_time_limit_ms = 300000"
                        | "active_process_limit = 32"
                        | "environment_inheritance = \"minimal_os_allowlist\""
                        | "network_policy = \"not_enforced\""
                        | "network_policy = \"disabled\""
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let config = RepositoryConfig::from_toml(&input, ".sunlight/config.toml").unwrap();
        assert_eq!(
            config.execution_policy.timeout_ms,
            DEFAULT_EXECUTION_TIMEOUT_MS
        );
        assert_eq!(
            config.execution_policy.stdout_limit_bytes,
            DEFAULT_EXECUTION_STDOUT_LIMIT_BYTES
        );
        assert_eq!(
            config.execution_policy.process_memory_limit_bytes,
            DEFAULT_EXECUTION_PROCESS_MEMORY_LIMIT_BYTES
        );
        assert_eq!(
            config.execution_policy.job_memory_limit_bytes,
            DEFAULT_EXECUTION_JOB_MEMORY_LIMIT_BYTES
        );
        assert_eq!(
            config.execution_policy.cpu_time_limit_ms,
            DEFAULT_EXECUTION_CPU_TIME_LIMIT_MS
        );
        assert_eq!(
            config.execution_policy.active_process_limit,
            DEFAULT_EXECUTION_ACTIVE_PROCESS_LIMIT
        );
        assert_eq!(
            config.execution_policy.environment_inheritance,
            CONSERVATIVE_ENVIRONMENT_INHERITANCE
        );
        assert_eq!(
            config.execution_policy.network_policy,
            default_local_network_policy()
        );
    }

    #[test]
    fn explicit_network_policies_are_preserved() {
        let input = RepositoryConfig::new("repo_test".to_string())
            .to_toml()
            .replace(
                "network_policy = \"not_enforced\"",
                "network_policy = \"disabled\"",
            );

        let config = RepositoryConfig::from_toml(&input, ".sunlight/config.toml").unwrap();

        assert_eq!(
            config.execution_policy.network_policy,
            DISABLED_NETWORK_POLICY
        );

        let input = input.replace(
            "network_policy = \"disabled\"",
            "network_policy = \"not_enforced\"",
        );
        let config = RepositoryConfig::from_toml(&input, ".sunlight/config.toml").unwrap();
        assert_eq!(
            config.execution_policy.network_policy,
            NOT_ENFORCED_NETWORK_POLICY
        );
    }

    #[test]
    fn config_rejects_unsafe_execution_policy_values() {
        let base = RepositoryConfig::new("repo_test".to_string()).to_toml();
        for (needle, replacement, expected) in [
            (
                "timeout_ms = 300000",
                "timeout_ms = 0",
                "execution_policy.timeout_ms",
            ),
            (
                "stdout_limit_bytes = 1048576",
                "stdout_limit_bytes = 999999999",
                "execution_policy.stdout_limit_bytes",
            ),
            (
                "environment_inheritance = \"minimal_os_allowlist\"",
                "environment_inheritance = \"all\"",
                "execution_policy.environment_inheritance",
            ),
            (
                "stderr_limit_bytes = 1048576",
                "stderr_limit_bytes = invalid",
                "execution_policy.stderr_limit_bytes",
            ),
            (
                "process_memory_limit_bytes = 2147483648",
                "process_memory_limit_bytes = 1024",
                "execution_policy.process_memory_limit_bytes",
            ),
            (
                "job_memory_limit_bytes = 4294967296",
                "job_memory_limit_bytes = 1073741824",
                "execution_policy.job_memory_limit_bytes",
            ),
            (
                "cpu_time_limit_ms = 300000",
                "cpu_time_limit_ms = 0",
                "execution_policy.cpu_time_limit_ms",
            ),
            (
                "active_process_limit = 32",
                "active_process_limit = invalid",
                "execution_policy.active_process_limit",
            ),
            (
                "environment_inheritance = \"minimal_os_allowlist\"",
                "environment_inheritance = 42",
                "execution_policy.environment_inheritance",
            ),
        ] {
            let error = RepositoryConfig::from_toml(
                &base.replace(needle, replacement),
                ".sunlight/config.toml",
            )
            .unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
        }
    }

    #[test]
    fn config_rejects_missing_and_unsupported_projection_path_policy() {
        let base = RepositoryConfig::new("repo_test".to_string()).to_toml();
        for (needle, replacement, expected) in [
            (
                "case_sensitive = true",
                "case_sensitive = false",
                "path_policy.case_sensitive",
            ),
            (
                "unicode_normalization = \"preserve\"",
                "unicode_normalization = \"nfc\"",
                "path_policy.unicode_normalization",
            ),
            (
                "symlinks = \"preserve\"",
                "symlinks = \"follow\"",
                "path_policy.symlinks",
            ),
            (
                "executable_bits = \"preserve\"",
                "executable_bits = \"ignore\"",
                "path_policy.executable_bits",
            ),
            (
                "default_root = \".sunlight/projections\"\n",
                "",
                "missing projection_policy.default_root",
            ),
        ] {
            let error = RepositoryConfig::from_toml(
                &base.replace(needle, replacement),
                ".sunlight/config.toml",
            )
            .unwrap_err();
            assert!(error.to_string().contains(expected), "{error}");
        }
    }

    #[test]
    fn projection_policy_resolves_safe_custom_root_and_rejects_unsafe_roots() {
        let repo = TestRepo::new("projection-policy");
        let external = TestRepo::new("projection-policy-external");
        fs::create_dir_all(repo.path().join(".sunlight")).unwrap();
        let mut config = RepositoryConfig::new("repo_test".to_string());
        config.projection_policy.default_root = ".sunlight/custom-projections".to_string();

        let resolved = resolve_projection_policy(repo.path(), &config).unwrap();
        assert_eq!(
            resolved.managed_root,
            fs::canonicalize(repo.path())
                .unwrap()
                .join(".sunlight/custom-projections")
        );
        assert_eq!(
            resolved.compatibility_root("projection_1"),
            resolved.managed_root.join("compat/projection_1/root")
        );
        assert_eq!(
            resolved.execution_root("projection_2"),
            resolved.managed_root.join("projection_2/root")
        );

        config.projection_policy.default_root = external.path().display().to_string();
        let resolved = resolve_projection_policy(repo.path(), &config).unwrap();
        assert_eq!(
            resolved.managed_root,
            fs::canonicalize(external.path()).unwrap()
        );
        assert!(resolved.managed_root_relative.is_absolute());

        for root in [
            "",
            ".sunlight/../outside",
            ".git/sunlight",
            "source-projections",
            ".sunlight",
            ".sunlight/objects/projections",
            ".sunlight/local/projections",
            ".sunlight/quarantine/projections",
            ".sunlight/config.toml/projections",
        ] {
            config.projection_policy.default_root = root.to_string();
            let error = resolve_projection_policy(repo.path(), &config).unwrap_err();
            assert!(
                error.to_string().contains("projection_policy.default_root"),
                "root {root:?}: {error}"
            );
        }
        config.projection_policy.default_root = repo.path().display().to_string();
        let error = resolve_projection_policy(repo.path(), &config).unwrap_err();
        assert!(error.to_string().contains("disjoint"));
    }

    struct TestRepo {
        path: PathBuf,
    }

    impl TestRepo {
        fn new(name: &str) -> Self {
            let mut path = std::env::temp_dir();
            path.push(format!(
                "sunlight-core-test-{}-{}",
                name,
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

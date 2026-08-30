use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, ExitStatus, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

mod agent_setup;
pub mod engine;
mod mcp;
#[cfg(windows)]
mod windows_isolation;
#[cfg(windows)]
mod windows_job;

use sha2::{Digest, Sha256};

use sunlight_core::artifacts::{
    ArtifactIoError, ArtifactKind, ContentBlob, ContentTree, DeleteRequest, ExpectedHash,
    InMemoryArtifactStore, ListResponse, MetadataSetRequest, MoveRequest, MutationArtifactView,
    MutationKind, MutationPayload, MutationRefs, MutationResponse, OperationTransactionRecord,
    PatchRequest, ReadResponse, SearchResponse, SessionGenerationMutationRecord, SessionView,
    SessionVisibleArtifactView, TopicRevisionRecord, TreeEntry, TreeIdentityView, WriteMode,
    WriteRequest, FILE_OPERATION_SEMANTICS_VERSION, FIXTURE_ACTOR_ID, FIXTURE_REPOSITORY_ID,
    FIXTURE_RESOLVED_VIEW_ID, FIXTURE_SESSION_GENERATION_ID, FIXTURE_SESSION_ID, FIXTURE_TREE_HASH,
    FIXTURE_WRITE_TOPIC_ID, POSIX_CASE_SENSITIVE_PATH_POLICY_ID,
};
use sunlight_core::checkpoint::{
    fixture_checkpoint_from_resolved_view, validated_execution_evidence, CheckpointRecord,
    CheckpointValidationError, EvidenceRef, GitExportMapRecord, FIXTURE_CHECKPOINT_ID,
    FIXTURE_CREATED_AT, FIXTURE_EXPORTED_GIT_REF, FIXTURE_EXPORT_MAP_ID, FIXTURE_GIT_COMMIT_ID,
    FIXTURE_VALIDATION_REPORT_ID,
};
use sunlight_core::compat_import::{
    diff_real_compat_projection, diff_real_worktree, fixture_basic_app_candidate_deltas,
    plan_fixture_basic_app_import, real_compat_baseline_manifest_digest,
    validate_real_compat_selection, validate_real_worktree_selection, CompatCandidateDelta,
    CompatCandidateKind, CompatFileOperationKind, CompatImportErrorCode, CompatImportRequest,
    CompatImportResponse, CompatImportValidationError, CompatImportedArtifact,
    FIXTURE_COMPAT_BASELINE_MANIFEST_DIGEST, FIXTURE_COMPAT_IMPORT_OPERATION_ID,
};
use sunlight_core::execution::{
    execution_output_promotion_record_from_ids,
    execution_output_promotion_record_from_mutation_response,
    fixture_execution_output_promotion_record, fixture_failing_execution_from_resolved_view,
    fixture_passing_execution_from_resolved_view, fixture_promotion_candidate_provenance,
    promotion_authored_context_id, ExecutionFoundationError, ExecutionOutputPromotionProvenanceRef,
    ExecutionOutputPromotionRecord, ExecutionRecord, OutputClassification, OutputKind,
    PromotionCandidateProvenance, FIXTURE_PASSING_EXECUTION_ID, FIXTURE_PROMOTION_ARTIFACT_ID,
    FIXTURE_PROMOTION_OPERATION_TRANSACTION_ID, FIXTURE_PROMOTION_RESOLVED_VIEW_ID,
    FIXTURE_PROMOTION_SESSION_GENERATION_ID, FIXTURE_PROMOTION_TOPIC_REVISION_ID,
    FIXTURE_PROMOTION_TREE_HASH,
};
use sunlight_core::git_export::{
    execute_git_export_writer_plan_fixture, execute_local_git_export_writer, git_export_checkpoint,
    load_git_export_validation_report, persist_git_export_validation_report,
    plan_git_export_writer, validate_persisted_git_export, GeneratedOutputExportRequirement,
    GitExportCommitPlan, GitExportContentFile, GitExportError, GitExportExecutionError,
    GitExportExecutionFixture, GitExportExecutionResult, GitExportExecutionStep,
    GitExportExecutionStepFixture, GitExportExecutionSummary, GitExportMapStore,
    GitExportPlanningError, GitExportRefUpdatePlan, GitExportRepositoryState, GitExportRequest,
    GitExportResponse, GitExportValidationFailure, GitExportValidationReport,
    GitExportValidationReportStoreError, GitExportWriterInput, GitExportWriterPlan, GitRefState,
    ImportedBaseGitCommit, InMemoryGitExportMapStore, PersistedGitExportMap,
    PersistedGitExportValidationInput,
};
use sunlight_core::policy::{
    validate_candidate_paths, validate_managed_ignore_block, ValidationFailure, ValidationReport,
};
use sunlight_core::projection::{
    cleanup_projection_quarantine_local_records,
    fixture_compatibility_projection_from_resolved_view,
    fixture_execution_projection_from_resolved_view, fixture_export_projection_from_resolved_view,
    fixture_inspection_projection_from_resolved_view,
    fixture_projection_manifest_from_content_tree, is_projection_local_metadata_parent_path,
    is_projection_local_metadata_path, materialize_fixture_projection_copy,
    persist_projection_quarantine_local_record, plan_fixture_projection_materialization,
    projection_manifest_local_record_path, projection_manifest_ref,
    projection_store_integrity_failed_quarantined, projection_store_integrity_from_manifest_scan,
    projection_store_integrity_not_checked, ProjectionFilesystemMaterialization,
    ProjectionManifestRecord, ProjectionMaterializationCapabilities,
    ProjectionMaterializationError, ProjectionMaterializationErrorCode,
    ProjectionMaterializationLocalMetadata, ProjectionMaterializationPlan,
    ProjectionMaterializationRequest, ProjectionPurpose, ProjectionQuarantineLocalCleanup,
    ProjectionRecord, ProjectionRootRef, ProjectionStoreIntegrityReasonCode,
    ProjectionStoreIntegrityResult, ProjectionStoreIntegrityStatus, ProjectionStrategy,
    ProjectionValidationError, WritablePolicy, FIXTURE_COMPATIBILITY_PROJECTION_ID,
    FIXTURE_EXECUTION_PROJECTION_ID, FIXTURE_EXPORT_PROJECTION_ID,
    FIXTURE_INSPECTION_PROJECTION_ID,
};
use sunlight_core::records::{canonical_json_bytes, parse_json_record, JsonValue};
use sunlight_core::repo_state::{
    materialize_real_projection, native_session_generation_id, path_is_gitignored,
    path_is_sunignored, process_may_be_live, read_real_blob, real_artifact_id_for_path,
    real_content_hash, real_execution_environment_summary_digest, real_tree_hash,
    scan_real_projection_files_with_quarantine, validate_topic_metadata,
    verify_real_readonly_projection, DerivedRecordPublication, RealArtifactEntry,
    RealCheckpointSnapshot, RealExecutionEnvironmentSummary, RealExecutionOutputSnapshot,
    RealExecutionPromotionSnapshot, RealExecutionSnapshot, RealExecutionToolHint,
    RealExportMapSnapshot, RealOperationEffect, RealOperationRecord, RealProjectionMaterialization,
    RealProjectionMaterializationMetrics, RealProjectionMaterializationRequest,
    RealProjectionSnapshot, RealProjectionStrategy, RealRepoState, RealResolvedRepoView,
    RealSessionGenerationRecord, RealSessionRecord, RealTopicRecord, RealWorktreeAnchor,
    RepoStateError, TopicMetadataValidationError,
};
use sunlight_core::repository::{
    init_repository, resolve_projection_policy, ExecutionPolicy, RepositoryConfig,
    ResolvedProjectionPolicy, CURRENT_STORAGE_SCHEMA_VERSION, DISABLED_NETWORK_POLICY,
    NOT_ENFORCED_NETWORK_POLICY,
};
use sunlight_core::resolver::{
    fixture_auth_revision, fixture_base_entries, fixture_overlapping_auth_revision,
    fixture_profile_revision, fixture_profile_revision_missing_auth_dependency,
    fixture_resolver_input, resolve_fixture_view, DependencyClosure, DeterministicResolverOrder,
    ResolvedViewResult, ResolverConflictOrStalenessRecord, ResolverRecordKind, SingleRepoTree,
    TopicRevisionRef, TopicRevisionSelection, TreeEntryState, FIXTURE_BASE_CHECKPOINT_ID,
    FIXTURE_BASE_RESOLVED_VIEW_ID,
};
use sunlight_core::topics::{TopicSlug, PHASE1_SESSION_CAPABILITIES};

pub use engine::{
    execute_engine, EngineCommandInput, EngineContext, EngineOutputFormat, EngineRequest,
    EngineResponse,
};

const FIXTURE_STALE_COMPATIBILITY_PROJECTION_ID: &str =
    "projection_compat_agent_a_stale_baseline_0001";
const MAX_PROMOTABLE_EXECUTION_OUTPUT_BYTES: u64 = 2 * 1024 * 1024;

pub fn cli_main(args: Vec<String>, current_dir: PathBuf) -> ExitCode {
    #[cfg(windows)]
    if args.first().map(String::as_str) == Some("__sunlight-low-integrity-bootstrap") {
        windows_isolation::bootstrap(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("mcp") {
        return match mcp::serve_from_args(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(message) => {
                eprintln!("sun mcp: {message}");
                ExitCode::FAILURE
            }
        };
    }
    let output_format = if args.iter().any(|arg| arg == "--json") {
        EngineOutputFormat::Json
    } else {
        EngineOutputFormat::Human
    };
    let arguments = args
        .into_iter()
        .filter(|argument| argument != "--json")
        .collect::<Vec<_>>();
    let requested_root = arguments
        .windows(2)
        .find(|pair| pair[0] == "--repo")
        .map(|pair| {
            let path = PathBuf::from(&pair[1]);
            if path.is_absolute() {
                path
            } else {
                current_dir.join(path)
            }
        })
        .unwrap_or(current_dir);
    let context = match EngineContext::new(requested_root) {
        Ok(context) => context,
        Err(message) => {
            eprintln!("sun: {message}");
            return ExitCode::FAILURE;
        }
    };
    let response = execute_engine(
        &context,
        EngineRequest {
            command: EngineCommandInput::Arguments(arguments),
            output_format,
            max_stdout_bytes: None,
            max_stderr_bytes: None,
        },
    );
    if !response.stdout.is_empty() {
        std::print!("{}", response.stdout);
    }
    if !response.stderr.is_empty() {
        eprint!("{}", response.stderr);
    }
    if response.success {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[derive(Debug, Clone)]
struct CommandContext {
    json: bool,
    args: Vec<String>,
    repo_root: PathBuf,
    cancellation: Arc<AtomicBool>,
    output: Rc<RefCell<OutputBuffer>>,
}

#[derive(Debug, Clone)]
struct OutputBuffer {
    retained: String,
    max_bytes: Option<usize>,
    overflowed: bool,
}

impl OutputBuffer {
    fn new(max_bytes: Option<usize>) -> Self {
        Self {
            retained: String::new(),
            max_bytes,
            overflowed: false,
        }
    }

    fn into_parts(self) -> (String, bool) {
        (self.retained, self.overflowed)
    }
}

impl fmt::Write for OutputBuffer {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        let Some(max_bytes) = self.max_bytes else {
            self.retained.push_str(value);
            return Ok(());
        };
        let remaining = max_bytes.saturating_sub(self.retained.len());
        if value.len() <= remaining {
            self.retained.push_str(value);
            return Ok(());
        }

        self.overflowed = true;
        if remaining == 0 {
            return Ok(());
        }
        let mut end = remaining;
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        self.retained.push_str(&value[..end]);
        Ok(())
    }
}

impl CommandContext {
    fn command_file_path(&self, path: &str) -> PathBuf {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            path
        } else {
            self.repo_root.join(path)
        }
    }

    fn write_output(&self, arguments: fmt::Arguments<'_>) {
        use fmt::Write as _;
        self.output
            .borrow_mut()
            .write_fmt(arguments)
            .expect("writing to a String cannot fail");
    }

    fn write_output_line(&self, arguments: fmt::Arguments<'_>) {
        use fmt::Write as _;
        let mut output = self.output.borrow_mut();
        output
            .write_fmt(arguments)
            .expect("writing engine output cannot fail");
        output
            .write_char('\n')
            .expect("writing engine output cannot fail");
    }
}

macro_rules! output {
    ($ctx:expr, $($arg:tt)*) => {
        $ctx.write_output(format_args!($($arg)*))
    };
}

macro_rules! outputln {
    ($ctx:expr) => {
        $ctx.write_output_line(format_args!(""))
    };
    ($ctx:expr, $($arg:tt)*) => {
        $ctx.write_output_line(format_args!($($arg)*))
    };
}

#[derive(Debug, Clone)]
struct CliError {
    code: &'static str,
    message: String,
    details: Vec<(&'static str, String)>,
    raw_details_json: Option<String>,
}

impl CliError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Vec::new(),
            raw_details_json: None,
        }
    }

    fn with_detail(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.details.push((key, value.into()));
        self
    }

    fn with_raw_details_json(mut self, value: impl Into<String>) -> Self {
        self.raw_details_json = Some(value.into());
        self
    }
}

impl From<RepoStateError> for CliError {
    fn from(error: RepoStateError) -> Self {
        match error {
            RepoStateError::NotInitialized { path } => {
                CliError::new("not_initialized", "Sunlight repository is not initialized")
                    .with_detail("missing", path.display().to_string())
            }
            RepoStateError::InvalidState { path, message }
                if message == "projection root must be an empty directory or a creatable path" =>
            {
                CliError::new(
                    "projection_materialization_projection_root_unavailable",
                    message,
                )
                .with_detail("path", path.display().to_string())
            }
            RepoStateError::InvalidState { path, message } => {
                invalid_request(message).with_detail("path", path.display().to_string())
            }
            RepoStateError::Io { path, message } => CliError::new("repository_io_failed", message)
                .with_detail("path", path.display().to_string()),
            RepoStateError::Json(message) => invalid_request(message),
            RepoStateError::Recovery {
                canonical,
                staged,
                backup,
                journal,
                message,
            } => CliError::new("state_recovery_failed", message)
                .with_detail("canonical", canonical.display().to_string())
                .with_detail("staged", staged.display().to_string())
                .with_detail("backup", backup.display().to_string())
                .with_detail("journal", journal.display().to_string()),
            RepoStateError::PublicationRecovery { manifest, message } => {
                CliError::new("publication_outbox_recovery_failed", message)
                    .with_detail("manifest", manifest.display().to_string())
            }
            RepoStateError::WriterBusy { lock, timeout_ms } => CliError::new(
                "repository_writer_busy",
                "another Sunlight command is publishing or recovering repository state",
            )
            .with_detail("lock", lock.display().to_string())
            .with_detail("timeout_ms", timeout_ms.to_string()),
            RepoStateError::ConcurrentStateUpdate {
                path,
                expected_sequence,
                actual_sequence,
            } => CliError::new(
                "concurrent_state_update",
                "repository state changed after this command loaded it; reload and retry",
            )
            .with_detail("path", path.display().to_string())
            .with_detail("expected_sequence", expected_sequence.to_string())
            .with_detail(
                "actual_sequence",
                actual_sequence
                    .map(|sequence| sequence.to_string())
                    .unwrap_or_else(|| "missing".to_string()),
            ),
            RepoStateError::SunignorePolicyChanged {
                path,
                expected_hash,
                actual_hash,
            } => CliError::new(
                "sunignore_policy_changed",
                ".sunignore changed after Sunlight initialization, so persisted views cannot be exposed safely",
            )
            .with_detail("path", path.display().to_string())
            .with_detail(
                "expected_hash",
                expected_hash.unwrap_or_else(|| "absent".to_string()),
            )
            .with_detail(
                "actual_hash",
                actual_hash.unwrap_or_else(|| "absent".to_string()),
            )
            .with_detail(
                "recovery",
                "Run sun init. A repository without authored Sunlight history is refreshed automatically; otherwise restore the prior .sunignore, preserve or export authored work, then back up and reinitialize .sunlight.",
            ),
            RepoStateError::ProjectionStrategyUnsupported {
                strategy,
                path,
                reason,
            } => CliError::new(
                "projection_materialization_unsupported_filesystem_strategy",
                "required projection materialization strategy is unsupported",
            )
            .with_detail("strategy", strategy)
            .with_detail("path", path.display().to_string())
            .with_detail("reason", reason),
            RepoStateError::ProjectionCacheIntegrity {
                cache_key,
                path,
                reason,
            } => CliError::new(
                "projection_cache_integrity_failed",
                "projection cache integrity validation or quarantine failed",
            )
            .with_detail("cache_key", cache_key)
            .with_detail("path", path.display().to_string())
            .with_detail("reason", reason),
        }
    }
}

fn run(ctx: &CommandContext) -> Result<(), CliError> {
    match ctx.args.as_slice() {
        [] => {
            print_help(ctx);
            Ok(())
        }
        [flag] if flag == "--help" || flag == "-h" => {
            print_help(ctx);
            Ok(())
        }
        [command] if command == "init" => init(ctx, ctx.repo_root.clone()),
        [command, flag, _path] if command == "init" && flag == "--repo" => {
            init(ctx, ctx.repo_root.clone())
        }
        [command, flag] if command == "init" && flag == "--help" => {
            print_init_help(ctx);
            Ok(())
        }
        [command, flag] if flag == "--help" || flag == "-h" => {
            print_command_help(ctx, command);
            Ok(())
        }
        [scope, command, flag] if flag == "--help" || flag == "-h" => {
            print_command_help(ctx, &format!("{scope} {command}"));
            Ok(())
        }
        [command, ..] if command == "init" => {
            Err(invalid_request("usage: sun init [--repo <path>]"))
        }
        [scope, command, ..] if scope == "agent" && command == "install" => agent_install(ctx),
        [scope, command, ..] if scope == "agent" && command == "doctor" => agent_doctor(ctx),
        [scope, command, ..] if scope == "topic" && command == "create" => topic_create(&ctx),
        [scope, command, ..] if scope == "topic" && command == "complete" => topic_complete(&ctx),
        [scope, command, ..] if scope == "session" && command == "start" => session_start(&ctx),
        [scope, command, ..] if scope == "session" && command == "refresh" => session_refresh(&ctx),
        [scope, command, ..] if scope == "view" && command == "resolve" => view_resolve(&ctx),
        [scope, command, ..] if scope == "project" && command == "materialize" => {
            project_materialize(&ctx)
        }
        [scope, command, ..] if scope == "projection" && command == "create" => {
            project_materialize(&ctx)
        }
        [scope, command, ..] if scope == "projection" && command == "quarantine-cleanup" => {
            projection_quarantine_cleanup(&ctx)
        }
        [scope, command, ..] if scope == "checkpoint" && command == "create" => {
            checkpoint_create(&ctx)
        }
        [scope, command, ..] if scope == "policy" && command == "check-export" => {
            policy_check_export(&ctx)
        }
        [scope, command, ..] if scope == "policy" && command == "explain" => policy_explain(&ctx),
        [scope, command, ..] if scope == "policy" && command == "check-commit" => {
            policy_check_commit(&ctx)
        }
        [scope, command, ..] if scope == "git" && command == "export" => git_export(&ctx),
        [scope, command, ..] if scope == "compat" && command == "project" => compat_project(&ctx),
        [scope, command, ..] if scope == "compat" && command == "diff" => compat_diff(&ctx),
        [scope, command, ..] if scope == "compat" && command == "import" => compat_import(&ctx),
        [scope, command, ..] if scope == "compat" && command == "capture" => worktree_capture(&ctx),
        [scope, command, ..] if scope == "execution" && command == "promote-output" => {
            execution_promote_output(&ctx)
        }
        [command, ..] if command == "run" => execution_run(&ctx),
        [command, ..] if command == "read" => artifact_read(&ctx),
        [command, ..] if command == "list" => artifact_list(&ctx),
        [command, ..] if command == "search" => artifact_search(&ctx),
        [command, ..] if command == "patch" => artifact_patch(&ctx),
        [command, ..] if command == "write" => artifact_write(&ctx),
        [command, ..] if command == "move" => artifact_move(&ctx),
        [command, ..] if command == "delete" => artifact_delete(&ctx),
        [scope, command, ..] if scope == "metadata" && command == "set" => {
            artifact_metadata_set(&ctx)
        }
        [command, ..] if command == "status" => status(&ctx),
        [command, ..] if command == "inspect" => inspect(&ctx),
        [command, ..] => Err(invalid_request(format!("unknown command `{command}`"))
            .with_detail("command", command.clone())),
    }
}

fn agent_install(ctx: &CommandContext) -> Result<(), CliError> {
    let (client, force) = parse_agent_options(ctx, true)?;
    let executable = env::current_exe().map_err(|error| {
        invalid_request(format!(
            "cannot resolve the running sun executable: {error}"
        ))
    })?;
    let report =
        agent_setup::install(&ctx.repo_root, &executable, client, force).map_err(|error| {
            invalid_request(error.to_string()).with_detail("command", "agent.install")
        })?;
    let repository = agent_setup::display_path(&ctx.repo_root);
    let generic_warning = (report.client == agent_setup::AgentClient::Generic).then_some(
        "generic installs the portable skill only; configure a client transport that runs `sun mcp serve --repo <repository>` separately",
    );

    if ctx.json {
        outputln!(
            ctx,
            "{}",
            serde_json::json!({
                "ok": true,
                "data": {
                    "command": "agent.install",
                    "repository": repository,
                    "client": report.client.as_str(),
                    "changed": report.changed,
                    "unchanged": report.unchanged,
                    "restart_required": report.restart_required,
                    "next_command": format!("sun agent doctor --client {} --repo \"{}\"", report.client.as_str(), repository),
                },
                "warnings": generic_warning.into_iter().collect::<Vec<_>>()
            })
        );
    } else {
        outputln!(
            ctx,
            "Sunlight agent setup installed for {} in {}",
            report.client.as_str(),
            repository
        );
        for path in &report.changed {
            outputln!(ctx, "  changed: {path}");
        }
        for path in &report.unchanged {
            outputln!(ctx, "  current: {path}");
        }
        if report.restart_required {
            outputln!(
                ctx,
                "Restart or reload the client before verifying MCP tools."
            );
        }
        if report.client == agent_setup::AgentClient::Generic {
            outputln!(
                ctx,
                "Generic setup installed the portable skill only. Configure the client to run `sun mcp serve --repo \"{}\"`, then confirm repository_status and artifact tools are visible.",
                repository
            );
        }
        outputln!(
            ctx,
            "Verify: sun agent doctor --client {} --repo \"{}\"",
            report.client.as_str(),
            repository
        );
    }
    Ok(())
}

fn agent_doctor(ctx: &CommandContext) -> Result<(), CliError> {
    let (client, _) = parse_agent_options(ctx, false)?;
    let executable = env::current_exe().map_err(|error| {
        invalid_request(format!(
            "cannot resolve the running sun executable: {error}"
        ))
    })?;
    let report = agent_setup::doctor(&ctx.repo_root, &executable, client);
    let repository = agent_setup::display_path(&ctx.repo_root);
    if !report.healthy {
        return Err(CliError::new(
            "agent_setup_incomplete",
            "Sunlight agent setup is missing or stale; rerun agent install with --force after reviewing managed files",
        )
        .with_raw_details_json(
            serde_json::json!({
                "client": report.client.as_str(),
                "repository": repository,
                "current": report.current,
                "missing_or_stale": report.missing_or_stale,
                "mcp_binding_verified": report.mcp_binding_verified,
                "repository_initialized": report.repository_initialized,
            })
            .to_string(),
        ));
    }

    let generic = report.client == agent_setup::AgentClient::Generic;
    let next_action = if generic {
        format!(
            "configure the client to run `sun mcp serve --repo \"{repository}\"`, restart it, and confirm repository_status and artifact tools are visible"
        )
    } else if report.repository_initialized {
        "restart the client and confirm repository_status and artifact tools are visible"
            .to_string()
    } else {
        "restart the client, confirm repository_init is visible, initialize the repository, and then confirm repository_status and artifact tools are visible".to_string()
    };
    let warnings = if generic {
        vec!["generic doctor verified the portable skill only; it did not verify any MCP client configuration or live tool visibility".to_string()]
    } else if report.repository_initialized {
        Vec::new()
    } else {
        vec![
            "repository is not initialized; the verified bound MCP server can initialize it"
                .to_string(),
        ]
    };

    if ctx.json {
        outputln!(
            ctx,
            "{}",
            serde_json::json!({
                "ok": true,
                "data": {
                    "command": "agent.doctor",
                    "repository": repository,
                    "client": report.client.as_str(),
                    "healthy": true,
                    "mcp_binding_verified": report.mcp_binding_verified,
                    "repository_initialized": report.repository_initialized,
                    "current": report.current,
                    "missing_or_stale": report.missing_or_stale,
                    "next_action": next_action,
                },
                "warnings": warnings
            })
        );
    } else {
        outputln!(
            ctx,
            "Sunlight agent files are current for {} in {}",
            report.client.as_str(),
            repository
        );
        outputln!(ctx, "  portable skill: current");
        outputln!(
            ctx,
            "  MCP binding: {}",
            if report.mcp_binding_verified {
                "verified"
            } else {
                "not verified by generic doctor"
            }
        );
        outputln!(
            ctx,
            "  repository: {}",
            if report.repository_initialized {
                "initialized"
            } else {
                "ready for repository_init"
            }
        );
        outputln!(ctx, "  Next: {next_action}");
    }
    Ok(())
}

fn parse_agent_options(
    ctx: &CommandContext,
    allow_force: bool,
) -> Result<(agent_setup::AgentClient, bool), CliError> {
    let mut client = None;
    let mut force = false;
    let mut args = ctx.args.iter().skip(2);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--client" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("sun agent requires --client generic|codex|cursor")
                })?;
                client = Some(
                    agent_setup::AgentClient::parse(value)
                        .map_err(|error| invalid_request(error.to_string()))?,
                );
            }
            "--force" if allow_force => force = true,
            "--repo" => {
                args.next().ok_or_else(|| {
                    invalid_request("sun agent --repo requires a repository path")
                })?;
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun agent"
                )))
            }
            value => {
                return Err(invalid_request(format!(
                    "unexpected argument `{value}` for sun agent"
                )))
            }
        }
    }
    let client = client
        .ok_or_else(|| invalid_request("sun agent requires --client generic|codex|cursor"))?;
    Ok((client, force))
}

fn init(ctx: &CommandContext, repo_root: PathBuf) -> Result<(), CliError> {
    let report = init_repository(&repo_root).map_err(|error| {
        invalid_request(error.to_string()).with_detail("command", "repository.init")
    })?;
    let mut legacy_quarantine_migrated = false;
    let mut sunignore_policy_refreshed = false;
    let state_exists = match RealRepoState::load_for_reinitialization(&report.repo_root) {
        Ok(state) => {
            if state.repository_id != report.repository_id {
                return Err(invalid_request(
                    "persisted Sunlight state belongs to a different repository",
                )
                .with_detail("config_repository_id", report.repository_id.clone())
                .with_detail("state_repository_id", state.repository_id));
            }
            let sunignore_policy_changed = state.sunignore_policy_changed(&report.repo_root)?;
            if !state.quarantine.is_empty() || sunignore_policy_changed {
                let has_authored_state = !state.topics.is_empty()
                    || !state.sessions.is_empty()
                    || !state.operations.is_empty()
                    || !state.projections.is_empty()
                    || !state.executions.is_empty()
                    || !state.promotions.is_empty()
                    || !state.checkpoints.is_empty()
                    || !state.export_maps.is_empty();
                if has_authored_state {
                    if sunignore_policy_changed {
                        return Err(CliError::new(
                            "sunignore_policy_change_blocked",
                            ".sunignore changed after initialization in a repository with authored Sunlight history",
                        )
                        .with_detail("policy_file", ".sunignore")
                        .with_detail("state_preserved", "true")
                        .with_detail("recovery", "Restore the previous .sunignore to continue using this state. To adopt the new policy, first preserve or export authored work, back up .sunlight, then remove only .sunlight and rerun sun init."));
                    }
                    return Err(CliError::new(
                        "legacy_quarantine_migration_blocked",
                        "legacy secret quarantine excluded source from a repository that now contains authored Sunlight history",
                    )
                    .with_detail("excluded_path_count", state.quarantine.len().to_string())
                    .with_detail("recovery", "Preserve or export needed historical work, back up .sunlight, then remove only .sunlight and rerun sun init with this build."));
                }
                let mut ingest = RealRepoState::ingest(&report.repo_root, &report.repository_id)?;
                ingest.publication_sequence = state.publication_sequence;
                ingest.worktree_anchor.generation = state
                    .worktree_anchor
                    .generation
                    .checked_add(1)
                    .ok_or_else(|| {
                        invalid_request(
                            "worktree anchor generation overflow during reinitialization",
                        )
                    })?;
                let records = base_ingest_publications(&ingest)?;
                ingest.save_with_records(&report.repo_root, &records)?;
                legacy_quarantine_migrated = !state.quarantine.is_empty();
                sunignore_policy_refreshed = sunignore_policy_changed;
            }
            true
        }
        Err(RepoStateError::NotInitialized { .. }) => false,
        Err(error) => return Err(error.into()),
    };
    if !state_exists {
        let ingest = RealRepoState::ingest(&report.repo_root, &report.repository_id)?;
        let records = base_ingest_publications(&ingest)?;
        match ingest.save_with_records(&report.repo_root, &records) {
            Ok(()) => {}
            Err(RepoStateError::ConcurrentStateUpdate { .. }) => {
                let state = RealRepoState::load(&report.repo_root)?;
                if state.repository_id != report.repository_id {
                    return Err(invalid_request(
                        "persisted Sunlight state belongs to a different repository",
                    )
                    .with_detail("config_repository_id", report.repository_id.clone())
                    .with_detail("state_repository_id", state.repository_id));
                }
            }
            Err(error) => return Err(error.into()),
        }
    }

    if ctx.json {
        outputln!(
            ctx,
            "{}",
            init_success_envelope(
                &report.repository_id,
                &report.repo_root.display().to_string(),
                &report.sunlight_dir.display().to_string(),
                report.created_config,
                report.created_gitignore,
                report.created_directories.len(),
                0,
                legacy_quarantine_migrated,
                sunignore_policy_refreshed,
            )
        );
    } else {
        outputln!(ctx, "initialized Sunlight repository");
        outputln!(ctx, "repo_root = {}", report.repo_root.display());
        outputln!(ctx, "sunlight_dir = {}", report.sunlight_dir.display());
        outputln!(ctx, "repository_id = {}", report.repository_id);
        outputln!(ctx, "created_config = {}", report.created_config);
        outputln!(ctx, "created_gitignore = {}", report.created_gitignore);
        outputln!(
            ctx,
            "created_directories = {}",
            report.created_directories.len()
        );
        if legacy_quarantine_migrated {
            outputln!(ctx, "legacy_quarantine_migrated = true");
        }
        if sunignore_policy_refreshed {
            outputln!(ctx, "sunignore_policy_refreshed = true");
        }
    }

    Ok(())
}

fn base_ingest_publications(
    ingest: &RealRepoState,
) -> Result<Vec<DerivedRecordPublication>, CliError> {
    Ok(vec![
        ingest.record_publication(
            "checkpoints",
            &ingest.base_checkpoint_id,
            &format!(
                "{{\"record_type\":\"checkpoint\",\"id\":\"{}\",\"repository_id\":\"{}\",\"resolved_view_id\":\"{}\",\"tree_hash\":\"{}\",\"source\":\"worktree_ingest\"}}\n",
                json_escape(&ingest.base_checkpoint_id),
                json_escape(&ingest.repository_id),
                json_escape(&ingest.base_resolved_view_id),
                json_escape(&ingest.tree_hash),
            ),
        )?,
        ingest.record_publication(
            "views",
            &ingest.base_resolved_view_id,
            &format!(
                "{{\"record_type\":\"resolved_view\",\"id\":\"{}\",\"repository_id\":\"{}\",\"base_checkpoint_ids\":[\"{}\"],\"tree_hash\":\"{}\"}}\n",
                json_escape(&ingest.base_resolved_view_id),
                json_escape(&ingest.repository_id),
                json_escape(&ingest.base_checkpoint_id),
                json_escape(&ingest.tree_hash),
            ),
        )?,
        real_worktree_anchor_publication(ingest)?,
    ])
}

fn real_worktree_anchor_publication(
    state: &RealRepoState,
) -> Result<DerivedRecordPublication, CliError> {
    let anchor = &state.worktree_anchor;
    state
        .record_publication(
            "worktree-anchors",
            &anchor.id(),
            &format!("{}\n", real_worktree_anchor_record_json(state, anchor)),
        )
        .map_err(Into::into)
}

fn real_worktree_anchor_record_json(state: &RealRepoState, anchor: &RealWorktreeAnchor) -> String {
    format!(
        "{{\"schema_version\":1,\"record_type\":\"worktree_anchor\",\"id\":\"{}\",\"repository_id\":\"{}\",\"generation\":{},\"base_checkpoint_ids\":{},\"resolved_view_id\":\"{}\",\"tree_hash\":\"{}\",\"topic_frontier\":{},\"manifest_digest\":\"{}\",\"path_policy_id\":\"{}\",\"operation_semantics_version\":\"{}\",\"sunignore_policy_hash\":{},\"privacy_class\":\"local_only\",\"source_truth\":\"sunlight_persisted_resolved_view\"}}",
        json_escape(&anchor.id()),
        json_escape(&state.repository_id),
        anchor.generation,
        string_array_json(anchor.base_checkpoint_ids.iter().map(String::as_str)),
        json_escape(&anchor.resolved_view_id),
        json_escape(&anchor.tree_hash),
        string_map_object_json(&anchor.topic_frontier),
        json_escape(&anchor.manifest_digest),
        json_escape(&anchor.path_policy_id),
        json_escape(&anchor.operation_semantics_version),
        optional_string_json(anchor.sunignore_policy_hash.as_deref()),
    )
}

fn topic_create(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_topic_create_options(ctx)?;
    if let Some(fixture) = &options.fixture {
        ensure_basic_app_fixture(fixture)?;
    } else {
        return real_topic_create(ctx, options);
    }
    TopicSlug::new(options.slug.clone()).map_err(|error| {
        invalid_request(error.to_string()).with_detail("slug", options.slug.clone())
    })?;

    if options.slug != "auth-nullability" {
        return Err(invalid_request(
            "fixture basic-app supports only topic slug `auth-nullability`",
        )
        .with_detail("slug", options.slug));
    }

    if ctx.json {
        outputln!(
            ctx,
            "{}",
            topic_create_success_envelope(&options.display_name)
        );
    } else {
        outputln!(ctx, "created topic {}", FIXTURE_WRITE_TOPIC_ID);
    }

    Ok(())
}

fn topic_complete(ctx: &CommandContext) -> Result<(), CliError> {
    real_topic_complete(ctx, parse_topic_complete_options(ctx)?)
}

fn session_start(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_session_start_options(ctx)?;
    if let Some(fixture) = &options.fixture {
        ensure_basic_app_fixture(fixture)?;
    } else {
        return real_session_start(ctx, options);
    }
    ensure_fixture_topic(&options.topic)?;
    if options.actor_id != FIXTURE_ACTOR_ID {
        return Err(invalid_request(
            "fixture basic-app supports only actor `agent_a` for session start",
        )
        .with_detail("actor_id", options.actor_id));
    }
    fixture_resolved_view_by_id(&options.view_id)
        .ok_or_else(|| object_not_found("view", &options.view_id))?;

    if ctx.json {
        outputln!(ctx, "{}", session_start_success_envelope(&options.view_id));
    } else {
        outputln!(ctx, "started session {}", FIXTURE_SESSION_ID);
    }

    Ok(())
}

fn session_refresh(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_session_refresh_options(ctx)?;
    real_session_refresh(ctx, options)
}

fn artifact_read(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_artifact_options(ctx, "read", 1, 1)?;
    let Some(fixture) = &options.fixture else {
        return real_artifact_read(ctx, options);
    };
    let store = fixture_store(fixture)?;
    let session_id = options
        .session_id
        .as_deref()
        .expect("fixture artifact reads require a session");
    let response = store
        .read(session_id, &options.operands[0])
        .map_err(artifact_error)?;

    if ctx.json {
        outputln!(ctx, "{}", read_success_envelope(&response));
    } else {
        output!(ctx, "{}", response.content.bytes);
    }

    Ok(())
}

fn artifact_list(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_artifact_options(ctx, "list", 0, 1)?;
    let prefix = options.operands.first().map(String::as_str).unwrap_or("");
    let Some(fixture) = &options.fixture else {
        return real_artifact_list(ctx, options);
    };
    let store = fixture_store(fixture)?;
    let session_id = options
        .session_id
        .as_deref()
        .expect("fixture artifact reads require a session");
    let response = store.list(session_id, prefix).map_err(artifact_error)?;

    if ctx.json {
        outputln!(ctx, "{}", list_success_envelope(&response));
    } else {
        for artifact in response.artifacts {
            outputln!(ctx, "{}", artifact.path);
        }
    }

    Ok(())
}

fn artifact_search(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_artifact_options(ctx, "search", 1, 1)?;
    let Some(fixture) = &options.fixture else {
        return real_artifact_search(ctx, options);
    };
    let store = fixture_store(fixture)?;
    let session_id = options
        .session_id
        .as_deref()
        .expect("fixture artifact reads require a session");
    let response = store
        .search(session_id, &options.operands[0])
        .map_err(artifact_error)?;

    if ctx.json {
        outputln!(ctx, "{}", search_success_envelope(&response));
    } else {
        for item in response.matches {
            outputln!(ctx, "{}:{}:{}", item.path, item.line, item.snippet);
        }
    }

    Ok(())
}

fn artifact_patch(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_mutation_options(ctx, "patch", 1)?;
    let expect_hash = options
        .expect_hash
        .clone()
        .ok_or_else(|| invalid_request("usage: sun patch requires --expect-hash <hash>"))?;
    let patch_file = options
        .patch_file
        .clone()
        .ok_or_else(|| invalid_request("usage: sun patch requires --patch-file <file>"))?;
    let patch = fs::read_to_string(ctx.command_file_path(&patch_file)).map_err(|error| {
        invalid_request(format!("failed to read patch file `{patch_file}`"))
            .with_detail("source", error.to_string())
    })?;
    let Some(fixture) = &options.fixture else {
        return real_artifact_patch(ctx, options, patch);
    };
    let mut store = fixture_store(fixture)?;
    let response = store
        .patch(PatchRequest {
            session_id: options.session_id,
            path: options.operands[0].clone(),
            expected_hash: expect_hash,
            patch,
        })
        .map_err(artifact_error)?;

    if ctx.json {
        outputln!(ctx, "{}", mutation_success_envelope(&response));
    } else {
        outputln!(
            ctx,
            "patched {} {} -> {}",
            response.artifact.path,
            response.artifact.before_hash.as_deref().unwrap_or("new"),
            response.artifact.after_hash
        );
    }

    Ok(())
}

fn artifact_write(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_mutation_options(ctx, "write", 1)?;
    let expect_hash = options
        .expect_hash
        .clone()
        .ok_or_else(|| invalid_request("usage: sun write requires --expect-hash <hash-or-new>"))?;
    let content_file = options
        .content_file
        .clone()
        .ok_or_else(|| invalid_request("usage: sun write requires --content-file <file>"))?;
    let classification = options.classification.clone().ok_or_else(|| {
        invalid_request("usage: sun write requires --classification source|generated")
    })?;
    let content = fs::read(ctx.command_file_path(&content_file)).map_err(|error| {
        invalid_request(format!("failed to read content file `{content_file}`"))
            .with_detail("source", error.to_string())
    })?;
    let expected_hash = if expect_hash == "new" {
        ExpectedHash::New
    } else {
        ExpectedHash::Existing(expect_hash)
    };
    let Some(fixture) = &options.fixture else {
        return real_artifact_write(ctx, options, content, expected_hash);
    };
    let mut store = fixture_store(fixture)?;
    let response = store
        .write(WriteRequest {
            session_id: options.session_id,
            path: options.operands[0].clone(),
            expected_hash,
            content,
            classification,
            executable: false,
            media_type: "text/plain; charset=utf-8".to_string(),
        })
        .map_err(artifact_error)?;

    if ctx.json {
        outputln!(ctx, "{}", mutation_success_envelope(&response));
    } else {
        outputln!(
            ctx,
            "wrote {} {}",
            response.artifact.path,
            response.artifact.after_hash
        );
    }

    Ok(())
}

fn artifact_move(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_mutation_options(ctx, "move", 2)?;
    let expect_hash = options
        .expect_hash
        .clone()
        .ok_or_else(|| invalid_request("usage: sun move requires --expect-hash <hash>"))?;
    let Some(fixture) = &options.fixture else {
        return real_artifact_move(ctx, options, expect_hash);
    };
    let mut store = fixture_store(fixture)?;
    let response = store
        .move_path(MoveRequest {
            session_id: options.session_id,
            source_path: options.operands[0].clone(),
            target_path: options.operands[1].clone(),
            expected_hash: expect_hash,
        })
        .map_err(artifact_error)?;

    if ctx.json {
        outputln!(ctx, "{}", mutation_success_envelope(&response));
    } else {
        outputln!(ctx, "moved {}", response.artifact.path);
    }

    Ok(())
}

fn artifact_delete(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_mutation_options(ctx, "delete", 1)?;
    let expect_hash = options
        .expect_hash
        .clone()
        .ok_or_else(|| invalid_request("usage: sun delete requires --expect-hash <hash>"))?;
    let Some(fixture) = &options.fixture else {
        return real_artifact_delete(ctx, options, expect_hash);
    };
    let mut store = fixture_store(fixture)?;
    let response = store
        .delete_path(DeleteRequest {
            session_id: options.session_id,
            path: options.operands[0].clone(),
            expected_hash: expect_hash,
        })
        .map_err(artifact_error)?;

    if ctx.json {
        outputln!(ctx, "{}", mutation_success_envelope(&response));
    } else {
        outputln!(ctx, "deleted {}", response.artifact.path);
    }

    Ok(())
}

fn artifact_metadata_set(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_mutation_options_with_skip(ctx, "metadata set", 1, 2)?;
    let expect_hash = options
        .expect_hash
        .clone()
        .ok_or_else(|| invalid_request("usage: sun metadata set requires --expect-hash <hash>"))?;
    let classification = options.classification.clone().ok_or_else(|| {
        invalid_request("usage: sun metadata set requires --classification source|generated")
    })?;
    let Some(fixture) = &options.fixture else {
        return real_artifact_metadata_set(ctx, options, expect_hash, classification);
    };
    let mut store = fixture_store(fixture)?;
    let response = store
        .metadata_set(MetadataSetRequest {
            session_id: options.session_id,
            path: options.operands[0].clone(),
            expected_hash: expect_hash,
            classification,
        })
        .map_err(artifact_error)?;

    if ctx.json {
        outputln!(ctx, "{}", mutation_success_envelope(&response));
    } else {
        outputln!(ctx, "updated metadata {}", response.artifact.path);
    }

    Ok(())
}

fn view_resolve(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_view_resolve_options(ctx)?;
    let Some(fixture) = &options.fixture else {
        return real_view_resolve(ctx, options);
    };
    if fixture != "basic-app" {
        return Err(invalid_request(format!("unknown fixture `{}`", fixture))
            .with_detail("fixture", fixture.clone()));
    }

    if let Some(base_checkpoint_id) = &options.base_checkpoint_id {
        if base_checkpoint_id != "checkpoint_base_0001" {
            return Err(invalid_request(format!(
                "unknown fixture base checkpoint `{base_checkpoint_id}`"
            ))
            .with_detail("base_checkpoint_id", base_checkpoint_id));
        }
    }

    let revisions = fixture_resolver_revisions();
    let mut frontier = Vec::new();
    for selection in options.include {
        let revision = revisions
            .iter()
            .find(|revision| revision.revision_id == selection.revision_id)
            .ok_or_else(|| object_not_found("revision", &selection.revision_id))?;
        if revision.topic_id != selection.topic_id {
            return Err(invalid_request(format!(
                "revision `{}` does not belong to topic `{}`",
                selection.revision_id, selection.topic_id
            ))
            .with_detail("topic_id", selection.topic_id)
            .with_detail("topic_revision_id", selection.revision_id));
        }
        frontier.push(selection);
    }

    let requested_frontier = exact_requested_frontier(&frontier)?;
    let result = resolve_fixture_view(
        fixture_resolver_input(frontier),
        fixture_base_entries(),
        revisions,
    );
    ensure_resolver_preserved_frontier(&requested_frontier, &result.topic_frontier)?;

    if ctx.json {
        outputln!(
            ctx,
            "{}",
            view_resolve_success_envelope(
                &result,
                &requested_frontier,
                "checkpoint_base_0001",
                &BTreeMap::new(),
            )
        );
    } else if result.conflict_free() {
        let tree_hash = result
            .tree_identity
            .as_ref()
            .map(|tree| tree.tree_hash.as_str())
            .unwrap_or("unavailable");
        outputln!(ctx, "{} {}", result.resolved_view_id, tree_hash);
    } else {
        outputln!(ctx, "{} blocked", result.resolved_view_id);
    }

    Ok(())
}

fn execution_run(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_execution_run_options(ctx)?;
    if options.fixture.is_none() {
        return real_execution_run(ctx, options);
    }
    let fixture = options.fixture.clone().unwrap();
    if fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{fixture}`")).with_detail("fixture", fixture)
        );
    }
    if options.command_argv.as_slice() != ["cargo", "test"]
        && options.command_argv.as_slice() != ["cargo", "test", "--fixture-fail"]
    {
        return Err(invalid_request(
            "fixture execution supports only `-- cargo test` or `-- cargo test --fixture-fail`",
        ));
    }

    let view = fixture_resolved_view_by_id(&options.view_id)
        .ok_or_else(|| object_not_found("resolved_view", &options.view_id))?;
    if let Some(integrity_fixture) = options.integrity_fixture {
        let projection =
            fixture_execution_projection_from_resolved_view(&view).map_err(projection_error)?;
        ensure_store_integrity_fixture_scope(&projection, options.integrity_fixture)?;
        let (manifest, blobs) = fixture_execution_projection_manifest_for_view(&projection, &view)?;
        let integrity = match integrity_fixture {
            StoreIntegrityFixture::Verified => {
                projection_store_integrity_from_manifest_scan(&projection, &manifest, &blobs)
            }
            StoreIntegrityFixture::ScanMissingBlob | StoreIntegrityFixture::StoreMismatch => {
                fixture_projection_store_integrity_result(
                    &projection,
                    &manifest,
                    Some(integrity_fixture),
                )
            }
        };
        if integrity.integrity_status == ProjectionStoreIntegrityStatus::Failed {
            return Err(execution_store_integrity_error(
                &projection,
                &integrity,
                integrity_fixture,
            ));
        }
    }
    let execution = if options.command_argv.as_slice() == ["cargo", "test", "--fixture-fail"] {
        fixture_failing_execution_from_resolved_view(&view)
    } else {
        fixture_passing_execution_from_resolved_view(&view)
    }
    .map_err(execution_error)?;

    if ctx.json {
        outputln!(ctx, "{}", execution_run_success_envelope(&execution));
    } else {
        outputln!(ctx, "{} {}", execution.id, execution.result.status.as_str());
    }

    Ok(())
}

fn execution_promote_output(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_execution_promote_output_options(ctx)?;
    if options.fixture.is_none() {
        return real_execution_promote_output(ctx, options);
    }
    let fixture = options.fixture.clone().unwrap();
    if fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{fixture}`")).with_detail("fixture", fixture)
        );
    }
    if options.execution_id != FIXTURE_PASSING_EXECUTION_ID {
        return Err(promotion_error(
            "promotion_precondition_failed",
            "execution output promotion requires the passing fixture execution",
            &options.execution_id,
            options.path.as_deref(),
            options.session_id.as_deref(),
            options.classification.as_deref(),
        ));
    }

    let view = fixture_resolved_view(vec![fixture_auth_revision(), fixture_profile_revision()]);
    let execution = fixture_passing_execution_from_resolved_view(&view).map_err(execution_error)?;
    let candidate = fixture_promotion_candidate_provenance(&execution);

    if options.path.as_deref() != Some(candidate.output_path.as_str()) {
        return Err(promotion_error(
            "promotion_precondition_failed",
            "execution output path is not a declared fixture promotion candidate",
            &options.execution_id,
            options.path.as_deref(),
            options.session_id.as_deref(),
            options.classification.as_deref(),
        ));
    }
    if options.session_id.as_deref() != Some(FIXTURE_SESSION_ID) {
        return Err(promotion_error(
            "promotion_precondition_failed",
            "execution output promotion requires fixture session `session_agent_a`",
            &options.execution_id,
            options.path.as_deref(),
            options.session_id.as_deref(),
            options.classification.as_deref(),
        ));
    }
    if options.classification.as_deref() != Some(candidate.classification.as_str()) {
        return Err(promotion_classification_error(
            &options.execution_id,
            options.path.as_deref(),
            options.session_id.as_deref(),
            options.classification.as_deref(),
            candidate.classification.as_str(),
        ));
    }

    let response = fixture_promotion_mutation_response(&candidate)?;

    if ctx.json {
        outputln!(ctx, "{}", promotion_success_envelope(&response, &candidate));
    } else {
        outputln!(
            ctx,
            "promoted {} {}",
            response.artifact.path,
            response.artifact.after_hash
        );
    }

    Ok(())
}

fn project_materialize(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_project_materialize_options(ctx)?;
    let Some(fixture) = &options.fixture else {
        return real_project_materialize(ctx, options);
    };
    if fixture != "basic-app" {
        return Err(invalid_request(format!("unknown fixture `{}`", fixture))
            .with_detail("fixture", fixture.clone()));
    }

    let view = fixture_resolved_view_by_id(&options.view_id)
        .ok_or_else(|| object_not_found("resolved_view", &options.view_id))?;

    if let Some(projection_root) = &options.projection_root {
        let store = InMemoryArtifactStore::fixture_basic_app();
        let materialization = materialize_fixture_projection_copy(
            &view,
            fixture_projection_materialization_request(&options),
            store.tree(),
            store.content_blobs(),
            projection_root,
        )
        .map_err(projection_materialization_error)?;

        if ctx.json {
            outputln!(
                ctx,
                "{}",
                projection_filesystem_materialize_success_envelope(&materialization)?
            );
        } else {
            outputln!(
                ctx,
                "{} {}",
                materialization.plan.projection.id,
                materialization.projection_root.display()
            );
        }

        return Ok(());
    }

    let plan = plan_fixture_projection_materialization(
        &view,
        fixture_projection_materialization_request(&options),
    )
    .map_err(projection_materialization_error)?;

    if ctx.json {
        outputln!(ctx, "{}", projection_materialize_success_envelope(&plan));
    } else {
        outputln!(
            ctx,
            "{} {}",
            plan.projection.id,
            plan.projection.root_ref.value
        );
    }

    Ok(())
}

fn projection_quarantine_cleanup(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_projection_quarantine_cleanup_options(ctx)?;
    if options.fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{}`", options.fixture))
                .with_detail("fixture", options.fixture),
        );
    }

    fixture_projection_by_id(&options.projection_id)
        .ok_or_else(|| object_not_found("projection", &options.projection_id))?
        .map_err(projection_error)?;

    let cleanup = cleanup_projection_quarantine_local_records(
        &options.projection_root,
        &options.projection_id,
    )
    .map_err(|error| {
        invalid_request(format!("projection quarantine cleanup failed: {}", error))
            .with_detail("projection_id", options.projection_id.clone())
    })?;

    if ctx.json {
        outputln!(
            ctx,
            "{}",
            projection_quarantine_cleanup_success_envelope(&cleanup)
        );
    } else {
        outputln!(
            ctx,
            "{} {}",
            cleanup.projection_id,
            cleanup.retention_state_after.as_str()
        );
    }

    Ok(())
}

fn checkpoint_create(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_checkpoint_create_options(ctx)?;
    let Some(fixture) = &options.fixture else {
        return real_checkpoint_create(ctx, options);
    };
    if fixture != "basic-app" {
        return Err(invalid_request(format!("unknown fixture `{}`", fixture))
            .with_detail("fixture", fixture.clone()));
    }

    let view = fixture_resolved_view_by_id(&options.view_id)
        .ok_or_else(|| object_not_found("resolved_view", &options.view_id))?;
    let execution = if view.conflict_free() {
        Some(fixture_passing_execution_from_resolved_view(&view).map_err(execution_error)?)
    } else {
        None
    };
    let checkpoint = fixture_checkpoint_from_resolved_view(&view, execution.as_ref())
        .map_err(checkpoint_error)?;

    if ctx.json {
        outputln!(ctx, "{}", checkpoint_create_success_envelope(&checkpoint));
    } else {
        outputln!(ctx, "{} {}", checkpoint.id, checkpoint.resolved_view_id);
    }

    Ok(())
}

fn git_export(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_git_export_options(ctx)?;
    let Some(fixture) = &options.fixture else {
        return real_git_export(ctx, options);
    };
    if fixture != "basic-app" {
        return Err(invalid_request(format!("unknown fixture `{}`", fixture))
            .with_detail("fixture", fixture.clone()));
    }

    let checkpoint = fixture_checkpoint()?;
    if checkpoint.id != options.checkpoint_id {
        return Err(object_not_found("checkpoint", &options.checkpoint_id));
    }

    let mut request = GitExportRequest::from_checkpoint(&checkpoint);
    request.git_ref = options.git_ref.clone();
    apply_fixture_generated_output_export_gate(&mut request);
    if options.execute_local {
        let input = local_fixture_git_export_writer_input(&options, request)?;
        let content_files = fixture_git_export_content_files();
        let result = if options.simulate_export_map_write_failure {
            let mut store = FailingGitExportMapStore;
            execute_local_git_export_writer(input, content_files, &mut store)
                .map_err(git_export_planning_error)?
        } else {
            let mut store = InMemoryGitExportMapStore::default();
            execute_local_git_export_writer(input, content_files, &mut store)
                .map_err(git_export_planning_error)?
        };

        if ctx.json {
            outputln!(ctx, "{}", git_export_execute_success_envelope(&result));
        } else {
            outputln!(
                ctx,
                "{} {}",
                result.checkpoint_id,
                result.lifecycle_state.as_str()
            );
        }

        return Ok(());
    }

    if let Some(execution_fixture) = options.execute_fixture {
        let input = fixture_git_export_writer_input(request);
        let plan = plan_git_export_writer(input).map_err(git_export_planning_error)?;
        let result = execute_git_export_writer_plan_fixture(&plan, execution_fixture.into());

        if ctx.json {
            outputln!(
                ctx,
                "{}",
                git_export_execute_fixture_success_envelope(&result)
            );
        } else {
            outputln!(
                ctx,
                "{} {}",
                result.checkpoint_id,
                result.lifecycle_state.as_str()
            );
        }

        return Ok(());
    }

    if options.write_plan {
        let input = fixture_git_export_writer_input(request);
        let plan = plan_git_export_writer(input).map_err(git_export_planning_error)?;

        if ctx.json {
            outputln!(ctx, "{}", git_export_write_plan_success_envelope(&plan));
        } else {
            outputln!(
                ctx,
                "{} {}",
                plan.commit.checkpoint_id,
                plan.ref_update.git_ref
            );
        }

        return Ok(());
    }

    let response = git_export_checkpoint(request).map_err(|error| {
        if is_fixture_generated_output_export_ref(&options.git_ref) {
            generated_output_git_export_error(error)
        } else {
            git_export_error(error)
        }
    })?;

    if ctx.json {
        outputln!(ctx, "{}", git_export_success_envelope(&response));
    } else {
        outputln!(ctx, "{} {}", response.checkpoint_id, response.git_ref);
    }

    Ok(())
}

fn policy_check_export(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_policy_check_export_options(ctx)?;
    let Some(fixture) = options.fixture.as_deref() else {
        return real_policy_check_export(ctx, options);
    };
    if fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{fixture}`")).with_detail("fixture", fixture)
        );
    }

    let checkpoint = fixture_checkpoint()?;
    if checkpoint.id != options.checkpoint_id {
        return Err(object_not_found("checkpoint", &options.checkpoint_id));
    }

    let mut request = GitExportRequest::from_checkpoint(&checkpoint);
    if let Some(git_ref) = options.git_ref {
        request.git_ref = git_ref;
    }
    apply_fixture_generated_output_export_gate(&mut request);

    let report = sunlight_core::git_export::validate_git_export_request(&request);
    if !report.ok {
        return Err(policy_check_export_error(&report));
    }

    if ctx.json {
        outputln!(ctx, "{}", policy_check_export_success_envelope(&report));
    } else {
        outputln!(ctx, "{} {}", report.checkpoint_id, report.id);
    }

    Ok(())
}

fn real_policy_check_export(
    ctx: &CommandContext,
    options: PolicyCheckExportOptions,
) -> Result<(), CliError> {
    let repo_root = ctx.repo_root.clone();
    let state = RealRepoState::load(&repo_root)?;
    let git_ref = options.git_ref.ok_or_else(|| {
        invalid_request(
            "usage: sun policy check-export --checkpoint <checkpoint-id> --branch <git-ref>",
        )
        .with_detail("missing", "branch")
    })?;
    let report =
        validate_real_export_candidate(&repo_root, &state, &options.checkpoint_id, &git_ref)?;
    let report = persist_and_reload_real_validation_report(&repo_root, &state, &report)?;
    if !report.ok {
        return Err(policy_check_export_error(&report));
    }

    if ctx.json {
        outputln!(
            ctx,
            "{}",
            policy_check_export_success_envelope_with_repository(&state.repository_id, &report)
        );
    } else {
        outputln!(ctx, "{} {}", report.checkpoint_id, report.id);
    }
    Ok(())
}

fn policy_check_commit(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_policy_check_commit_options(ctx)?;
    let config = require_repository_config(&ctx.repo_root)?;
    let gitignore =
        fs::read_to_string(ctx.repo_root.join(".sunlight/.gitignore")).unwrap_or_default();

    let ignore_report = validate_managed_ignore_block(&gitignore);
    let path_report = validate_candidate_paths(options.paths.iter().map(String::as_str));
    let report = combine_policy_check_commit_reports(ignore_report, path_report);

    if !report.ok {
        return Err(policy_check_commit_error(&report, options.paths.len()));
    }

    if ctx.json {
        outputln!(
            ctx,
            "{}",
            policy_check_commit_success_envelope(
                &config.repository_id,
                &report,
                options.paths.len(),
            )
        );
    } else {
        outputln!(ctx, "policy.check-commit ok");
    }

    Ok(())
}

fn policy_explain(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_policy_explain_options(ctx)?;
    if !ctx.json {
        return Err(
            invalid_request("usage: sun policy explain <validation-report-id> --json")
                .with_detail("missing", "json"),
        );
    }

    let repo_root = ctx.repo_root.clone();
    let (repository_id, report) = match RealRepoState::load(&repo_root) {
        Ok(state) => {
            let report = load_git_export_validation_report(
                &repo_root,
                &state.repository_id,
                &options.validation_report_id,
            )
            .map_err(|error| validation_report_load_error(&options.validation_report_id, error))?;
            (Some(state.repository_id), report)
        }
        Err(RepoStateError::NotInitialized { .. }) => (
            None,
            fixture_policy_explain_validation_report(&options.validation_report_id)?,
        ),
        Err(error) => return Err(error.into()),
    };
    outputln!(
        ctx,
        "{}",
        policy_explain_success_envelope(repository_id.as_deref(), &report)
    );

    Ok(())
}

fn compat_project(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_compat_project_options(ctx)?;
    let Some(fixture) = options.fixture.as_deref() else {
        return real_compat_project(ctx, options);
    };
    if fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{fixture}`")).with_detail("fixture", fixture)
        );
    }
    ensure_fixture_session(&options.session_id)?;

    let view = fixture_base_resolved_content_view();
    let projection =
        fixture_compatibility_projection_from_resolved_view(&view, FIXTURE_SESSION_GENERATION_ID)
            .map_err(projection_error)?;

    if ctx.json {
        outputln!(
            ctx,
            "{}",
            compat_project_success_envelope(&projection, &options.session_id)
        );
    } else {
        outputln!(ctx, "{} {}", projection.id, projection.root_ref.value);
    }

    Ok(())
}

fn compat_diff(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_compat_diff_options(ctx)?;
    if options.worktree {
        return real_worktree_diff(ctx);
    }
    let Some(fixture) = options.fixture.as_deref() else {
        return real_compat_diff(ctx, options);
    };
    if fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{fixture}`")).with_detail("fixture", fixture)
        );
    }

    let projection_id = options
        .projection_id
        .as_deref()
        .expect("validated compatibility projection");
    let projection = fixture_compat_import_projection_by_id(projection_id)
        .ok_or_else(|| object_not_found("projection", projection_id))?
        .map_err(projection_error)?;
    if projection.id != FIXTURE_COMPATIBILITY_PROJECTION_ID {
        return Err(CliError::new(
            "compat_projection_invalid",
            "compat diff requires a compatibility projection",
        )
        .with_detail("projection_id", projection_id));
    }
    let current_view = fixture_compat_import_view_for_projection(&projection);
    let candidates = fixture_basic_app_candidate_deltas();

    if ctx.json {
        outputln!(
            ctx,
            "{}",
            compat_diff_success_envelope(&projection, &current_view, &candidates)
        );
    } else {
        outputln!(ctx, "{} {} candidates", projection.id, candidates.len());
    }

    Ok(())
}

fn compat_import(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_compat_import_options(ctx)?;
    let Some(fixture) = options.fixture.as_deref() else {
        return real_compat_import(ctx, options);
    };
    if fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{fixture}`")).with_detail("fixture", fixture)
        );
    }

    let projection = fixture_compat_import_projection_by_id(&options.projection_id)
        .ok_or_else(|| object_not_found("projection", &options.projection_id))?
        .map_err(projection_error)?;
    let current_view = fixture_compat_import_view_for_projection(&projection);
    let response = plan_fixture_basic_app_import(
        &projection,
        &current_view,
        CompatImportRequest {
            projection_id: options.projection_id,
            session_id: FIXTURE_SESSION_ID.to_string(),
            session_generation_id: options
                .session_generation_id
                .unwrap_or_else(|| FIXTURE_SESSION_GENERATION_ID.to_string()),
            resolved_view_id: current_view.resolved_view_id.clone(),
            write_topic_id: FIXTURE_WRITE_TOPIC_ID.to_string(),
            parent_topic_revision_id: None,
            selected_candidate_delta_ids: options.candidate_delta_ids,
        },
        &fixture_basic_app_candidate_deltas(),
    )
    .map_err(compat_import_error)?;

    if ctx.json {
        outputln!(ctx, "{}", compat_import_success_envelope(&response));
    } else {
        outputln!(
            ctx,
            "{} {}",
            response.operation_id,
            response.topic_revision_id
        );
    }

    Ok(())
}

fn real_compat_project(
    ctx: &CommandContext,
    options: CompatProjectOptions,
) -> Result<(), CliError> {
    let repo_root = ctx.repo_root.clone();
    let projection_policy = require_projection_policy(&repo_root)?;
    let mut state = RealRepoState::load(&repo_root)?;
    let session = real_session(&state, &options.session_id)?.clone();
    let resolved = state.resolve_session_view(&session);
    if !resolved.result.conflict_free() {
        return Err(CliError::new(
            "compat_projection_invalid",
            "cannot create a compatibility projection from a conflicted session view",
        )
        .with_detail("session_id", options.session_id));
    }
    let provisional_projection_id = format!(
        "projection_compat_native_{:04}",
        state.projections.len() + 1
    );
    let provisional_root = projection_policy.compatibility_root(&provisional_projection_id);
    let view_state = real_view_state(&state, &resolved);
    let materialization = materialize_repo_projection(
        &repo_root,
        &view_state,
        &provisional_root,
        ProjectionPurpose::Compatibility,
        None,
        true,
    )?;
    let projection_id = selected_real_projection_id(
        ProjectionPurpose::Compatibility,
        materialization.strategy,
        state.projections.len() + 1,
    );
    let root = projection_policy.compatibility_root(&projection_id);
    relocate_managed_projection_root(&provisional_root, &root)?;
    let manifest_digest = real_compat_baseline_manifest_digest(
        &state.repository_id,
        &projection_id,
        &session.session_id,
        &session.session_generation_id,
        &resolved.result.resolved_view_id,
        &view_state.tree_hash,
        &view_state.entries,
    );
    let projection = RealProjectionSnapshot {
        projection_id: projection_id.clone(),
        repository_id: state.repository_id.clone(),
        purpose: ProjectionPurpose::Compatibility.as_str().to_string(),
        resolved_view_id: resolved.result.resolved_view_id.clone(),
        tree_hash: view_state.tree_hash.clone(),
        topic_frontier: resolved.result.topic_frontier.clone(),
        manifest_digest,
        created_from_content_tree: view_state.tree_hash.clone(),
        materialized_root: Some(root.display().to_string()),
        session_id: Some(session.session_id.clone()),
        session_generation_id: Some(session.session_generation_id.clone()),
        path_policy_id: POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string(),
        operation_semantics_version: FILE_OPERATION_SEMANTICS_VERSION.to_string(),
        cache_key: materialization.cache_key.clone(),
        strategy: materialization.strategy.as_str().to_string(),
        materialization: Some(materialization.metrics),
        retention_state: "active".to_string(),
        privacy_class: "local_only".to_string(),
        last_import_operation_id: None,
        entries: view_state.entries.clone(),
    };
    state.projections.push(projection.clone());
    let records = vec![real_projection_publication(&state, &projection)?];
    state.save_with_records(&repo_root, &records)?;

    if ctx.json {
        outputln!(ctx, "{}", real_compat_project_envelope(&state, &projection));
    } else {
        outputln!(ctx, "{} {}", projection_id, root.display());
    }
    Ok(())
}

fn real_compat_diff(ctx: &CommandContext, options: CompatDiffOptions) -> Result<(), CliError> {
    let repo_root = ctx.repo_root.clone();
    let projection_policy = require_projection_policy(&repo_root)?;
    let state = RealRepoState::load(&repo_root)?;
    let projection_id = options
        .projection_id
        .as_deref()
        .expect("validated compatibility projection");
    let projection = state
        .projections
        .iter()
        .find(|projection| projection.projection_id == projection_id)
        .ok_or_else(|| {
            CliError::new(
                "compat_projection_not_found",
                "compatibility projection was not found",
            )
            .with_detail("projection_id", projection_id)
        })?;
    let diff = diff_real_compat_projection(&repo_root, &projection_policy.managed_root, projection)
        .map_err(compat_import_error)?;
    if ctx.json {
        outputln!(
            ctx,
            "{}",
            real_compat_diff_envelope(&state, projection, &diff.candidates)
        );
    } else {
        outputln!(
            ctx,
            "{} {} candidates",
            projection.projection_id,
            diff.candidates.len()
        );
    }
    Ok(())
}

fn real_worktree_diff(ctx: &CommandContext) -> Result<(), CliError> {
    let state = RealRepoState::load(&ctx.repo_root)?;
    let anchored = state.resolve_worktree_anchor_view()?;
    let diff = diff_real_worktree(
        &ctx.repo_root,
        &state.repository_id,
        &state.worktree_anchor,
        &anchored.entries,
    )
    .map_err(worktree_capture_error)?;
    if ctx.json {
        outputln!(
            ctx,
            "{}",
            real_worktree_diff_envelope(&state, &diff.candidates)
        );
    } else {
        outputln!(
            ctx,
            "{} generation={} candidates={}",
            if diff.candidates.is_empty() {
                "worktree clean"
            } else {
                "worktree dirty"
            },
            state.worktree_anchor.generation,
            diff.candidates.len()
        );
    }
    Ok(())
}

fn worktree_capture(ctx: &CommandContext) -> Result<(), CliError> {
    real_worktree_capture(ctx, parse_worktree_capture_options(ctx)?)
}

fn real_worktree_capture(
    ctx: &CommandContext,
    options: WorktreeCaptureOptions,
) -> Result<(), CliError> {
    TopicSlug::new(options.topic_slug.clone()).map_err(|error| {
        invalid_request(error.to_string()).with_detail("slug", options.topic_slug.clone())
    })?;
    validate_topic_metadata(&options.actor_id, "local", &[]).map_err(|error| {
        invalid_request(error.to_string()).with_detail("actor_id", options.actor_id.clone())
    })?;

    let repo_root = ctx.repo_root.clone();
    let mut state = RealRepoState::load(&repo_root)?;
    let anchor_before = state.worktree_anchor.clone();
    if options
        .anchor_generation
        .is_some_and(|expected| expected != anchor_before.generation)
    {
        return Err(CliError::new(
            "worktree_anchor_stale",
            "the requested worktree anchor generation is stale",
        )
        .with_detail(
            "expected_anchor_generation",
            options
                .anchor_generation
                .expect("checked generation")
                .to_string(),
        )
        .with_detail(
            "actual_anchor_generation",
            anchor_before.generation.to_string(),
        )
        .with_detail(
            "next_action",
            "Call worktree_diff and retry with current candidate IDs.",
        ));
    }
    let anchored = state.resolve_worktree_anchor_view()?;
    let diff = diff_real_worktree(
        &repo_root,
        &state.repository_id,
        &anchor_before,
        &anchored.entries,
    )
    .map_err(worktree_capture_error)?;
    if diff.candidates.is_empty() {
        if ctx.json {
            outputln!(ctx, "{}", real_worktree_capture_noop_envelope(&state));
        } else {
            outputln!(ctx, "worktree clean; no capture created");
        }
        return Ok(());
    }

    let selected_ids = selected_worktree_candidate_ids(&options, &diff.candidates)?;
    let selected = validate_real_worktree_selection(
        &state.repository_id,
        &anchor_before,
        &anchored.entries,
        &selected_ids,
        &diff,
    )
    .map_err(worktree_capture_error)?;
    let verified = diff_real_worktree(
        &repo_root,
        &state.repository_id,
        &anchor_before,
        &anchored.entries,
    )
    .map_err(worktree_capture_error)?;
    if verified.candidates != diff.candidates {
        return Err(CliError::new(
            "worktree_changed_during_capture",
            "the worktree changed while capture was preparing its exact candidate set",
        )
        .with_detail("anchor_generation", anchor_before.generation.to_string())
        .with_detail("next_action", "Call worktree_diff and retry."));
    }

    let topic_id = format!("topic_{}", options.topic_slug.replace('-', "_"));
    if state.topic_by_id_or_slug(&options.topic_slug).is_some()
        || state.topic_by_id_or_slug(&topic_id).is_some()
    {
        return Err(CliError::new(
            "topic_conflict",
            format!("topic slug `{}` already exists", options.topic_slug),
        )
        .with_detail("slug", options.topic_slug)
        .with_detail("topic_id", topic_id));
    }

    let mut changes = Vec::new();
    for candidate in &selected {
        ensure_path_visible_to_sunlight(&repo_root, &candidate.path)?;
        if let Some(source_path) = candidate.source_path.as_deref() {
            ensure_path_visible_to_sunlight(&repo_root, source_path)?;
        }
        let before_path = candidate
            .source_path
            .as_deref()
            .unwrap_or(candidate.path.as_str());
        let before = candidate.before_hash.as_ref().and_then(|_| {
            anchored
                .entries
                .iter()
                .find(|entry| entry.path == before_path && !entry.tombstone)
                .cloned()
        });
        match (&candidate.before_hash, &before) {
            (Some(expected), Some(entry)) if expected == &entry.content_hash => {}
            (Some(_), _) | (None, Some(_)) => {
                return Err(CliError::new(
                    "worktree_capture_precondition_failed",
                    "a selected candidate no longer matches the anchored artifact",
                )
                .with_detail("candidate_delta_id", candidate.candidate_delta_id.clone())
                .with_detail("path", before_path));
            }
            (None, None) => {}
        }
        let after = match candidate.operation_kind {
            CompatFileOperationKind::Delete => {
                let mut entry = before.clone().expect("validated worktree delete");
                entry.tombstone = true;
                entry
            }
            CompatFileOperationKind::Patch
            | CompatFileOperationKind::Write
            | CompatFileOperationKind::Metadata
            | CompatFileOperationKind::Move => {
                let bytes = diff
                    .after_bytes
                    .get(&candidate.candidate_delta_id)
                    .cloned()
                    .ok_or_else(|| {
                        CliError::new(
                            "worktree_diff_failed",
                            "selected worktree candidate bytes were not available",
                        )
                        .with_detail("candidate_delta_id", candidate.candidate_delta_id.clone())
                    })?;
                RealArtifactEntry {
                    artifact_id: before
                        .as_ref()
                        .map(|entry| entry.artifact_id.clone())
                        .or_else(|| candidate.artifact_id.clone())
                        .unwrap_or_else(|| real_artifact_id_for_path(&candidate.path)),
                    path: candidate.path.clone(),
                    content_hash: real_content_hash(&bytes),
                    executable: candidate.executable,
                    classification: candidate.classification.clone(),
                    tombstone: false,
                    bytes,
                }
            }
        };
        changes.push((candidate.clone(), before, after));
    }

    let session_id = worktree_capture_session_id(&options.actor_id, &options.topic_slug);
    let initial_session_generation_id = native_session_generation_id(&session_id, 1);
    state.topics.push(RealTopicRecord {
        topic_id: topic_id.clone(),
        slug: options.topic_slug.clone(),
        display_name: format!("Worktree capture: {}", options.topic_slug),
        owner_actor_id: options.actor_id.clone(),
        visibility: "local".to_string(),
        acceptance_criteria: Vec::new(),
        base_checkpoint_id: anchor_before
            .base_checkpoint_ids
            .first()
            .cloned()
            .unwrap_or_else(|| state.base_checkpoint_id.clone()),
        head_revision_id: None,
        completed_revision_id: None,
        revision_number: 0,
    });
    state.sessions.push(RealSessionRecord {
        session_id: session_id.clone(),
        actor_id: options.actor_id.clone(),
        write_topic_id: topic_id.clone(),
        resolved_view_id: anchor_before.resolved_view_id.clone(),
        session_generation_id: initial_session_generation_id.clone(),
        generation_number: 1,
        topic_frontier: anchor_before.topic_frontier.clone(),
        refresh_policy: "none".to_string(),
    });
    state.session_generations.push(RealSessionGenerationRecord {
        session_generation_id: initial_session_generation_id.clone(),
        session_id: session_id.clone(),
        write_topic_id: topic_id.clone(),
        base_resolved_view_id: anchor_before.resolved_view_id.clone(),
        resolved_view_id: anchor_before.resolved_view_id.clone(),
        topic_frontier: anchor_before.topic_frontier.clone(),
        generation_number: 1,
        refresh_policy: "none".to_string(),
        created_by: "worktree_capture_start".to_string(),
    });

    let (_, primary_before, primary_after) = changes
        .first()
        .cloned()
        .expect("validated worktree candidates");
    let mut response = real_accept_mutation(
        &mut state,
        &session_id,
        "worktree_capture",
        selected[0]
            .source_path
            .as_deref()
            .unwrap_or(&selected[0].path),
        primary_before,
        primary_after,
    )?;
    let operation = state.operations.last_mut().expect("accepted operation");
    operation.worktree_candidate_delta_ids = selected
        .iter()
        .map(|candidate| candidate.candidate_delta_id.clone())
        .collect();
    operation.worktree_anchor_generation = Some(anchor_before.generation);
    operation.worktree_anchor_resolved_view_id = Some(anchor_before.resolved_view_id.clone());
    operation.worktree_anchor_tree_hash = Some(anchor_before.tree_hash.clone());
    operation.worktree_anchor_manifest_digest = Some(anchor_before.manifest_digest.clone());
    operation.effects = changes
        .iter()
        .flat_map(|(candidate, before, after)| {
            let mut effects = Vec::new();
            if candidate.operation_kind == CompatFileOperationKind::Move {
                let source = before.as_ref().expect("validated worktree move");
                effects.push(RealOperationEffect {
                    artifact_id: source.artifact_id.clone(),
                    path: source.path.clone(),
                    base_content_hash: Some(source.content_hash.clone()),
                    result_content_hash: source.content_hash.clone(),
                    classification: source.classification.clone(),
                    executable: source.executable,
                    tombstone: true,
                    bytes: source.bytes.clone(),
                });
            }
            effects.push(RealOperationEffect {
                artifact_id: after.artifact_id.clone(),
                path: after.path.clone(),
                base_content_hash: before.as_ref().map(|entry| entry.content_hash.clone()),
                result_content_hash: after.content_hash.clone(),
                classification: after.classification.clone(),
                executable: after.executable,
                tombstone: after.tombstone,
                bytes: after.bytes.clone(),
            });
            effects
        })
        .collect();

    let refreshed = state.resolve_session_view(real_session(&state, &session_id)?);
    if !refreshed.result.conflict_free() {
        return Err(CliError::new(
            "worktree_capture_resolution_failed",
            "the selected worktree changes did not resolve to one exact view",
        ));
    }
    state.resolved_view_id = refreshed.result.resolved_view_id.clone();
    state.current_topic_frontier = refreshed.result.topic_frontier.clone();
    state.tree_hash = refreshed
        .result
        .tree_identity
        .as_ref()
        .map(|tree| tree.tree_hash.clone())
        .unwrap_or_else(|| real_tree_hash(&refreshed.entries));
    state.entries = refreshed.entries.clone();
    response.view.tree_identity.tree_hash = state.tree_hash.clone();
    let topic_index = state
        .topics
        .iter()
        .position(|topic| topic.topic_id == topic_id)
        .expect("capture topic exists");
    state.topics[topic_index].completed_revision_id = Some(response.topic_revision.id.clone());
    state.advance_worktree_anchor(&refreshed)?;
    state.sync_compat_fields();

    let operation = state.operations.last().expect("captured operation");
    let capture_record = real_worktree_capture_record_json(
        &state,
        &anchor_before,
        &state.worktree_anchor,
        operation,
        &selected,
        &response,
    );
    let topic = state.topics[topic_index].clone();
    let summary = format!(
        "Captured {} external worktree change{} from anchor generation {}.",
        selected.len(),
        if selected.len() == 1 { "" } else { "s" },
        anchor_before.generation
    );
    let completion_id = format!("completion_{}", topic.topic_id);
    let records = vec![
        state.record_publication(
            "records",
            &session_id,
            &format!(
                "{{\"record_type\":\"session\",\"id\":\"{}\",\"repository_id\":\"{}\",\"write_topic_id\":\"{}\",\"resolved_view_id\":\"{}\",\"session_generation_id\":\"{}\",\"actor_id\":\"{}\"}}\n",
                json_escape(&session_id),
                json_escape(&state.repository_id),
                json_escape(&topic_id),
                json_escape(&response.view.resolved_view_id),
                json_escape(&response.view.session_generation_id),
                json_escape(&options.actor_id),
            ),
        )?,
        real_session_generation_publication(
            &state,
            &session_id,
            &initial_session_generation_id,
        )?,
        real_session_generation_publication(
            &state,
            &session_id,
            &response.session_generation.id,
        )?,
        state.record_publication(
            "operations",
            &response.operation.id,
            &format!("{capture_record}\n"),
        )?,
        state.record_publication(
            "worktree-captures",
            &response.operation.id,
            &format!("{capture_record}\n"),
        )?,
        state.record_publication(
            "topics",
            &topic.topic_id,
            &format!(
                "{{\"record_type\":\"topic\",\"id\":\"{}\",\"repository_id\":\"{}\",\"slug\":\"{}\",\"display_name\":\"{}\",\"owner_actor_id\":\"{}\",\"visibility\":\"{}\",\"acceptance_criteria\":[],\"base_checkpoint_id\":\"{}\",\"head_revision_id\":\"{}\",\"completed_revision_id\":\"{}\"}}\n",
                json_escape(&topic.topic_id),
                json_escape(&state.repository_id),
                json_escape(&topic.slug),
                json_escape(&topic.display_name),
                json_escape(&topic.owner_actor_id),
                json_escape(&topic.visibility),
                json_escape(&topic.base_checkpoint_id),
                json_escape(&response.topic_revision.id),
                json_escape(&response.topic_revision.id),
            ),
        )?,
        state.record_publication(
            "topics",
            &response.topic_revision.id,
            &format!("{}\n", topic_revision_json(&response)),
        )?,
        state.record_publication(
            "topics",
            &completion_id,
            &format!(
                "{{\"record_type\":\"topic_completion\",\"id\":\"{}\",\"repository_id\":\"{}\",\"topic_id\":\"{}\",\"completed_revision_id\":\"{}\",\"session_id\":\"{}\",\"actor_id\":\"{}\",\"summary\":\"{}\",\"immutable\":true}}\n",
                json_escape(&completion_id),
                json_escape(&state.repository_id),
                json_escape(&topic.topic_id),
                json_escape(&response.topic_revision.id),
                json_escape(&session_id),
                json_escape(&options.actor_id),
                json_escape(&summary),
            ),
        )?,
        state.record_publication(
            "views",
            &refreshed.result.resolved_view_id,
            &format!("{}\n", resolved_view_record_json(&refreshed.result)),
        )?,
        real_worktree_anchor_publication(&state)?,
    ];
    state.save_with_records(&repo_root, &records)?;

    let remaining_count = diff.candidates.len().saturating_sub(selected.len());
    if ctx.json {
        outputln!(
            ctx,
            "{}",
            real_worktree_capture_envelope(
                &repo_root,
                &state,
                &anchor_before,
                &selected,
                &response,
                remaining_count,
            )
        );
    } else {
        outputln!(
            ctx,
            "captured {} candidates as completed topic {} at {}; anchor generation={}",
            selected.len(),
            topic.topic_id,
            response.topic_revision.id,
            state.worktree_anchor.generation
        );
    }
    Ok(())
}

fn selected_worktree_candidate_ids(
    options: &WorktreeCaptureOptions,
    candidates: &[CompatCandidateDelta],
) -> Result<Vec<String>, CliError> {
    if !options.candidate_delta_ids.is_empty() {
        return Ok(options.candidate_delta_ids.clone());
    }
    if !options.paths.is_empty() {
        let mut selected = Vec::new();
        let mut missing = Vec::new();
        for path in &options.paths {
            let matches = candidates
                .iter()
                .filter(|candidate| {
                    candidate.path == *path || candidate.source_path.as_deref() == Some(path)
                })
                .collect::<Vec<_>>();
            if matches.is_empty() {
                missing.push(path.clone());
            } else {
                selected.extend(
                    matches
                        .into_iter()
                        .map(|candidate| candidate.candidate_delta_id.clone()),
                );
            }
        }
        if !missing.is_empty() {
            return Err(CliError::new(
                "worktree_candidate_not_found",
                "one or more selected paths have no current worktree candidate",
            )
            .with_detail("paths", missing.join(","))
            .with_detail(
                "next_action",
                "Call worktree_diff and select returned paths.",
            ));
        }
        selected.sort();
        selected.dedup();
        return Ok(selected);
    }
    let selected = candidates
        .iter()
        .filter(|candidate| is_safe_default_compat_candidate(candidate))
        .map(|candidate| candidate.candidate_delta_id.clone())
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Err(CliError::new(
            "worktree_no_eligible_changes",
            "the worktree has differences but none are eligible for default capture",
        )
        .with_detail("candidate_count", candidates.len().to_string())
        .with_detail(
            "next_action",
            "Call worktree_diff and review candidate classifications.",
        ));
    }
    Ok(selected)
}

fn worktree_capture_session_id(actor_id: &str, topic_slug: &str) -> String {
    let actor = actor_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!(
        "session_worktree_{}_{}",
        actor.trim_matches('_'),
        topic_slug.replace('-', "_")
    )
}

fn real_compat_import(ctx: &CommandContext, options: CompatImportOptions) -> Result<(), CliError> {
    let repo_root = ctx.repo_root.clone();
    let projection_policy = require_projection_policy(&repo_root)?;
    let mut state = RealRepoState::load(&repo_root)?;
    let projection_index = state
        .projections
        .iter()
        .position(|projection| projection.projection_id == options.projection_id)
        .ok_or_else(|| {
            CliError::new(
                "compat_projection_not_found",
                "compatibility projection was not found",
            )
            .with_detail("projection_id", options.projection_id.clone())
        })?;
    let projection = state.projections[projection_index].clone();
    let session_id = projection.session_id.clone().ok_or_else(|| {
        CliError::new(
            "compat_projection_invalid",
            "compatibility projection is not bound to a session",
        )
        .with_detail("projection_id", projection.projection_id.clone())
    })?;
    let session = real_session(&state, &session_id)?.clone();
    let expected_generation = options
        .session_generation_id
        .as_deref()
        .unwrap_or_else(|| projection.session_generation_id.as_deref().unwrap_or(""));
    if session.session_generation_id != expected_generation {
        return Err(CliError::new(
            "compat_precondition_failed",
            "compatibility import session generation precondition failed",
        )
        .with_detail("projection_id", projection.projection_id)
        .with_detail("session_id", session.session_id)
        .with_detail("expected", expected_generation)
        .with_detail("actual", session.session_generation_id));
    }
    let resolved = state.resolve_session_view(&session);
    let current_tree = resolved
        .result
        .tree_identity
        .as_ref()
        .map(|tree| tree.tree_hash.as_str())
        .unwrap_or("");
    if resolved.result.resolved_view_id != projection.resolved_view_id
        || current_tree != projection.tree_hash
    {
        return Err(CliError::new(
            "compat_projection_stale",
            "compatibility projection baseline no longer matches the session view",
        )
        .with_detail("projection_id", projection.projection_id)
        .with_detail("session_id", session.session_id)
        .with_detail(
            "current_session_generation_id",
            session.session_generation_id,
        )
        .with_detail("baseline_resolved_view_id", projection.resolved_view_id)
        .with_detail("current_resolved_view_id", resolved.result.resolved_view_id));
    }

    let diff =
        diff_real_compat_projection(&repo_root, &projection_policy.managed_root, &projection)
            .map_err(compat_import_error)?;
    let selected = validate_real_compat_selection(&projection, &options.candidate_delta_ids, &diff)
        .map_err(compat_import_error)?;
    let mut changes = Vec::new();
    for candidate in &selected {
        ensure_path_visible_to_sunlight(&repo_root, &candidate.path)?;
        if let Some(source_path) = candidate.source_path.as_deref() {
            ensure_path_visible_to_sunlight(&repo_root, source_path)?;
        }
        let before_path = candidate
            .source_path
            .as_deref()
            .unwrap_or(candidate.path.as_str());
        let before = candidate
            .before_hash
            .as_ref()
            .and_then(|_| {
                resolved
                    .entries
                    .iter()
                    .find(|entry| entry.path == before_path && !entry.tombstone)
            })
            .cloned();
        match (&candidate.before_hash, &before) {
            (Some(expected), Some(entry)) if expected == &entry.content_hash => {}
            (Some(_), _) | (None, Some(_)) => {
                return Err(CliError::new(
                    "compat_precondition_failed",
                    "compatibility candidate no longer matches the session artifact precondition",
                )
                .with_detail("projection_id", projection.projection_id.clone())
                .with_detail("candidate_delta_id", candidate.candidate_delta_id.clone())
                .with_detail("path", before_path));
            }
            (None, None) => {}
        }
        if candidate.operation_kind == CompatFileOperationKind::Move
            && resolved
                .entries
                .iter()
                .any(|entry| entry.path == candidate.path && !entry.tombstone)
        {
            return Err(CliError::new(
                "compat_precondition_failed",
                "compatibility rename target already exists in the session view",
            )
            .with_detail("projection_id", projection.projection_id.clone())
            .with_detail("candidate_delta_id", candidate.candidate_delta_id.clone())
            .with_detail("path", candidate.path.clone()));
        }
        let after = match candidate.operation_kind {
            CompatFileOperationKind::Delete => {
                let mut entry = before.clone().expect("validated delete baseline");
                entry.tombstone = true;
                entry
            }
            CompatFileOperationKind::Patch
            | CompatFileOperationKind::Write
            | CompatFileOperationKind::Metadata
            | CompatFileOperationKind::Move => {
                let bytes = diff
                    .after_bytes
                    .get(&candidate.candidate_delta_id)
                    .cloned()
                    .ok_or_else(|| {
                        CliError::new(
                            "compat_diff_failed",
                            "selected compatibility candidate bytes were not available",
                        )
                        .with_detail("candidate_delta_id", candidate.candidate_delta_id.clone())
                    })?;
                RealArtifactEntry {
                    artifact_id: before
                        .as_ref()
                        .map(|entry| entry.artifact_id.clone())
                        .or_else(|| candidate.artifact_id.clone())
                        .unwrap_or_else(|| real_artifact_id_for_path(&candidate.path)),
                    path: candidate.path.clone(),
                    content_hash: real_content_hash(&bytes),
                    executable: candidate.executable,
                    classification: candidate.classification.clone(),
                    tombstone: false,
                    bytes,
                }
            }
        };
        changes.push((before, after));
    }
    let (primary_before, primary_after) = changes.first().cloned().expect("validated candidates");
    let mut response = real_accept_mutation(
        &mut state,
        &session_id,
        "compat_import",
        selected[0]
            .source_path
            .as_deref()
            .unwrap_or(&selected[0].path),
        primary_before,
        primary_after,
    )?;
    if let Some(operation) = state.operations.last_mut() {
        operation.compat_projection_id = Some(projection.projection_id.clone());
        operation.compat_candidate_delta_ids = selected
            .iter()
            .map(|candidate| candidate.candidate_delta_id.clone())
            .collect();
        operation.effects = selected
            .iter()
            .zip(&changes)
            .flat_map(|(candidate, (before, after))| {
                let mut effects = Vec::new();
                if candidate.operation_kind == CompatFileOperationKind::Move {
                    let source = before.as_ref().expect("validated move baseline");
                    effects.push(RealOperationEffect {
                        artifact_id: source.artifact_id.clone(),
                        path: source.path.clone(),
                        base_content_hash: Some(source.content_hash.clone()),
                        result_content_hash: source.content_hash.clone(),
                        classification: source.classification.clone(),
                        executable: source.executable,
                        tombstone: true,
                        bytes: source.bytes.clone(),
                    });
                }
                effects.push(RealOperationEffect {
                    artifact_id: after.artifact_id.clone(),
                    path: after.path.clone(),
                    base_content_hash: before.as_ref().map(|entry| entry.content_hash.clone()),
                    result_content_hash: after.content_hash.clone(),
                    classification: after.classification.clone(),
                    executable: after.executable,
                    tombstone: after.tombstone,
                    bytes: after.bytes.clone(),
                });
                effects
            })
            .collect();
        operation.authored_context_id =
            format!("ctx_compat_{}", operation.operation_transaction_id);
    }
    let refreshed = state.resolve_session_view(real_session(&state, &session_id)?);
    state.resolved_view_id = refreshed.result.resolved_view_id.clone();
    state.current_topic_frontier = refreshed.result.topic_frontier.clone();
    state.tree_hash = refreshed
        .result
        .tree_identity
        .as_ref()
        .map(|tree| tree.tree_hash.clone())
        .unwrap_or_else(|| real_tree_hash(&refreshed.entries));
    state.entries = refreshed.entries;
    response.view.tree_identity.tree_hash = state.tree_hash.clone();
    state.projections[projection_index].last_import_operation_id =
        Some(response.operation.id.clone());
    let projection_after = state.projections[projection_index].clone();
    let operation_record = state.operations.last().expect("accepted operation");
    let import_record_json = real_compat_import_record_json(
        &state,
        &projection_after,
        operation_record,
        &selected,
        &response,
    );
    let records = vec![
        real_session_generation_publication(
            &state,
            &response.session_id,
            &response.session_generation.id,
        )?,
        real_projection_publication(&state, &projection_after)?,
        state.record_publication(
            "operations",
            &response.operation.id,
            &format!("{import_record_json}\n"),
        )?,
        state.record_publication(
            "compat-imports",
            &response.operation.id,
            &format!("{import_record_json}\n"),
        )?,
        state.record_publication(
            "topics",
            &response.topic_revision.id,
            &format!("{}\n", topic_revision_json(&response)),
        )?,
        state.record_publication(
            "views",
            &state.resolved_view_id,
            &format!(
                "{}\n",
                resolved_view_record_json(&real_resolved_view(&state))
            ),
        )?,
    ];
    state.save_with_records(&repo_root, &records)?;

    if ctx.json {
        outputln!(
            ctx,
            "{}",
            real_compat_import_envelope(&state, &projection_after, &selected, &response)
        );
    } else {
        outputln!(
            ctx,
            "{} {}",
            response.operation.id,
            response.topic_revision.id
        );
    }
    Ok(())
}

fn status(ctx: &CommandContext) -> Result<(), CliError> {
    if let Some(options) = parse_status_options(ctx)? {
        if options.fixture != "basic-app" {
            return Err(
                invalid_request(format!("unknown fixture `{}`", options.fixture))
                    .with_detail("fixture", options.fixture),
            );
        }
        let output = match options.scope {
            StatusScope::Repository => {
                if ctx.json {
                    fixture_status_repository_json()
                } else {
                    fixture_status_repository_text()
                }
            }
            StatusScope::Session(session_id) => {
                ensure_fixture_session(&session_id)?;
                if ctx.json {
                    fixture_status_session_json()
                } else {
                    fixture_status_session_text()
                }
            }
            StatusScope::Topic(topic) => {
                ensure_fixture_topic(&topic)?;
                if ctx.json {
                    fixture_status_topic_json()
                } else {
                    fixture_status_topic_text()
                }
            }
            StatusScope::View(view_id) => {
                let view = fixture_resolved_view_by_id(&view_id)
                    .ok_or_else(|| object_not_found("resolved_view", &view_id))?;
                if ctx.json {
                    fixture_status_view_json(&view)
                } else {
                    format!(
                        "{} {}",
                        view.resolved_view_id,
                        resolved_view_lifecycle_state(&view)
                    )
                }
            }
            StatusScope::Projection(projection_id) => {
                let projection = fixture_projection_by_id(&projection_id)
                    .ok_or_else(|| object_not_found("projection", &projection_id))?
                    .map_err(projection_error)?;
                ensure_store_integrity_fixture_scope(&projection, options.integrity_fixture)?;
                if ctx.json {
                    fixture_status_projection_json(
                        &projection,
                        options.projection_root.as_deref(),
                        options.integrity_fixture,
                    )?
                } else {
                    format!(
                        "{} {}",
                        projection.id,
                        projection_lifecycle_state(&projection, options.projection_root.as_deref())
                    )
                }
            }
            StatusScope::Checkpoint(checkpoint_id) => {
                ensure_fixture_checkpoint(&checkpoint_id)?;
                if ctx.json {
                    fixture_status_checkpoint_json()?
                } else {
                    format!("{checkpoint_id} export_ready=true")
                }
            }
            StatusScope::ExportMap(export_map_id) => {
                let export = ensure_fixture_export_map(&export_map_id)?;
                if ctx.json {
                    fixture_status_export_map_json(&export)
                } else {
                    format!(
                        "{} exported {}",
                        export.response.export_map.id, export.response.export_map.git_ref
                    )
                }
            }
            StatusScope::Git(selector) => {
                let export = ensure_fixture_git_export_by_selector(&selector)?;
                if ctx.json {
                    fixture_status_git_json(&export)
                } else {
                    format!(
                        "{} exported {}",
                        export.response.export_map.git_ref, export.response.export_map.id
                    )
                }
            }
            StatusScope::CompatImport(operation_id) => {
                let response = fixture_compat_import_response_by_operation_id(&operation_id)?;
                if ctx.json {
                    fixture_status_compat_import_json(&response)
                } else {
                    format!("{} {}", response.operation_id, response.topic_revision_id)
                }
            }
            StatusScope::Execution(execution_id) => {
                let execution = fixture_execution_by_id(&execution_id)?;
                let promotion = fixture_execution_promotion_record(options.promoted)?;
                if ctx.json {
                    fixture_status_execution_json(&execution, promotion.as_ref())
                } else {
                    format!(
                        "{} {} promotion_status={}",
                        execution.id,
                        execution.result.status.as_str(),
                        fixture_execution_promotion_status(promotion.as_ref())
                    )
                }
            }
        };
        outputln!(ctx, "{output}");
        return Ok(());
    }

    if real_status(ctx)? {
        return Ok(());
    }

    let config = require_repository_config(&ctx.repo_root)?;
    let command = match ctx.args.as_slice() {
        [_, flag, _] if flag == "--session" => "status.session",
        [_, flag, _] if flag == "--topic" => "status.topic",
        _ => "status.repository",
    };

    Err(unimplemented_command(
        command,
        format!(
            "sun status is parsed for repository {}, but native status records are not persisted yet",
            config.repository_id
        ),
    ))
}

fn inspect(ctx: &CommandContext) -> Result<(), CliError> {
    if let Some(options) = parse_inspect_options(ctx)? {
        if options.fixture != "basic-app" {
            return Err(
                invalid_request(format!("unknown fixture `{}`", options.fixture))
                    .with_detail("fixture", options.fixture),
            );
        }
        let output = fixture_inspect(&options, ctx.json)?;
        outputln!(ctx, "{output}");
        return Ok(());
    }

    if real_inspect(ctx)? {
        return Ok(());
    }

    require_repository_config(&ctx.repo_root)?;
    Err(CliError::new(
        "object_not_found",
        "Sunlight object was not found",
    ))
}

fn real_session<'a>(
    state: &'a RealRepoState,
    session_id: &str,
) -> Result<&'a RealSessionRecord, CliError> {
    state.session_by_id(session_id).ok_or_else(|| {
        CliError::new(
            "session_not_found",
            format!("session `{session_id}` was not found"),
        )
        .with_detail("session_id", session_id.to_string())
    })
}

fn real_topic<'a>(state: &'a RealRepoState, topic: &str) -> Result<&'a RealTopicRecord, CliError> {
    state.topic_by_id_or_slug(topic).ok_or_else(|| {
        CliError::new("topic_not_found", format!("topic `{topic}` was not found"))
            .with_detail("topic", topic.to_string())
    })
}

fn real_view(state: &RealRepoState) -> SessionView {
    let resolved = state.resolve_head_view();
    let tree = resolved
        .result
        .tree_identity
        .clone()
        .unwrap_or_else(|| SingleRepoTree {
            repository_id: state.repository_id.clone(),
            tree_hash: real_tree_hash(&resolved.entries),
        });
    SessionView {
        resolved_view_id: resolved.result.resolved_view_id,
        session_generation_id: format!("gen_native_{:04}", state.generation_number.max(1)),
        tree_identity: TreeIdentityView {
            kind: "SingleRepoTree".to_string(),
            repository_id: tree.repository_id,
            tree_hash: tree.tree_hash,
        },
    }
}

fn real_session_view(state: &RealRepoState, session: &RealSessionRecord) -> SessionView {
    let resolved = state.resolve_session_view(session);
    real_resolved_session_view(state, session, &resolved)
}

fn real_resolved_session_view(
    state: &RealRepoState,
    session: &RealSessionRecord,
    resolved: &RealResolvedRepoView,
) -> SessionView {
    let tree_identity = resolved
        .result
        .tree_identity
        .as_ref()
        .cloned()
        .unwrap_or_else(|| SingleRepoTree {
            repository_id: state.repository_id.clone(),
            tree_hash: real_tree_hash(&resolved.entries),
        });
    SessionView {
        resolved_view_id: resolved.result.resolved_view_id.clone(),
        session_generation_id: session.session_generation_id.clone(),
        tree_identity: TreeIdentityView {
            kind: "SingleRepoTree".to_string(),
            repository_id: tree_identity.repository_id,
            tree_hash: tree_identity.tree_hash,
        },
    }
}

fn real_resolved_view(state: &RealRepoState) -> ResolvedViewResult {
    state.resolve_head_view().result
}

fn real_resolve_view_by_id(
    repo_root: &Path,
    state: &RealRepoState,
    view_id: &str,
) -> Result<RealResolvedRepoView, CliError> {
    if view_id == state.base_resolved_view_id {
        return Ok(real_base_resolved_repo_view(state));
    }

    let head = state.resolve_head_view();
    if view_id == head.result.resolved_view_id {
        return Ok(head);
    }

    for session in &state.sessions {
        if view_id == session.resolved_view_id {
            return Ok(state.resolve_session_view(session));
        }
    }

    if view_id == state.resolved_view_id {
        let resolved = state.resolve_view(
            state
                .current_topic_frontier
                .iter()
                .map(|(topic_id, revision_id)| TopicRevisionSelection {
                    topic_id: topic_id.clone(),
                    revision_id: revision_id.clone(),
                })
                .collect(),
        );
        if resolved.result.resolved_view_id == view_id {
            return Ok(resolved);
        }
    }

    if let Some(frontier) = load_persisted_view_frontier(repo_root, state, view_id)? {
        let resolved = state.resolve_view(
            frontier
                .into_iter()
                .map(|(topic_id, revision_id)| TopicRevisionSelection {
                    topic_id,
                    revision_id,
                })
                .collect(),
        );
        if resolved.result.resolved_view_id != view_id {
            return Err(CliError::new(
                "persisted_view_mismatch",
                "the persisted view frontier no longer resolves to its recorded identity",
            )
            .with_detail("recorded_view_id", view_id)
            .with_detail("resolved_view_id", resolved.result.resolved_view_id));
        }
        return Ok(resolved);
    }

    if let Some(checkpoint) = state
        .checkpoints
        .iter()
        .find(|checkpoint| checkpoint.resolved_view_id == view_id)
    {
        return real_resolve_checkpoint_by_id(state, &checkpoint.checkpoint_id);
    }

    Err(object_not_found("resolved_view", view_id))
}

fn real_resolve_checkpoint_by_id(
    state: &RealRepoState,
    checkpoint_id: &str,
) -> Result<RealResolvedRepoView, CliError> {
    if checkpoint_id == state.base_checkpoint_id {
        return Ok(real_base_resolved_repo_view(state));
    }

    let checkpoint = state
        .checkpoints
        .iter()
        .find(|checkpoint| checkpoint.checkpoint_id == checkpoint_id)
        .ok_or_else(|| object_not_found("checkpoint", checkpoint_id))?;
    let resolved = real_checkpoint_snapshot_resolved_view(state, checkpoint);
    if !resolved.result.conflict_free()
        || resolved.result.resolved_view_id != checkpoint.resolved_view_id
        || resolved
            .result
            .tree_identity
            .as_ref()
            .map(|tree| tree.tree_hash.as_str())
            != Some(checkpoint.tree_hash.as_str())
    {
        return Err(CliError::new(
            "checkpoint_view_mismatch",
            "the checkpoint frontier does not reproduce its exact persisted view",
        )
        .with_detail("checkpoint_id", checkpoint.checkpoint_id.clone())
        .with_detail("resolved_view_id", checkpoint.resolved_view_id.clone()));
    }
    Ok(resolved)
}

fn load_persisted_view_frontier(
    repo_root: &Path,
    state: &RealRepoState,
    view_id: &str,
) -> Result<Option<BTreeMap<String, String>>, CliError> {
    if view_id.is_empty()
        || !view_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Ok(None);
    }
    let path = repo_root
        .join(".sunlight")
        .join("views")
        .join(format!("{view_id}.json"));
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(CliError::new(
                "persisted_view_read_failed",
                "the persisted view record could not be read",
            )
            .with_detail("resolved_view_id", view_id)
            .with_detail("path", path.display().to_string())
            .with_detail("reason", error.to_string()));
        }
    };
    let record = parse_json_record(&bytes).map_err(|error| {
        CliError::new(
            "persisted_view_invalid",
            "the persisted view record is not valid JSON",
        )
        .with_detail("resolved_view_id", view_id)
        .with_detail("reason", error.to_string())
    })?;
    let JsonValue::Object(object) = record else {
        return Err(CliError::new(
            "persisted_view_invalid",
            "the persisted view record must be a JSON object",
        )
        .with_detail("resolved_view_id", view_id));
    };
    if !matches!(object.get("record_type"), Some(JsonValue::String(value)) if value == "resolved_view")
        || !matches!(object.get("id"), Some(JsonValue::String(value)) if value == view_id)
        || !matches!(object.get("repository_id"), Some(JsonValue::String(value)) if value == &state.repository_id)
    {
        return Err(CliError::new(
            "persisted_view_invalid",
            "the persisted view identity does not match the requested repository view",
        )
        .with_detail("resolved_view_id", view_id));
    }
    let Some(JsonValue::Object(values)) = object.get("topic_frontier") else {
        return Err(CliError::new(
            "persisted_view_invalid",
            "the persisted view record has no valid topic_frontier",
        )
        .with_detail("resolved_view_id", view_id));
    };
    let mut frontier = BTreeMap::new();
    for (topic_id, value) in values {
        let JsonValue::String(revision_id) = value else {
            return Err(CliError::new(
                "persisted_view_invalid",
                "persisted topic frontier revisions must be strings",
            )
            .with_detail("resolved_view_id", view_id)
            .with_detail("topic_id", topic_id));
        };
        frontier.insert(topic_id.clone(), revision_id.clone());
    }
    Ok(Some(frontier))
}

fn real_view_state(state: &RealRepoState, resolved: &RealResolvedRepoView) -> RealRepoState {
    let mut view_state = state.clone();
    view_state.entries = resolved.entries.clone();
    view_state.resolved_view_id = resolved.result.resolved_view_id.clone();
    view_state.current_topic_frontier = resolved.result.topic_frontier.clone();
    for topic in &mut view_state.topics {
        topic.head_revision_id = resolved.result.topic_frontier.get(&topic.topic_id).cloned();
    }
    if let Some(tree) = &resolved.result.tree_identity {
        view_state.tree_hash = tree.tree_hash.clone();
    } else {
        view_state.tree_hash = real_tree_hash(&resolved.entries);
    }
    view_state
}

fn real_checkpoint_snapshot_resolved_view(
    state: &RealRepoState,
    snapshot: &RealCheckpointSnapshot,
) -> RealResolvedRepoView {
    if snapshot.entries.is_empty() {
        return state.resolve_view(
            snapshot
                .topic_frontier
                .iter()
                .map(|(topic_id, revision_id)| TopicRevisionSelection {
                    topic_id: topic_id.clone(),
                    revision_id: revision_id.clone(),
                })
                .collect(),
        );
    }
    let tree_entries = snapshot
        .entries
        .iter()
        .filter(|entry| !entry.tombstone)
        .map(|entry| {
            (
                entry.path.clone(),
                TreeEntryState {
                    artifact_id: entry.artifact_id.clone(),
                    path: entry.path.clone(),
                    content_hash: entry.content_hash.clone(),
                },
            )
        })
        .collect();
    RealResolvedRepoView {
        result: ResolvedViewResult {
            resolved_view_id: snapshot.resolved_view_id.clone(),
            repository_id: state.repository_id.clone(),
            base_checkpoint_ids: vec![state.base_checkpoint_id.clone()],
            topic_frontier: snapshot.topic_frontier.iter().cloned().collect(),
            dependency_closure: DependencyClosure {
                revision_ids: snapshot
                    .topic_frontier
                    .iter()
                    .map(|(_topic_id, revision_id)| revision_id.clone())
                    .collect(),
            },
            operation_semantics_version: FILE_OPERATION_SEMANTICS_VERSION.to_string(),
            path_policy_id: POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string(),
            resolver_order: DeterministicResolverOrder {
                operation_ids: Vec::new(),
            },
            tree_identity: Some(SingleRepoTree {
                repository_id: state.repository_id.clone(),
                tree_hash: snapshot.tree_hash.clone(),
            }),
            records: Vec::new(),
            tree_entries,
        },
        entries: snapshot.entries.clone(),
    }
}

fn real_base_resolved_repo_view(state: &RealRepoState) -> RealResolvedRepoView {
    let entries = state
        .base_entries
        .iter()
        .filter(|entry| !entry.tombstone)
        .cloned()
        .collect::<Vec<_>>();
    let tree_hash = real_tree_hash(&entries);
    let tree_entries = entries
        .iter()
        .map(|entry| {
            (
                entry.path.clone(),
                TreeEntryState {
                    artifact_id: entry.artifact_id.clone(),
                    path: entry.path.clone(),
                    content_hash: entry.content_hash.clone(),
                },
            )
        })
        .collect();
    RealResolvedRepoView {
        result: ResolvedViewResult {
            resolved_view_id: state.base_resolved_view_id.clone(),
            repository_id: state.repository_id.clone(),
            base_checkpoint_ids: vec![state.base_checkpoint_id.clone()],
            topic_frontier: BTreeMap::new(),
            dependency_closure: DependencyClosure {
                revision_ids: Vec::new(),
            },
            operation_semantics_version: FILE_OPERATION_SEMANTICS_VERSION.to_string(),
            path_policy_id: POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string(),
            resolver_order: DeterministicResolverOrder {
                operation_ids: Vec::new(),
            },
            tree_identity: Some(SingleRepoTree {
                repository_id: state.repository_id.clone(),
                tree_hash,
            }),
            records: Vec::new(),
            tree_entries,
        },
        entries,
    }
}

fn real_entry<'a>(
    state: &RealRepoState,
    entries: &'a [RealArtifactEntry],
    path: &str,
) -> Result<&'a RealArtifactEntry, CliError> {
    entries
        .iter()
        .find(|entry| entry.path == path && !entry.tombstone)
        .ok_or_else(|| {
            CliError::new("path_not_found", format!("path `{path}` was not found"))
                .with_detail("path", path)
                .with_detail(
                    "session_generation_id",
                    real_view(state).session_generation_id,
                )
        })
}

fn real_topic_create(ctx: &CommandContext, options: TopicCreateOptions) -> Result<(), CliError> {
    TopicSlug::new(options.slug.clone()).map_err(|error| {
        invalid_request(error.to_string()).with_detail("slug", options.slug.clone())
    })?;
    validate_topic_metadata(
        &options.owner_actor_id,
        &options.visibility,
        &options.acceptance_criteria,
    )
    .map_err(|error| match error {
        TopicMetadataValidationError::InvalidOwner => invalid_request(error.to_string())
            .with_detail("owner_actor_id", options.owner_actor_id.clone()),
        TopicMetadataValidationError::UnsupportedVisibility => {
            invalid_request(error.to_string()).with_detail("visibility", options.visibility.clone())
        }
        TopicMetadataValidationError::InvalidAcceptanceCriteria => {
            invalid_request(error.to_string())
        }
    })?;
    let repo_root = ctx.repo_root.clone();
    let mut state = RealRepoState::load_metadata(&repo_root)?;
    let topic_id = format!("topic_{}", options.slug.replace('-', "_"));
    if state.topic_by_id_or_slug(&options.slug).is_some()
        || state.topic_by_id_or_slug(&topic_id).is_some()
    {
        return Err(CliError::new(
            "topic_conflict",
            format!("topic slug `{}` already exists", options.slug),
        )
        .with_detail("slug", options.slug)
        .with_detail("topic_id", topic_id));
    }
    state.topics.push(RealTopicRecord {
        topic_id: topic_id.clone(),
        slug: options.slug.clone(),
        display_name: options.display_name.clone(),
        owner_actor_id: options.owner_actor_id.clone(),
        visibility: options.visibility.clone(),
        acceptance_criteria: options.acceptance_criteria.clone(),
        base_checkpoint_id: state.base_checkpoint_id.clone(),
        head_revision_id: None,
        completed_revision_id: None,
        revision_number: 0,
    });
    state.sync_compat_fields();
    let records = vec![state.record_publication(
        "topics",
        &topic_id,
        &format!(
            "{{\"record_type\":\"topic\",\"id\":\"{}\",\"repository_id\":\"{}\",\"slug\":\"{}\",\"display_name\":\"{}\",\"owner_actor_id\":\"{}\",\"visibility\":\"{}\",\"acceptance_criteria\":{},\"base_checkpoint_id\":\"{}\",\"head_revision_id\":null}}\n",
            json_escape(&topic_id),
            json_escape(&state.repository_id),
            json_escape(&options.slug),
            json_escape(&options.display_name),
            json_escape(&options.owner_actor_id),
            json_escape(&options.visibility),
            string_array_json(options.acceptance_criteria.iter().map(String::as_str)),
            json_escape(&state.base_checkpoint_id),
        ),
    )?];
    state.save_with_records(&repo_root, &records)?;

    if ctx.json {
        outputln!(ctx, "{}", real_topic_create_success_envelope(&state));
    } else {
        outputln!(
            ctx,
            "created topic {topic_id} owner={} visibility={} base={} acceptance-criteria={}",
            options.owner_actor_id,
            options.visibility,
            state.base_checkpoint_id,
            options.acceptance_criteria.len()
        );
    }
    Ok(())
}

fn real_topic_complete(
    ctx: &CommandContext,
    options: TopicCompleteOptions,
) -> Result<(), CliError> {
    let repo_root = ctx.repo_root.clone();
    let mut state = RealRepoState::load_metadata(&repo_root)?;
    let session = real_session(&state, &options.session_id)?.clone();
    if session.write_topic_id != options.topic_id {
        return Err(CliError::new(
            "topic_session_mismatch",
            "the session does not own the topic being completed",
        )
        .with_detail("session_id", session.session_id)
        .with_detail("session_topic_id", session.write_topic_id)
        .with_detail("requested_topic_id", options.topic_id));
    }
    let topic_index = state
        .topics
        .iter()
        .position(|topic| topic.topic_id == options.topic_id)
        .ok_or_else(|| object_not_found("topic", &options.topic_id))?;
    let topic = &state.topics[topic_index];
    if topic.head_revision_id.as_deref() != Some(options.revision_id.as_str()) {
        return Err(CliError::new(
            "topic_head_mismatch",
            "topic completion requires the exact current head revision",
        )
        .with_detail("topic_id", topic.topic_id.clone())
        .with_detail("expected_revision_id", options.revision_id)
        .with_detail(
            "current_head_revision_id",
            topic.head_revision_id.as_deref().unwrap_or("none"),
        ));
    }
    let changed = match topic.completed_revision_id.as_deref() {
        None => true,
        Some(revision_id) if revision_id == options.revision_id => false,
        Some(revision_id) => {
            return Err(CliError::new(
                "topic_already_completed",
                "the topic is already completed at an immutable revision",
            )
            .with_detail("topic_id", topic.topic_id.clone())
            .with_detail("completed_revision_id", revision_id)
            .with_detail("requested_revision_id", options.revision_id));
        }
    };

    if changed {
        state.topics[topic_index].completed_revision_id = Some(options.revision_id.clone());
        state.sync_compat_fields();
        let topic = state.topics[topic_index].clone();
        let summary = options.summary.as_deref().unwrap_or("");
        let completion_id = format!("completion_{}", topic.topic_id);
        let records = vec![
            state.record_publication(
                "topics",
                &topic.topic_id,
                &format!(
                    "{{\"record_type\":\"topic\",\"id\":\"{}\",\"repository_id\":\"{}\",\"slug\":\"{}\",\"display_name\":\"{}\",\"owner_actor_id\":\"{}\",\"visibility\":\"{}\",\"acceptance_criteria\":{},\"base_checkpoint_id\":\"{}\",\"head_revision_id\":\"{}\",\"completed_revision_id\":\"{}\"}}\n",
                    json_escape(&topic.topic_id),
                    json_escape(&state.repository_id),
                    json_escape(&topic.slug),
                    json_escape(&topic.display_name),
                    json_escape(&topic.owner_actor_id),
                    json_escape(&topic.visibility),
                    string_array_json(topic.acceptance_criteria.iter().map(String::as_str)),
                    json_escape(&topic.base_checkpoint_id),
                    json_escape(&options.revision_id),
                    json_escape(&options.revision_id),
                ),
            )?,
            state.record_publication(
                "topics",
                &completion_id,
                &format!(
                    "{{\"record_type\":\"topic_completion\",\"id\":\"{}\",\"repository_id\":\"{}\",\"topic_id\":\"{}\",\"completed_revision_id\":\"{}\",\"session_id\":\"{}\",\"actor_id\":\"{}\",\"summary\":\"{}\",\"immutable\":true}}\n",
                    json_escape(&completion_id),
                    json_escape(&state.repository_id),
                    json_escape(&topic.topic_id),
                    json_escape(&options.revision_id),
                    json_escape(&session.session_id),
                    json_escape(&session.actor_id),
                    json_escape(summary),
                ),
            )?,
        ];
        state.save_with_records(&repo_root, &records)?;
    }

    let topic = &state.topics[topic_index];
    if ctx.json {
        outputln!(
            ctx,
            "{{\"ok\":true,\"data\":{{\"command\":\"topic.complete\",\"repository_id\":\"{}\",\"ids\":{{\"topic_id\":\"{}\",\"completed_revision_id\":\"{}\",\"session_id\":\"{}\"}},\"changed\":{},\"immutable\":true,\"topic\":{},\"handoff\":{}}},\"warnings\":[]}}",
            json_escape(&state.repository_id),
            json_escape(&topic.topic_id),
            json_escape(&options.revision_id),
            json_escape(&session.session_id),
            changed,
            real_topic_metadata_json(topic),
            real_topic_handoff_json(&repo_root, &state, topic),
        );
    } else {
        outputln!(
            ctx,
            "completed topic {} at {}",
            topic.topic_id,
            options.revision_id
        );
    }
    Ok(())
}

fn real_session_start(ctx: &CommandContext, options: SessionStartOptions) -> Result<(), CliError> {
    let repo_root = ctx.repo_root.clone();
    let mut state = RealRepoState::load_metadata(&repo_root)?;
    let topic = real_topic(&state, &options.topic)?.clone();
    let selected = real_resolve_view_by_id(&repo_root, &state, &options.view_id)?;
    if !selected.result.conflict_free() {
        return Err(CliError::new(
            "conflicted_view",
            "cannot start a session from a conflicted resolved view",
        )
        .with_detail("resolved_view_id", selected.result.resolved_view_id));
    }
    let topic_frontier = selected.result.topic_frontier.clone();
    let actor_slug = options.actor_id.replace('-', "_");
    let base_session_id = format!("session_{actor_slug}");
    let preferred_session_id = match state.session_by_id(&base_session_id) {
        None => base_session_id,
        Some(existing) => {
            if existing.actor_id == options.actor_id && existing.write_topic_id == topic.topic_id {
                existing.session_id.clone()
            } else {
                format!("session_{}_{}", actor_slug, topic.slug.replace('-', "_"))
            }
        }
    };
    let mut session_id = preferred_session_id.clone();
    for discriminator in 2u64.. {
        let Some(existing) = state.session_by_id(&session_id) else {
            break;
        };
        if existing.actor_id == options.actor_id && existing.write_topic_id == topic.topic_id {
            break;
        }
        session_id = format!("{preferred_session_id}_{discriminator:04}");
    }
    if let Some(existing) = state.session_by_id(&session_id) {
        if existing.actor_id != options.actor_id || existing.write_topic_id != topic.topic_id {
            return Err(CliError::new(
                "session_identity_mismatch",
                "the resolved session identity belongs to another actor or topic",
            )
            .with_detail("session_id", existing.session_id.clone()));
        }
        if existing.resolved_view_id != selected.result.resolved_view_id
            || existing.topic_frontier != topic_frontier
        {
            return Err(CliError::new(
                "session_view_mismatch",
                "the existing actor/topic session is pinned to a different exact view",
            )
            .with_detail("session_id", existing.session_id.clone())
            .with_detail(
                "existing_resolved_view_id",
                existing.resolved_view_id.clone(),
            )
            .with_detail(
                "requested_resolved_view_id",
                selected.result.resolved_view_id.clone(),
            ));
        }
        if ctx.json {
            outputln!(
                ctx,
                "{}",
                real_session_start_success_envelope(&state, &topic, existing)
            );
        } else {
            outputln!(ctx, "started session {session_id}");
        }
        return Ok(());
    }
    let generation_number = state
        .session_by_id(&session_id)
        .map(|session| session.generation_number.max(1))
        .unwrap_or(1);
    let session_generation_id = native_session_generation_id(&session_id, generation_number);
    let resolved_view_id = selected.result.resolved_view_id.clone();
    state.sessions.push(RealSessionRecord {
        session_id: session_id.clone(),
        actor_id: options.actor_id.clone(),
        write_topic_id: topic.topic_id.clone(),
        resolved_view_id: resolved_view_id.clone(),
        session_generation_id: session_generation_id.clone(),
        generation_number,
        topic_frontier: topic_frontier.clone(),
        refresh_policy: "none".to_string(),
    });
    if !state.session_generations.iter().any(|generation| {
        generation.session_generation_id == session_generation_id
            && generation.session_id == session_id
    }) {
        state.session_generations.push(RealSessionGenerationRecord {
            session_generation_id: session_generation_id.clone(),
            session_id: session_id.clone(),
            write_topic_id: topic.topic_id.clone(),
            base_resolved_view_id: state.base_resolved_view_id.clone(),
            resolved_view_id: resolved_view_id.clone(),
            topic_frontier: topic_frontier.clone(),
            generation_number,
            refresh_policy: "none".to_string(),
            created_by: "session_start".to_string(),
        });
    }
    state.sync_compat_fields();
    let records = vec![
        state.record_publication(
            "records",
            &session_id,
            &format!(
            "{{\"record_type\":\"session\",\"id\":\"{}\",\"repository_id\":\"{}\",\"write_topic_id\":\"{}\",\"resolved_view_id\":\"{}\",\"session_generation_id\":\"{}\",\"actor_id\":\"{}\"}}\n",
            json_escape(&session_id),
            json_escape(&state.repository_id),
            json_escape(&topic.topic_id),
            json_escape(&resolved_view_id),
            json_escape(&session_generation_id),
            json_escape(&options.actor_id),
            ),
        )?,
        real_session_generation_publication(&state, &session_id, &session_generation_id)?,
        state.record_publication(
            "views",
            &resolved_view_id,
            &format!("{}\n", resolved_view_record_json(&selected.result)),
        )?,
    ];
    state.save_with_records(&repo_root, &records)?;

    if ctx.json {
        let session = real_session(&state, &session_id)?;
        outputln!(
            ctx,
            "{}",
            real_session_start_success_envelope(&state, &topic, session)
        );
    } else {
        outputln!(ctx, "started session {session_id}");
    }
    Ok(())
}

fn real_session_refresh(
    ctx: &CommandContext,
    options: SessionRefreshOptions,
) -> Result<(), CliError> {
    let repo_root = ctx.repo_root.clone();
    let mut state = RealRepoState::load_metadata(&repo_root)?;
    let current = real_session(&state, &options.session_id)?.clone();
    let mut candidate_frontier = current.topic_frontier.clone();

    if matches!(options.policy.as_str(), "manual" | "follow") {
        for topic_id in current.topic_frontier.keys() {
            if topic_id == &current.write_topic_id {
                continue;
            }
            if let Some(head) = state
                .topics
                .iter()
                .find(|topic| topic.topic_id == *topic_id)
                .and_then(|topic| topic.head_revision_id.clone())
            {
                candidate_frontier.insert(topic_id.clone(), head);
            }
        }
    }

    if candidate_frontier == current.topic_frontier && options.policy == current.refresh_policy {
        if ctx.json {
            outputln!(
                ctx,
                "{}",
                real_session_refresh_success_envelope(
                    &state,
                    &current,
                    false,
                    Some("policy_and_frontier_unchanged")
                )
            );
        } else {
            outputln!(ctx, "{} unchanged", current.session_generation_id);
        }
        return Ok(());
    }

    let candidate = state.resolve_view(
        candidate_frontier
            .iter()
            .map(|(topic_id, revision_id)| TopicRevisionSelection {
                topic_id: topic_id.clone(),
                revision_id: revision_id.clone(),
            })
            .collect(),
    );
    if !candidate.result.conflict_free() {
        state.persist_record(
            &repo_root,
            "views",
            &candidate.result.resolved_view_id,
            &format!("{}\n", resolved_view_record_json(&candidate.result)),
        )?;
        for record in &candidate.result.records {
            state.persist_record(
                &repo_root,
                "conflicts",
                &record.id,
                &format!("{}\n", resolver_record_json(record)),
            )?;
        }
        let conflict_ids = candidate
            .result
            .conflicts()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>();
        let staleness_ids = candidate
            .result
            .staleness()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>();
        return Err(CliError::new(
            "session_refresh_blocked",
            "candidate session refresh is conflicted or stale; current generation is unchanged",
        )
        .with_raw_details_json(format!(
            "{{\"session_id\":\"{}\",\"current_session_generation_id\":\"{}\",\"current_resolved_view_id\":\"{}\",\"candidate_resolved_view_id\":\"{}\",\"conflict_ids\":{},\"staleness_ids\":{}}}",
            json_escape(&current.session_id),
            json_escape(&current.session_generation_id),
            json_escape(&current.resolved_view_id),
            json_escape(&candidate.result.resolved_view_id),
            string_array_json(conflict_ids.iter().copied()),
            string_array_json(staleness_ids.iter().copied()),
        )));
    }

    let mut records = vec![state.record_publication(
        "views",
        &candidate.result.resolved_view_id,
        &format!("{}\n", resolved_view_record_json(&candidate.result)),
    )?];
    for record in &candidate.result.records {
        records.push(state.record_publication(
            "conflicts",
            &record.id,
            &format!("{}\n", resolver_record_json(record)),
        )?);
    }
    let generation_number = current.generation_number + 1;
    let generation_id = native_session_generation_id(&current.session_id, generation_number);
    let generation = RealSessionGenerationRecord {
        session_generation_id: generation_id.clone(),
        session_id: current.session_id.clone(),
        write_topic_id: current.write_topic_id.clone(),
        base_resolved_view_id: state.base_resolved_view_id.clone(),
        resolved_view_id: candidate.result.resolved_view_id.clone(),
        topic_frontier: candidate.result.topic_frontier.clone(),
        generation_number,
        refresh_policy: options.policy.clone(),
        created_by: "session_refresh".to_string(),
    };
    state.session_generations.push(generation);
    let session = state
        .session_by_id_mut(&current.session_id)
        .expect("loaded session remains present");
    session.resolved_view_id = candidate.result.resolved_view_id;
    session.session_generation_id = generation_id.clone();
    session.generation_number = generation_number;
    session.topic_frontier = candidate.result.topic_frontier;
    session.refresh_policy = options.policy;
    state.sync_compat_fields();
    records.push(real_session_generation_publication(
        &state,
        &current.session_id,
        &generation_id,
    )?);
    state.save_with_records(&repo_root, &records)?;
    let updated = real_session(&state, &current.session_id)?;

    if ctx.json {
        outputln!(
            ctx,
            "{}",
            real_session_refresh_success_envelope(&state, updated, true, None)
        );
    } else {
        outputln!(
            ctx,
            "{} {}",
            updated.session_generation_id,
            updated.resolved_view_id
        );
    }
    Ok(())
}

fn real_artifact_read(
    ctx: &CommandContext,
    options: ArtifactCommandOptions,
) -> Result<(), CliError> {
    let state = RealRepoState::load_metadata(&ctx.repo_root)?;
    if let Some(view_id) = options.view_id.as_deref() {
        return real_artifact_read_view(ctx, &state, view_id, &options.operands[0]);
    }
    let session_id = options
        .session_id
        .expect("artifact read scope was validated");
    let session = real_session(&state, &session_id)?;
    let resolved = state.resolve_session_view(session);
    let mut entry = real_entry(&state, &resolved.entries, &options.operands[0])?.clone();
    entry.bytes = read_real_blob(&ctx.repo_root, &entry.content_hash)?;
    let bytes = std::str::from_utf8(&entry.bytes)
        .map_err(|_| CliError::new("invalid_content_encoding", "path is not UTF-8 text"))?;
    let response = ReadResponse {
        command: "artifact.read",
        repository_id: state.repository_id.clone(),
        session_id,
        view: real_resolved_session_view(&state, session, &resolved),
        artifact: real_artifact_view(&entry),
        content: sunlight_core::artifacts::ContentView {
            encoding: "utf-8".to_string(),
            bytes: bytes.to_string(),
        },
    };
    if ctx.json {
        outputln!(ctx, "{}", read_success_envelope(&response));
    } else {
        output!(ctx, "{}", response.content.bytes);
    }
    Ok(())
}

fn real_artifact_list(
    ctx: &CommandContext,
    options: ArtifactCommandOptions,
) -> Result<(), CliError> {
    let state = RealRepoState::load_metadata(&ctx.repo_root)?;
    if let Some(view_id) = options.view_id.as_deref() {
        return real_artifact_list_view(ctx, &state, view_id, options.operands.first());
    }
    let session_id = options
        .session_id
        .expect("artifact read scope was validated");
    let session = real_session(&state, &session_id)?;
    let resolved = state.resolve_session_view(session);
    let prefix = options.operands.first().map(String::as_str).unwrap_or("");
    let response = ListResponse {
        command: "artifact.list",
        repository_id: state.repository_id.clone(),
        session_id,
        view: real_resolved_session_view(&state, session, &resolved),
        artifacts: resolved
            .entries
            .iter()
            .filter(|entry| {
                !entry.tombstone
                    && (prefix.is_empty()
                        || entry.path == prefix
                        || entry.path.starts_with(&format!("{prefix}/")))
            })
            .map(real_artifact_view)
            .collect(),
    };
    if ctx.json {
        outputln!(ctx, "{}", list_success_envelope(&response));
    } else {
        for artifact in response.artifacts {
            outputln!(ctx, "{}", artifact.path);
        }
    }
    Ok(())
}

fn real_artifact_search(
    ctx: &CommandContext,
    options: ArtifactCommandOptions,
) -> Result<(), CliError> {
    let state = RealRepoState::load(&ctx.repo_root)?;
    if let Some(view_id) = options.view_id.as_deref() {
        return real_artifact_search_view(ctx, &state, view_id, &options.operands[0]);
    }
    let session_id = options
        .session_id
        .expect("artifact read scope was validated");
    let session = real_session(&state, &session_id)?;
    let resolved = state.resolve_session_view(session);
    let query = &options.operands[0];
    let mut matches = Vec::new();
    for entry in resolved.entries.iter().filter(|entry| !entry.tombstone) {
        if let Ok(text) = std::str::from_utf8(&entry.bytes) {
            for (line_index, line) in text.lines().enumerate() {
                if line.contains(query) {
                    matches.push(sunlight_core::artifacts::SearchMatch {
                        artifact_id: entry.artifact_id.clone(),
                        path: entry.path.clone(),
                        content_hash: entry.content_hash.clone(),
                        line: line_index + 1,
                        snippet: line.to_string(),
                    });
                }
            }
        }
    }
    let response = SearchResponse {
        command: "artifact.search",
        repository_id: state.repository_id.clone(),
        session_id,
        view: real_resolved_session_view(&state, session, &resolved),
        matches,
    };
    if ctx.json {
        outputln!(ctx, "{}", search_success_envelope(&response));
    } else {
        for item in response.matches {
            outputln!(ctx, "{}:{}:{}", item.path, item.line, item.snippet);
        }
    }
    Ok(())
}

fn real_readable_view(
    ctx: &CommandContext,
    state: &RealRepoState,
    view_id: &str,
) -> Result<RealResolvedRepoView, CliError> {
    let resolved = real_resolve_view_by_id(&ctx.repo_root, state, view_id)?;
    if resolved.result.conflict_free() {
        return Ok(resolved);
    }

    let conflict_ids = resolved
        .result
        .conflicts()
        .map(|record| record.id.as_str())
        .collect::<Vec<_>>();
    let staleness_ids = resolved
        .result
        .staleness()
        .map(|record| record.id.as_str())
        .collect::<Vec<_>>();
    Err(CliError::new(
        "view_not_readable",
        "the resolved view has conflicts or staleness and has no readable tree",
    )
    .with_raw_details_json(format!(
        "{{\"resolved_view_id\":\"{}\",\"conflict_ids\":{},\"staleness_ids\":{},\"inspect_ref\":\"view:{}\"}}",
        json_escape(view_id),
        string_array_json(conflict_ids.iter().copied()),
        string_array_json(staleness_ids.iter().copied()),
        json_escape(view_id),
    )))
}

fn real_view_entry<'a>(
    entries: &'a [RealArtifactEntry],
    path_or_artifact_id: &str,
    view_id: &str,
) -> Result<&'a RealArtifactEntry, CliError> {
    entries
        .iter()
        .find(|entry| {
            !entry.tombstone
                && (entry.path == path_or_artifact_id || entry.artifact_id == path_or_artifact_id)
        })
        .ok_or_else(|| {
            CliError::new(
                "path_not_found",
                format!("path or artifact `{path_or_artifact_id}` was not found"),
            )
            .with_detail("path_or_artifact_id", path_or_artifact_id)
            .with_detail("resolved_view_id", view_id)
        })
}

fn real_artifact_read_view(
    ctx: &CommandContext,
    state: &RealRepoState,
    view_id: &str,
    path_or_artifact_id: &str,
) -> Result<(), CliError> {
    let resolved = real_readable_view(ctx, state, view_id)?;
    let mut entry = real_view_entry(&resolved.entries, path_or_artifact_id, view_id)?.clone();
    entry.bytes = read_real_blob(&ctx.repo_root, &entry.content_hash)?;
    let bytes = std::str::from_utf8(&entry.bytes)
        .map_err(|_| CliError::new("invalid_content_encoding", "path is not UTF-8 text"))?;
    if ctx.json {
        outputln!(
            ctx,
            "{}",
            view_read_success_envelope(state, &resolved.result, &entry, bytes)
        );
    } else {
        output!(ctx, "{}", bytes);
    }
    Ok(())
}

fn real_artifact_list_view(
    ctx: &CommandContext,
    state: &RealRepoState,
    view_id: &str,
    prefix: Option<&String>,
) -> Result<(), CliError> {
    let resolved = real_readable_view(ctx, state, view_id)?;
    let prefix = prefix.map(String::as_str).unwrap_or("");
    let artifacts = resolved
        .entries
        .iter()
        .filter(|entry| {
            !entry.tombstone
                && (prefix.is_empty()
                    || entry.path == prefix
                    || entry.path.starts_with(&format!("{prefix}/")))
        })
        .map(real_artifact_view)
        .collect::<Vec<_>>();
    if ctx.json {
        outputln!(
            ctx,
            "{}",
            view_list_success_envelope(state, &resolved.result, &artifacts)
        );
    } else {
        for artifact in artifacts {
            outputln!(ctx, "{}", artifact.path);
        }
    }
    Ok(())
}

fn real_artifact_search_view(
    ctx: &CommandContext,
    state: &RealRepoState,
    view_id: &str,
    query: &str,
) -> Result<(), CliError> {
    let resolved = real_readable_view(ctx, state, view_id)?;
    let mut matches = Vec::new();
    for entry in resolved.entries.iter().filter(|entry| !entry.tombstone) {
        if let Ok(text) = std::str::from_utf8(&entry.bytes) {
            for (line_index, line) in text.lines().enumerate() {
                if line.contains(query) {
                    matches.push(sunlight_core::artifacts::SearchMatch {
                        artifact_id: entry.artifact_id.clone(),
                        path: entry.path.clone(),
                        content_hash: entry.content_hash.clone(),
                        line: line_index + 1,
                        snippet: line.to_string(),
                    });
                }
            }
        }
    }
    if ctx.json {
        outputln!(
            ctx,
            "{}",
            view_search_success_envelope(state, &resolved.result, &matches)
        );
    } else {
        for item in matches {
            outputln!(ctx, "{}:{}:{}", item.path, item.line, item.snippet);
        }
    }
    Ok(())
}

fn ensure_path_visible_to_sunlight(repo_root: &Path, path: &str) -> Result<(), CliError> {
    if path == ".sunignore" {
        return Err(CliError::new(
            "sunignore_policy_external",
            ".sunignore is human-owned worktree policy and cannot be mutated through Sunlight",
        )
        .with_detail("path", path)
        .with_detail(
            "recovery",
            "Edit .sunignore in the worktree, then run sun init to adopt the policy change.",
        ));
    }
    if path_is_sunignored(repo_root, path, false)? {
        return Err(CliError::new(
            "sunignore_excluded",
            "the path is explicitly excluded from Sunlight by .sunignore",
        )
        .with_detail("path", path)
        .with_detail("policy_file", ".sunignore"));
    }
    Ok(())
}

fn real_artifact_patch(
    ctx: &CommandContext,
    options: MutationCommandOptions,
    patch: String,
) -> Result<(), CliError> {
    let mut state = RealRepoState::load(&ctx.repo_root)?;
    let session = real_session(&state, &options.session_id)?.clone();
    let resolved = state.resolve_session_view(&session);
    let path = options.operands[0].clone();
    ensure_path_visible_to_sunlight(&ctx.repo_root, &path)?;
    let before = real_entry(&state, &resolved.entries, &path)?.clone();
    if before.content_hash != options.expect_hash.as_deref().unwrap_or("") {
        return Err(real_precondition_error(
            &state,
            &before,
            options.expect_hash.as_deref().unwrap_or(""),
        ));
    }
    let before_text = std::str::from_utf8(&before.bytes)
        .map_err(|_| CliError::new("invalid_content_encoding", "path is not UTF-8 text"))?;
    let (after_text, _) = apply_real_patch(before_text, &patch)?;
    let mut after = before.clone();
    after.bytes = after_text.into_bytes();
    after.content_hash = real_content_hash(&after.bytes);
    let response = real_accept_mutation(
        &mut state,
        &options.session_id,
        "patch",
        &path,
        Some(before),
        after,
    )?;
    finish_real_mutation(ctx, state, response)
}

fn real_artifact_write(
    ctx: &CommandContext,
    options: MutationCommandOptions,
    content: Vec<u8>,
    expected_hash: ExpectedHash,
) -> Result<(), CliError> {
    let mut state = match &expected_hash {
        ExpectedHash::New => RealRepoState::load_metadata(&ctx.repo_root)?,
        ExpectedHash::Existing(_) => RealRepoState::load(&ctx.repo_root)?,
    };
    let session = real_session(&state, &options.session_id)?.clone();
    let resolved = state.resolve_session_view(&session);
    let path = options.operands[0].clone();
    ensure_path_visible_to_sunlight(&ctx.repo_root, &path)?;
    let existing = resolved
        .entries
        .iter()
        .find(|entry| entry.path == path && !entry.tombstone)
        .cloned();
    let before = existing;
    match (&expected_hash, &before) {
        (ExpectedHash::New, Some(entry)) => {
            return Err(real_precondition_error(&state, entry, "new"))
        }
        (ExpectedHash::Existing(expected), Some(entry)) if entry.content_hash != *expected => {
            return Err(real_precondition_error(&state, entry, expected));
        }
        (ExpectedHash::Existing(expected), None) => {
            return Err(CliError::new(
                "precondition_failed",
                "the path is absent, so an existing content hash cannot match",
            )
            .with_detail("failed_precondition", "expected_hash")
            .with_detail("path", path)
            .with_detail("path_state", "absent")
            .with_detail("expected", expected)
            .with_detail("actual", "absent")
            .with_detail(
                "hint",
                "Use expect_hash \"new\" only if creating this absent path is intended.",
            )
            .with_detail("session_generation_id", session.session_generation_id)
            .with_detail("resolved_view_id", session.resolved_view_id));
        }
        _ => {}
    }
    let after = if let Some(mut entry) = before.clone() {
        entry.bytes = content;
        entry.content_hash = real_content_hash(&entry.bytes);
        entry.classification = options
            .classification
            .clone()
            .unwrap_or_else(|| "source".to_string());
        entry
    } else {
        RealArtifactEntry {
            artifact_id: real_artifact_id_for_path(&path),
            path: path.clone(),
            content_hash: real_content_hash(&content),
            executable: false,
            classification: options
                .classification
                .clone()
                .unwrap_or_else(|| "source".to_string()),
            tombstone: false,
            bytes: content,
        }
    };
    let response = real_accept_mutation(
        &mut state,
        &options.session_id,
        "write",
        &path,
        before,
        after,
    )?;
    finish_real_mutation(ctx, state, response)
}

fn real_artifact_move(
    ctx: &CommandContext,
    options: MutationCommandOptions,
    expected: String,
) -> Result<(), CliError> {
    let mut state = RealRepoState::load(&ctx.repo_root)?;
    let session = real_session(&state, &options.session_id)?.clone();
    let resolved = state.resolve_session_view(&session);
    let source = options.operands[0].clone();
    let target = options.operands[1].clone();
    ensure_path_visible_to_sunlight(&ctx.repo_root, &source)?;
    ensure_path_visible_to_sunlight(&ctx.repo_root, &target)?;
    let before = real_entry(&state, &resolved.entries, &source)?.clone();
    if before.content_hash != expected {
        return Err(real_precondition_error(&state, &before, &expected));
    }
    let mut after = before.clone();
    after.path = target.clone();
    let response = real_accept_mutation(
        &mut state,
        &options.session_id,
        "move",
        &target,
        Some(before),
        after,
    )?;
    finish_real_mutation(ctx, state, response)
}

fn real_artifact_delete(
    ctx: &CommandContext,
    options: MutationCommandOptions,
    expected: String,
) -> Result<(), CliError> {
    let mut state = RealRepoState::load(&ctx.repo_root)?;
    let session = real_session(&state, &options.session_id)?.clone();
    let resolved = state.resolve_session_view(&session);
    let path = options.operands[0].clone();
    ensure_path_visible_to_sunlight(&ctx.repo_root, &path)?;
    let before = real_entry(&state, &resolved.entries, &path)?.clone();
    if before.content_hash != expected {
        return Err(real_precondition_error(&state, &before, &expected));
    }
    let mut after = before.clone();
    after.tombstone = true;
    let response = real_accept_mutation(
        &mut state,
        &options.session_id,
        "delete",
        &path,
        Some(before),
        after,
    )?;
    finish_real_mutation(ctx, state, response)
}

fn real_artifact_metadata_set(
    ctx: &CommandContext,
    options: MutationCommandOptions,
    expected: String,
    classification: String,
) -> Result<(), CliError> {
    let mut state = RealRepoState::load(&ctx.repo_root)?;
    let session = real_session(&state, &options.session_id)?.clone();
    let resolved = state.resolve_session_view(&session);
    let path = options.operands[0].clone();
    ensure_path_visible_to_sunlight(&ctx.repo_root, &path)?;
    let before = real_entry(&state, &resolved.entries, &path)?.clone();
    if before.content_hash != expected {
        return Err(real_precondition_error(&state, &before, &expected));
    }
    let mut after = before.clone();
    after.classification = classification;
    let response = real_accept_mutation(
        &mut state,
        &options.session_id,
        "metadata_set",
        &path,
        Some(before),
        after,
    )?;
    finish_real_mutation(ctx, state, response)
}

fn real_view_resolve(ctx: &CommandContext, options: ViewResolveOptions) -> Result<(), CliError> {
    let state = RealRepoState::load_metadata(&ctx.repo_root)?;
    let starting_checkpoint_id = options
        .base_checkpoint_id
        .as_deref()
        .unwrap_or(&state.base_checkpoint_id);
    let starting = real_resolve_checkpoint_by_id(&state, starting_checkpoint_id)?;
    let starting_frontier = starting.result.topic_frontier.clone();
    let included = options
        .include
        .iter()
        .map(|selection| {
            let topic = real_topic(&state, &selection.topic_id)?;
            if !state.operations.iter().any(|operation| {
                operation.topic_id == topic.topic_id
                    && operation.topic_revision_id == selection.revision_id
            }) {
                return Err(object_not_found("topic_revision", &selection.revision_id));
            }
            Ok(TopicRevisionSelection {
                topic_id: topic.topic_id.clone(),
                revision_id: selection.revision_id.clone(),
            })
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    let included_frontier = exact_requested_frontier(&included)?;
    let discovery_only = included.is_empty() && starting_checkpoint_id == state.base_checkpoint_id;
    let mut requested_frontier = starting_frontier.clone();
    requested_frontier.extend(included_frontier);
    let resolved = if discovery_only {
        state.resolve_head_view()
    } else {
        state.resolve_view(
            requested_frontier
                .iter()
                .map(|(topic_id, revision_id)| TopicRevisionSelection {
                    topic_id: topic_id.clone(),
                    revision_id: revision_id.clone(),
                })
                .collect(),
        )
    };
    let view = resolved.result;
    ensure_resolver_preserved_frontier(&requested_frontier, &view.topic_frontier)?;
    state.persist_record(
        &ctx.repo_root,
        "views",
        &view.resolved_view_id,
        &format!("{}\n", resolved_view_record_json(&view)),
    )?;
    for record in &view.records {
        state.persist_record(
            &ctx.repo_root,
            "conflicts",
            &record.id,
            &format!("{}\n", resolver_record_json(record)),
        )?;
    }
    if ctx.json {
        outputln!(
            ctx,
            "{}",
            view_resolve_success_envelope(
                &view,
                &requested_frontier,
                starting_checkpoint_id,
                &starting_frontier,
            )
        );
    } else {
        let tree_hash = view
            .tree_identity
            .as_ref()
            .map(|tree| tree.tree_hash.as_str())
            .unwrap_or("conflicted");
        outputln!(ctx, "{} {}", view.resolved_view_id, tree_hash);
    }
    Ok(())
}

fn real_project_materialize(
    ctx: &CommandContext,
    options: ProjectMaterializeOptions,
) -> Result<(), CliError> {
    let repo_root = ctx.repo_root.clone();
    let projection_policy = require_projection_policy(&repo_root)?;
    let mut state = RealRepoState::load(&repo_root)?;
    let resolved = real_resolve_view_by_id(&repo_root, &state, &options.view_id)?;
    if !resolved.result.conflict_free() {
        return Err(CliError::new(
            "conflicted_view",
            "cannot materialize a conflicted resolved view",
        )
        .with_detail("resolved_view_id", resolved.result.resolved_view_id));
    }
    let view_state = real_view_state(&state, &resolved);
    let readonly_reuse_requested = options.purpose == ProjectionPurpose::Inspection
        && options.projection_root.is_none()
        && matches!(
            options.strategy,
            None | Some(ProjectionStrategy::HardlinkReadonly)
        );
    if readonly_reuse_requested {
        for projection in state.projections.iter().rev().filter(|projection| {
            projection.purpose == ProjectionPurpose::Inspection.as_str()
                && projection.resolved_view_id == view_state.resolved_view_id
                && projection.tree_hash == view_state.tree_hash
                && projection.strategy == RealProjectionStrategy::HardlinkReadonly.as_str()
                && projection.retention_state == "active"
        }) {
            let Some(root) = projection.materialized_root.as_ref().map(PathBuf::from) else {
                continue;
            };
            let verification_started = Instant::now();
            if verify_real_readonly_projection(&root, &view_state.entries).is_err() {
                continue;
            }
            let logical_bytes = view_state
                .entries
                .iter()
                .filter(|entry| !entry.tombstone)
                .map(|entry| entry.bytes.len() as u64)
                .sum::<u64>();
            let file_count = view_state
                .entries
                .iter()
                .filter(|entry| !entry.tombstone)
                .count() as u64;
            let materialization = RealProjectionMaterialization {
                cache_key: projection.cache_key.clone(),
                strategy: RealProjectionStrategy::HardlinkReadonly,
                metrics: RealProjectionMaterializationMetrics {
                    elapsed_ms: verification_started
                        .elapsed()
                        .as_millis()
                        .min(u64::MAX as u128) as u64,
                    logical_bytes,
                    physically_materialized_bytes: Some(0),
                    physical_allocation_bytes: Some(0),
                    file_count,
                    cache_hit: true,
                    reuse: "reused_verified_projection".to_string(),
                    integrity_revalidated: true,
                    storage_amplification_millionths: if logical_bytes == 0 {
                        None
                    } else {
                        Some(0)
                    },
                },
            };
            if ctx.json {
                outputln!(
                    ctx,
                    "{}",
                    real_projection_materialized_envelope(
                        &view_state,
                        &projection.projection_id,
                        options.purpose,
                        &root,
                        &materialization,
                    )
                );
            } else {
                outputln!(ctx, "{} {}", projection.projection_id, root.display());
            }
            return Ok(());
        }
    }
    let provisional_projection_id = format!(
        "projection_{}_native_{:04}",
        options.purpose.as_str(),
        state.projections.len() + 1
    );
    let caller_root = options.projection_root;
    let provisional_root = caller_root.clone().unwrap_or_else(|| {
        projection_policy.projection_root(options.purpose.as_str(), &provisional_projection_id)
    });
    let materialization = materialize_repo_projection(
        &repo_root,
        &view_state,
        &provisional_root,
        options.purpose,
        options.strategy,
        options.fallback_to_copy,
    )?;
    let projection_id = selected_real_projection_id(
        options.purpose,
        materialization.strategy,
        state.projections.len() + 1,
    );
    let root = caller_root.unwrap_or_else(|| {
        projection_policy.projection_root(options.purpose.as_str(), &projection_id)
    });
    relocate_managed_projection_root(&provisional_root, &root)?;
    let manifest_digest = real_projection_manifest_digest(
        &view_state,
        &projection_id,
        options.purpose,
        materialization.strategy,
    );
    state.projections.push(RealProjectionSnapshot {
        projection_id: projection_id.clone(),
        repository_id: state.repository_id.clone(),
        purpose: options.purpose.as_str().to_string(),
        resolved_view_id: view_state.resolved_view_id.clone(),
        tree_hash: view_state.tree_hash.clone(),
        topic_frontier: resolved.result.topic_frontier.clone(),
        manifest_digest,
        created_from_content_tree: view_state.tree_hash.clone(),
        materialized_root: Some(root.display().to_string()),
        session_id: None,
        session_generation_id: None,
        path_policy_id: POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string(),
        operation_semantics_version: FILE_OPERATION_SEMANTICS_VERSION.to_string(),
        cache_key: materialization.cache_key.clone(),
        strategy: materialization.strategy.as_str().to_string(),
        materialization: Some(materialization.metrics.clone()),
        retention_state: "active".to_string(),
        privacy_class: "local_only".to_string(),
        last_import_operation_id: None,
        entries: view_state.entries.clone(),
    });
    let records = vec![real_projection_publication(
        &state,
        state.projections.last().unwrap(),
    )?];
    state.save_with_records(&repo_root, &records)?;
    if ctx.json {
        outputln!(
            ctx,
            "{}",
            real_projection_materialized_envelope(
                &view_state,
                &projection_id,
                options.purpose,
                &root,
                &materialization,
            )
        );
    } else {
        outputln!(ctx, "{} {}", projection_id, root.display());
    }
    Ok(())
}

#[derive(Debug)]
struct BoundedStreamSummary {
    observed_digest: String,
    observed_byte_length: u64,
    captured_byte_length: u64,
    captured_bytes: Vec<u8>,
    truncated: bool,
    capture_failed: bool,
}

#[derive(Debug)]
struct BoundedProcessOutput {
    status: Option<ExitStatus>,
    cancelled: bool,
    timed_out: bool,
    termination_failed: bool,
    wait_failed: bool,
    resource_termination: Option<String>,
    stdout: BoundedStreamSummary,
    stderr: BoundedStreamSummary,
}

#[derive(Debug)]
enum ProcessRunError {
    Command(io::Error),
    #[cfg(windows)]
    ContainmentSetup(io::Error),
    Publication(RepoStateError),
    #[cfg(windows)]
    FilesystemIsolationSetup(io::Error),
    #[cfg(windows)]
    NetworkIsolationSetup(io::Error),
}

struct ExecutionChild {
    #[cfg(windows)]
    contained: windows_job::ContainedChild,
    #[cfg(not(windows))]
    child: Child,
}

impl ExecutionChild {
    fn spawn(command: Command, policy: &ExecutionPolicy) -> Result<Self, ProcessRunError> {
        #[cfg(windows)]
        {
            return windows_job::ContainedChild::spawn_with_process_overhead(command, policy, 1)
                .map(|contained| Self { contained })
                .map_err(|error| match error {
                    windows_job::ContainmentSpawnError::Setup(error) => {
                        ProcessRunError::ContainmentSetup(error)
                    }
                    windows_job::ContainmentSpawnError::Command(error) => {
                        ProcessRunError::Command(error)
                    }
                });
        }
        #[cfg(not(windows))]
        {
            let _ = policy;
            let mut command = command;
            command
                .spawn()
                .map(|child| Self { child })
                .map_err(ProcessRunError::Command)
        }
    }

    fn child_mut(&mut self) -> &mut Child {
        #[cfg(windows)]
        {
            &mut self.contained.child
        }
        #[cfg(not(windows))]
        {
            &mut self.child
        }
    }

    fn poll_resource_termination(&self) -> io::Result<Option<String>> {
        #[cfg(windows)]
        {
            self.contained
                .poll_resource_termination()
                .map(|reason| reason.map(|reason| reason.as_str().to_string()))
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    }

    fn terminate_tree(&mut self) -> (bool, bool) {
        #[cfg(windows)]
        {
            let terminated = self.contained.terminate().is_ok();
            (!terminated, terminated)
        }
        #[cfg(not(windows))]
        {
            terminate_execution_process_tree(&mut self.child)
        }
    }
}

fn execution_environment_allowlist() -> Vec<&'static str> {
    if cfg!(windows) {
        vec![
            "PATH",
            "PATHEXT",
            "SYSTEMROOT",
            "WINDIR",
            "COMSPEC",
            "TEMP",
            "TMP",
            "USERPROFILE",
            "HOMEDRIVE",
            "HOMEPATH",
            "LOCALAPPDATA",
            "APPDATA",
        ]
    } else {
        vec!["PATH", "HOME", "TMPDIR"]
    }
}

const MAX_TOOL_EXECUTABLE_HASH_BYTES: u64 = 256 * 1024 * 1024;
const COMMAND_RUNNER_VERSION: &str = "bounded_local_process_v3";

fn redacted_environment_allowlist_digest(allowlist: &[String]) -> String {
    let mut names = allowlist.to_vec();
    names.sort();
    names.dedup();
    let mut hasher = Sha256::new();
    for name in names {
        hasher.update((name.len() as u64).to_le_bytes());
        hasher.update(name.as_bytes());
        match env::var_os(&name) {
            Some(value) => {
                let value = value.to_string_lossy();
                hasher.update([1]);
                hasher.update((value.len() as u64).to_le_bytes());
                hasher.update(value.as_bytes());
            }
            None => hasher.update([0]),
        }
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn resolve_execution_program(program: &str, cwd: &Path) -> Option<PathBuf> {
    let path = Path::new(program);
    if path.is_absolute() || path.components().count() > 1 {
        let candidate = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };
        if candidate.is_file() {
            return fs::canonicalize(candidate).ok();
        }
        #[cfg(windows)]
        if candidate.extension().is_none() {
            let candidate = candidate.with_extension("exe");
            if candidate.is_file() {
                return fs::canonicalize(candidate).ok();
            }
        }
        return None;
    }
    #[cfg(windows)]
    {
        windows_isolation::search_executable(program)
            .ok()
            .and_then(|path| fs::canonicalize(path).ok())
    }
    #[cfg(not(windows))]
    {
        let path = env::var_os("PATH")?;
        env::split_paths(&path)
            .map(|root| root.join(program))
            .find(|candidate| candidate.is_file())
            .and_then(|path| fs::canonicalize(path).ok())
    }
}

fn bounded_file_digest(path: &Path) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_TOOL_EXECUTABLE_HASH_BYTES {
        return None;
    }
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return None,
        };
        hasher.update(&buffer[..count]);
    }
    Some(format!("sha256:{:x}", hasher.finalize()))
}

fn capture_execution_environment_summary(
    argv: &[String],
    cwd: &Path,
    environment_policy: &str,
    environment_allowlist: &[String],
    network_policy: &str,
    filesystem_write_policy: &str,
) -> RealExecutionEnvironmentSummary {
    let tool_hints = argv
        .first()
        .and_then(|program| {
            let path = resolve_execution_program(program, cwd)?;
            let version_or_digest = bounded_file_digest(&path)?;
            let mut name = Path::new(program)
                .file_name()?
                .to_string_lossy()
                .to_string();
            if cfg!(windows) && name.to_ascii_lowercase().ends_with(".exe") {
                name.truncate(name.len() - 4);
            }
            if name.is_empty() || name.chars().count() > 64 {
                return None;
            }
            Some(RealExecutionToolHint {
                name,
                version_or_digest,
            })
        })
        .into_iter()
        .collect();
    let mut summary = RealExecutionEnvironmentSummary {
        os: env::consts::OS.to_string(),
        platform_hint: if cfg!(windows) {
            "windows_low_integrity_local".to_string()
        } else {
            "local_process".to_string()
        },
        arch: env::consts::ARCH.to_string(),
        sunlight_build_id: format!("sun/{}", env!("CARGO_PKG_VERSION")),
        command_runner_version: COMMAND_RUNNER_VERSION.to_string(),
        tool_hints,
        redacted_env_allowlist_digest: redacted_environment_allowlist_digest(environment_allowlist),
        digest: String::new(),
    };
    if cfg!(windows) && network_policy == "windows_appcontainer_no_network_capabilities_v1" {
        summary.platform_hint = "windows_low_integrity_appcontainer".to_string();
    }
    summary.digest = real_execution_environment_summary_digest(
        &summary,
        environment_policy,
        environment_allowlist,
        network_policy,
        filesystem_write_policy,
    );
    summary
}

fn summarize_bounded_stream<R: Read>(mut stream: R, limit: u64) -> BoundedStreamSummary {
    let mut hasher = Sha256::new();
    let mut observed = 0_u64;
    let mut captured = 0_u64;
    let mut captured_bytes = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut capture_failed = false;
    loop {
        let count = match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => {
                capture_failed = true;
                break;
            }
        };
        observed = observed.saturating_add(count as u64);
        hasher.update(&buffer[..count]);
        let remaining = limit.saturating_sub(captured) as usize;
        let captured_now = count.min(remaining);
        captured += captured_now as u64;
        captured_bytes.extend_from_slice(&buffer[..captured_now]);
    }
    BoundedStreamSummary {
        observed_digest: format!("sha256:{:x}", hasher.finalize()),
        observed_byte_length: observed,
        captured_byte_length: captured,
        captured_bytes,
        truncated: observed > captured,
        capture_failed,
    }
}

#[cfg(not(windows))]
fn terminate_execution_process_tree(child: &mut Child) -> (bool, bool) {
    let pid = child.id().to_string();
    #[cfg(unix)]
    {
        let process_group = format!("-{pid}");
        let group_killed = Command::new("kill")
            .args(["-KILL", "--", &process_group])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if !group_killed {
            return (true, child.kill().is_ok());
        }
        return (false, true);
    }
    #[cfg(not(any(windows, unix)))]
    {
        let root_killed = child.kill().is_ok();
        (!root_killed, root_killed)
    }
}

fn run_bounded_process(
    argv: &[String],
    cwd: &Path,
    policy: &ExecutionPolicy,
    cancellation: &AtomicBool,
    #[cfg(windows)] isolation: &windows_isolation::PreparedIsolation,
    on_started: impl FnOnce() -> Result<(), RepoStateError>,
) -> Result<BoundedProcessOutput, ProcessRunError> {
    let allowlist = execution_environment_allowlist();
    #[cfg(windows)]
    let mut command = isolation
        .bootstrap_command(argv)
        .map_err(ProcessRunError::ContainmentSetup)?;
    #[cfg(not(windows))]
    let mut command = Command::new(&argv[0]);
    command
        .args(argv.iter().skip(1))
        .current_dir(cwd)
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for name in &allowlist {
        if let Some(value) = env::var_os(name) {
            command.env(name, value);
        }
    }
    #[cfg(windows)]
    isolation.configure_environment(&mut command);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut execution_child = ExecutionChild::spawn(command, policy)?;
    if let Err(error) = on_started() {
        let (_, root_termination_started) = execution_child.terminate_tree();
        if root_termination_started {
            let _ = execution_child.child_mut().wait();
        }
        return Err(ProcessRunError::Publication(error));
    }
    let stdout_limit = policy.stdout_limit_bytes;
    let stderr_limit = policy.stderr_limit_bytes;
    let stdout_reader = execution_child.child_mut().stdout.take().map(|stdout| {
        thread::Builder::new()
            .name("sun-stdout-reader".to_string())
            .spawn(move || summarize_bounded_stream(stdout, stdout_limit))
    });
    let stderr_reader = execution_child.child_mut().stderr.take().map(|stderr| {
        thread::Builder::new()
            .name("sun-stderr-reader".to_string())
            .spawn(move || summarize_bounded_stream(stderr, stderr_limit))
    });
    let capture_setup_failed = stdout_reader.as_ref().is_some_and(Result::is_err)
        || stderr_reader.as_ref().is_some_and(Result::is_err);
    let deadline = Instant::now() + Duration::from_millis(policy.timeout_ms);
    let mut status = None;
    let mut cancelled = false;
    let mut timed_out = false;
    let mut termination_failed = false;
    let mut wait_failed = false;
    let mut resource_termination = None;
    let mut child_reaped = false;
    loop {
        if cancellation.load(Ordering::Acquire) {
            cancelled = true;
            let (tree_failed, root_termination_started) = execution_child.terminate_tree();
            termination_failed = tree_failed;
            if root_termination_started {
                match execution_child.child_mut().wait() {
                    Ok(exit_status) => {
                        status = Some(exit_status);
                        child_reaped = true;
                    }
                    Err(_) => wait_failed = true,
                }
            } else {
                wait_failed = true;
            }
            break;
        }
        if capture_setup_failed {
            wait_failed = true;
            let (tree_failed, root_termination_started) = execution_child.terminate_tree();
            termination_failed = tree_failed;
            if root_termination_started {
                match execution_child.child_mut().wait() {
                    Ok(exit_status) => {
                        status = Some(exit_status);
                        child_reaped = true;
                    }
                    Err(_) => wait_failed = true,
                }
            }
            break;
        }
        match execution_child.poll_resource_termination() {
            Ok(Some(reason)) => {
                resource_termination = Some(reason);
                let (tree_failed, root_termination_started) = execution_child.terminate_tree();
                termination_failed = tree_failed;
                if root_termination_started {
                    match execution_child.child_mut().wait() {
                        Ok(exit_status) => {
                            status = Some(exit_status);
                            child_reaped = true;
                        }
                        Err(_) => wait_failed = true,
                    }
                }
                break;
            }
            Ok(None) => {}
            Err(_) => {
                wait_failed = true;
                let (tree_failed, root_termination_started) = execution_child.terminate_tree();
                termination_failed = tree_failed;
                if root_termination_started {
                    match execution_child.child_mut().wait() {
                        Ok(exit_status) => {
                            status = Some(exit_status);
                            child_reaped = true;
                        }
                        Err(_) => wait_failed = true,
                    }
                }
                break;
            }
        }
        match execution_child.child_mut().try_wait() {
            Ok(Some(exit_status)) => {
                status = Some(exit_status);
                child_reaped = true;
                break;
            }
            Ok(None) => {}
            Err(_) => {
                wait_failed = true;
                let (tree_failed, root_termination_started) = execution_child.terminate_tree();
                termination_failed = tree_failed;
                if root_termination_started {
                    match execution_child.child_mut().wait() {
                        Ok(exit_status) => {
                            status = Some(exit_status);
                            child_reaped = true;
                        }
                        Err(_) => wait_failed = true,
                    }
                }
                break;
            }
        }
        let now = Instant::now();
        if now >= deadline {
            match execution_child.child_mut().try_wait() {
                Ok(Some(exit_status)) => {
                    status = Some(exit_status);
                    child_reaped = true;
                    break;
                }
                Ok(None) => {}
                Err(_) => wait_failed = true,
            }
            timed_out = true;
            let (tree_failed, root_termination_started) = execution_child.terminate_tree();
            termination_failed = tree_failed;
            if root_termination_started {
                match execution_child.child_mut().wait() {
                    Ok(exit_status) => {
                        status = Some(exit_status);
                        child_reaped = true;
                    }
                    Err(_) => wait_failed = true,
                }
            } else {
                wait_failed = true;
            }
            break;
        }
        thread::sleep((deadline - now).min(Duration::from_millis(10)));
    }
    #[cfg(windows)]
    if child_reaped && !cancelled && !timed_out && resource_termination.is_none() {
        let (tree_failed, _) = execution_child.terminate_tree();
        termination_failed |= tree_failed;
        if let Ok(reason) = execution_child.poll_resource_termination() {
            resource_termination = reason;
        }
    }
    let failed_summary = || BoundedStreamSummary {
        observed_digest: real_content_hash(&[]),
        observed_byte_length: 0,
        captured_byte_length: 0,
        captured_bytes: Vec::new(),
        truncated: false,
        capture_failed: true,
    };
    let streams_closed = child_reaped && !termination_failed;
    let stdout = if streams_closed {
        stdout_reader
            .and_then(Result::ok)
            .and_then(|reader| reader.join().ok())
            .unwrap_or_else(failed_summary)
    } else {
        failed_summary()
    };
    let stderr = if streams_closed {
        stderr_reader
            .and_then(Result::ok)
            .and_then(|reader| reader.join().ok())
            .unwrap_or_else(failed_summary)
    } else {
        failed_summary()
    };
    #[cfg(windows)]
    let bootstrap_verification = isolation
        .verify_bootstrap(status.as_ref().and_then(ExitStatus::code))
        .map_err(|error| match isolation.setup_dimension() {
            windows_isolation::IsolationSetupDimension::Filesystem => {
                ProcessRunError::FilesystemIsolationSetup(error)
            }
            windows_isolation::IsolationSetupDimension::Network => {
                ProcessRunError::NetworkIsolationSetup(error)
            }
        })?;
    #[cfg(windows)]
    if matches!(
        bootstrap_verification,
        windows_isolation::BootstrapVerification::FailedAfterCommandMayHaveStarted
    ) {
        wait_failed = true;
    }
    Ok(BoundedProcessOutput {
        status,
        cancelled,
        timed_out,
        termination_failed,
        wait_failed,
        resource_termination,
        stdout,
        stderr,
    })
}

fn real_execution_run(ctx: &CommandContext, options: ExecutionRunOptions) -> Result<(), CliError> {
    let overall_started = Instant::now();
    let repo_root = ctx.repo_root.clone();
    let config = require_repository_config(repo_root.clone())?;
    let projection_policy = resolve_required_projection_policy(&repo_root, &config)?;
    #[cfg(windows)]
    windows_isolation::recover_stale_app_containers(&repo_root, &projection_policy.managed_root)
        .map_err(|error| {
            CliError::new(
                "execution_network_isolation_cleanup_failed",
                format!("failed to recover stale Windows isolation state: {error}"),
            )
            .with_detail("platform", "windows")
            .with_detail("command_started", "false")
        })?;
    let mut execution_policy = config.execution_policy.clone();
    if let Some(network_policy) = options.network_policy {
        execution_policy.network_policy = network_policy.as_str().to_string();
    }
    let mut state = RealRepoState::load(&repo_root)?;
    let relative_cwd = real_execution_relative_cwd(&options.cwd)?;
    let resolved = real_resolve_view_by_id(&repo_root, &state, &options.view_id)?;
    if !resolved.result.conflict_free() {
        return Err(CliError::new(
            "execution_conflicted_view",
            "cannot execute a conflicted resolved view",
        )
        .with_detail("resolved_view_id", resolved.result.resolved_view_id));
    }
    let view_state = real_view_state(&state, &resolved);
    let provisional_projection_id = format!(
        "projection_execution_native_{:04}",
        state.projections.len() + 1
    );
    let execution_id = format!("exec_native_{:04}", state.executions.len() + 1);
    let provisional_root = projection_policy.execution_root(&provisional_projection_id);
    let materialization_started = Instant::now();
    let materialization = materialize_repo_projection(
        &repo_root,
        &view_state,
        &provisional_root,
        ProjectionPurpose::Execution,
        None,
        true,
    )?;
    let projection_id = selected_real_projection_id(
        ProjectionPurpose::Execution,
        materialization.strategy,
        state.projections.len() + 1,
    );
    let projection_root = projection_policy.execution_root(&projection_id);
    relocate_managed_projection_root(&provisional_root, &projection_root)?;
    let materialization_ms = materialization_started.elapsed().as_millis();
    #[cfg(windows)]
    let mut isolation = windows_isolation::PreparedIsolation::prepare(
        &repo_root,
        &projection_root,
        &execution_id,
        execution_policy.network_policy == DISABLED_NETWORK_POLICY,
    )
    .map_err(|error| {
        remove_unpublished_execution_root(&projection_root);
        let (code, dimension) = match error.dimension() {
            windows_isolation::IsolationSetupDimension::Network => {
                ("execution_network_isolation_setup_failed", "network")
            }
            windows_isolation::IsolationSetupDimension::Filesystem => {
                ("execution_filesystem_isolation_setup_failed", "filesystem")
            }
        };
        CliError::new(
            code,
            format!("failed to establish Windows {dimension} isolation: {error}"),
        )
        .with_detail("platform", "windows")
        .with_detail("command_started", "false")
    })?;
    #[cfg(windows)]
    isolation
        .validate_command_compatibility(&options.command_argv)
        .map_err(|error| {
            remove_unpublished_execution_root(&projection_root);
            CliError::new(
                "execution_network_isolation_incompatible_toolchain",
                error.to_string(),
            )
            .with_detail("platform", "windows")
            .with_detail("network_policy_requested", "disabled")
            .with_detail("command_started", "false")
        })?;
    let execution_cwd = real_execution_cwd_path(&projection_root, &relative_cwd, &options.cwd)
        .map_err(|error| {
            remove_unpublished_execution_root(&projection_root);
            error
        })?;
    let manifest_digest = real_projection_manifest_digest(
        &view_state,
        &projection_id,
        ProjectionPurpose::Execution,
        materialization.strategy,
    );
    state.projections.push(RealProjectionSnapshot {
        projection_id: projection_id.clone(),
        repository_id: state.repository_id.clone(),
        purpose: "execution".to_string(),
        resolved_view_id: view_state.resolved_view_id.clone(),
        tree_hash: view_state.tree_hash.clone(),
        topic_frontier: resolved.result.topic_frontier.clone(),
        manifest_digest: manifest_digest.clone(),
        created_from_content_tree: view_state.tree_hash.clone(),
        materialized_root: Some(projection_root.display().to_string()),
        session_id: None,
        session_generation_id: None,
        path_policy_id: POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string(),
        operation_semantics_version: FILE_OPERATION_SEMANTICS_VERSION.to_string(),
        cache_key: materialization.cache_key.clone(),
        strategy: materialization.strategy.as_str().to_string(),
        materialization: Some(materialization.metrics),
        retention_state: "active".to_string(),
        privacy_class: "local_only".to_string(),
        last_import_operation_id: None,
        entries: view_state.entries.clone(),
    });

    let environment_allowlist = execution_environment_allowlist()
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let network_policy_effective =
        if cfg!(windows) && execution_policy.network_policy == DISABLED_NETWORK_POLICY {
            "windows_appcontainer_no_network_capabilities_v1".to_string()
        } else {
            "not_enforced".to_string()
        };
    let filesystem_write_policy_effective = if cfg!(windows) {
        "windows_low_integrity_private_projection_v1".to_string()
    } else {
        "not_enforced".to_string()
    };
    let environment_summary = capture_execution_environment_summary(
        &options.command_argv,
        &execution_cwd,
        &execution_policy.environment_inheritance,
        &environment_allowlist,
        &network_policy_effective,
        &filesystem_write_policy_effective,
    );
    let started_at = real_now_id();
    let empty_output_digest = real_content_hash(&[]);
    let running_execution = RealExecutionSnapshot {
        execution_id: execution_id.clone(),
        projection_id: projection_id.clone(),
        resolved_view_id: view_state.resolved_view_id.clone(),
        tree_hash: view_state.tree_hash.clone(),
        command_argv: options.command_argv.clone(),
        working_directory: options.cwd.clone(),
        exit_code: None,
        status: "running".to_string(),
        runner_process_id: Some(std::process::id()),
        command_started: true,
        timed_out: false,
        termination_reason: None,
        termination_failed: false,
        wait_failed: false,
        stdout_observed_digest: empty_output_digest.clone(),
        stdout_byte_length: 0,
        stdout_captured_byte_length: 0,
        stdout_truncated: false,
        stdout_capture_failed: false,
        stderr_observed_digest: empty_output_digest,
        stderr_byte_length: 0,
        stderr_captured_byte_length: 0,
        stderr_truncated: false,
        stderr_capture_failed: false,
        timeout_ms: Some(execution_policy.timeout_ms),
        process_memory_limit_bytes: Some(execution_policy.process_memory_limit_bytes),
        job_memory_limit_bytes: Some(execution_policy.job_memory_limit_bytes),
        cpu_time_limit_ms: Some(execution_policy.cpu_time_limit_ms),
        active_process_limit: Some(execution_policy.active_process_limit),
        process_tree_policy: if cfg!(windows) {
            "windows_job_object_kill_on_close".to_string()
        } else {
            "not_enforced".to_string()
        },
        cpu_policy: if cfg!(windows) {
            "windows_job_object_cpu_time".to_string()
        } else {
            "not_enforced".to_string()
        },
        memory_policy: if cfg!(windows) {
            "windows_job_object_process_and_job_memory".to_string()
        } else {
            "not_enforced".to_string()
        },
        environment_policy: execution_policy.environment_inheritance.clone(),
        environment_allowlist: environment_allowlist.clone(),
        environment_summary: environment_summary.clone(),
        network_policy_requested: execution_policy.network_policy.clone(),
        network_policy: network_policy_effective.clone(),
        filesystem_write_policy_requested: execution_policy.filesystem_write_policy.clone(),
        filesystem_write_policy: filesystem_write_policy_effective.clone(),
        outputs: Vec::new(),
        started_at: started_at.clone(),
        finished_at: String::new(),
        privacy_class: "policy_gated".to_string(),
    };
    state.executions.push(running_execution.clone());
    let running_records = vec![
        real_projection_publication(&state, state.projections.last().unwrap())?,
        state.record_publication(
            "executions",
            &running_execution.execution_id,
            &format!(
                "{}\n",
                real_execution_snapshot_record_json(&state, &running_execution)
            ),
        )?,
    ];
    let command_started = Instant::now();
    let command_output = run_bounded_process(
        &options.command_argv,
        &execution_cwd,
        &execution_policy,
        &ctx.cancellation,
        #[cfg(windows)]
        &isolation,
        || state.save_with_records(&repo_root, &running_records),
    )
    .map_err(|error| match error {
        ProcessRunError::Command(error) => {
            remove_unpublished_execution_root(&projection_root);
            CliError::new(
                "execution_command_failed",
                format!("failed to run command: {error}"),
            )
            .with_detail("command", options.command_argv.join(" "))
        }
        #[cfg(windows)]
        ProcessRunError::ContainmentSetup(error) => {
            remove_unpublished_execution_root(&projection_root);
            CliError::new(
                "execution_containment_setup_failed",
                format!("failed to establish execution containment: {error}"),
            )
            .with_detail("platform", std::env::consts::OS)
            .with_detail("command_started", "false")
        }
        ProcessRunError::Publication(error) => {
            remove_unpublished_execution_root(&projection_root);
            CliError::from(error)
        }
        #[cfg(windows)]
        ProcessRunError::FilesystemIsolationSetup(error) => {
            remove_unpublished_execution_root(&projection_root);
            CliError::new(
                "execution_filesystem_isolation_setup_failed",
                format!("failed to establish Windows filesystem isolation: {error}"),
            )
            .with_detail("platform", "windows")
            .with_detail("command_started", "false")
        }
        #[cfg(windows)]
        ProcessRunError::NetworkIsolationSetup(error) => {
            remove_unpublished_execution_root(&projection_root);
            CliError::new(
                "execution_network_isolation_setup_failed",
                format!("failed to establish Windows network isolation: {error}"),
            )
            .with_detail("platform", "windows")
            .with_detail("command_started", "false")
        }
    })?;
    let command_ms = command_started.elapsed().as_millis();
    let stdout_text = String::from_utf8_lossy(&command_output.stdout.captured_bytes).into_owned();
    let stderr_text = String::from_utf8_lossy(&command_output.stderr.captured_bytes).into_owned();
    let finished_at = real_now_id();
    let output_scan_started = Instant::now();
    let mut projected_entries = Vec::new();
    let mut quarantine = Vec::new();
    scan_real_projection_files_with_quarantine(
        &projection_root,
        &projection_root,
        &mut projected_entries,
        &mut quarantine,
    )?;
    projected_entries.sort_by(|left, right| left.path.cmp(&right.path));
    let outputs = if command_output.cancelled {
        Vec::new()
    } else {
        real_execution_outputs(
            &repo_root,
            &view_state.entries,
            &projected_entries,
            &ctx.cancellation,
        )?
    };
    let output_scan_ms = output_scan_started.elapsed().as_millis();
    state = RealRepoState::load(&repo_root)?;
    #[cfg(windows)]
    let cleanup_error = isolation.finish().err().map(|error| error.to_string());
    #[cfg(not(windows))]
    let cleanup_error: Option<String> = None;
    if cleanup_error.is_some() {
        if let Some(projection) = state.projections.last_mut() {
            projection.retention_state = "quarantined".to_string();
        }
    }
    let status = if command_output.cancelled {
        "canceled"
    } else if command_output.resource_termination.is_some() || cleanup_error.is_some() {
        "policy_blocked"
    } else if command_output.stdout.capture_failed
        || command_output.stderr.capture_failed
        || command_output.termination_failed
        || command_output.wait_failed
    {
        "fail"
    } else if command_output.timed_out {
        "timeout"
    } else if command_output.status.is_some_and(|status| status.success()) {
        "pass"
    } else {
        "fail"
    };
    let termination_reason = if command_output.cancelled {
        Some("request_cancelled".to_string())
    } else {
        cleanup_error
            .as_ref()
            .map(|_| "execution_network_isolation_cleanup_failed".to_string())
            .or_else(|| command_output.resource_termination.clone())
            .or_else(|| {
                if command_output.timed_out {
                    Some("wall_clock_timeout".to_string())
                } else if command_output.stdout.capture_failed
                    || command_output.stderr.capture_failed
                    || command_output.termination_failed
                    || command_output.wait_failed
                {
                    Some("runner_failure".to_string())
                } else {
                    Some("command_exit".to_string())
                }
            })
    };
    let mut execution = running_execution;
    execution.exit_code = command_output.status.and_then(|status| status.code());
    execution.status = status.to_string();
    execution.runner_process_id = None;
    execution.timed_out = command_output.timed_out;
    execution.termination_reason = termination_reason;
    execution.termination_failed = command_output.termination_failed;
    execution.wait_failed = command_output.wait_failed;
    execution.stdout_observed_digest = command_output.stdout.observed_digest;
    execution.stdout_byte_length = command_output.stdout.observed_byte_length;
    execution.stdout_captured_byte_length = command_output.stdout.captured_byte_length;
    execution.stdout_truncated = command_output.stdout.truncated;
    execution.stdout_capture_failed = command_output.stdout.capture_failed;
    execution.stderr_observed_digest = command_output.stderr.observed_digest;
    execution.stderr_byte_length = command_output.stderr.observed_byte_length;
    execution.stderr_captured_byte_length = command_output.stderr.captured_byte_length;
    execution.stderr_truncated = command_output.stderr.truncated;
    execution.stderr_capture_failed = command_output.stderr.capture_failed;
    execution.outputs = outputs;
    execution.finished_at = finished_at;
    let publication_started = Instant::now();
    let persisted_execution = state
        .executions
        .iter_mut()
        .find(|persisted| persisted.execution_id == execution.execution_id)
        .ok_or_else(|| object_not_found("execution", &execution.execution_id))?;
    *persisted_execution = execution.clone();
    let records = vec![
        real_projection_publication(&state, state.projections.last().unwrap())?,
        state.record_publication(
            "executions",
            &execution.execution_id,
            &format!(
                "{}\n",
                real_execution_snapshot_record_json(&state, &execution)
            ),
        )?,
    ];
    state.save_with_records(&repo_root, &records)?;
    let publication_ms = publication_started.elapsed().as_millis();
    let total_ms = overall_started.elapsed().as_millis();
    if let Some(cleanup_error) = cleanup_error {
        return Err(CliError::new(
            "execution_network_isolation_cleanup_failed",
            format!("failed to clean up Windows execution isolation: {cleanup_error}"),
        )
        .with_detail("platform", "windows")
        .with_detail("command_started", "true")
        .with_detail("execution_id", execution.execution_id));
    }
    if command_output.cancelled {
        return Err(CliError::new(
            "request_cancelled",
            "tool call was cancelled; the canceled execution record is durable",
        )
        .with_detail("execution_id", execution.execution_id));
    }
    if ctx.json {
        outputln!(
            ctx,
            "{}",
            real_execution_run_success_envelope(
                &state,
                &execution,
                &stdout_text,
                &stderr_text,
                materialization_ms,
                command_ms,
                output_scan_ms,
                publication_ms,
                total_ms,
            )
        );
    } else {
        outputln!(ctx, "{} {}", execution.execution_id, execution.status);
    }
    Ok(())
}

fn remove_unpublished_execution_root(projection_root: &Path) {
    if let Some(allocation_root) = projection_root.parent() {
        let _ = fs::remove_dir_all(allocation_root);
    }
}

fn recover_interrupted_executions(
    repo_root: &Path,
    state: &mut RealRepoState,
) -> Result<(), CliError> {
    let interrupted_ids = state
        .executions
        .iter()
        .filter(|execution| {
            execution.status == "running"
                && execution
                    .runner_process_id
                    .is_none_or(|process_id| !process_may_be_live(process_id))
        })
        .map(|execution| execution.execution_id.clone())
        .collect::<Vec<_>>();
    if interrupted_ids.is_empty() {
        return Ok(());
    }

    let recovered_at = real_now_id();
    for execution in &mut state.executions {
        if interrupted_ids.contains(&execution.execution_id) {
            execution.status = "interrupted".to_string();
            execution.runner_process_id = None;
            execution.termination_reason = Some("runner_process_terminated".to_string());
            execution.wait_failed = true;
            execution.finished_at = recovered_at.clone();
        }
    }
    for projection in &mut state.projections {
        if state.executions.iter().any(|execution| {
            interrupted_ids.contains(&execution.execution_id)
                && execution.projection_id == projection.projection_id
        }) {
            projection.retention_state = "quarantined".to_string();
        }
    }

    let mut records = Vec::new();
    for execution_id in &interrupted_ids {
        let execution = state
            .executions
            .iter()
            .find(|execution| execution.execution_id == *execution_id)
            .expect("interrupted execution remains present");
        records.push(
            state
                .record_publication(
                    "executions",
                    execution_id,
                    &format!(
                        "{}\n",
                        real_execution_snapshot_record_json(&state, execution)
                    ),
                )
                .map_err(CliError::from)?,
        );
    }
    state
        .save_with_records(repo_root, &records)
        .map_err(CliError::from)
}

fn real_execution_relative_cwd(cwd: &str) -> Result<PathBuf, CliError> {
    let display_cwd = if cwd.is_empty() { "." } else { cwd };
    let requested = PathBuf::from(display_cwd);
    if requested.is_absolute() {
        return Err(
            invalid_request("execution cwd must be relative to the projection root")
                .with_detail("cwd", display_cwd),
        );
    }
    let mut relative = PathBuf::new();
    for component in requested.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(part) => relative.push(part),
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(
                    invalid_request("execution cwd must stay inside the projection root")
                        .with_detail("cwd", display_cwd),
                );
            }
        }
    }
    Ok(relative)
}

fn real_execution_cwd_path(
    projection_root: &PathBuf,
    relative_cwd: &PathBuf,
    original_cwd: &str,
) -> Result<PathBuf, CliError> {
    let root = fs::canonicalize(projection_root).map_err(|error| {
        invalid_request(format!(
            "failed to resolve execution projection root: {error}"
        ))
    })?;
    let candidate = fs::canonicalize(projection_root.join(relative_cwd)).map_err(|error| {
        invalid_request(format!(
            "execution cwd was not found in the projection: {error}"
        ))
        .with_detail(
            "cwd",
            if original_cwd.is_empty() {
                "."
            } else {
                original_cwd
            },
        )
    })?;
    if !candidate.starts_with(&root) {
        return Err(
            invalid_request("execution cwd must stay inside the projection root").with_detail(
                "cwd",
                if original_cwd.is_empty() {
                    "."
                } else {
                    original_cwd
                },
            ),
        );
    }
    if !candidate.is_dir() {
        return Err(
            invalid_request("execution cwd is not a directory").with_detail(
                "cwd",
                if original_cwd.is_empty() {
                    "."
                } else {
                    original_cwd
                },
            ),
        );
    }
    Ok(candidate)
}

fn real_execution_promote_output(
    ctx: &CommandContext,
    options: ExecutionPromoteOutputOptions,
) -> Result<(), CliError> {
    let repo_root = ctx.repo_root.clone();
    let projection_policy = require_projection_policy(&repo_root)?;
    let mut state = RealRepoState::load(&repo_root)?;
    let path = options.path.clone().ok_or_else(|| {
        invalid_request("usage: sun execution promote-output requires --path <path>")
    })?;
    ensure_path_visible_to_sunlight(&repo_root, &path)?;
    let session_id = options.session_id.clone().ok_or_else(|| {
        invalid_request("usage: sun execution promote-output requires --session <session>")
    })?;
    let classification = options.classification.clone().ok_or_else(|| {
        invalid_request(
            "usage: sun execution promote-output requires --classification source_like_delta|generated_artifact",
        )
    })?;
    let execution = state
        .executions
        .iter()
        .find(|execution| execution.execution_id == options.execution_id)
        .cloned()
        .ok_or_else(|| object_not_found("execution", &options.execution_id))?;
    let output = execution
        .outputs
        .iter()
        .find(|output| output.path == path)
        .cloned()
        .ok_or_else(|| {
            promotion_error(
                "promotion_precondition_failed",
                "execution output path is not a declared promotion candidate",
                &options.execution_id,
                Some(&path),
                Some(&session_id),
                Some(&classification),
            )
        })?;
    if state.promotions.iter().any(|promotion| {
        promotion.execution_id == execution.execution_id && promotion.output_path == path
    }) {
        return Err(promotion_error(
            "promotion_precondition_failed",
            "execution output has already been promoted",
            &options.execution_id,
            Some(&path),
            Some(&session_id),
            Some(&classification),
        ));
    }
    if classification != output.classification {
        return Err(promotion_classification_error(
            &options.execution_id,
            Some(&path),
            Some(&session_id),
            Some(&classification),
            &output.classification,
        ));
    }
    if classification == "ignored" || classification == "log" || classification == "cache" {
        return Err(promotion_error(
            "promotion_policy_failed",
            "ignored, log, and cache execution outputs cannot be promoted",
            &options.execution_id,
            Some(&path),
            Some(&session_id),
            Some(&classification),
        ));
    }
    let session = real_session(&state, &session_id)?.clone();
    let target_topic_id = if let Some(topic) = &options.topic {
        real_topic(&state, topic)?.topic_id.clone()
    } else {
        session.write_topic_id.clone()
    };
    if target_topic_id != session.write_topic_id {
        return Err(CliError::new(
            "promotion_precondition_failed",
            "promotion target topic must match the session write topic",
        )
        .with_detail("topic_id", target_topic_id)
        .with_detail("session_id", session_id));
    }
    if session.resolved_view_id != execution.resolved_view_id {
        return Err(promotion_error(
            "promotion_precondition_failed",
            "session resolved view is stale for this execution",
            &options.execution_id,
            Some(&path),
            Some(&session_id),
            Some(&classification),
        ));
    }
    let projection = state
        .projections
        .iter()
        .find(|projection| projection.projection_id == execution.projection_id)
        .cloned()
        .ok_or_else(|| object_not_found("projection", &execution.projection_id))?;
    let root = projection
        .materialized_root
        .as_ref()
        .ok_or_else(|| object_not_found("projection", &execution.projection_id))?;
    let root = validate_execution_projection_binding(
        &projection_policy,
        &execution.projection_id,
        Path::new(root),
    )?;
    let output_path = root.join(&path);
    let metadata = fs::symlink_metadata(&output_path).map_err(|error| {
        invalid_request(format!("failed to inspect execution output: {error}"))
            .with_detail("path", &path)
    })?;
    if !metadata.is_file() {
        return Err(promotion_error(
            "promotion_policy_failed",
            "execution output promotion requires a regular file",
            &options.execution_id,
            Some(&path),
            Some(&session_id),
            Some(&classification),
        ));
    }
    if metadata.len() > MAX_PROMOTABLE_EXECUTION_OUTPUT_BYTES {
        return Err(oversized_promotion_error(
            &options.execution_id,
            &path,
            &session_id,
            &classification,
            metadata.len(),
        ));
    }
    let bytes = fs::read(&output_path).map_err(|error| {
        invalid_request(format!("failed to read execution output: {error}"))
            .with_detail("path", &path)
    })?;
    let resolved = state.resolve_session_view(&session);
    let before = resolved
        .entries
        .iter()
        .find(|entry| entry.path == path && !entry.tombstone)
        .cloned();
    if before.as_ref().map(|entry| entry.content_hash.clone()) != output.before_hash {
        return Err(promotion_error(
            "promotion_precondition_failed",
            "execution output no longer matches the session precondition",
            &options.execution_id,
            Some(&path),
            Some(&session_id),
            Some(&classification),
        ));
    }
    let after = RealArtifactEntry {
        artifact_id: before
            .as_ref()
            .map(|entry| entry.artifact_id.clone())
            .unwrap_or_else(|| real_artifact_id_for_path(&path)),
        path: path.clone(),
        content_hash: real_content_hash(&bytes),
        executable: false,
        classification: if classification == "generated_artifact" {
            "generated".to_string()
        } else {
            "source".to_string()
        },
        tombstone: false,
        bytes,
    };
    if after.content_hash != output.after_hash {
        return Err(promotion_error(
            "promotion_precondition_failed",
            "execution output bytes changed after the execution record was written",
            &options.execution_id,
            Some(&path),
            Some(&session_id),
            Some(&classification),
        ));
    }
    let response = real_accept_mutation(
        &mut state,
        &session_id,
        "execution_output_promotion",
        &path,
        before,
        after,
    )?;
    let candidate = PromotionCandidateProvenance {
        execution_id: execution.execution_id.clone(),
        projection_id: execution.projection_id.clone(),
        output_path: path.clone(),
        target_topic_id: session.write_topic_id.clone(),
        classification: real_output_classification(&classification),
        before_hash: output.before_hash.clone(),
        after_hash: output.after_hash.clone(),
    };
    let record = execution_output_promotion_record_from_mutation_response(&candidate, &response);
    state.promotions.push(RealExecutionPromotionSnapshot {
        execution_id: record.execution_id.clone(),
        projection_id: record.projection_id.clone(),
        output_path: record.output_path.clone(),
        target_topic_id: record.target_topic_id.clone(),
        classification: record.classification.as_str().to_string(),
        before_hash: record.before_hash.clone(),
        after_hash: record.after_hash.clone(),
        operation_transaction_id: record.operation_transaction_id.clone(),
        topic_revision_id: record.topic_revision_id.clone(),
        session_generation_id: record.session_generation_id.clone(),
        authored_context_id: record.authored_context_id.clone(),
    });
    finish_real_promotion(ctx, state, response, record)
}

fn real_checkpoint_create(
    ctx: &CommandContext,
    options: CheckpointCreateOptions,
) -> Result<(), CliError> {
    let mut state = RealRepoState::load(&ctx.repo_root)?;
    let resolved = real_resolve_view_by_id(&ctx.repo_root, &state, &options.view_id)?;
    if !resolved.result.conflict_free() {
        return Err(CliError::new(
            "conflicted_view",
            "cannot checkpoint a conflicted resolved view",
        )
        .with_detail("resolved_view_id", resolved.result.resolved_view_id));
    }
    let view_state = real_view_state(&state, &resolved);
    reject_real_export_blocked_entries(&view_state)?;
    let evidence_refs = match options.execution_id.as_deref() {
        Some(execution_id) => {
            let execution = state
                .executions
                .iter()
                .find(|execution| execution.execution_id == execution_id)
                .ok_or_else(|| object_not_found("execution", execution_id))?;
            let execution = real_execution_record(&state, execution);
            let tree_identity = resolved
                .result
                .tree_identity
                .as_ref()
                .expect("conflict-free persisted view has a tree identity");
            vec![
                validated_execution_evidence(&resolved.result, tree_identity, &execution)
                    .map_err(checkpoint_error)?,
            ]
        }
        None => Vec::new(),
    };
    let checkpoint = real_checkpoint(&view_state, evidence_refs);
    if !state
        .checkpoints
        .iter()
        .any(|snapshot| snapshot.checkpoint_id == checkpoint.id)
    {
        state.checkpoints.push(RealCheckpointSnapshot {
            checkpoint_id: checkpoint.id.clone(),
            resolved_view_id: checkpoint.resolved_view_id.clone(),
            tree_hash: checkpoint.tree_identity.tree_hash.clone(),
            topic_frontier: checkpoint
                .topic_frontier
                .iter()
                .map(|entry| (entry.topic_id.clone(), entry.topic_revision_id.clone()))
                .collect(),
            evidence_refs: checkpoint.evidence_refs.clone(),
            created_at: checkpoint.created_at.clone(),
            entries: view_state.entries.clone(),
        });
    }
    let records = vec![state.record_publication(
        "checkpoints",
        &checkpoint.id,
        &format!("{}\n", checkpoint_json(&checkpoint)),
    )?];
    state.save_with_records(&ctx.repo_root, &records)?;
    if ctx.json {
        outputln!(ctx, "{}", checkpoint_create_success_envelope(&checkpoint));
    } else {
        outputln!(ctx, "{} {}", checkpoint.id, checkpoint.resolved_view_id);
    }
    Ok(())
}

fn real_git_export(ctx: &CommandContext, options: GitExportOptions) -> Result<(), CliError> {
    let repo_root = ctx.repo_root.clone();
    let mut state = RealRepoState::load(&repo_root)?;
    let git_ref = normalize_git_export_ref(&options.git_ref);
    let (resolved, view_state, checkpoint) =
        real_persisted_checkpoint_export_context(&state, &options.checkpoint_id)?;
    let report = validate_real_export_candidate_with_context(
        &repo_root,
        &state,
        &resolved,
        &view_state,
        checkpoint.clone(),
        &git_ref,
    )?;
    let report = persist_and_reload_real_validation_report(&repo_root, &state, &report)?;
    if !report.ok {
        return Err(real_git_export_policy_error(&report));
    }
    if options.write_plan {
        let plan_json = format!(
            "{{\"ok\":true,\"data\":{{\"command\":\"git.export.plan\",\"repository_id\":\"{}\",\"ids\":{{\"checkpoint_id\":\"{}\",\"validation_report_id\":\"{}\"}},\"git_ref\":\"{}\",\"content_files\":{},\"validation_report\":{}}},\"warnings\":[]}}",
            json_escape(&state.repository_id),
            json_escape(&checkpoint.id),
            json_escape(&report.id),
            json_escape(&git_ref),
            view_state.entries.iter().filter(|entry| !entry.tombstone).count(),
            git_export_validation_report_json(&report),
        );
        outputln!(ctx, "{plan_json}");
        return Ok(());
    }
    if options.execute_local {
        let content_files = real_git_export_content_files(&view_state);
        let tree_hash = checkpoint.tree_identity.tree_hash.clone();
        let input = real_git_export_writer_input(
            &repo_root,
            &state,
            &options,
            checkpoint.clone(),
            report.clone(),
        )?;
        let mut store = InMemoryGitExportMapStore::default();
        let result = execute_local_git_export_writer(input, content_files, &mut store)
            .map_err(git_export_planning_error)?;
        let commit_id = result.created_commit_id.clone().ok_or_else(|| {
            invalid_request(
                result
                    .error
                    .as_ref()
                    .map(|error| error.message.clone())
                    .unwrap_or_else(|| "git export did not create a commit".to_string()),
            )
        })?;
        let export_id = format!("export_map_{}", checkpoint.id);
        let export_snapshot = RealExportMapSnapshot {
            export_map_id: export_id.clone(),
            checkpoint_id: checkpoint.id.clone(),
            tree_hash,
            git_ref: git_ref.clone(),
            git_commit_ids: vec![commit_id.clone()],
            exported_at: real_now_id(),
            validation_report_id: Some(report.id.clone()),
        };
        if let Some(existing) = state
            .export_maps
            .iter()
            .find(|existing| existing.export_map_id == export_id)
        {
            if existing.checkpoint_id != export_snapshot.checkpoint_id
                || existing.tree_hash != export_snapshot.tree_hash
                || existing.git_ref != export_snapshot.git_ref
                || existing.git_commit_ids != export_snapshot.git_commit_ids
                || existing.validation_report_id != export_snapshot.validation_report_id
            {
                return Err(CliError::new(
                    "export_map_conflict",
                    "the durable export-map ID already identifies a different Git handoff",
                )
                .with_detail("export_map_id", export_id));
            }
        } else {
            state.export_maps.push(export_snapshot);
            let records = vec![state.record_publication(
                "export-map",
                &export_id,
                &format!(
                    "{{\"record_type\":\"git_export_map\",\"id\":\"{}\",\"repository_id\":\"{}\",\"checkpoint_id\":\"{}\",\"tree_hash\":\"{}\",\"git_ref\":\"{}\",\"git_commit_ids\":[\"{}\"],\"validation_report_id\":\"{}\"}}\n",
                    json_escape(&export_id),
                    json_escape(&state.repository_id),
                    json_escape(&checkpoint.id),
                    json_escape(&checkpoint.tree_identity.tree_hash),
                    json_escape(&git_ref),
                    json_escape(&commit_id),
                    json_escape(&report.id),
                ),
            )?];
            state.save_with_records(&repo_root, &records)?;
        }
        if ctx.json {
            outputln!(ctx,
                "{{\"ok\":true,\"data\":{{\"command\":\"git.export.execute\",\"repository_id\":\"{}\",\"ids\":{{\"checkpoint_id\":\"{}\",\"resolved_view_id\":\"{}\",\"tree_id\":\"{}\",\"export_map_id\":\"{}\",\"validation_report_id\":\"{}\"}},\"checkpoint_id\":\"{}\",\"git_ref\":\"{}\",\"git_commit_ids\":[\"{}\"],\"validation_report\":{},\"lifecycle_state\":\"exported\"}},\"warnings\":[]}}",
                json_escape(&state.repository_id),
                json_escape(&checkpoint.id),
                json_escape(&checkpoint.resolved_view_id),
                json_escape(&checkpoint.tree_identity.tree_hash),
                json_escape(&export_id),
                json_escape(&report.id),
                json_escape(&checkpoint.id),
                json_escape(&git_ref),
                json_escape(&commit_id),
                git_export_validation_report_json(&report),
            );
        } else {
            outputln!(ctx, "{} exported", checkpoint.id);
        }
        return Ok(());
    }
    if ctx.json {
        outputln!(ctx,
            "{{\"ok\":true,\"data\":{{\"command\":\"git.export\",\"repository_id\":\"{}\",\"ids\":{{\"checkpoint_id\":\"{}\",\"resolved_view_id\":\"{}\",\"tree_id\":\"{}\",\"validation_report_id\":\"{}\"}},\"checkpoint_id\":\"{}\",\"git_ref\":\"{}\",\"git_commit_ids\":[],\"export_map\":null,\"validation_report\":{},\"write_performed\":false}},\"warnings\":[]}}",
            json_escape(&state.repository_id),
            json_escape(&checkpoint.id),
            json_escape(&checkpoint.resolved_view_id),
            json_escape(&checkpoint.tree_identity.tree_hash),
            json_escape(&report.id),
            json_escape(&checkpoint.id),
            json_escape(&git_ref),
            git_export_validation_report_json(&report),
        );
    } else {
        outputln!(ctx, "{} {}", checkpoint.id, git_ref);
    }
    Ok(())
}

fn normalize_git_export_ref(git_ref: &str) -> String {
    if git_ref.starts_with("refs/") {
        git_ref.to_string()
    } else {
        format!("refs/heads/{git_ref}")
    }
}

fn real_persisted_checkpoint_export_context(
    state: &RealRepoState,
    checkpoint_id: &str,
) -> Result<(RealResolvedRepoView, RealRepoState, CheckpointRecord), CliError> {
    let snapshot = state
        .checkpoints
        .iter()
        .find(|snapshot| snapshot.checkpoint_id == checkpoint_id)
        .ok_or_else(|| object_not_found("checkpoint", checkpoint_id))?;
    let resolved = real_checkpoint_snapshot_resolved_view(state, snapshot);
    let view_state = real_view_state(state, &resolved);
    let mut checkpoint = real_checkpoint(&view_state, snapshot.evidence_refs.clone());
    checkpoint.id = snapshot.checkpoint_id.clone();
    checkpoint.resolved_view_id = snapshot.resolved_view_id.clone();
    checkpoint.created_at = snapshot.created_at.clone();
    Ok((resolved, view_state, checkpoint))
}

fn validate_real_export_candidate(
    repo_root: &PathBuf,
    state: &RealRepoState,
    checkpoint_id: &str,
    git_ref: &str,
) -> Result<GitExportValidationReport, CliError> {
    let (resolved, view_state, checkpoint) =
        real_persisted_checkpoint_export_context(state, checkpoint_id)?;
    validate_real_export_candidate_with_context(
        repo_root,
        state,
        &resolved,
        &view_state,
        checkpoint,
        &normalize_git_export_ref(git_ref),
    )
}

fn validate_real_export_candidate_with_context(
    repo_root: &PathBuf,
    state: &RealRepoState,
    resolved: &RealResolvedRepoView,
    view_state: &RealRepoState,
    checkpoint: CheckpointRecord,
    git_ref: &str,
) -> Result<GitExportValidationReport, CliError> {
    let config = require_repository_config(repo_root.clone())?;
    let mut request = GitExportRequest::from_checkpoint(&checkpoint);
    request.git_ref = git_ref.to_string();
    request.validation_report_id = "validation_pending".to_string();
    Ok(validate_persisted_git_export(
        PersistedGitExportValidationInput {
            config: &config,
            request: &request,
            resolved_view: &resolved.result,
            entries: &view_state.entries,
            state,
        },
    ))
}

fn persist_and_reload_real_validation_report(
    repo_root: &PathBuf,
    state: &RealRepoState,
    report: &GitExportValidationReport,
) -> Result<GitExportValidationReport, CliError> {
    persist_git_export_validation_report(repo_root, &state.repository_id, report)
        .map_err(|error| validation_report_write_error(&report.id, error))?;
    load_git_export_validation_report(repo_root, &state.repository_id, &report.id)
        .map_err(|error| validation_report_load_error(&report.id, error))
}

fn reject_real_export_blocked_entries(state: &RealRepoState) -> Result<(), CliError> {
    let blocked = state
        .entries
        .iter()
        .filter(|entry| {
            !entry.tombstone && matches!(entry.classification.as_str(), "local_only" | "local-only")
        })
        .collect::<Vec<_>>();
    if blocked.is_empty() {
        return Ok(());
    }
    let paths = blocked
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let classifications = blocked
        .iter()
        .map(|entry| entry.classification.as_str())
        .collect::<Vec<_>>()
        .join(",");
    Err(CliError::new(
        "export_policy_failed",
        "state contains local-only artifacts and cannot be checkpointed or exported",
    )
    .with_detail("blocked_count", blocked.len().to_string())
    .with_detail("blocked_paths", paths)
    .with_detail("blocked_classifications", classifications))
}

fn real_status(ctx: &CommandContext) -> Result<bool, CliError> {
    let repo_root = ctx.repo_root.clone();
    let load_result = if ctx.args.len() == 1 {
        RealRepoState::load_metadata(&repo_root)
    } else {
        RealRepoState::load(&repo_root)
    };
    let mut state = match load_result {
        Ok(state) => state,
        Err(RepoStateError::NotInitialized { .. }) => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if ctx.args.len() == 1
        && state
            .projections
            .iter()
            .any(|projection| projection.purpose == "compatibility")
    {
        state = RealRepoState::load(&repo_root)?;
    }
    recover_interrupted_executions(&repo_root, &mut state)?;
    if let [_, flag, value] = ctx.args.as_slice() {
        match flag.as_str() {
            "--projection" => {
                let projection = state
                    .projections
                    .iter()
                    .find(|projection| projection.projection_id == *value)
                    .ok_or_else(|| object_not_found("projection", value))?;
                if projection.purpose == "compatibility" {
                    let projection_policy = require_projection_policy(&repo_root)?;
                    let diff = diff_real_compat_projection(
                        &repo_root,
                        &projection_policy.managed_root,
                        projection,
                    )
                    .map_err(compat_import_error)?;
                    if ctx.json {
                        outputln!(
                            ctx,
                            "{}",
                            real_compat_projection_status_envelope(
                                &state,
                                projection,
                                &diff.candidates
                            )
                        );
                    } else {
                        outputln!(
                            ctx,
                            "projection {} compatibility {} candidates={} quarantined={}",
                            projection.projection_id,
                            if diff.candidates.is_empty() {
                                "clean"
                            } else {
                                "dirty"
                            },
                            diff.candidates.len(),
                            diff.candidates
                                .iter()
                                .filter(|candidate| candidate.quarantine_ref.is_some())
                                .count()
                        );
                    }
                } else {
                    if ctx.json {
                        outputln!(
                            ctx,
                            "{}",
                            real_projection_status_envelope(&state, projection)
                        );
                    } else {
                        outputln!(
                            ctx,
                            "projection {} {} materialized view={}",
                            projection.projection_id,
                            projection.purpose,
                            projection.resolved_view_id
                        );
                    }
                }
                return Ok(true);
            }
            "--checkpoint" => {
                let checkpoint = state
                    .checkpoints
                    .iter()
                    .find(|checkpoint| checkpoint.checkpoint_id == *value)
                    .ok_or_else(|| object_not_found("checkpoint", value))?;
                if ctx.json {
                    outputln!(
                        ctx,
                        "{}",
                        real_checkpoint_status_envelope(&state, checkpoint)
                    );
                } else {
                    let exported = state
                        .export_maps
                        .iter()
                        .any(|map| map.checkpoint_id == checkpoint.checkpoint_id);
                    outputln!(
                        ctx,
                        "checkpoint {} export_ready=true exported={exported} view={}",
                        checkpoint.checkpoint_id,
                        checkpoint.resolved_view_id
                    );
                }
                return Ok(true);
            }
            "--export" | "--export-map" => {
                let export_map = state
                    .export_maps
                    .iter()
                    .find(|export_map| export_map.export_map_id == *value)
                    .ok_or_else(|| object_not_found("export_map", value))?;
                if ctx.json {
                    outputln!(
                        ctx,
                        "{}",
                        real_export_map_status_envelope(&state, export_map)
                    );
                } else {
                    outputln!(
                        ctx,
                        "export {} checkpoint={} ref={} commits={}",
                        export_map.export_map_id,
                        export_map.checkpoint_id,
                        export_map.git_ref,
                        export_map.git_commit_ids.len()
                    );
                }
                return Ok(true);
            }
            "--execution" => {
                let execution = state
                    .executions
                    .iter()
                    .find(|execution| execution.execution_id == *value)
                    .ok_or_else(|| object_not_found("execution", value))?;
                if ctx.json {
                    outputln!(ctx, "{}", real_execution_status_envelope(&state, execution));
                } else {
                    outputln!(
                        ctx,
                        "execution {} status={} promotion={} view={} network={}",
                        execution.execution_id,
                        execution.status,
                        real_execution_promotion_status(&state, execution),
                        execution.resolved_view_id,
                        execution.network_policy
                    );
                }
                return Ok(true);
            }
            "--view" => {
                let resolved = real_resolve_view_by_id(&repo_root, &state, value)?;
                if ctx.json {
                    outputln!(
                        ctx,
                        "{}",
                        real_view_envelope("status.view", &state, &resolved.result)
                    );
                } else {
                    outputln!(
                        ctx,
                        "{} {}",
                        resolved.result.resolved_view_id,
                        resolved_view_lifecycle_state(&resolved.result)
                    );
                }
                return Ok(true);
            }
            "--compat-import" => {
                let operation = state
                    .operations
                    .iter()
                    .find(|operation| {
                        operation.operation_transaction_id == *value
                            && operation.compat_projection_id.is_some()
                    })
                    .ok_or_else(|| object_not_found("compat_import", value))?;
                outputln!(
                    ctx,
                    "{}",
                    real_compat_operation_inspect_envelope(
                        &state,
                        operation,
                        "status.compat-import"
                    )
                );
                return Ok(true);
            }
            _ => {}
        }
    }
    let mut selected_topic = None;
    let mut selected_session = None;
    let command = match ctx.args.as_slice() {
        [_, flag, value] if flag == "--session" => {
            let session = real_session(&state, value)?;
            selected_session = Some(session);
            selected_topic = state
                .topics
                .iter()
                .find(|topic| topic.topic_id == session.write_topic_id);
            "status.session"
        }
        [_, flag, value] if flag == "--topic" => {
            selected_topic = Some(real_topic(&state, value)?);
            "status.topic"
        }
        [_, flag, value] if flag == "--view" => {
            if value != &state.resolved_view_id && value != &state.base_resolved_view_id {
                return Err(object_not_found("resolved_view", value));
            }
            "status.view"
        }
        [_] => "status.repository",
        _ => return Ok(false),
    };
    let summary = real_operational_summary(&repo_root, &state);
    if ctx.json {
        let repository_summary = (command == "status.repository").then_some(&summary);
        outputln!(
            ctx,
            "{}",
            real_status_envelope(
                &state,
                &repo_root,
                command,
                selected_topic,
                selected_session,
                repository_summary
            )
        );
    } else {
        print_real_status_text(
            ctx,
            &state,
            command,
            selected_topic,
            selected_session,
            &summary,
        );
    }
    Ok(true)
}

fn real_inspect(ctx: &CommandContext) -> Result<bool, CliError> {
    let mut state = match RealRepoState::load(&ctx.repo_root) {
        Ok(state) => state,
        Err(RepoStateError::NotInitialized { .. }) => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    recover_interrupted_executions(&ctx.repo_root, &mut state)?;
    let selector = match ctx.args.iter().skip(1).find(|arg| !arg.starts_with("--")) {
        Some(selector) => selector,
        None => return Ok(false),
    };
    if let Some(execution_id) = selector.strip_prefix("execution:") {
        let execution = state
            .executions
            .iter()
            .find(|execution| execution.execution_id == execution_id)
            .ok_or_else(|| object_not_found("execution", execution_id))?;
        if ctx.json {
            outputln!(
                ctx,
                "{}",
                real_execution_inspect_envelope(&state, execution)
            );
        } else {
            outputln!(
                ctx,
                "execution {} {}",
                execution.execution_id,
                execution.status
            );
        }
        return Ok(true);
    }
    if ctx.json {
        outputln!(
            ctx,
            "{}",
            real_inspect_envelope(&ctx.repo_root, &state, selector)?
        );
    } else {
        print_real_inspect_text(ctx, &state, selector)?;
    }
    Ok(true)
}

fn print_real_inspect_text(
    ctx: &CommandContext,
    state: &RealRepoState,
    selector: &str,
) -> Result<(), CliError> {
    if selector == "repository" || selector == format!("repository:{}", state.repository_id) {
        let summary = real_operational_summary(&ctx.repo_root, state);
        print_real_status_text(ctx, state, "inspect.repository", None, None, &summary);
        return Ok(());
    }
    if let Some(value) = selector.strip_prefix("topic:") {
        let topic = real_topic(state, value)?;
        outputln!(
            ctx,
            "topic {} ({}) head={} revisions={} owner={} visibility={} base={}",
            topic.topic_id,
            topic.slug,
            topic.head_revision_id.as_deref().unwrap_or("none"),
            topic.revision_number,
            topic.owner_actor_id,
            topic.visibility,
            topic.base_checkpoint_id,
        );
        for criterion in &topic.acceptance_criteria {
            outputln!(ctx, "acceptance: {criterion}");
        }
        return Ok(());
    }
    if let Some(value) = selector.strip_prefix("session:") {
        let session = real_session(state, value)?;
        outputln!(
            ctx,
            "session {} topic={} generation={} view={}",
            session.session_id,
            session.write_topic_id,
            session.session_generation_id,
            session.resolved_view_id
        );
        return Ok(());
    }
    if let Some(value) = selector.strip_prefix("checkpoint:") {
        let checkpoint = state
            .checkpoints
            .iter()
            .find(|checkpoint| checkpoint.checkpoint_id == value)
            .ok_or_else(|| object_not_found("checkpoint", value))?;
        outputln!(
            ctx,
            "checkpoint {} view={} tree={} artifacts={}",
            checkpoint.checkpoint_id,
            checkpoint.resolved_view_id,
            checkpoint.tree_hash,
            checkpoint
                .entries
                .iter()
                .filter(|entry| !entry.tombstone)
                .count()
        );
        return Ok(());
    }
    if let Some(value) = selector
        .strip_prefix("export:")
        .or_else(|| selector.strip_prefix("export_map:"))
    {
        let map = state
            .export_maps
            .iter()
            .find(|map| map.export_map_id == value)
            .ok_or_else(|| object_not_found("export_map", value))?;
        outputln!(
            ctx,
            "export {} checkpoint={} ref={} commits={}",
            map.export_map_id,
            map.checkpoint_id,
            map.git_ref,
            map.git_commit_ids.len()
        );
        return Ok(());
    }
    if let Some(value) = selector.strip_prefix("projection:") {
        let projection = state
            .projections
            .iter()
            .find(|projection| projection.projection_id == value)
            .ok_or_else(|| object_not_found("projection", value))?;
        outputln!(
            ctx,
            "projection {} purpose={} view={} retention={}",
            projection.projection_id,
            projection.purpose,
            projection.resolved_view_id,
            projection.retention_state
        );
        return Ok(());
    }
    let envelope = real_inspect_envelope(&ctx.repo_root, state, selector)?;
    outputln!(
        ctx,
        "{selector}: persisted Sunlight record available (use --json for provenance details)"
    );
    debug_assert!(envelope.starts_with("{\"ok\":true"));
    Ok(())
}

fn real_accept_mutation(
    state: &mut RealRepoState,
    session_id: &str,
    mutation: &str,
    path: &str,
    before: Option<RealArtifactEntry>,
    after: RealArtifactEntry,
) -> Result<MutationResponse, CliError> {
    let session = real_session(state, session_id)?.clone();
    let topic = state
        .topics
        .iter()
        .find(|topic| topic.topic_id == session.write_topic_id)
        .cloned()
        .ok_or_else(|| {
            CliError::new(
                "topic_not_found",
                format!("topic `{}` was not found", session.write_topic_id),
            )
            .with_detail("topic", session.write_topic_id.clone())
        })?;
    if let Some(completed_revision_id) = &topic.completed_revision_id {
        return Err(CliError::new(
            "topic_completed",
            "the topic is completed and its head revision is immutable",
        )
        .with_detail("topic_id", topic.topic_id)
        .with_detail("completed_revision_id", completed_revision_id.clone())
        .with_detail("session_id", session.session_id));
    }
    let topic_id = topic.topic_id.clone();
    let prior_view = real_session_view(state, &session);
    let parent_revision_id = topic.head_revision_id.clone();
    let mutated_artifact_id = before
        .as_ref()
        .map(|entry| entry.artifact_id.clone())
        .unwrap_or_else(|| after.artifact_id.clone());
    state.revision_number += 1;
    let next_topic_revision_number = topic.revision_number + 1;
    let next_session_generation_number = session.generation_number + 1;
    let operation_id = format!("op_native_{:04}", state.revision_number);
    let revision_id = format!(
        "rev_{}_{:04}",
        topic_id.trim_start_matches("topic_"),
        next_topic_revision_number
    );
    let session_generation_id =
        native_session_generation_id(&session.session_id, next_session_generation_number);
    if let Some(topic) = state
        .topics
        .iter_mut()
        .find(|candidate| candidate.topic_id == topic_id)
    {
        topic.head_revision_id = Some(revision_id.clone());
        topic.revision_number = next_topic_revision_number;
    }
    if let Some(session) = state.session_by_id_mut(session_id) {
        session
            .topic_frontier
            .insert(topic_id.clone(), revision_id.clone());
    }
    let before_hash = before.as_ref().map(|entry| entry.content_hash.clone());
    let after_hash = after.content_hash.clone();
    let authored_context_id = prior_view.resolved_view_id.clone();
    let dependency_revision_ids = session
        .topic_frontier
        .iter()
        .filter(|(dependency_topic_id, _)| *dependency_topic_id != &topic_id)
        .map(|(_, revision_id)| revision_id.clone())
        .collect::<Vec<_>>();
    state.operations.push(RealOperationRecord {
        operation_transaction_id: operation_id.clone(),
        topic_id: topic_id.clone(),
        topic_revision_id: revision_id.clone(),
        session_id: session_id.to_string(),
        artifact_id: mutated_artifact_id.clone(),
        path: after.path.clone(),
        mutation: mutation.to_string(),
        base_content_hash: before_hash.clone(),
        result_content_hash: after_hash.clone(),
        authored_context_id: authored_context_id.clone(),
        dependency_revision_ids: dependency_revision_ids.clone(),
        classification: after.classification.clone(),
        executable: after.executable,
        tombstone: after.tombstone,
        bytes: after.bytes.clone(),
        compat_projection_id: None,
        compat_candidate_delta_ids: Vec::new(),
        worktree_candidate_delta_ids: Vec::new(),
        worktree_anchor_generation: None,
        worktree_anchor_resolved_view_id: None,
        worktree_anchor_tree_hash: None,
        worktree_anchor_manifest_digest: None,
        effects: Vec::new(),
    });
    let session_after = state
        .sessions
        .iter()
        .find(|candidate| candidate.session_id == session_id)
        .cloned()
        .unwrap_or(session.clone());
    let mut session_resolved = state.resolve_session_view(&session_after);
    let resolved_view_id = session_resolved.result.resolved_view_id.clone();
    let tree_hash = session_resolved
        .result
        .tree_identity
        .as_ref()
        .map(|tree| tree.tree_hash.clone())
        .ok_or_else(|| {
            CliError::new(
                "conflicted_view",
                "mutation would produce a conflicted session view",
            )
            .with_detail("resolved_view_id", resolved_view_id.clone())
            .with_detail("session_id", session_id.to_string())
        })?;
    state.resolved_view_id = resolved_view_id.clone();
    state.current_topic_frontier = session_resolved.result.topic_frontier.clone();
    state.tree_hash = tree_hash;
    state.entries = session_resolved.entries.clone();
    if let Some(session) = state.session_by_id_mut(session_id) {
        session.resolved_view_id = resolved_view_id.clone();
        session.session_generation_id = session_generation_id.clone();
        session.generation_number = next_session_generation_number;
    }
    state.sync_compat_fields();
    let response_after = after.clone();
    session_resolved = state.resolve_session_view(real_session(state, session_id)?);
    let tree_identity =
        real_resolved_session_view(state, real_session(state, session_id)?, &session_resolved)
            .tree_identity;
    let kind = match mutation {
        "patch" => MutationKind::Patch,
        "move" => MutationKind::Move,
        "delete" => MutationKind::Delete,
        "metadata_set" => MutationKind::MetadataSet,
        _ => MutationKind::Write,
    };
    let payload = MutationPayload::Write {
        write_mode: if before.is_some() {
            WriteMode::Replace
        } else {
            WriteMode::Create
        },
        content_hash: after_hash.clone(),
        byte_length: after.bytes.len(),
        media_type: media_type_for_path(&after.path).to_string(),
        executable: after.executable,
        classification: after.classification.clone(),
    };
    let operation = OperationTransactionRecord {
        id: operation_id.clone(),
        repository_id: state.repository_id.clone(),
        topic_id: topic_id.clone(),
        session_id: session_id.to_string(),
        session_generation_id: prior_view.session_generation_id.clone(),
        actor_id: session.actor_id.clone(),
        authored_context_id,
        preconditions: sunlight_core::artifacts::MutationPreconditions {
            resolved_view_id: prior_view.resolved_view_id.clone(),
            session_generation_id: prior_view.session_generation_id.clone(),
            write_topic_id: topic_id.clone(),
            parent_topic_revision_id: parent_revision_id.clone(),
            path_policy_id: POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string(),
            operation_semantics_version: FILE_OPERATION_SEMANTICS_VERSION.to_string(),
            expected_path: path.to_string(),
            expected_hash: before_hash
                .clone()
                .map(ExpectedHash::Existing)
                .unwrap_or(ExpectedHash::New),
        },
        read_set: "full_authored_context".to_string(),
        write_set: vec![sunlight_core::artifacts::WriteSetEntry {
            artifact_id: after.artifact_id.clone(),
            path: after.path.clone(),
            mutation: kind.clone(),
        }],
        mutation_payload: payload,
        before_refs: MutationRefs {
            artifacts: vec![real_mutation_ref(before.as_ref(), path, before.is_none())],
            tree_identity: prior_view.tree_identity,
        },
        after_refs: MutationRefs {
            artifacts: vec![real_mutation_ref(Some(&after), &after.path, false)],
            tree_identity: tree_identity.clone(),
        },
        classification: after.classification.clone(),
        parent_topic_revision_id: parent_revision_id.clone(),
        next_topic_revision_number,
        parents: parent_revision_id.iter().cloned().collect(),
    };
    let topic_revision = TopicRevisionRecord {
        id: revision_id.clone(),
        repository_id: state.repository_id.clone(),
        topic_id: topic_id.clone(),
        revision_number: next_topic_revision_number,
        parent_revision_id,
        operation_transaction_id: operation_id.clone(),
        tree_delta_ref: format!("delta_native_{:04}", state.revision_number),
        dependency_revision_ids,
    };
    let session_generation = SessionGenerationMutationRecord {
        id: session_generation_id.clone(),
        repository_id: state.repository_id.clone(),
        session_id: session_id.to_string(),
        write_topic_id: topic_id.clone(),
        base_resolved_view_id: state.base_resolved_view_id.clone(),
        resolved_view_id: state.resolved_view_id.clone(),
        topic_frontier: real_session(state, session_id)?.topic_frontier.clone(),
        generation_number: next_session_generation_number,
        refresh_policy: real_session(state, session_id)?.refresh_policy.clone(),
        created_by_operation_id: operation_id,
    };
    state.session_generations.push(RealSessionGenerationRecord {
        session_generation_id: session_generation_id.clone(),
        session_id: session_id.to_string(),
        write_topic_id: session.write_topic_id.clone(),
        base_resolved_view_id: state.base_resolved_view_id.clone(),
        resolved_view_id: state.resolved_view_id.clone(),
        topic_frontier: real_session(state, session_id)?.topic_frontier.clone(),
        generation_number: next_session_generation_number,
        refresh_policy: real_session(state, session_id)?.refresh_policy.clone(),
        created_by: format!("operation:{}", session_generation.created_by_operation_id),
    });
    Ok(MutationResponse {
        command: match mutation {
            "patch" => "artifact.patch",
            "move" => "artifact.move",
            "delete" => "artifact.delete",
            "metadata_set" => "artifact.metadata_set",
            _ => "artifact.write",
        },
        repository_id: state.repository_id.clone(),
        session_id: session_id.to_string(),
        view: SessionView {
            resolved_view_id: state.resolved_view_id.clone(),
            session_generation_id,
            tree_identity: tree_identity.clone(),
        },
        artifact: MutationArtifactView {
            artifact_id: response_after.artifact_id,
            path: response_after.path,
            kind: ArtifactKind::File,
            before_hash,
            after_hash,
            classification: response_after.classification,
            executable: response_after.executable,
        },
        operation,
        topic_revision,
        session_generation,
    })
}

fn finish_real_mutation(
    ctx: &CommandContext,
    state: RealRepoState,
    response: MutationResponse,
) -> Result<(), CliError> {
    let repo_root = ctx.repo_root.clone();
    let records = vec![
        state.record_publication(
            "operations",
            &response.operation.id,
            &format!("{}\n", operation_json(&response)),
        )?,
        state.record_publication(
            "topics",
            &response.topic_revision.id,
            &format!("{}\n", topic_revision_json(&response)),
        )?,
        state.record_publication(
            "views",
            &state.resolved_view_id,
            &format!(
                "{}\n",
                resolved_view_record_json(
                    &state
                        .resolve_session_view(real_session(&state, &response.session_id)?)
                        .result
                )
            ),
        )?,
        real_session_generation_publication(
            &state,
            &response.session_id,
            &response.session_generation.id,
        )?,
    ];
    state.save_with_records(&repo_root, &records)?;
    if ctx.json {
        outputln!(ctx, "{}", mutation_success_envelope(&response));
    } else {
        outputln!(
            ctx,
            "{} {}",
            response.artifact.path,
            response.artifact.after_hash
        );
    }
    Ok(())
}

fn finish_real_promotion(
    ctx: &CommandContext,
    state: RealRepoState,
    response: MutationResponse,
    record: ExecutionOutputPromotionRecord,
) -> Result<(), CliError> {
    let repo_root = ctx.repo_root.clone();
    let records = vec![
        real_session_generation_publication(
            &state,
            &response.session_id,
            &response.session_generation.id,
        )?,
        state.record_publication(
            "operations",
            &response.operation.id,
            &format!("{}\n", operation_json(&response)),
        )?,
        state.record_publication(
            "topics",
            &response.topic_revision.id,
            &format!("{}\n", topic_revision_json(&response)),
        )?,
        state.record_publication(
            "views",
            &state.resolved_view_id,
            &format!(
                "{}\n",
                resolved_view_record_json(&real_resolved_view(&state))
            ),
        )?,
        state.record_publication(
            "promotions",
            &record.operation_transaction_id,
            &format!("{}\n", promotion_record_json(&record)),
        )?,
    ];
    state.save_with_records(&repo_root, &records)?;
    if ctx.json {
        let candidate = PromotionCandidateProvenance {
            execution_id: record.execution_id.clone(),
            projection_id: record.projection_id.clone(),
            output_path: record.output_path.clone(),
            target_topic_id: record.target_topic_id.clone(),
            classification: record.classification,
            before_hash: record.before_hash.clone(),
            after_hash: record.after_hash.clone(),
        };
        outputln!(ctx, "{}", promotion_success_envelope(&response, &candidate));
    } else {
        outputln!(
            ctx,
            "promoted {} {}",
            response.artifact.path,
            response.artifact.after_hash
        );
    }
    Ok(())
}

fn real_session_generation_publication(
    state: &RealRepoState,
    session_id: &str,
    session_generation_id: &str,
) -> Result<DerivedRecordPublication, CliError> {
    let generation = state
        .session_generations
        .iter()
        .find(|generation| {
            generation.session_generation_id == session_generation_id
                && generation.session_id == session_id
        })
        .ok_or_else(|| {
            CliError::new(
                "invalid_native_state",
                "canonical session generation is missing from native state",
            )
            .with_detail("session_id", session_id.to_string())
            .with_detail("session_generation_id", session_generation_id.to_string())
        })?;
    Ok(state.record_publication(
        "session-generations",
        session_generation_id,
        &format!("{}\n", real_session_generation_record_json(generation)),
    )?)
}

fn repository_ignored_execution_paths(
    repo_root: &Path,
    paths: &[String],
    cancellation: &AtomicBool,
) -> Result<BTreeSet<String>, CliError> {
    if paths.is_empty() {
        return Ok(BTreeSet::new());
    }
    if !repo_root.join(".git").exists() {
        let mut ignored = BTreeSet::new();
        for path in paths {
            if path_is_gitignored(repo_root, path)? {
                ignored.insert(path.clone());
            }
        }
        return Ok(ignored);
    }
    let mut child = match Command::new("git")
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.untrackedCache=false",
            "check-ignore",
            "--stdin",
            "-z",
        ])
        .current_dir(repo_root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return Err(CliError::new(
                "execution_ignore_policy_failed",
                format!("failed to start Git ignore evaluation: {error}"),
            ))
        }
    };
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(CliError::new(
            "execution_ignore_policy_failed",
            "failed to open Git ignore policy input",
        ));
    };
    let input_written = paths
        .iter()
        .all(|path| stdin.write_all(path.as_bytes()).is_ok() && stdin.write_all(&[0]).is_ok());
    drop(stdin);
    let stdout_reader = child.stdout.take().and_then(|mut stdout| {
        thread::Builder::new()
            .name("sun-git-ignore-reader".to_string())
            .spawn(move || {
                let mut bytes = Vec::new();
                stdout.read_to_end(&mut bytes).map(|_| bytes)
            })
            .ok()
    });
    let stderr_reader = child.stderr.take().and_then(|mut stderr| {
        thread::Builder::new()
            .name("sun-git-ignore-stderr-reader".to_string())
            .spawn(move || {
                let mut bytes = Vec::new();
                stderr.read_to_end(&mut bytes).map(|_| bytes)
            })
            .ok()
    });
    if !input_written {
        let _ = child.kill();
        let _ = child.wait();
        return Err(CliError::new(
            "execution_ignore_policy_failed",
            "failed to send paths to Git ignore evaluation",
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let status = loop {
        if cancellation.load(Ordering::Acquire) || Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CliError::new(
                if cancellation.load(Ordering::Acquire) {
                    "request_cancelled"
                } else {
                    "execution_ignore_policy_failed"
                },
                if cancellation.load(Ordering::Acquire) {
                    "request cancelled during Git ignore evaluation"
                } else {
                    "Git ignore evaluation exceeded its two-second deadline"
                },
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(CliError::new(
                    "execution_ignore_policy_failed",
                    "failed while waiting for Git ignore evaluation",
                ));
            }
        }
    };
    let stdout = stdout_reader
        .ok_or_else(|| {
            CliError::new(
                "execution_ignore_policy_failed",
                "failed to capture Git ignore evaluation output",
            )
        })?
        .join()
        .map_err(|_| {
            CliError::new(
                "execution_ignore_policy_failed",
                "Git ignore output reader stopped unexpectedly",
            )
        })?
        .map_err(|error| {
            CliError::new(
                "execution_ignore_policy_failed",
                format!("failed to read Git ignore evaluation output: {error}"),
            )
        })?;
    let stderr = stderr_reader
        .ok_or_else(|| {
            CliError::new(
                "execution_ignore_policy_failed",
                "failed to capture Git ignore evaluation diagnostics",
            )
        })?
        .join()
        .map_err(|_| {
            CliError::new(
                "execution_ignore_policy_failed",
                "Git ignore diagnostic reader stopped unexpectedly",
            )
        })?
        .map_err(|error| {
            CliError::new(
                "execution_ignore_policy_failed",
                format!("failed to read Git ignore evaluation diagnostics: {error}"),
            )
        })?;
    if !matches!(status.code(), Some(0 | 1)) {
        return Err(CliError::new(
            "execution_ignore_policy_failed",
            format!(
                "Git ignore evaluation failed: {}",
                String::from_utf8_lossy(&stderr).trim()
            ),
        ));
    }
    let mut ignored = BTreeSet::new();
    for path in stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        ignored.insert(
            std::str::from_utf8(path)
                .map_err(|_| {
                    CliError::new(
                        "execution_ignore_policy_failed",
                        "Git returned a non-UTF-8 ignored path",
                    )
                })?
                .to_string(),
        );
    }
    Ok(ignored)
}

fn common_created_output_classification(path: &str) -> Option<&'static str> {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let segments = normalized.split('/').collect::<Vec<_>>();
    if normalized.ends_with(".tsbuildinfo")
        || normalized.ends_with(".pyc")
        || segments
            .iter()
            .any(|segment| matches!(*segment, ".cache" | ".tmp" | "__pycache__"))
        || segments
            .windows(2)
            .any(|pair| pair[0] == "node_modules" && matches!(pair[1], ".cache" | ".tmp"))
    {
        return Some("cache");
    }
    if segments
        .iter()
        .any(|segment| matches!(*segment, "dist" | "build" | "target" | "coverage" | ".next"))
    {
        return Some("ignored");
    }
    None
}

fn classify_execution_output(
    entry: &RealArtifactEntry,
    created: bool,
    ignored_paths: &BTreeSet<String>,
) -> String {
    if entry.classification == "generated" {
        "generated_artifact".to_string()
    } else if created {
        common_created_output_classification(&entry.path).map_or_else(
            || {
                if ignored_paths.contains(&entry.path) {
                    "ignored"
                } else {
                    "source_like_delta"
                }
                .to_string()
            },
            str::to_string,
        )
    } else if ignored_paths.contains(&entry.path) {
        "ignored".to_string()
    } else {
        "source_like_delta".to_string()
    }
}

fn real_execution_outputs(
    repo_root: &Path,
    before: &[RealArtifactEntry],
    after: &[RealArtifactEntry],
    cancellation: &AtomicBool,
) -> Result<Vec<RealExecutionOutputSnapshot>, CliError> {
    let changed = after
        .iter()
        .filter(|entry| !entry.tombstone)
        .filter_map(|entry| {
            let before_entry = before
                .iter()
                .find(|candidate| candidate.path == entry.path && !candidate.tombstone);
            if before_entry.map(|candidate| candidate.content_hash.as_str())
                == Some(entry.content_hash.as_str())
            {
                None
            } else {
                Some((entry, before_entry))
            }
        })
        .collect::<Vec<_>>();
    let paths_needing_git = changed
        .iter()
        .filter(|(entry, before_entry)| {
            entry.classification != "generated"
                && !(before_entry.is_none()
                    && common_created_output_classification(&entry.path).is_some())
        })
        .map(|(entry, _)| entry.path.clone())
        .collect::<Vec<_>>();
    let mut ignored_paths =
        repository_ignored_execution_paths(repo_root, &paths_needing_git, cancellation)?;
    for path in &paths_needing_git {
        if path_is_sunignored(repo_root, path, false)? {
            ignored_paths.insert(path.clone());
        }
    }
    Ok(changed
        .into_iter()
        .map(|(entry, before_entry)| RealExecutionOutputSnapshot {
            path: entry.path.clone(),
            classification: classify_execution_output(
                entry,
                before_entry.is_none(),
                &ignored_paths,
            ),
            before_hash: before_entry.map(|candidate| candidate.content_hash.clone()),
            after_hash: entry.content_hash.clone(),
            byte_length: entry.bytes.len() as u64,
        })
        .collect())
}

fn real_output_classification(value: &str) -> OutputClassification {
    match value {
        "generated_artifact" => OutputClassification::GeneratedArtifact,
        "log" => OutputClassification::Log,
        "cache" => OutputClassification::Cache,
        "coverage" => OutputClassification::Coverage,
        "secret" => OutputClassification::Secret,
        "ignored" => OutputClassification::Ignored,
        _ => OutputClassification::SourceLikeDelta,
    }
}

fn real_now_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("unix_ms_{millis}")
}

fn real_artifact_view(entry: &RealArtifactEntry) -> SessionVisibleArtifactView {
    SessionVisibleArtifactView {
        artifact_id: entry.artifact_id.clone(),
        path: entry.path.clone(),
        kind: ArtifactKind::File,
        content_hash: entry.content_hash.clone(),
        byte_length: entry.bytes.len(),
        classification: entry.classification.clone(),
        executable: entry.executable,
        tombstone: entry.tombstone,
    }
}

fn real_precondition_error(
    state: &RealRepoState,
    entry: &RealArtifactEntry,
    expected: &str,
) -> CliError {
    CliError::new(
        "precondition_failed",
        "mutation precondition failed: expected_hash",
    )
    .with_detail("failed_precondition", "expected_hash")
    .with_detail("path", entry.path.clone())
    .with_detail("artifact_id", entry.artifact_id.clone())
    .with_detail("expected", expected)
    .with_detail("actual", entry.content_hash.clone())
    .with_detail(
        "session_generation_id",
        real_view(&state).session_generation_id,
    )
    .with_detail("resolved_view_id", state.resolved_view_id.clone())
}

#[derive(Debug)]
struct UnifiedPatchLine {
    patch_line: usize,
    kind: char,
    text: String,
}

#[derive(Debug)]
struct UnifiedPatchHunk {
    old_start: Option<usize>,
    lines: Vec<UnifiedPatchLine>,
}

fn patch_failure(
    code: &'static str,
    message: &'static str,
    hunk: usize,
    patch_line: usize,
    reason: impl Into<String>,
) -> CliError {
    CliError::new(code, message)
        .with_detail("failed_hunk", hunk.to_string())
        .with_detail("patch_line", patch_line.to_string())
        .with_detail("reason", reason.into())
}

fn parse_unified_range(
    value: &str,
    side: &'static str,
    hunk: usize,
    patch_line: usize,
) -> Result<(usize, usize), CliError> {
    let (start, count) = value
        .split_once(',')
        .map(|(start, count)| (start, Some(count)))
        .unwrap_or((value, None));
    let start = start.parse::<usize>().map_err(|_| {
        patch_failure(
            "patch_parse_failed",
            "unified diff hunk header is invalid",
            hunk,
            patch_line,
            format!("{side} range start is not an unsigned integer"),
        )
    })?;
    let count = count.unwrap_or("1").parse::<usize>().map_err(|_| {
        patch_failure(
            "patch_parse_failed",
            "unified diff hunk header is invalid",
            hunk,
            patch_line,
            format!("{side} range count is not an unsigned integer"),
        )
    })?;
    Ok((start, count))
}

fn parse_unified_hunk_header(
    line: &str,
    hunk: usize,
    patch_line: usize,
) -> Result<Option<usize>, CliError> {
    if line == "@@" || (line.starts_with("@@ ") && !line.starts_with("@@ -")) {
        return Ok(None);
    }
    let Some(body) = line.strip_prefix("@@ -") else {
        return Err(patch_failure(
            "patch_parse_failed",
            "unified diff hunk header is invalid",
            hunk,
            patch_line,
            "expected @@ or @@ -old_start,old_count +new_start,new_count @@",
        ));
    };
    let Some((ranges, _label)) = body.split_once(" @@") else {
        return Err(patch_failure(
            "patch_parse_failed",
            "unified diff hunk header is invalid",
            hunk,
            patch_line,
            "hunk header is missing its closing @@",
        ));
    };
    let Some((old_range, new_range)) = ranges.split_once(" +") else {
        return Err(patch_failure(
            "patch_parse_failed",
            "unified diff hunk header is invalid",
            hunk,
            patch_line,
            "hunk header is missing the +new range",
        ));
    };
    let (old_start, _old_count) = parse_unified_range(old_range, "old", hunk, patch_line)?;
    let _ = parse_unified_range(new_range, "new", hunk, patch_line)?;
    Ok(Some(old_start))
}

fn parse_unified_patch(patch: &str) -> Result<Vec<UnifiedPatchHunk>, CliError> {
    let mut hunks = Vec::new();
    let mut current: Option<UnifiedPatchHunk> = None;
    for (index, raw_line) in patch.lines().enumerate() {
        let patch_line = index + 1;
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if matches!(line, "*** Begin Patch" | "*** End Patch")
            || line.starts_with("*** Update File:")
        {
            continue;
        }
        if line.starts_with("@@") {
            if let Some(hunk) = current.take() {
                hunks.push(hunk);
            }
            let hunk_number = hunks.len() + 1;
            let old_start = parse_unified_hunk_header(line, hunk_number, patch_line)?;
            current = Some(UnifiedPatchHunk {
                old_start,
                lines: Vec::new(),
            });
            continue;
        }

        if let Some(hunk) = current.as_mut() {
            if line == "\\ No newline at end of file" {
                continue;
            }
            let Some(kind) = line.chars().next() else {
                return Err(patch_failure(
                    "patch_parse_failed",
                    "unified diff contains an unprefixed hunk line",
                    hunks.len() + 1,
                    patch_line,
                    "empty context lines must start with one space",
                ));
            };
            if !matches!(kind, ' ' | '-' | '+') {
                return Err(patch_failure(
                    "patch_parse_failed",
                    "unified diff contains an unsupported hunk line",
                    hunks.len() + 1,
                    patch_line,
                    "hunk lines must start with space, -, or +",
                ));
            }
            hunk.lines.push(UnifiedPatchLine {
                patch_line,
                kind,
                text: line[1..].to_string(),
            });
            continue;
        }

        if line.is_empty()
            || line.starts_with("diff ")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
        {
            continue;
        }
        return Err(patch_failure(
            "patch_parse_failed",
            "content appeared before the first unified diff hunk",
            0,
            patch_line,
            "start changes with an @@ hunk header; optional diff/---/+++ headers may precede it",
        ));
    }
    if let Some(hunk) = current {
        hunks.push(hunk);
    }
    if hunks.is_empty() {
        return Err(patch_failure(
            "patch_parse_failed",
            "unified diff contains no hunks",
            0,
            0,
            "add at least one @@ or @@ -old +new @@ hunk",
        ));
    }
    Ok(hunks)
}

fn patch_line_preview(value: &str) -> String {
    let mut preview = value.chars().take(256).collect::<String>();
    if value.chars().count() > 256 {
        preview.push_str("...");
    }
    preview
}

fn hunk_matches_source(source: &[&str], target: usize, hunk: &UnifiedPatchHunk) -> bool {
    let old_lines = hunk
        .lines
        .iter()
        .filter(|line| matches!(line.kind, ' ' | '-'))
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>();
    source
        .get(target..target.saturating_add(old_lines.len()))
        .is_some_and(|candidate| candidate == old_lines.as_slice())
}

fn locate_patch_hunk(
    source: &[&str],
    source_cursor: usize,
    hunk: &UnifiedPatchHunk,
    hunk_number: usize,
) -> Result<usize, CliError> {
    let old_line_count = hunk
        .lines
        .iter()
        .filter(|line| matches!(line.kind, ' ' | '-'))
        .count();
    if old_line_count == 0 {
        return Ok(hunk
            .old_start
            .map(|start| start.saturating_sub(1))
            .unwrap_or(source_cursor));
    }
    if let Some(declared) = hunk.old_start.map(|start| start.saturating_sub(1)) {
        if declared >= source_cursor && hunk_matches_source(source, declared, hunk) {
            return Ok(declared);
        }
    }
    let candidates = (source_cursor..=source.len().saturating_sub(old_line_count))
        .filter(|target| hunk_matches_source(source, *target, hunk))
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [target] => Ok(*target),
        [] => Err(patch_failure(
            "patch_apply_failed",
            "patch context did not match the source",
            hunk_number,
            hunk.lines.first().map(|line| line.patch_line).unwrap_or(0),
            "refresh the artifact and include exact surrounding context",
        )),
        _ => Err(patch_failure(
            "patch_apply_failed",
            "patch context is ambiguous",
            hunk_number,
            hunk.lines.first().map(|line| line.patch_line).unwrap_or(0),
            "include more unchanged surrounding lines so the target is unique",
        )),
    }
}

fn apply_real_patch(before: &str, patch: &str) -> Result<(String, usize), CliError> {
    let hunks = parse_unified_patch(patch)?;
    let line_ending = if before.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let normalized = before.replace("\r\n", "\n");
    let had_final_newline = normalized.ends_with('\n');
    let source = normalized.split_terminator('\n').collect::<Vec<_>>();
    let mut output = Vec::<String>::new();
    let mut source_cursor = 0usize;

    for (index, hunk) in hunks.iter().enumerate() {
        let hunk_number = index + 1;
        let target = locate_patch_hunk(&source, source_cursor, hunk, hunk_number)?;
        if target < source_cursor || target > source.len() {
            return Err(patch_failure(
                "patch_apply_failed",
                "unified diff hunk starts outside the available source range",
                hunk_number,
                hunk.lines.first().map(|line| line.patch_line).unwrap_or(0),
                format!(
                    "target source index {}, current cursor={}, source lines={}",
                    target,
                    source_cursor,
                    source.len()
                ),
            ));
        }
        output.extend(
            source[source_cursor..target]
                .iter()
                .map(|line| (*line).to_string()),
        );
        source_cursor = target;

        for line in &hunk.lines {
            match line.kind {
                ' ' | '-' => {
                    let Some(actual) = source.get(source_cursor) else {
                        return Err(patch_failure(
                            "patch_apply_failed",
                            "unified diff expected a source line past end of file",
                            hunk_number,
                            line.patch_line,
                            format!("expected {:?}", patch_line_preview(&line.text)),
                        ));
                    };
                    if *actual != line.text {
                        return Err(patch_failure(
                            "patch_apply_failed",
                            "unified diff context or removal did not match the source",
                            hunk_number,
                            line.patch_line,
                            format!(
                                "source_line={} expected={:?} actual={:?}",
                                source_cursor + 1,
                                patch_line_preview(&line.text),
                                patch_line_preview(actual)
                            ),
                        ));
                    }
                    if line.kind == ' ' {
                        output.push(line.text.clone());
                    }
                    source_cursor += 1;
                }
                '+' => {
                    output.push(line.text.clone());
                }
                _ => unreachable!("parser accepts only unified diff line prefixes"),
            }
        }
    }

    output.extend(
        source[source_cursor..]
            .iter()
            .map(|line| (*line).to_string()),
    );
    let mut result = output.join(line_ending);
    if had_final_newline {
        result.push_str(line_ending);
    }
    Ok((result, hunks.len()))
}

#[cfg(test)]
mod unified_patch_tests {
    use super::{
        apply_real_patch, common_created_output_classification, execution_output_requires_promotion,
    };

    #[test]
    fn applies_multiple_standard_hunks() {
        let before = "alpha\nbeta\ngamma\ndelta\nepsilon\n";
        let patch =
            "@@ -1,2 +1,2 @@\n alpha\n-beta\n+BETA\n@@ -4,2 +4,2 @@\n delta\n-epsilon\n+EPSILON\n";
        let (after, hunks) = apply_real_patch(before, patch).unwrap();
        assert_eq!(hunks, 2);
        assert_eq!(after, "alpha\nBETA\ngamma\ndelta\nEPSILON\n");
    }

    #[test]
    fn preserves_crlf_line_endings() {
        let before = "alpha\r\nbeta\r\n";
        let patch = "@@ -1,2 +1,2 @@\n alpha\n-beta\n+BETA\n";
        let (after, hunks) = apply_real_patch(before, patch).unwrap();
        assert_eq!(hunks, 1);
        assert_eq!(after, "alpha\r\nBETA\r\n");
    }

    #[test]
    fn accepts_apply_patch_envelopes_and_context_located_hunks() {
        let (after, hunks) = apply_real_patch(
            "alpha\n",
            "*** Begin Patch\n*** Update File: src.txt\n@@\n-alpha\n+ALPHA\n*** End Patch\n",
        )
        .unwrap();
        assert_eq!(hunks, 1);
        assert_eq!(after, "ALPHA\n");
    }

    #[test]
    fn infers_incorrect_header_counts_when_context_is_exact() {
        let (after, hunks) = apply_real_patch(
            "alpha\nbeta\ngamma\n",
            "@@ -99,50 +99,60 @@\n beta\n-gamma\n+GAMMA\n",
        )
        .unwrap();
        assert_eq!(hunks, 1);
        assert_eq!(after, "alpha\nbeta\nGAMMA\n");
    }
    #[test]
    fn common_build_outputs_are_not_source_promotion_candidates() {
        for path in [
            "dist/app.js",
            "build/output.bin",
            "target/debug/app.exe",
            "coverage/report.json",
            ".next/server/app.js",
        ] {
            assert_eq!(common_created_output_classification(path), Some("ignored"));
        }
        for path in [
            "tsconfig.tsbuildinfo",
            "node_modules/.tmp/cache.json",
            ".cache/tool/state",
            "src/__pycache__/module.pyc",
        ] {
            assert_eq!(common_created_output_classification(path), Some("cache"));
        }
        let ignored = super::RealExecutionOutputSnapshot {
            path: "dist/app.js".to_string(),
            classification: "ignored".to_string(),
            before_hash: None,
            after_hash: "sha256:after".to_string(),
            byte_length: 1,
        };
        let mut source = ignored.clone();
        source.classification = "source_like_delta".to_string();
        assert!(!execution_output_requires_promotion(&ignored));
        assert!(execution_output_requires_promotion(&source));
        assert_eq!(
            common_created_output_classification("src/generated/client.ts"),
            None
        );
    }
}

fn real_mutation_ref(
    entry: Option<&RealArtifactEntry>,
    path: &str,
    absent: bool,
) -> sunlight_core::artifacts::MutationArtifactRef {
    sunlight_core::artifacts::MutationArtifactRef {
        artifact_id: entry.map(|entry| entry.artifact_id.clone()),
        path: entry
            .map(|entry| entry.path.clone())
            .unwrap_or_else(|| path.to_string()),
        path_state: if absent {
            "absent".to_string()
        } else if entry.is_some_and(|entry| entry.tombstone) {
            "tombstone".to_string()
        } else {
            "active".to_string()
        },
        content_hash: entry.map(|entry| entry.content_hash.clone()),
        executable: entry.map(|entry| entry.executable),
        classification: entry.map(|entry| entry.classification.clone()),
    }
}

fn real_checkpoint(state: &RealRepoState, evidence_refs: Vec<EvidenceRef>) -> CheckpointRecord {
    let tree_component = state
        .tree_hash
        .trim_start_matches("tree_")
        .chars()
        .take(16)
        .collect::<String>();
    let id = if evidence_refs.is_empty() {
        format!("checkpoint_{tree_component}")
    } else {
        let evidence_identity = evidence_refs
            .iter()
            .map(|evidence| match evidence {
                EvidenceRef::Execution(execution) => format!(
                    "{}\0{}\0{}\0{}",
                    execution.execution_id,
                    execution.result.as_str(),
                    execution.resolved_view_id,
                    execution.tree_identity.tree_hash
                ),
            })
            .collect::<Vec<_>>()
            .join("\0");
        let evidence_component = real_content_hash(evidence_identity.as_bytes())
            .trim_start_matches("sha256:")
            .chars()
            .take(12)
            .collect::<String>();
        format!("checkpoint_{tree_component}_{evidence_component}")
    };
    CheckpointRecord {
        id,
        repository_id: state.repository_id.clone(),
        resolved_view_id: state.resolved_view_id.clone(),
        tree_identity: SingleRepoTree {
            repository_id: state.repository_id.clone(),
            tree_hash: state.tree_hash.clone(),
        },
        topic_frontier: state
            .topics
            .iter()
            .filter_map(|topic| {
                topic.head_revision_id.as_ref().map(|revision_id| {
                    sunlight_core::checkpoint::TopicFrontierEntry {
                        topic_id: topic.topic_id.clone(),
                        topic_revision_id: revision_id.clone(),
                    }
                })
            })
            .collect(),
        evidence_refs,
        conflict_free: true,
        created_by: sunlight_core::checkpoint::CreatedBy {
            actor_id: state
                .actor_id
                .clone()
                .unwrap_or_else(|| "operator".to_string()),
            command: "checkpoint.create".to_string(),
        },
        created_at: real_now_id(),
        retention_class: sunlight_core::checkpoint::RetentionClass::Landable,
        export_refs: Vec::new(),
        privacy_class: sunlight_core::records::PrivacyClass::CommitDefault,
    }
}

fn real_projection_manifest_digest(
    state: &RealRepoState,
    projection_id: &str,
    purpose: ProjectionPurpose,
    strategy: RealProjectionStrategy,
) -> String {
    let payload = format!(
        "{}\0{}\0{}\0{}\0{}\0{}",
        projection_id,
        purpose.as_str(),
        state.repository_id,
        state.resolved_view_id,
        state.tree_hash,
        strategy.as_str(),
    );
    real_content_hash(payload.as_bytes())
}

fn real_projection_strategy(strategy: ProjectionStrategy) -> RealProjectionStrategy {
    match strategy {
        ProjectionStrategy::Copy => RealProjectionStrategy::Copy,
        ProjectionStrategy::Reflink => RealProjectionStrategy::Reflink,
        ProjectionStrategy::HardlinkReadonly => RealProjectionStrategy::HardlinkReadonly,
        ProjectionStrategy::OverlayCopyup => RealProjectionStrategy::OverlayCopyup,
    }
}

fn materialize_repo_projection(
    repo_root: &Path,
    state: &RealRepoState,
    root: &Path,
    purpose: ProjectionPurpose,
    strategy: Option<ProjectionStrategy>,
    fallback_to_copy: bool,
) -> Result<RealProjectionMaterialization, CliError> {
    materialize_real_projection(
        repo_root,
        state,
        root,
        &RealProjectionMaterializationRequest {
            purpose,
            writable_policy: match purpose {
                ProjectionPurpose::Execution => WritablePolicy::ReadOnlySourcePrivateOutputs,
                ProjectionPurpose::Compatibility => WritablePolicy::WritableWithExplicitImport,
                ProjectionPurpose::Inspection => WritablePolicy::ReadOnly,
                ProjectionPurpose::Export => WritablePolicy::ExportMaterializationOnly,
            },
            path_policy_id: POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string(),
            operation_semantics_version: FILE_OPERATION_SEMANTICS_VERSION.to_string(),
            required_strategy: strategy.map(real_projection_strategy),
            fallback_to_copy,
        },
    )
    .map_err(CliError::from)
}

fn selected_real_projection_id(
    purpose: ProjectionPurpose,
    strategy: RealProjectionStrategy,
    sequence: usize,
) -> String {
    format!(
        "projection_{}_{}_native_{sequence:04}",
        purpose.as_str(),
        strategy.as_str()
    )
}

fn relocate_managed_projection_root(from: &Path, to: &Path) -> Result<(), CliError> {
    if from == to {
        return Ok(());
    }
    let from_container = from
        .parent()
        .ok_or_else(|| invalid_request("managed projection root has no container"))?;
    let to_container = to
        .parent()
        .ok_or_else(|| invalid_request("managed projection root has no container"))?;
    let parent = to_container
        .parent()
        .ok_or_else(|| invalid_request("managed projection destination has no parent"))?;
    fs::create_dir_all(parent).map_err(|error| {
        invalid_request(format!(
            "failed to create managed projection parent: {error}"
        ))
    })?;
    if let Err(error) = fs::rename(from_container, to_container) {
        let _ = fs::remove_dir_all(from_container);
        return Err(CliError::new(
            "projection_materialization_projection_root_unavailable",
            format!("failed to bind selected projection identity to its managed root: {error}"),
        )
        .with_detail("path", to.display().to_string()));
    }
    Ok(())
}

fn real_projection_publication(
    state: &RealRepoState,
    projection: &RealProjectionSnapshot,
) -> Result<DerivedRecordPublication, CliError> {
    Ok(state
        .record_publication(
            "projections",
            &projection.projection_id,
            &format!("{}\n", real_projection_snapshot_json(state, projection)),
        )
        .map_err(CliError::from)?)
}

fn real_git_export_content_files(state: &RealRepoState) -> Vec<GitExportContentFile> {
    state
        .entries
        .iter()
        .filter(|entry| !entry.tombstone)
        .map(|entry| GitExportContentFile {
            path: entry.path.clone(),
            bytes: entry.bytes.clone(),
            executable: entry.executable,
        })
        .collect()
}

fn real_git_export_writer_input(
    repo_root: &PathBuf,
    state: &RealRepoState,
    options: &GitExportOptions,
    checkpoint: CheckpointRecord,
    validation_report: GitExportValidationReport,
) -> Result<GitExportWriterInput, CliError> {
    let repo_root = fs::canonicalize(repo_root).map_err(|error| {
        invalid_request(format!(
            "failed to resolve local Git repository path `{}`: {error}",
            repo_root.display()
        ))
    })?;
    let repo_root_string = repo_root.display().to_string();
    let base_commit_id = run_git_capture(&repo_root, &["rev-parse", "--verify", "HEAD^{commit}"])
        .map_err(|message| {
            invalid_request(format!(
                "sun git export --execute-local requires a local Git repository with HEAD: {message}"
            ))
        })?
        .trim()
        .to_string();
    let target_ref = if options.git_ref.starts_with("refs/") {
        options.git_ref.clone()
    } else {
        format!("refs/heads/{}", options.git_ref)
    };
    let target_ref_commit_id = run_git_capture(&repo_root, &["rev-parse", "--verify", &target_ref])
        .ok()
        .map(|commit_id| commit_id.trim().to_string())
        .filter(|commit_id| !commit_id.is_empty());
    let refs = target_ref_commit_id
        .clone()
        .map(|commit_id| GitRefState {
            git_ref: target_ref.clone(),
            commit_id,
        })
        .into_iter()
        .collect();
    let mut reachable_commit_ids = vec![base_commit_id.clone()];
    if let Some(commit_id) = target_ref_commit_id {
        if !reachable_commit_ids.contains(&commit_id) {
            reachable_commit_ids.push(commit_id);
        }
    }
    let mut request = GitExportRequest::from_checkpoint(&checkpoint);
    request.git_ref = target_ref.clone();
    request.validation_report_id = validation_report.id.clone();
    let prior_export_maps = state
        .export_maps
        .iter()
        .filter_map(|export_map| {
            Some(GitExportMapRecord {
                id: export_map.export_map_id.clone(),
                repository_id: state.repository_id.clone(),
                checkpoint_id: export_map.checkpoint_id.clone(),
                tree_identity: SingleRepoTree {
                    repository_id: state.repository_id.clone(),
                    tree_hash: export_map.tree_hash.clone(),
                },
                git_remote: None,
                git_ref: export_map.git_ref.clone(),
                git_commit_ids: export_map.git_commit_ids.clone(),
                export_shape: request.export_shape.clone(),
                validation_report_id: export_map.validation_report_id.clone()?,
                exported_at: export_map.exported_at.clone(),
                privacy_class: sunlight_core::records::PrivacyClass::CommitDefault,
            })
        })
        .collect();
    Ok(GitExportWriterInput {
        base_checkpoint_ids: vec!["checkpoint_base_0001".to_string()],
        imported_base_commits: vec![ImportedBaseGitCommit {
            checkpoint_id: "checkpoint_base_0001".to_string(),
            git_commit_id: base_commit_id.clone(),
        }],
        prior_export_maps,
        planned_commit_id: "planned_commit_id_replaced_by_real_git".to_string(),
        export_map_id: format!("export_map_{}", checkpoint.id),
        exported_at: FIXTURE_CREATED_AT.to_string(),
        request,
        validation_report,
        repository: GitExportRepositoryState {
            repository_id: checkpoint.repository_id.clone(),
            git_root: repo_root_string.clone(),
            sunlight_repo_root: repo_root_string,
            reachable_commit_ids,
            refs,
        },
    })
}

fn media_type_for_path(path: &str) -> &'static str {
    if path.ends_with(".md") {
        "text/markdown; charset=utf-8"
    } else if path.ends_with(".rs") {
        "text/rust; charset=utf-8"
    } else if path.ends_with(".ts") {
        "text/typescript; charset=utf-8"
    } else if path.ends_with(".sh") {
        "text/x-shellscript; charset=utf-8"
    } else {
        "text/plain; charset=utf-8"
    }
}

fn real_topic_create_success_envelope(state: &RealRepoState) -> String {
    let topic = state
        .topic_id
        .as_deref()
        .and_then(|topic_id| state.topic_by_id_or_slug(topic_id))
        .expect("created topic is present after compatibility fields synchronize");
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"topic.create\",\"repository_id\":\"{}\",\"ids\":{{\"topic_id\":\"{}\",\"base_checkpoint_id\":\"{}\",\"head_revision_id\":null}},\"view\":null,\"topic\":{}}},\"warnings\":[]}}",
        json_escape(&state.repository_id),
        json_escape(&topic.topic_id),
        json_escape(&topic.base_checkpoint_id),
        real_topic_metadata_json(topic),
    )
}

fn real_topic_metadata_json(topic: &RealTopicRecord) -> String {
    let status = if topic.completed_revision_id.is_some() {
        "completed"
    } else {
        "open"
    };
    format!(
        "{{\"topic_id\":\"{}\",\"slug\":\"{}\",\"display_name\":\"{}\",\"status\":\"{}\",\"lifecycle\":\"{}\",\"base_checkpoint_id\":\"{}\",\"head_revision_id\":{},\"completed_revision_id\":{},\"owner_actor_id\":\"{}\",\"visibility\":\"{}\",\"acceptance_criteria\":{}}}",
        json_escape(&topic.topic_id),
        json_escape(&topic.slug),
        json_escape(&topic.display_name),
        status,
        status,
        json_escape(&topic.base_checkpoint_id),
        optional_string_json(topic.head_revision_id.as_deref()),
        optional_string_json(topic.completed_revision_id.as_deref()),
        json_escape(&topic.owner_actor_id),
        json_escape(&topic.visibility),
        string_array_json(topic.acceptance_criteria.iter().map(String::as_str)),
    )
}

fn real_topic_handoff_json(
    repo_root: &Path,
    state: &RealRepoState,
    topic: &RealTopicRecord,
) -> String {
    let completion = topic.completed_revision_id.as_ref().and_then(|_| {
        let path = repo_root.join(format!(
            ".sunlight/topics/completion_{}.json",
            topic.topic_id
        ));
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
    });
    let completion_string = |field: &str| {
        completion
            .as_ref()
            .and_then(|value| value.get(field))
            .and_then(serde_json::Value::as_str)
    };
    let topic_operations = state
        .operations
        .iter()
        .filter(|operation| operation.topic_id == topic.topic_id)
        .collect::<Vec<_>>();
    let mut changed_paths = BTreeSet::new();
    let operations_json = topic_operations
        .iter()
        .map(|operation| {
            let effects = operation
                .artifact_effects()
                .into_iter()
                .map(|effect| {
                    changed_paths.insert(effect.path.clone());
                    format!(
                        "{{\"artifact_id\":\"{}\",\"path\":\"{}\",\"before_hash\":{},\"after_hash\":\"{}\",\"classification\":\"{}\",\"tombstone\":{}}}",
                        json_escape(&effect.artifact_id),
                        json_escape(&effect.path),
                        optional_string_json(effect.base_content_hash.as_deref()),
                        json_escape(&effect.result_content_hash),
                        json_escape(&effect.classification),
                        effect.tombstone,
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"operation_transaction_id\":\"{}\",\"topic_revision_id\":\"{}\",\"session_id\":\"{}\",\"mutation\":\"{}\",\"effects\":[{}]}}",
                json_escape(&operation.operation_transaction_id),
                json_escape(&operation.topic_revision_id),
                json_escape(&operation.session_id),
                json_escape(&operation.mutation),
                effects,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"available\":{},\"immutable\":{},\"exact_ids\":{{\"topic_id\":\"{}\",\"completed_revision_id\":{},\"session_id\":{}}},\"completed_revision_id\":{},\"summary\":{},\"session_id\":{},\"actor_id\":{},\"operation_count\":{},\"changed_paths\":{},\"operations\":[{}]}}",
        topic.completed_revision_id.is_some(),
        topic.completed_revision_id.is_some(),
        json_escape(&topic.topic_id),
        optional_string_json(topic.completed_revision_id.as_deref()),
        optional_string_json(completion_string("session_id")),
        optional_string_json(topic.completed_revision_id.as_deref()),
        optional_string_json(completion_string("summary")),
        optional_string_json(completion_string("session_id")),
        optional_string_json(completion_string("actor_id")),
        topic_operations.len(),
        string_array_json(changed_paths.iter().map(String::as_str)),
        operations_json,
    )
}

fn real_session_start_success_envelope(
    state: &RealRepoState,
    topic: &RealTopicRecord,
    session: &RealSessionRecord,
) -> String {
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"session.start\",\"repository_id\":\"{}\",\"ids\":{{\"topic_id\":\"{}\",\"session_id\":\"{}\",\"resolved_view_id\":\"{}\",\"session_generation_id\":\"{}\"}},\"view\":{},\"session\":{{\"session_id\":\"{}\",\"actor_id\":\"{}\",\"write_topic_id\":\"{}\",\"resolved_view_id\":\"{}\",\"session_generation_id\":\"{}\",\"refresh_policy\":\"{}\",\"capabilities\":{}}},\"topic_frontier\":{}}},\"warnings\":[]}}",
        json_escape(&state.repository_id),
        json_escape(&topic.topic_id),
        json_escape(&session.session_id),
        json_escape(&session.resolved_view_id),
        json_escape(&session.session_generation_id),
        view_json(&real_session_view(state, session)),
        json_escape(&session.session_id),
        json_escape(&session.actor_id),
        json_escape(&topic.topic_id),
        json_escape(&session.resolved_view_id),
        json_escape(&session.session_generation_id),
        json_escape(&session.refresh_policy),
        phase1_capabilities_json(),
        string_map_object_json(&session.topic_frontier),
    )
}

fn real_projection_materialized_envelope(
    state: &RealRepoState,
    projection_id: &str,
    purpose: ProjectionPurpose,
    root: &PathBuf,
    materialization: &RealProjectionMaterialization,
) -> String {
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"projection.materialize\",\"repository_id\":\"{}\",\"ids\":{{\"projection_id\":\"{}\",\"resolved_view_id\":\"{}\"}},\"view\":{{\"resolved_view_id\":\"{}\",\"tree_identity\":{}}},\"projection_id\":\"{}\",\"purpose\":\"{}\",\"selected_strategy\":\"{}\",\"strategy\":\"{}\",\"tree_identity\":{},\"source\":\"resolved_content_tree\",\"cache_key\":\"{}\",\"projection_root\":\"{}\",\"materialization\":{},\"files_written\":{},\"bytes_written\":{},\"retention_state\":\"local_only\"}},\"warnings\":[]}}",
        json_escape(&state.repository_id),
        json_escape(projection_id),
        json_escape(&state.resolved_view_id),
        json_escape(&state.resolved_view_id),
        single_repo_tree_json(&SingleRepoTree { repository_id: state.repository_id.clone(), tree_hash: state.tree_hash.clone() }),
        json_escape(projection_id),
        purpose.as_str(),
        materialization.strategy.as_str(),
        materialization.strategy.as_str(),
        single_repo_tree_json(&SingleRepoTree { repository_id: state.repository_id.clone(), tree_hash: state.tree_hash.clone() }),
        json_escape(&materialization.cache_key),
        json_escape(&root.display().to_string()),
        real_materialization_metrics_json(&materialization.metrics),
        materialization.metrics.file_count,
        materialization.metrics.logical_bytes,
    )
}

fn real_materialization_metrics_json(
    metrics: &sunlight_core::repo_state::RealProjectionMaterializationMetrics,
) -> String {
    let amplification = metrics
        .storage_amplification_millionths
        .map(|value| format!("{:.6}", value as f64 / 1_000_000.0))
        .unwrap_or_else(|| "null".to_string());
    format!(
        "{{\"elapsed_ms\":{},\"logical_bytes\":{},\"physically_materialized_bytes\":{},\"physical_allocation_bytes\":{},\"file_count\":{},\"cache_hit\":{},\"reuse\":\"{}\",\"integrity_revalidated\":{},\"storage_amplification\":{}}}",
        metrics.elapsed_ms,
        metrics.logical_bytes,
        metrics.physically_materialized_bytes.map(|value| value.to_string()).unwrap_or_else(|| "null".to_string()),
        metrics.physical_allocation_bytes.map(|value| value.to_string()).unwrap_or_else(|| "null".to_string()),
        metrics.file_count,
        metrics.cache_hit,
        json_escape(&metrics.reuse),
        metrics.integrity_revalidated,
        amplification,
    )
}

fn real_projection_snapshot_json(
    state: &RealRepoState,
    projection: &RealProjectionSnapshot,
) -> String {
    format!(
        concat!(
            "{{",
            "\"schema_version\":1,",
            "\"record_type\":\"projection\",",
            "\"id\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"purpose\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{},",
            "\"manifest_digest\":\"{}\",",
            "\"created_from_content_tree\":\"{}\",",
            "\"materialized_root\":{},",
            "\"session_id\":{},",
            "\"session_generation_id\":{},",
            "\"path_policy_id\":\"{}\",",
            "\"operation_semantics_version\":\"{}\",",
            "\"cache_key\":\"{}\",",
            "\"strategy\":\"{}\",",
            "\"materialization\":{},",
            "\"retention_state\":\"{}\",",
            "\"privacy_class\":\"{}\",",
            "\"last_import_operation_id\":{},",
            "\"entry_count\":{},",
            "\"source_truth\":\"sunlight_persisted_resolved_view\"",
            "}}"
        ),
        json_escape(&projection.projection_id),
        json_escape(&state.repository_id),
        json_escape(&projection.projection_id),
        json_escape(&projection.purpose),
        json_escape(&projection.resolved_view_id),
        single_repo_tree_json(&SingleRepoTree {
            repository_id: state.repository_id.clone(),
            tree_hash: projection.tree_hash.clone(),
        }),
        json_escape(&projection.manifest_digest),
        json_escape(&projection.created_from_content_tree),
        optional_string_json(projection.materialized_root.as_deref()),
        optional_string_json(projection.session_id.as_deref()),
        optional_string_json(projection.session_generation_id.as_deref()),
        json_escape(&projection.path_policy_id),
        json_escape(&projection.operation_semantics_version),
        json_escape(&projection.cache_key),
        json_escape(&projection.strategy),
        projection
            .materialization
            .as_ref()
            .map(real_materialization_metrics_json)
            .unwrap_or_else(|| "null".to_string()),
        json_escape(&projection.retention_state),
        json_escape(&projection.privacy_class),
        optional_string_json(projection.last_import_operation_id.as_deref()),
        projection
            .entries
            .iter()
            .filter(|entry| !entry.tombstone)
            .count(),
    )
}

fn real_compat_project_envelope(
    state: &RealRepoState,
    projection: &RealProjectionSnapshot,
) -> String {
    let tree = SingleRepoTree {
        repository_id: state.repository_id.clone(),
        tree_hash: projection.tree_hash.clone(),
    };
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"compat.project\",\"repository_id\":\"{}\",\"ids\":{{\"projection_id\":\"{}\",\"session_id\":{},\"resolved_view_id\":\"{}\",\"session_generation_id\":{}}},\"projection_id\":\"{}\",\"session_id\":{},\"baseline\":{{\"resolved_view_id\":\"{}\",\"session_generation_id\":{},\"tree_identity\":{},\"manifest_digest\":\"{}\"}},\"purpose\":\"compatibility\",\"root_ref\":{{\"value\":{},\"privacy\":\"local_only_path\",\"privacy_class\":\"local_only\"}},\"strategy\":\"{}\",\"retention_state\":\"{}\",\"privacy_class\":\"{}\",\"path_policy\":{{\"path_policy_id\":\"{}\",\"operation_semantics_version\":\"{}\"}},\"projection\":{}}},\"warnings\":[]}}",
        json_escape(&state.repository_id),
        json_escape(&projection.projection_id),
        optional_string_json(projection.session_id.as_deref()),
        json_escape(&projection.resolved_view_id),
        optional_string_json(projection.session_generation_id.as_deref()),
        json_escape(&projection.projection_id),
        optional_string_json(projection.session_id.as_deref()),
        json_escape(&projection.resolved_view_id),
        optional_string_json(projection.session_generation_id.as_deref()),
        single_repo_tree_json(&tree),
        json_escape(&projection.manifest_digest),
        optional_string_json(projection.materialized_root.as_deref()),
        json_escape(&projection.strategy),
        json_escape(&projection.retention_state),
        json_escape(&projection.privacy_class),
        json_escape(&projection.path_policy_id),
        json_escape(&projection.operation_semantics_version),
        real_projection_snapshot_json(state, projection),
    )
}

fn real_compat_diff_envelope(
    state: &RealRepoState,
    projection: &RealProjectionSnapshot,
    candidates: &[CompatCandidateDelta],
) -> String {
    let safe = candidates
        .iter()
        .filter(|candidate| is_safe_default_compat_candidate(candidate))
        .map(|candidate| candidate.candidate_delta_id.as_str());
    let quarantine = candidates
        .iter()
        .filter_map(|candidate| candidate.quarantine_ref.as_deref());
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"compat.diff\",\"repository_id\":\"{}\",\"ids\":{{\"projection_id\":\"{}\",\"resolved_view_id\":\"{}\",\"session_generation_id\":{}}},\"projection_id\":\"{}\",\"session_generation_id\":{},\"baseline\":{{\"resolved_view_id\":\"{}\",\"session_generation_id\":{},\"tree_identity\":{},\"manifest_digest\":\"{}\"}},\"candidate_counts\":{},\"selected_candidate_delta_ids\":{},\"quarantine_refs\":{},\"candidates\":[{}],\"native_operation_ids\":[],\"native_revision_ids\":[]}},\"warnings\":[]}}",
        json_escape(&state.repository_id),
        json_escape(&projection.projection_id),
        json_escape(&projection.resolved_view_id),
        optional_string_json(projection.session_generation_id.as_deref()),
        json_escape(&projection.projection_id),
        optional_string_json(projection.session_generation_id.as_deref()),
        json_escape(&projection.resolved_view_id),
        optional_string_json(projection.session_generation_id.as_deref()),
        single_repo_tree_json(&SingleRepoTree {
            repository_id: state.repository_id.clone(),
            tree_hash: projection.tree_hash.clone(),
        }),
        json_escape(&projection.manifest_digest),
        compat_candidate_counts_json(candidates),
        string_array_json(safe),
        string_array_json(quarantine),
        candidates
            .iter()
            .map(compat_candidate_json)
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn real_worktree_diff_id(
    anchor: &RealWorktreeAnchor,
    candidates: &[CompatCandidateDelta],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(anchor.id().as_bytes());
    hasher.update([0]);
    hasher.update(anchor.manifest_digest.as_bytes());
    for candidate in candidates {
        hasher.update([0]);
        hasher.update(candidate.candidate_delta_id.as_bytes());
    }
    let digest = format!("{:x}", hasher.finalize());
    format!("worktree_diff_{}", &digest[..24])
}

fn real_worktree_anchor_summary_json(anchor: &RealWorktreeAnchor) -> String {
    format!(
        "{{\"worktree_anchor_id\":\"{}\",\"generation\":{},\"base_checkpoint_ids\":{},\"resolved_view_id\":\"{}\",\"tree_hash\":\"{}\",\"topic_frontier\":{},\"manifest_digest\":\"{}\",\"path_policy_id\":\"{}\",\"operation_semantics_version\":\"{}\"}}",
        json_escape(&anchor.id()),
        anchor.generation,
        string_array_json(anchor.base_checkpoint_ids.iter().map(String::as_str)),
        json_escape(&anchor.resolved_view_id),
        json_escape(&anchor.tree_hash),
        string_map_object_json(&anchor.topic_frontier),
        json_escape(&anchor.manifest_digest),
        json_escape(&anchor.path_policy_id),
        json_escape(&anchor.operation_semantics_version),
    )
}

fn real_worktree_diff_envelope(
    state: &RealRepoState,
    candidates: &[CompatCandidateDelta],
) -> String {
    let anchor = &state.worktree_anchor;
    let safe = candidates
        .iter()
        .filter(|candidate| is_safe_default_compat_candidate(candidate))
        .map(|candidate| candidate.candidate_delta_id.as_str());
    let worktree_state = if candidates.is_empty() {
        "clean"
    } else {
        "dirty"
    };
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"worktree.diff\",\"repository_id\":\"{}\",\"ids\":{{\"worktree_anchor_id\":\"{}\",\"resolved_view_id\":\"{}\",\"worktree_diff_id\":\"{}\"}},\"worktree_state\":\"{}\",\"anchor\":{},\"candidate_counts\":{},\"selected_candidate_delta_ids\":{},\"candidates\":[{}],\"native_operation_ids\":[],\"native_revision_ids\":[],\"next_action\":{}}},\"warnings\":[]}}",
        json_escape(&state.repository_id),
        json_escape(&anchor.id()),
        json_escape(&anchor.resolved_view_id),
        json_escape(&real_worktree_diff_id(anchor, candidates)),
        worktree_state,
        real_worktree_anchor_summary_json(anchor),
        compat_candidate_counts_json(candidates),
        string_array_json(safe),
        candidates
            .iter()
            .map(compat_candidate_json)
            .collect::<Vec<_>>()
            .join(","),
        if candidates.is_empty() {
            "null".to_string()
        } else {
            "\"Call worktree_capture with a topic slug and actor; omit selection to capture eligible source candidates.\"".to_string()
        },
    )
}

fn real_worktree_capture_noop_envelope(state: &RealRepoState) -> String {
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"worktree.capture\",\"repository_id\":\"{}\",\"ids\":{{\"worktree_anchor_id\":\"{}\",\"resolved_view_id\":\"{}\"}},\"captured\":false,\"reason\":\"worktree_clean\",\"anchor\":{},\"selected_delta_count\":0,\"candidate_delta_ids\":[],\"remaining_candidate_count\":0,\"native_operation_ids\":[],\"native_revision_ids\":[]}},\"warnings\":[]}}",
        json_escape(&state.repository_id),
        json_escape(&state.worktree_anchor.id()),
        json_escape(&state.worktree_anchor.resolved_view_id),
        real_worktree_anchor_summary_json(&state.worktree_anchor),
    )
}

fn real_worktree_capture_record_json(
    state: &RealRepoState,
    anchor_before: &RealWorktreeAnchor,
    anchor_after: &RealWorktreeAnchor,
    operation: &RealOperationRecord,
    candidates: &[CompatCandidateDelta],
    response: &MutationResponse,
) -> String {
    format!(
        "{{\"schema_version\":1,\"record_type\":\"operation_transaction\",\"id\":\"{}\",\"repository_id\":\"{}\",\"topic_id\":\"{}\",\"topic_revision_id\":\"{}\",\"session_id\":\"{}\",\"session_generation_id\":\"{}\",\"actor_id\":\"{}\",\"authored_context_id\":\"{}\",\"mutation\":\"worktree_capture\",\"preconditions\":{{\"worktree_anchor_id\":\"{}\",\"worktree_anchor_generation\":{},\"worktree_anchor_resolved_view_id\":\"{}\",\"worktree_anchor_tree_hash\":\"{}\",\"worktree_anchor_manifest_digest\":\"{}\",\"selected_candidate_delta_ids\":{}}},\"mutation_payload\":{{\"kind\":\"worktree_capture\",\"selected_deltas\":[{}]}},\"after_refs\":{{\"resolved_view_id\":\"{}\",\"tree_identity\":{},\"worktree_anchor\":{}}}}}",
        json_escape(&operation.operation_transaction_id),
        json_escape(&state.repository_id),
        json_escape(&operation.topic_id),
        json_escape(&operation.topic_revision_id),
        json_escape(&operation.session_id),
        json_escape(&response.view.session_generation_id),
        json_escape(&response.operation.actor_id),
        json_escape(&operation.authored_context_id),
        json_escape(&anchor_before.id()),
        anchor_before.generation,
        json_escape(&anchor_before.resolved_view_id),
        json_escape(&anchor_before.tree_hash),
        json_escape(&anchor_before.manifest_digest),
        string_array_json(candidates.iter().map(|candidate| candidate.candidate_delta_id.as_str())),
        candidates
            .iter()
            .map(real_compat_selected_delta_json)
            .collect::<Vec<_>>()
            .join(","),
        json_escape(&response.view.resolved_view_id),
        single_repo_tree_json(&SingleRepoTree {
            repository_id: response.view.tree_identity.repository_id.clone(),
            tree_hash: response.view.tree_identity.tree_hash.clone(),
        }),
        real_worktree_anchor_summary_json(anchor_after),
    )
}

fn real_worktree_capture_envelope(
    repo_root: &Path,
    state: &RealRepoState,
    anchor_before: &RealWorktreeAnchor,
    candidates: &[CompatCandidateDelta],
    response: &MutationResponse,
    remaining_candidate_count: usize,
) -> String {
    let topic = state
        .topics
        .iter()
        .find(|topic| topic.topic_id == response.operation.topic_id)
        .expect("capture topic");
    let operation = state.operations.last().expect("capture operation");
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"worktree.capture\",\"repository_id\":\"{}\",\"ids\":{{\"worktree_anchor_id\":\"{}\",\"topic_id\":\"{}\",\"session_id\":\"{}\",\"operation_transaction_id\":\"{}\",\"topic_revision_id\":\"{}\",\"session_generation_id\":\"{}\",\"resolved_view_id\":\"{}\"}},\"captured\":true,\"anchor_before\":{},\"anchor_after\":{},\"view\":{},\"selected_delta_count\":{},\"candidate_delta_ids\":{},\"remaining_candidate_count\":{},\"topic\":{},\"handoff\":{},\"operation\":{},\"topic_revision\":{},\"session_generation\":{}}},\"warnings\":[]}}",
        json_escape(&state.repository_id),
        json_escape(&state.worktree_anchor.id()),
        json_escape(&topic.topic_id),
        json_escape(&response.session_id),
        json_escape(&response.operation.id),
        json_escape(&response.topic_revision.id),
        json_escape(&response.view.session_generation_id),
        json_escape(&response.view.resolved_view_id),
        real_worktree_anchor_summary_json(anchor_before),
        real_worktree_anchor_summary_json(&state.worktree_anchor),
        view_json(&response.view),
        candidates.len(),
        string_array_json(candidates.iter().map(|candidate| candidate.candidate_delta_id.as_str())),
        remaining_candidate_count,
        real_topic_metadata_json(topic),
        real_topic_handoff_json(repo_root, state, topic),
        real_worktree_capture_record_json(
            state,
            anchor_before,
            &state.worktree_anchor,
            operation,
            candidates,
            response,
        ),
        topic_revision_json(response),
        session_generation_json(response),
    )
}

fn real_compat_import_record_json(
    state: &RealRepoState,
    projection: &RealProjectionSnapshot,
    operation: &RealOperationRecord,
    candidates: &[CompatCandidateDelta],
    response: &MutationResponse,
) -> String {
    format!(
        "{{\"schema_version\":1,\"record_type\":\"operation_transaction\",\"id\":\"{}\",\"repository_id\":\"{}\",\"topic_id\":\"{}\",\"topic_revision_id\":\"{}\",\"session_id\":\"{}\",\"session_generation_id\":\"{}\",\"actor_id\":\"{}\",\"authored_context_id\":\"{}\",\"mutation\":\"compat_import\",\"preconditions\":{{\"projection_id\":\"{}\",\"projection_baseline_resolved_view_id\":\"{}\",\"projection_baseline_tree_hash\":\"{}\",\"session_generation_id\":{},\"selected_candidate_delta_ids\":{}}},\"mutation_payload\":{{\"kind\":\"compat_import\",\"projection_id\":\"{}\",\"baseline_manifest_digest\":\"{}\",\"selected_deltas\":[{}]}},\"before_refs\":{{\"content_hash\":{}}},\"after_refs\":{{\"content_hash\":{},\"tree_identity\":{}}}}}",
        json_escape(&operation.operation_transaction_id),
        json_escape(&state.repository_id),
        json_escape(&operation.topic_id),
        json_escape(&operation.topic_revision_id),
        json_escape(&operation.session_id),
        json_escape(&response.operation.session_generation_id),
        json_escape(&response.operation.actor_id),
        json_escape(&operation.authored_context_id),
        json_escape(&projection.projection_id),
        json_escape(&projection.resolved_view_id),
        json_escape(&projection.tree_hash),
        optional_string_json(projection.session_generation_id.as_deref()),
        string_array_json(candidates.iter().map(|candidate| candidate.candidate_delta_id.as_str())),
        json_escape(&projection.projection_id),
        json_escape(&projection.manifest_digest),
        candidates.iter().map(real_compat_selected_delta_json).collect::<Vec<_>>().join(","),
        optional_string_json(operation.base_content_hash.as_deref()),
        if operation.tombstone {
            "null".to_string()
        } else {
            optional_string_json(Some(&operation.result_content_hash))
        },
        single_repo_tree_json(&SingleRepoTree {
            repository_id: response.view.tree_identity.repository_id.clone(),
            tree_hash: response.view.tree_identity.tree_hash.clone(),
        }),
    )
}

fn real_compat_import_envelope(
    state: &RealRepoState,
    projection: &RealProjectionSnapshot,
    candidates: &[CompatCandidateDelta],
    response: &MutationResponse,
) -> String {
    let operation = state.operations.last().expect("compat operation");
    let effects = operation.artifact_effects();
    let imported = candidates.iter().map(|candidate| {
        let effect = effects.iter().rev().find(|effect| {
            effect.path == candidate.path && !effect.tombstone
        }).or_else(|| effects.iter().rev().find(|effect| effect.path == candidate.path))
            .expect("compat candidate should have a corresponding operation effect");
        format!(
        "{{\"candidate_delta_id\":\"{}\",\"artifact_id\":\"{}\",\"path\":\"{}\",\"operation_kind\":\"{}\",\"before_hash\":{},\"after_hash\":{},\"classification\":\"{}\",\"privacy_class\":\"{}\"}}",
        json_escape(&candidate.candidate_delta_id), json_escape(&effect.artifact_id), json_escape(&effect.path),
        candidate.operation_kind.as_str(), optional_string_json(effect.base_content_hash.as_deref()),
        if effect.tombstone { "null".to_string() } else { optional_string_json(Some(&effect.result_content_hash)) },
        json_escape(&effect.classification), candidate.privacy_class.as_str())
    }).collect::<Vec<_>>().join(",");
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"compat.import\",\"repository_id\":\"{}\",\"ids\":{{\"projection_id\":\"{}\",\"session_id\":\"{}\",\"operation_transaction_id\":\"{}\",\"topic_revision_id\":\"{}\",\"session_generation_id\":\"{}\",\"resolved_view_id\":\"{}\"}},\"view\":{},\"projection_id\":\"{}\",\"operation_transaction_id\":\"{}\",\"topic_revision_id\":\"{}\",\"session_generation_id\":\"{}\",\"resolved_view_id\":\"{}\",\"tree_identity\":{},\"selected_delta_count\":{},\"candidate_delta_ids\":{},\"imported_artifacts\":[{}],\"ignored_candidate_delta_ids\":[],\"quarantine_refs\":[],\"operation\":{},\"topic_revision\":{},\"session_generation\":{}}},\"warnings\":[]}}",
        json_escape(&state.repository_id),
        json_escape(&projection.projection_id),
        json_escape(&response.session_id),
        json_escape(&response.operation.id),
        json_escape(&response.topic_revision.id),
        json_escape(&response.view.session_generation_id),
        json_escape(&response.view.resolved_view_id),
        view_json(&response.view),
        json_escape(&projection.projection_id),
        json_escape(&response.operation.id),
        json_escape(&response.topic_revision.id),
        json_escape(&response.view.session_generation_id),
        json_escape(&response.view.resolved_view_id),
        single_repo_tree_json(&SingleRepoTree {
            repository_id: response.view.tree_identity.repository_id.clone(),
            tree_hash: response.view.tree_identity.tree_hash.clone(),
        }),
        candidates.len(),
        string_array_json(candidates.iter().map(|candidate| candidate.candidate_delta_id.as_str())),
        imported,
        real_compat_import_record_json(
            state,
            projection,
            state.operations.last().expect("compat operation"),
            candidates,
            response,
        ),
        topic_revision_json(response),
        session_generation_json(response),
    )
}

fn real_compat_selected_delta_json(candidate: &CompatCandidateDelta) -> String {
    let operations = if candidate.operation_kind == CompatFileOperationKind::Move {
        format!("[{{\"operation_kind\":\"move\",\"source_path\":{},\"target_path\":\"{}\",\"base_content_hash\":{},\"result_content_hash\":{}}}]",
            optional_string_json(candidate.source_path.as_deref()), json_escape(&candidate.path),
            optional_string_json(candidate.before_hash.as_deref()), optional_string_json(candidate.after_hash.as_deref()))
    } else {
        "[]".to_string()
    };
    format!("{{\"candidate_delta_id\":\"{}\",\"operation_kind\":\"{}\",\"path\":\"{}\",\"source_path\":{},\"base_content_hash\":{},\"result_content_hash\":{},\"operations\":{},\"classification\":\"{}\",\"privacy_class\":\"{}\"}}",
        json_escape(&candidate.candidate_delta_id), candidate.operation_kind.as_str(), json_escape(&candidate.path),
        optional_string_json(candidate.source_path.as_deref()),
        optional_string_json(candidate.before_hash.as_deref()), optional_string_json(candidate.after_hash.as_deref()),
        operations, json_escape(&candidate.classification), candidate.privacy_class.as_str())
}

fn real_compat_provenance_json(operation: &RealOperationRecord) -> String {
    format!(
        "{{\"operation_transaction_id\":\"{}\",\"projection_id\":{},\"candidate_delta_ids\":{},\"authored_context_id\":\"{}\"}}",
        json_escape(&operation.operation_transaction_id),
        optional_string_json(operation.compat_projection_id.as_deref()),
        string_array_json(
            operation
                .compat_candidate_delta_ids
                .iter()
                .map(String::as_str)
        ),
        json_escape(&operation.authored_context_id),
    )
}

fn real_worktree_provenance_json(operation: &RealOperationRecord) -> String {
    let anchor_id = operation
        .worktree_anchor_generation
        .map(|generation| format!("worktree_anchor_{generation:04}"));
    format!(
        "{{\"operation_transaction_id\":\"{}\",\"candidate_delta_ids\":{},\"anchor_id\":{},\"anchor_generation\":{},\"anchor_resolved_view_id\":{},\"anchor_tree_hash\":{},\"anchor_manifest_digest\":{},\"authored_context_id\":\"{}\"}}",
        json_escape(&operation.operation_transaction_id),
        string_array_json(
            operation
                .worktree_candidate_delta_ids
                .iter()
                .map(String::as_str)
        ),
        optional_string_json(anchor_id.as_deref()),
        operation
            .worktree_anchor_generation
            .map(|generation| generation.to_string())
            .unwrap_or_else(|| "null".to_string()),
        optional_string_json(operation.worktree_anchor_resolved_view_id.as_deref()),
        optional_string_json(operation.worktree_anchor_tree_hash.as_deref()),
        optional_string_json(operation.worktree_anchor_manifest_digest.as_deref()),
        json_escape(&operation.authored_context_id),
    )
}

fn real_compat_operation_inspect_envelope(
    state: &RealRepoState,
    operation: &RealOperationRecord,
    command: &str,
) -> String {
    let projection = operation
        .compat_projection_id
        .as_deref()
        .and_then(|projection_id| {
            state
                .projections
                .iter()
                .find(|projection| projection.projection_id == projection_id)
        });
    let selected_candidate_ids = if operation.compat_projection_id.is_some() {
        &operation.compat_candidate_delta_ids
    } else {
        &operation.worktree_candidate_delta_ids
    };
    let worktree_provenance = if operation.worktree_candidate_delta_ids.is_empty() {
        "null".to_string()
    } else {
        real_worktree_provenance_json(operation)
    };
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"{}\",\"repository_id\":\"{}\",\"ids\":{{\"operation_transaction_id\":\"{}\",\"topic_revision_id\":\"{}\",\"projection_id\":{}}},\"operation\":{{\"operation_transaction_id\":\"{}\",\"topic_id\":\"{}\",\"topic_revision_id\":\"{}\",\"session_id\":\"{}\",\"artifact_id\":\"{}\",\"path\":\"{}\",\"mutation\":\"{}\",\"authored_context_id\":\"{}\",\"base_content_hash\":{},\"result_content_hash\":\"{}\",\"tombstone\":{},\"artifact_effects\":{},\"mutation_payload\":{{\"kind\":\"{}\",\"projection_id\":{},\"selected_candidate_delta_ids\":{}}},\"worktree_capture_provenance\":{}}},\"projection\":{}}},\"warnings\":[]}}",
        json_escape(command),
        json_escape(&state.repository_id),
        json_escape(&operation.operation_transaction_id),
        json_escape(&operation.topic_revision_id),
        optional_string_json(operation.compat_projection_id.as_deref()),
        json_escape(&operation.operation_transaction_id),
        json_escape(&operation.topic_id),
        json_escape(&operation.topic_revision_id),
        json_escape(&operation.session_id),
        json_escape(&operation.artifact_id),
        json_escape(&operation.path),
        json_escape(&operation.mutation),
        json_escape(&operation.authored_context_id),
        optional_string_json(operation.base_content_hash.as_deref()),
        json_escape(&operation.result_content_hash),
        operation.tombstone,
        real_operation_effects_json(operation),
        if operation.compat_projection_id.is_some() {
            "compat_import"
        } else {
            operation.mutation.as_str()
        },
        optional_string_json(operation.compat_projection_id.as_deref()),
        string_array_json(selected_candidate_ids.iter().map(String::as_str)),
        worktree_provenance,
        projection
            .map(|projection| real_projection_snapshot_json(state, projection))
            .unwrap_or_else(|| "null".to_string()),
    )
}

fn real_operation_effects_json(operation: &RealOperationRecord) -> String {
    let effects = operation.artifact_effects().into_iter().map(|effect| format!(
        "{{\"artifact_id\":\"{}\",\"path\":\"{}\",\"base_content_hash\":{},\"result_content_hash\":{},\"classification\":\"{}\",\"executable\":{},\"tombstone\":{}}}",
        json_escape(&effect.artifact_id), json_escape(&effect.path), optional_string_json(effect.base_content_hash.as_deref()),
        if effect.tombstone { "null".to_string() } else { optional_string_json(Some(&effect.result_content_hash)) },
        json_escape(&effect.classification), effect.executable, effect.tombstone)).collect::<Vec<_>>().join(",");
    format!("[{effects}]")
}

fn real_projection_status_envelope(
    state: &RealRepoState,
    projection: &RealProjectionSnapshot,
) -> String {
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"status.projection\",\"repository_id\":\"{}\",\"ids\":{{\"projection_id\":\"{}\",\"resolved_view_id\":\"{}\"}},\"lifecycle_state\":\"materialized\",\"projection\":{}}},\"warnings\":[]}}",
        json_escape(&state.repository_id),
        json_escape(&projection.projection_id),
        json_escape(&projection.resolved_view_id),
        real_projection_snapshot_json(state, projection),
    )
}

fn real_compat_projection_status_envelope(
    state: &RealRepoState,
    projection: &RealProjectionSnapshot,
    candidates: &[CompatCandidateDelta],
) -> String {
    let quarantine_count = candidates
        .iter()
        .filter(|candidate| candidate.quarantine_ref.is_some())
        .count();
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"status.projection\",\"repository_id\":\"{}\",\"ids\":{{\"projection_id\":\"{}\",\"resolved_view_id\":\"{}\"}},\"lifecycle_state\":\"{}\",\"projection\":{},\"dirty_candidate_summary\":{{\"total\":{},\"counts\":{}}},\"quarantine_count\":{},\"last_import_operation_id\":{}}},\"warnings\":{}}}",
        json_escape(&state.repository_id),
        json_escape(&projection.projection_id),
        json_escape(&projection.resolved_view_id),
        if candidates.is_empty() { "clean" } else { "dirty" },
        real_projection_snapshot_json(state, projection),
        candidates.len(),
        compat_candidate_counts_json(candidates),
        quarantine_count,
        optional_string_json(projection.last_import_operation_id.as_deref()),
        if candidates.is_empty() {
            "[]".to_string()
        } else {
            format!("[{{\"code\":\"dirty_compatibility_projection\",\"message\":\"review the compatibility diff and explicitly import or discard candidates\",\"details\":{{\"projection_id\":\"{}\",\"candidate_count\":{},\"quarantined_count\":{}}}}}]", json_escape(&projection.projection_id), candidates.len(), quarantine_count)
        },
    )
}

fn real_projection_inspect_envelope(
    state: &RealRepoState,
    projection: &RealProjectionSnapshot,
    candidates: Option<&[CompatCandidateDelta]>,
) -> String {
    let candidate_count = candidates.map(<[CompatCandidateDelta]>::len).unwrap_or(0);
    let lifecycle = if candidate_count == 0 {
        "materialized"
    } else {
        "dirty"
    };
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"inspect.projection\",\"repository_id\":\"{}\",\"ids\":{{\"projection_id\":\"{}\",\"resolved_view_id\":\"{}\"}},\"lifecycle_state\":\"{}\",\"projection\":{},\"dirty_candidate_summary\":{{\"total\":{}}},\"manifest\":{{\"manifest_digest\":\"{}\",\"source_truth\":\"sunlight_persisted_resolved_view\",\"entries\":{}}}}},\"warnings\":{}}}",
        json_escape(&state.repository_id),
        json_escape(&projection.projection_id),
        json_escape(&projection.resolved_view_id),
        lifecycle,
        real_projection_snapshot_json(state, projection),
        candidate_count,
        json_escape(&projection.manifest_digest),
        real_entries_manifest_json(&projection.entries),
        if candidate_count == 0 {
            "[]".to_string()
        } else {
            format!("[{{\"code\":\"dirty_compatibility_projection\",\"message\":\"review the compatibility diff and explicitly import or discard candidates\",\"details\":{{\"projection_id\":\"{}\",\"candidate_count\":{}}}}}]", json_escape(&projection.projection_id), candidate_count)
        },
    )
}

fn real_execution_record(
    state: &RealRepoState,
    execution: &RealExecutionSnapshot,
) -> ExecutionRecord {
    ExecutionRecord {
        id: execution.execution_id.clone(),
        repository_id: state.repository_id.clone(),
        resolved_view_id: execution.resolved_view_id.clone(),
        tree_identity: SingleRepoTree {
            repository_id: state.repository_id.clone(),
            tree_hash: execution.tree_hash.clone(),
        },
        command: sunlight_core::execution::ExecutionCommand {
            argv: execution.command_argv.clone(),
            shell: None,
        },
        working_directory: execution.working_directory.clone(),
        environment_summary: sunlight_core::execution::EnvironmentSummary {
            id: format!(
                "env_{}",
                execution
                    .environment_summary
                    .digest
                    .trim_start_matches("sha256:")
            ),
            os: execution.environment_summary.os.clone(),
            platform_hint: execution.environment_summary.platform_hint.clone(),
            arch: execution.environment_summary.arch.clone(),
            sunlight_build_id: execution.environment_summary.sunlight_build_id.clone(),
            command_runner_version: execution.environment_summary.command_runner_version.clone(),
            tool_hints: execution
                .environment_summary
                .tool_hints
                .iter()
                .map(|hint| sunlight_core::execution::ToolHint {
                    name: hint.name.clone(),
                    version_or_digest: hint.version_or_digest.clone(),
                })
                .collect(),
            env_policy: execution.environment_policy.clone(),
            redacted_env_allowlist_digest: execution
                .environment_summary
                .redacted_env_allowlist_digest
                .clone(),
            network_policy: if execution.network_policy
                == "windows_appcontainer_no_network_capabilities_v1"
            {
                sunlight_core::execution::NetworkPolicy::Disabled
            } else {
                sunlight_core::execution::NetworkPolicy::NotEnforced
            },
            sandbox_writable_policy: if execution.filesystem_write_policy
                == "windows_low_integrity_private_projection_v1"
            {
                sunlight_core::execution::WritablePolicy::PrivateProjectionWritableIsolated
            } else {
                sunlight_core::execution::WritablePolicy::ManagedProjectionWritableNotIsolated
            },
            digest: execution.environment_summary.digest.clone(),
        },
        projection_id: execution.projection_id.clone(),
        inputs: sunlight_core::execution::ExecutionInputs {
            resolved_view_id: execution.resolved_view_id.clone(),
            tree_hash: execution.tree_hash.clone(),
            path_policy_id: POSIX_CASE_SENSITIVE_PATH_POLICY_ID.to_string(),
            operation_semantics_version: FILE_OPERATION_SEMANTICS_VERSION.to_string(),
        },
        outputs: real_execution_output_summaries(execution),
        promotions: Vec::new(),
        result: sunlight_core::execution::ExecutionResult {
            status: match execution.status.as_str() {
                "running" => sunlight_core::execution::ExecutionStatus::Running,
                "pass" => sunlight_core::execution::ExecutionStatus::Pass,
                "canceled" => sunlight_core::execution::ExecutionStatus::Canceled,
                "timeout" => sunlight_core::execution::ExecutionStatus::Timeout,
                "interrupted" => sunlight_core::execution::ExecutionStatus::Interrupted,
                "policy_blocked" => sunlight_core::execution::ExecutionStatus::PolicyBlocked,
                _ => sunlight_core::execution::ExecutionStatus::Fail,
            },
            exit_code: execution.exit_code,
            timed_out: execution.timed_out,
            termination_reason: execution.termination_reason.clone(),
        },
        started_at: execution.started_at.clone(),
        finished_at: execution.finished_at.clone(),
        privacy_class: if execution.privacy_class == "local_only" {
            sunlight_core::records::PrivacyClass::LocalOnly
        } else {
            sunlight_core::records::PrivacyClass::PolicyGated
        },
    }
}

fn real_execution_output_summaries(
    execution: &RealExecutionSnapshot,
) -> Vec<sunlight_core::execution::OutputSummary> {
    let mut outputs = vec![
        sunlight_core::execution::OutputSummary {
            kind: OutputKind::StdoutSummary,
            classification: OutputClassification::Log,
            path: None,
            digest: execution.stdout_observed_digest.clone(),
            byte_length: execution.stdout_byte_length,
            privacy_class: sunlight_core::records::PrivacyClass::LocalOnly,
        },
        sunlight_core::execution::OutputSummary {
            kind: OutputKind::StderrSummary,
            classification: OutputClassification::Log,
            path: None,
            digest: execution.stderr_observed_digest.clone(),
            byte_length: execution.stderr_byte_length,
            privacy_class: sunlight_core::records::PrivacyClass::LocalOnly,
        },
    ];
    outputs.extend(execution.outputs.iter().map(|output| {
        sunlight_core::execution::OutputSummary {
            kind: OutputKind::FileDelta,
            classification: real_output_classification(&output.classification),
            path: Some(output.path.clone()),
            digest: output.after_hash.clone(),
            byte_length: output.byte_length,
            privacy_class: sunlight_core::records::PrivacyClass::PolicyGated,
        }
    }));
    outputs
}

fn real_execution_promotion_record(
    promotion: &RealExecutionPromotionSnapshot,
) -> ExecutionOutputPromotionRecord {
    let candidate = PromotionCandidateProvenance {
        execution_id: promotion.execution_id.clone(),
        projection_id: promotion.projection_id.clone(),
        output_path: promotion.output_path.clone(),
        target_topic_id: promotion.target_topic_id.clone(),
        classification: real_output_classification(&promotion.classification),
        before_hash: promotion.before_hash.clone(),
        after_hash: promotion.after_hash.clone(),
    };
    execution_output_promotion_record_from_ids(
        &candidate,
        &promotion.operation_transaction_id,
        &promotion.topic_revision_id,
        &promotion.session_generation_id,
    )
}

fn real_execution_snapshot_record_json(
    state: &RealRepoState,
    execution: &RealExecutionSnapshot,
) -> String {
    let record = real_execution_record(state, execution);
    let output_paths = execution
        .outputs
        .iter()
        .map(|output| {
            format!(
                "{{\"path\":\"{}\",\"classification\":\"{}\",\"before_hash\":{},\"after_hash\":\"{}\",\"byte_length\":{}}}",
                json_escape(&output.path),
                json_escape(&output.classification),
                optional_string_json(output.before_hash.as_deref()),
                json_escape(&output.after_hash),
                output.byte_length,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{{},\"command_started\":{},\"runtime_policy\":{},\"output_capture\":{},\"output_files\":[{}],\"raw_output_policy\":\"not_persisted\",\"source_truth\":\"sunlight_persisted_execution\"}}",
        execution_record_json(&record).trim_start_matches('{').trim_end_matches('}'),
        execution.command_started,
        real_execution_runtime_policy_json(execution),
        real_execution_output_capture_json(execution),
        output_paths,
    )
}

fn real_execution_runtime_policy_json(execution: &RealExecutionSnapshot) -> String {
    format!(
        concat!(
            "{{\"timeout_ms\":{},",
            "\"limits\":{{\"process_memory_bytes\":{},\"job_memory_bytes\":{},\"cpu_time_ms\":{},\"active_processes\":{}}},",
            "\"enforcement\":{{\"process_tree\":\"{}\",\"cpu\":\"{}\",\"memory\":\"{}\"}},",
            "\"environment\":{{\"inheritance\":\"{}\",\"allowlist\":{},\"values_recorded\":false}},",
            "\"network\":{{\"requested\":\"{}\",\"effective\":\"{}\"}},",
            "\"filesystem_writes_requested\":\"{}\",",
            "\"filesystem_writes\":\"{}\"}}"
        ),
        execution
            .timeout_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string()),
        execution
            .process_memory_limit_bytes
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string()),
        execution
            .job_memory_limit_bytes
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string()),
        execution
            .cpu_time_limit_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string()),
        execution
            .active_process_limit
            .map(|value| value.to_string())
            .unwrap_or_else(|| "null".to_string()),
        json_escape(&execution.process_tree_policy),
        json_escape(&execution.cpu_policy),
        json_escape(&execution.memory_policy),
        json_escape(&execution.environment_policy),
        string_array_json(execution.environment_allowlist.iter().map(String::as_str)),
        json_escape(&execution.network_policy_requested),
        json_escape(&execution.network_policy),
        json_escape(&execution.filesystem_write_policy_requested),
        json_escape(&execution.filesystem_write_policy),
    )
}

fn real_execution_output_capture_json(execution: &RealExecutionSnapshot) -> String {
    format!(
        concat!(
            "{{\"stdout\":{{\"observed_digest\":\"{}\",\"observed_byte_length\":{},\"captured_byte_length\":{},\"truncated\":{},\"capture_failed\":{}}},",
            "\"stderr\":{{\"observed_digest\":\"{}\",\"observed_byte_length\":{},\"captured_byte_length\":{},\"truncated\":{},\"capture_failed\":{}}},",
            "\"process_control\":{{\"termination_reason\":{},\"termination_failed\":{},\"wait_failed\":{}}}}}"
        ),
        json_escape(&execution.stdout_observed_digest),
        execution.stdout_byte_length,
        execution.stdout_captured_byte_length,
        execution.stdout_truncated,
        execution.stdout_capture_failed,
        json_escape(&execution.stderr_observed_digest),
        execution.stderr_byte_length,
        execution.stderr_captured_byte_length,
        execution.stderr_truncated,
        execution.stderr_capture_failed,
        optional_string_json(execution.termination_reason.as_deref()),
        execution.termination_failed,
        execution.wait_failed,
    )
}

fn execution_output_requires_promotion(output: &RealExecutionOutputSnapshot) -> bool {
    matches!(
        output.classification.as_str(),
        "source_like_delta" | "generated_artifact"
    )
}

fn real_execution_promotion_status(
    state: &RealRepoState,
    execution: &RealExecutionSnapshot,
) -> &'static str {
    let promotable = execution
        .outputs
        .iter()
        .filter(|output| execution_output_requires_promotion(output))
        .collect::<Vec<_>>();
    if promotable.is_empty() || execution.status != "pass" {
        "none"
    } else if promotable.iter().all(|output| {
        state.promotions.iter().any(|promotion| {
            promotion.execution_id == execution.execution_id && promotion.output_path == output.path
        })
    }) {
        "promoted"
    } else {
        "promotion_required"
    }
}

fn real_execution_promotion_candidates_json(
    state: &RealRepoState,
    execution: &RealExecutionSnapshot,
) -> String {
    if execution.status != "pass" {
        return String::new();
    }
    execution
        .outputs
        .iter()
        .filter(|output| {
            execution_output_requires_promotion(output)
                && !state.promotions.iter().any(|promotion| {
                    promotion.execution_id == execution.execution_id
                        && promotion.output_path == output.path
                })
        })
        .map(|output| {
            format!(
                concat!(
                    "{{\"execution_id\":\"{}\",\"projection_id\":\"{}\",",
                    "\"output_path\":\"{}\",\"target_topic_id\":null,",
                    "\"classification\":\"{}\",\"before_hash\":{},\"after_hash\":\"{}\"}}"
                ),
                json_escape(&execution.execution_id),
                json_escape(&execution.projection_id),
                json_escape(&output.path),
                output.classification,
                optional_string_json(output.before_hash.as_deref()),
                json_escape(&output.after_hash),
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn real_execution_promotions_json(
    state: &RealRepoState,
    execution: &RealExecutionSnapshot,
) -> String {
    state
        .promotions
        .iter()
        .filter(|promotion| promotion.execution_id == execution.execution_id)
        .map(|promotion| promotion_record_json(&real_execution_promotion_record(promotion)))
        .collect::<Vec<_>>()
        .join(",")
}

fn real_execution_projection_json(
    state: &RealRepoState,
    execution: &RealExecutionSnapshot,
) -> String {
    state
        .projections
        .iter()
        .find(|projection| projection.projection_id == execution.projection_id)
        .map(|projection| real_projection_snapshot_json(state, projection))
        .unwrap_or_else(|| "null".to_string())
}

fn real_execution_status_envelope(
    state: &RealRepoState,
    execution: &RealExecutionSnapshot,
) -> String {
    let record = real_execution_record(state, execution);
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"status.execution\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"execution_id\":\"{}\",\"projection_id\":\"{}\",\"resolved_view_id\":\"{}\"}},",
            "\"execution_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"projection\":{},",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{},",
            "\"result\":{},",
            "\"environment_summary\":{},",
            "\"runtime_policy\":{},",
            "\"output_capture\":{},",
            "\"output_summary_counts\":{},",
            "\"promotion_status\":\"{}\",",
            "\"promotion_candidates\":[{}],",
            "\"promotions\":[{}],",
            "\"privacy_semantics\":{{\"execution_record\":\"policy_gated\",\"raw_outputs\":\"not_persisted\",\"promotion_record\":\"policy_gated\",\"durability\":\"persisted_repo_state\"}}",
            "}},\"warnings\":{}}}"
        ),
        json_escape(&state.repository_id),
        json_escape(&execution.execution_id),
        json_escape(&execution.projection_id),
        json_escape(&execution.resolved_view_id),
        json_escape(&execution.execution_id),
        json_escape(&execution.projection_id),
        real_execution_projection_json(state, execution),
        json_escape(&execution.resolved_view_id),
        single_repo_tree_json(&record.tree_identity),
        execution_result_json(&record),
        execution_environment_summary_json(&record.environment_summary),
        real_execution_runtime_policy_json(execution),
        real_execution_output_capture_json(execution),
        output_summary_counts_json(&record),
        real_execution_promotion_status(state, execution),
        real_execution_promotion_candidates_json(state, execution),
        real_execution_promotions_json(state, execution),
        real_execution_warnings_json(state, execution),
    )
}

fn real_execution_run_success_envelope(
    state: &RealRepoState,
    execution: &RealExecutionSnapshot,
    stdout_text: &str,
    stderr_text: &str,
    materialization_ms: u128,
    command_ms: u128,
    output_scan_ms: u128,
    publication_ms: u128,
    total_ms: u128,
) -> String {
    let record = real_execution_record(state, execution);
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"execution.run\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"execution_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\"",
            "}},",
            "\"view\":{{\"resolved_view_id\":\"{}\",\"tree_identity\":{}}},",
            "\"execution_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"projection\":{},",
            "\"tree_identity\":{},",
            "\"result\":{},",
            "\"environment_summary\":{},",
            "\"runtime_policy\":{},",
            "\"output_capture\":{},",
            "\"output_text\":{{\"stdout\":\"{}\",\"stderr\":\"{}\",\"durability\":\"response_only\"}},",
            "\"phase_timings_ms\":{{\"materialization\":{},\"command\":{},",
            "\"output_scan_and_classification\":{},\"publication\":{},\"total\":{}}},",
            "\"output_summary_counts\":{},",
            "\"promotion_candidates\":[{}]",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&state.repository_id),
        json_escape(&execution.execution_id),
        json_escape(&execution.projection_id),
        json_escape(&execution.resolved_view_id),
        json_escape(&execution.resolved_view_id),
        single_repo_tree_json(&record.tree_identity),
        json_escape(&execution.execution_id),
        json_escape(&execution.projection_id),
        real_execution_projection_json(state, execution),
        single_repo_tree_json(&record.tree_identity),
        execution_result_json(&record),
        execution_environment_summary_json(&record.environment_summary),
        real_execution_runtime_policy_json(execution),
        real_execution_output_capture_json(execution),
        json_escape(stdout_text),
        json_escape(stderr_text),
        materialization_ms,
        command_ms,
        output_scan_ms,
        publication_ms,
        total_ms,
        output_summary_counts_json(&record),
        real_execution_promotion_candidates_json(state, execution),
    )
}

fn real_execution_inspect_envelope(
    state: &RealRepoState,
    execution: &RealExecutionSnapshot,
) -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.execution\",",
            "\"repository_id\":\"{}\",",
            "\"execution\":{},",
            "\"projection\":{},",
            "\"promotion_status\":\"{}\",",
            "\"promotion_candidates\":[{}],",
            "\"promotions\":[{}],",
            "\"privacy_semantics\":{{\"execution_record\":\"policy_gated\",\"raw_outputs\":\"not_persisted\",\"promotion_record\":\"policy_gated\",\"durability\":\"persisted_repo_state\"}}",
            "}},\"warnings\":{}}}"
        ),
        json_escape(&state.repository_id),
        real_execution_snapshot_record_json(state, execution),
        real_execution_projection_json(state, execution),
        real_execution_promotion_status(state, execution),
        real_execution_promotion_candidates_json(state, execution),
        real_execution_promotions_json(state, execution),
        real_execution_warnings_json(state, execution),
    )
}

fn real_execution_warnings_json(
    state: &RealRepoState,
    execution: &RealExecutionSnapshot,
) -> String {
    let mut warnings = Vec::new();
    if execution.status == "timeout" {
        warnings.push("{\"code\":\"execution_timeout\",\"message\":\"inspect the timeout and rerun with an appropriate bounded policy\",\"details\":{}}".to_string());
    } else if execution.termination_reason.as_deref()
        == Some("execution_network_isolation_cleanup_failed")
    {
        warnings.push("{\"code\":\"execution_network_isolation_cleanup_failed\",\"message\":\"execution ran, but Windows isolation cleanup requires attention and retry\",\"details\":{\"command_started\":true}}".to_string());
    } else if execution.status == "policy_blocked" {
        warnings.push(format!(
            "{{\"code\":\"execution_resource_policy_blocked\",\"message\":\"the execution was terminated by a configured resource limit\",\"details\":{{\"termination_reason\":{}}}}}",
            optional_string_json(execution.termination_reason.as_deref())
        ));
    } else if execution.status != "pass" {
        warnings.push("{\"code\":\"execution_failed\",\"message\":\"inspect the failed execution before checkpointing\",\"details\":{}}".to_string());
    }
    if real_execution_promotion_status(state, execution) == "promotion_required" {
        warnings.push(format!("{{\"code\":\"pending_promotion\",\"message\":\"promote or discard selected execution outputs\",\"details\":{{\"execution_id\":\"{}\"}}}}", json_escape(&execution.execution_id)));
    }
    format!("[{}]", warnings.join(","))
}

fn real_checkpoint_snapshot_json(
    state: &RealRepoState,
    checkpoint: &RealCheckpointSnapshot,
) -> String {
    format!(
        concat!(
            "{{",
            "\"schema_version\":1,",
            "\"record_type\":\"checkpoint\",",
            "\"id\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"checkpoint_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{},",
            "\"topic_frontier\":{},",
            "\"evidence_refs\":[{}],",
            "\"entry_count\":{},",
            "\"created_at\":\"{}\",",
            "\"source_truth\":\"sunlight_persisted_checkpoint\",",
            "\"privacy_class\":\"commit_default\"",
            "}}"
        ),
        json_escape(&checkpoint.checkpoint_id),
        json_escape(&state.repository_id),
        json_escape(&checkpoint.checkpoint_id),
        json_escape(&checkpoint.resolved_view_id),
        single_repo_tree_json(&SingleRepoTree {
            repository_id: state.repository_id.clone(),
            tree_hash: checkpoint.tree_hash.clone(),
        }),
        real_topic_frontier_pairs_json(&checkpoint.topic_frontier),
        checkpoint
            .evidence_refs
            .iter()
            .map(evidence_ref_json)
            .collect::<Vec<_>>()
            .join(","),
        checkpoint
            .entries
            .iter()
            .filter(|entry| !entry.tombstone)
            .count(),
        json_escape(&checkpoint.created_at),
    )
}

fn real_checkpoint_status_envelope(
    state: &RealRepoState,
    checkpoint: &RealCheckpointSnapshot,
) -> String {
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"status.checkpoint\",\"repository_id\":\"{}\",\"ids\":{{\"checkpoint_id\":\"{}\",\"resolved_view_id\":\"{}\"}},\"checkpoint_id\":\"{}\",\"export_ready\":true,\"checkpoint\":{}}},\"warnings\":[]}}",
        json_escape(&state.repository_id),
        json_escape(&checkpoint.checkpoint_id),
        json_escape(&checkpoint.resolved_view_id),
        json_escape(&checkpoint.checkpoint_id),
        real_checkpoint_snapshot_json(state, checkpoint),
    )
}

fn real_checkpoint_inspect_envelope(
    state: &RealRepoState,
    checkpoint: &RealCheckpointSnapshot,
) -> String {
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"inspect.checkpoint\",\"repository_id\":\"{}\",\"ids\":{{\"checkpoint_id\":\"{}\",\"resolved_view_id\":\"{}\"}},\"checkpoint\":{},\"content_snapshot\":{{\"source_truth\":\"sunlight_persisted_checkpoint\",\"entries\":{}}}}},\"warnings\":[]}}",
        json_escape(&state.repository_id),
        json_escape(&checkpoint.checkpoint_id),
        json_escape(&checkpoint.resolved_view_id),
        real_checkpoint_snapshot_json(state, checkpoint),
        real_entries_manifest_json(&checkpoint.entries),
    )
}

fn real_export_map_snapshot_json(
    state: &RealRepoState,
    export_map: &RealExportMapSnapshot,
) -> String {
    format!(
        concat!(
            "{{",
            "\"schema_version\":1,",
            "\"record_type\":\"git_export_map\",",
            "\"id\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"export_map_id\":\"{}\",",
            "\"checkpoint_id\":\"{}\",",
            "\"tree_identity\":{},",
            "\"git_ref\":\"{}\",",
            "\"git_commit_ids\":{},",
            "\"exported_at\":\"{}\",",
            "\"source_truth\":\"sunlight_persisted_checkpoint\"",
            "}}"
        ),
        json_escape(&export_map.export_map_id),
        json_escape(&state.repository_id),
        json_escape(&export_map.export_map_id),
        json_escape(&export_map.checkpoint_id),
        single_repo_tree_json(&SingleRepoTree {
            repository_id: state.repository_id.clone(),
            tree_hash: export_map.tree_hash.clone(),
        }),
        json_escape(&export_map.git_ref),
        string_array_json(export_map.git_commit_ids.iter().map(String::as_str)),
        json_escape(&export_map.exported_at),
    )
}

fn real_export_map_status_envelope(
    state: &RealRepoState,
    export_map: &RealExportMapSnapshot,
) -> String {
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"status.export_map\",\"repository_id\":\"{}\",\"ids\":{{\"export_map_id\":\"{}\",\"checkpoint_id\":\"{}\"}},\"git_export\":{{\"lifecycle_state\":\"exported\",\"source_truth\":\"sunlight_persisted_checkpoint\"}},\"export_map\":{}}},\"warnings\":[]}}",
        json_escape(&state.repository_id),
        json_escape(&export_map.export_map_id),
        json_escape(&export_map.checkpoint_id),
        real_export_map_snapshot_json(state, export_map),
    )
}

fn real_export_map_inspect_envelope(
    state: &RealRepoState,
    export_map: &RealExportMapSnapshot,
) -> String {
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"inspect.export_map\",\"repository_id\":\"{}\",\"ids\":{{\"export_map_id\":\"{}\",\"checkpoint_id\":\"{}\"}},\"export_map\":{}}},\"warnings\":[]}}",
        json_escape(&state.repository_id),
        json_escape(&export_map.export_map_id),
        json_escape(&export_map.checkpoint_id),
        real_export_map_snapshot_json(state, export_map),
    )
}

fn real_entries_manifest_json(entries: &[RealArtifactEntry]) -> String {
    format!(
        "[{}]",
        entries
            .iter()
            .map(|entry| {
                format!(
                    "{{\"path\":\"{}\",\"artifact_id\":\"{}\",\"content_hash\":\"{}\",\"classification\":\"{}\",\"tombstone\":{}}}",
                    json_escape(&entry.path),
                    json_escape(&entry.artifact_id),
                    json_escape(&entry.content_hash),
                    json_escape(&entry.classification),
                    entry.tombstone,
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn real_topic_frontier_pairs_json(frontier: &[(String, String)]) -> String {
    let fields = frontier
        .iter()
        .map(|(topic_id, revision_id)| {
            format!(
                "\"{}\":\"{}\"",
                json_escape(topic_id),
                json_escape(revision_id)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{fields}}}")
}

#[derive(Debug, Default)]
struct RealPolicyStatus {
    report_count: usize,
    passed_count: usize,
    failed_count: usize,
    invalid_count: usize,
    invalid_report_ids: Vec<String>,
    missing_export_report_count: usize,
}

#[derive(Debug, Default)]
struct RealOperationalSummary {
    recovery_rollback_evidence: bool,
    git_working_tree_changed: bool,
    worktree_state: String,
    worktree_candidate_count: usize,
    worktree_candidate_counts: BTreeMap<String, usize>,
    worktree_diff_id: Option<String>,
    worktree_scan_error: Option<String>,
    artifact_count: usize,
    quarantined_count: usize,
    conflict_count: usize,
    staleness_count: usize,
    projection_counts: BTreeMap<String, usize>,
    dirty_compat_projection_ids: Vec<String>,
    invalid_projection_ids: Vec<String>,
    execution_passed: usize,
    execution_failed: usize,
    execution_timeouts: usize,
    pending_promotions: usize,
    unexported_checkpoint_ids: Vec<String>,
    policy: RealPolicyStatus,
}

fn real_operational_summary(repo_root: &Path, state: &RealRepoState) -> RealOperationalSummary {
    let resolved = state.resolve_head_view();
    let mut summary = RealOperationalSummary {
        recovery_rollback_evidence: fs::read_dir(
            repo_root.join(".sunlight/local/recovery/native-state"),
        )
        .ok()
        .is_some_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().starts_with("evidence-"))
        }),
        git_working_tree_changed: git_working_tree_changed_outside_sunlight(repo_root),
        artifact_count: state
            .entries
            .iter()
            .filter(|entry| !entry.tombstone)
            .count(),
        quarantined_count: state.quarantine.len(),
        conflict_count: resolved.result.conflicts().count(),
        staleness_count: resolved.result.staleness().count(),
        worktree_state: "scan_required".to_string(),
        ..RealOperationalSummary::default()
    };
    match state.resolve_worktree_anchor_view().and_then(|anchored| {
        diff_real_worktree(
            repo_root,
            &state.repository_id,
            &state.worktree_anchor,
            &anchored.entries,
        )
        .map_err(|error| RepoStateError::InvalidState {
            path: repo_root.to_path_buf(),
            message: error.message,
        })
    }) {
        Ok(diff) => {
            summary.worktree_state = if diff.candidates.is_empty() {
                "clean".to_string()
            } else {
                "dirty".to_string()
            };
            summary.worktree_candidate_count = diff.candidates.len();
            for candidate in &diff.candidates {
                *summary
                    .worktree_candidate_counts
                    .entry(candidate.kind.as_str().to_string())
                    .or_default() += 1;
            }
            summary.worktree_diff_id = Some(real_worktree_diff_id(
                &state.worktree_anchor,
                &diff.candidates,
            ));
        }
        Err(error) => {
            summary.worktree_state = "unavailable".to_string();
            summary.worktree_scan_error = Some(error.to_string());
        }
    }
    let projection_policy = require_repository_config(repo_root.to_path_buf())
        .ok()
        .and_then(|config| resolve_projection_policy(repo_root, &config).ok());
    for projection in &state.projections {
        *summary
            .projection_counts
            .entry(projection.purpose.clone())
            .or_default() += 1;
        if projection.purpose == "compatibility" {
            let Some(policy) = projection_policy.as_ref() else {
                summary
                    .invalid_projection_ids
                    .push(projection.projection_id.clone());
                continue;
            };
            match diff_real_compat_projection(repo_root, &policy.managed_root, projection) {
                Ok(diff) if !diff.candidates.is_empty() => summary
                    .dirty_compat_projection_ids
                    .push(projection.projection_id.clone()),
                Ok(_) => {}
                Err(_) => summary
                    .invalid_projection_ids
                    .push(projection.projection_id.clone()),
            }
        }
    }
    for execution in &state.executions {
        match execution.status.as_str() {
            "pass" => summary.execution_passed += 1,
            "timeout" => summary.execution_timeouts += 1,
            _ => summary.execution_failed += 1,
        }
        if real_execution_promotion_status(state, execution) == "promotion_required" {
            summary.pending_promotions += execution
                .outputs
                .iter()
                .filter(|output| execution_output_requires_promotion(output))
                .filter(|output| {
                    !state.promotions.iter().any(|promotion| {
                        promotion.execution_id == execution.execution_id
                            && promotion.output_path == output.path
                    })
                })
                .count();
        }
    }
    summary.unexported_checkpoint_ids = state
        .checkpoints
        .iter()
        .filter(|checkpoint| {
            !state
                .export_maps
                .iter()
                .any(|map| map.checkpoint_id == checkpoint.checkpoint_id)
        })
        .map(|checkpoint| checkpoint.checkpoint_id.clone())
        .collect();
    summary.policy = real_policy_status(repo_root, state);
    summary
}

fn real_policy_status(repo_root: &Path, state: &RealRepoState) -> RealPolicyStatus {
    let mut status = RealPolicyStatus::default();
    let root = repo_root.join(".sunlight/records/validation-reports");
    if let Ok(entries) = fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("invalid_filename")
                .to_string();
            match load_git_export_validation_report(repo_root, &state.repository_id, &id) {
                Ok(report) => {
                    status.report_count += 1;
                    if report.ok {
                        status.passed_count += 1;
                    } else {
                        status.failed_count += 1;
                    }
                }
                Err(_) => {
                    status.invalid_count += 1;
                    status.invalid_report_ids.push(id);
                }
            }
        }
    }
    status.missing_export_report_count = state
        .export_maps
        .iter()
        .filter(|map| match map.validation_report_id.as_deref() {
            Some(id) => {
                load_git_export_validation_report(repo_root, &state.repository_id, id).is_err()
            }
            None => true,
        })
        .count();
    status
}

fn real_topic_heads_json(state: &RealRepoState) -> String {
    format!(
        "[{}]",
        state
            .topics
            .iter()
            .map(|topic| format!(
                "{{\"topic_id\":\"{}\",\"slug\":\"{}\",\"status\":\"{}\",\"head_revision_id\":{},\"completed_revision_id\":{},\"revision_number\":{}}}",
                json_escape(&topic.topic_id),
                json_escape(&topic.slug),
                if topic.completed_revision_id.is_some() { "completed" } else { "open" },
                optional_string_json(topic.head_revision_id.as_deref()),
                optional_string_json(topic.completed_revision_id.as_deref()),
                topic.revision_number
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn real_session_heads_json(state: &RealRepoState) -> String {
    format!(
        "[{}]",
        state
            .sessions
            .iter()
            .map(|session| format!(
                "{{\"session_id\":\"{}\",\"topic_id\":\"{}\",\"resolved_view_id\":\"{}\",\"session_generation_id\":\"{}\"}}",
                json_escape(&session.session_id),
                json_escape(&session.write_topic_id),
                json_escape(&session.resolved_view_id),
                json_escape(&session.session_generation_id)
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn operational_summary_json(state: &RealRepoState, summary: &RealOperationalSummary) -> String {
    let head = state.resolve_head_view();
    let head_tree_hash = head
        .result
        .tree_identity
        .as_ref()
        .map(|tree| tree.tree_hash.clone())
        .unwrap_or_else(|| real_tree_hash(&head.entries));
    let projections = summary
        .projection_counts
        .iter()
        .map(|(purpose, count)| format!("\"{}\":{}", json_escape(purpose), count))
        .collect::<Vec<_>>()
        .join(",");
    let checkpoints = state
        .checkpoints
        .iter()
        .map(|checkpoint| {
            format!(
                "{{\"checkpoint_id\":\"{}\",\"resolved_view_id\":\"{}\",\"tree_hash\":\"{}\",\"exported\":{}}}",
                json_escape(&checkpoint.checkpoint_id),
                json_escape(&checkpoint.resolved_view_id),
                json_escape(&checkpoint.tree_hash),
                state
                    .export_maps
                    .iter()
                    .any(|map| map.checkpoint_id == checkpoint.checkpoint_id)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let export_maps = state
        .export_maps
        .iter()
        .map(|map| {
            format!(
                "{{\"export_map_id\":\"{}\",\"checkpoint_id\":\"{}\",\"git_ref\":\"{}\",\"git_commit_count\":{}}}",
                json_escape(&map.export_map_id),
                json_escape(&map.checkpoint_id),
                json_escape(&map.git_ref),
                map.git_commit_ids.len()
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let worktree_counts = summary
        .worktree_candidate_counts
        .iter()
        .map(|(kind, count)| format!("\"{}\":{}", json_escape(kind), count))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{\"repository\":{{\"repository_id\":\"{}\",\"base_checkpoint_id\":\"{}\",\"base_resolved_view_id\":\"{}\",\"current_resolved_view_id\":\"{}\",\"tree_hash\":\"{}\"}},",
            "\"worktree\":{{\"state\":\"{}\",\"anchor\":{},\"candidate_count\":{},\"candidate_counts\":{{{}}},\"worktree_diff_id\":{},\"scan_error\":{}}},",
            "\"artifacts\":{{\"active\":{},\"legacy_quarantine_excluded\":{}}},",
            "\"topics\":{{\"count\":{},\"heads\":{}}},",
            "\"sessions\":{{\"count\":{},\"heads\":{}}},",
            "\"resolution\":{{\"conflicts\":{},\"staleness\":{}}},",
            "\"projections\":{{\"total\":{},\"by_purpose\":{{{}}},\"lifecycle\":{{\"materialized\":{},\"dirty\":{},\"invalid\":{}}}}},",
            "\"executions\":{{\"total\":{},\"passed\":{},\"failed\":{},\"timeouts\":{},\"pending_promotions\":{}}},",
            "\"checkpoints\":{{\"count\":{},\"unexported\":{},\"records\":[{}]}},",
            "\"exports\":{{\"count\":{},\"maps\":[{}]}},",
            "\"policy\":{{\"reports\":{},\"passed\":{},\"failed\":{},\"invalid_or_tampered\":{},\"missing_export_reports\":{}}},",
            "\"execution_isolation\":{{\"enforced\":{},\"process_tree\":\"{}\",\"network\":{{\"requested\":\"{}\",\"effective\":\"{}\"}},\"filesystem_writes_requested\":\"private_projection_isolated\",\"filesystem_writes\":\"{}\",\"cpu\":\"{}\",\"memory\":\"{}\"}}}}"
        ),
        json_escape(&state.repository_id),
        json_escape(&state.base_checkpoint_id),
        json_escape(&state.base_resolved_view_id),
        json_escape(&head.result.resolved_view_id),
        json_escape(&head_tree_hash),
        json_escape(&summary.worktree_state),
        real_worktree_anchor_summary_json(&state.worktree_anchor),
        summary.worktree_candidate_count,
        worktree_counts,
        optional_string_json(summary.worktree_diff_id.as_deref()),
        optional_string_json(summary.worktree_scan_error.as_deref()),
        summary.artifact_count,
        summary.quarantined_count,
        state.topics.len(),
        real_topic_heads_json(state),
        state.sessions.len(),
        real_session_heads_json(state),
        summary.conflict_count,
        summary.staleness_count,
        state.projections.len(),
        projections,
        state
            .projections
            .len()
            .saturating_sub(summary.dirty_compat_projection_ids.len() + summary.invalid_projection_ids.len()),
        summary.dirty_compat_projection_ids.len(),
        summary.invalid_projection_ids.len(),
        state.executions.len(),
        summary.execution_passed,
        summary.execution_failed,
        summary.execution_timeouts,
        summary.pending_promotions,
        state.checkpoints.len(),
        summary.unexported_checkpoint_ids.len(),
        checkpoints,
        state.export_maps.len(),
        export_maps,
        summary.policy.report_count,
        summary.policy.passed_count,
        summary.policy.failed_count,
        summary.policy.invalid_count,
        summary.policy.missing_export_report_count,
        cfg!(windows),
        if cfg!(windows) { "windows_job_object" } else { "unenforced" },
        state.executions.last().map_or("not_enforced", |execution| execution.network_policy_requested.as_str()),
        state.executions.last().map_or("not_enforced", |execution| execution.network_policy.as_str()),
        if cfg!(windows) { "windows_low_integrity_private_projection_v1" } else { "not_enforced" },
        if cfg!(windows) { "windows_job_object_cpu_time" } else { "unenforced" },
        if cfg!(windows) { "windows_job_object_process_and_job_memory" } else { "unenforced" },
    )
}

fn real_recommended_start_json(state: &RealRepoState) -> String {
    for checkpoint in state.checkpoints.iter().rev() {
        if real_resolve_checkpoint_by_id(state, &checkpoint.checkpoint_id).is_ok() {
            return format!(
                "{{\"checkpoint_id\":\"{}\",\"resolved_view_id\":\"{}\",\"tree_hash\":\"{}\",\"topic_frontier\":{},\"source\":\"latest_checkpoint\"}}",
                json_escape(&checkpoint.checkpoint_id),
                json_escape(&checkpoint.resolved_view_id),
                json_escape(&checkpoint.tree_hash),
                real_topic_frontier_pairs_json(&checkpoint.topic_frontier),
            );
        }
    }

    let base = real_base_resolved_repo_view(state);
    let tree_hash = base
        .result
        .tree_identity
        .as_ref()
        .map(|tree| tree.tree_hash.as_str())
        .unwrap_or(&state.tree_hash);
    format!(
        "{{\"checkpoint_id\":\"{}\",\"resolved_view_id\":\"{}\",\"tree_hash\":\"{}\",\"topic_frontier\":{},\"source\":\"repository_base\"}}",
        json_escape(&state.base_checkpoint_id),
        json_escape(&state.base_resolved_view_id),
        json_escape(tree_hash),
        string_string_map_json(&BTreeMap::new()),
    )
}

fn real_operational_warnings_json(
    state: &RealRepoState,
    summary: &RealOperationalSummary,
    command: &str,
) -> String {
    let mut warnings = Vec::new();
    if matches!(command, "status.repository" | "inspect.repository")
        && summary.git_working_tree_changed
    {
        warnings.push("{\"code\":\"git_working_tree_changed\",\"message\":\"the Git working tree changed outside Sunlight; native Sunlight state remains authoritative\",\"details\":{}}".to_string());
    }
    if matches!(command, "status.repository" | "inspect.repository")
        && summary.worktree_state == "dirty"
    {
        warnings.push(format!(
            "{{\"code\":\"uncaptured_worktree_changes\",\"message\":\"the repository worktree has changes outside native Sunlight history\",\"details\":{{\"candidate_count\":{},\"anchor_generation\":{},\"next_action\":\"Call worktree_diff, then worktree_capture if these edits should enter Sunlight.\"}}}}",
            summary.worktree_candidate_count,
            state.worktree_anchor.generation,
        ));
    } else if matches!(command, "status.repository" | "inspect.repository")
        && summary.worktree_state == "unavailable"
    {
        warnings.push(format!(
            "{{\"code\":\"worktree_scan_unavailable\",\"message\":\"Sunlight could not compare the worktree with its anchor\",\"details\":{{\"reason\":{}}}}}",
            optional_string_json(summary.worktree_scan_error.as_deref()),
        ));
    }
    if summary.recovery_rollback_evidence {
        warnings.push("{\"code\":\"state_recovery_rolled_back\",\"message\":\"an interrupted malformed publication was rolled back to the newest fully valid state\",\"details\":{\"evidence_root\":\"local://.sunlight/local/recovery/native-state\"}}".to_string());
    }
    if summary.quarantined_count > 0 {
        warnings.push(format!("{{\"code\":\"legacy_ingest_quarantine\",\"message\":\"legacy automatic secret detection excluded files from this native state\",\"details\":{{\"count\":{},\"next_action\":\"Run repository_init with the current build to migrate a clean state, or follow its preservation instructions when authored history exists.\"}}}}", summary.quarantined_count));
    }
    if matches!(command, "status.repository" | "inspect.repository") && summary.conflict_count > 0 {
        warnings.push(format!("{{\"code\":\"resolver_conflicts\",\"message\":\"inspect and resolve conflicting topic operations\",\"details\":{{\"count\":{}}}}}", summary.conflict_count));
    }
    if matches!(command, "status.repository" | "inspect.repository") && summary.staleness_count > 0
    {
        warnings.push(format!("{{\"code\":\"resolver_staleness\",\"message\":\"refresh or reselect stale topic dependencies\",\"details\":{{\"count\":{}}}}}", summary.staleness_count));
    }
    if summary.execution_failed > 0 || summary.execution_timeouts > 0 {
        warnings.push(format!("{{\"code\":\"executions_need_attention\",\"message\":\"inspect failed or timed-out executions before checkpointing\",\"details\":{{\"failed\":{},\"timeouts\":{}}}}}", summary.execution_failed, summary.execution_timeouts));
    }
    if summary.pending_promotions > 0 {
        warnings.push(format!("{{\"code\":\"pending_promotions\",\"message\":\"promote or discard execution outputs\",\"details\":{{\"count\":{}}}}}", summary.pending_promotions));
    }
    if !summary.dirty_compat_projection_ids.is_empty() {
        warnings.push(format!("{{\"code\":\"dirty_compatibility_projections\",\"message\":\"review compatibility diffs and explicitly import or discard them\",\"details\":{{\"projection_ids\":{}}}}}", string_array_json(summary.dirty_compat_projection_ids.iter().map(String::as_str))));
    }
    if !summary.invalid_projection_ids.is_empty() {
        warnings.push(format!("{{\"code\":\"invalid_projection_records\",\"message\":\"inspect or recreate unreadable compatibility projections\",\"details\":{{\"projection_ids\":{}}}}}", string_array_json(summary.invalid_projection_ids.iter().map(String::as_str))));
    }
    if !state.operations.is_empty() && state.checkpoints.is_empty() {
        warnings.push("{\"code\":\"checkpoint_missing\",\"message\":\"create a checkpoint for the authored resolved view\",\"details\":{}}".to_string());
    }
    if !summary.unexported_checkpoint_ids.is_empty() {
        warnings.push(format!("{{\"code\":\"checkpoint_not_exported\",\"message\":\"validate and export ready checkpoints when Git delivery is required\",\"details\":{{\"checkpoint_ids\":{}}}}}", string_array_json(summary.unexported_checkpoint_ids.iter().map(String::as_str))));
    }
    if summary.policy.invalid_count > 0 || summary.policy.missing_export_report_count > 0 {
        warnings.push(format!("{{\"code\":\"policy_report_integrity\",\"message\":\"rerun policy checks for missing, invalid, or tampered validation reports\",\"details\":{{\"invalid_or_tampered\":{},\"missing_export_reports\":{},\"report_ids\":{}}}}}", summary.policy.invalid_count, summary.policy.missing_export_report_count, string_array_json(summary.policy.invalid_report_ids.iter().map(String::as_str))));
    }
    format!("[{}]", warnings.join(","))
}

fn print_real_status_text(
    ctx: &CommandContext,
    state: &RealRepoState,
    command: &str,
    topic: Option<&RealTopicRecord>,
    session: Option<&RealSessionRecord>,
    summary: &RealOperationalSummary,
) {
    let head = state.resolve_head_view();
    let head_tree_hash = head
        .result
        .tree_identity
        .as_ref()
        .map(|tree| tree.tree_hash.clone())
        .unwrap_or_else(|| real_tree_hash(&head.entries));
    outputln!(ctx, "Sunlight {}", state.repository_id);
    outputln!(
        ctx,
        "base {}  view {}  tree {}",
        state.base_checkpoint_id,
        head.result.resolved_view_id,
        head_tree_hash
    );
    outputln!(
        ctx,
        "worktree {}  anchor-generation={}  candidates={}",
        summary.worktree_state,
        state.worktree_anchor.generation,
        summary.worktree_candidate_count
    );
    outputln!(
        ctx,
        "artifacts {}  legacy-quarantine-excluded {}  topics {}  sessions {}",
        summary.artifact_count,
        summary.quarantined_count,
        state.topics.len(),
        state.sessions.len()
    );
    if let Some(topic) = topic {
        outputln!(
            ctx,
            "topic {} ({})  head {}  owner {}  visibility {}  base {}",
            topic.topic_id,
            topic.slug,
            topic.head_revision_id.as_deref().unwrap_or("none"),
            topic.owner_actor_id,
            topic.visibility,
            topic.base_checkpoint_id,
        );
        for criterion in &topic.acceptance_criteria {
            outputln!(ctx, "  acceptance: {criterion}");
        }
    }
    if let Some(session) = session {
        outputln!(
            ctx,
            "session {}  generation {}  view {}  refresh-policy {}",
            session.session_id,
            session.session_generation_id,
            session.resolved_view_id,
            session.refresh_policy
        );
        outputln!(
            ctx,
            "frontier {}",
            string_map_object_json(&session.topic_frontier)
        );
        outputln!(
            ctx,
            "available-newer-heads {}",
            string_map_object_json(&available_newer_topic_heads(state, session))
        );
    }
    if command == "status.repository" || command == "inspect.repository" {
        for topic in &state.topics {
            outputln!(
                ctx,
                "  topic {}  head {}",
                topic.slug,
                topic.head_revision_id.as_deref().unwrap_or("none")
            );
        }
        for session in &state.sessions {
            outputln!(
                ctx,
                "  session {}  topic {}  generation {}",
                session.session_id,
                session.write_topic_id,
                session.session_generation_id
            );
        }
    }
    outputln!(
        ctx,
        "resolution conflicts={} stale={}  projections={} dirty-compat={}",
        summary.conflict_count,
        summary.staleness_count,
        state.projections.len(),
        summary.dirty_compat_projection_ids.len()
    );
    outputln!(
        ctx,
        "executions pass={} fail={} timeout={} pending-promotions={}",
        summary.execution_passed,
        summary.execution_failed,
        summary.execution_timeouts,
        summary.pending_promotions
    );
    outputln!(
        ctx,
        "checkpoints {}  exports {}  policy-reports {} (failed={} invalid={})",
        state.checkpoints.len(),
        state.export_maps.len(),
        summary.policy.report_count,
        summary.policy.failed_count,
        summary.policy.invalid_count
    );
    if cfg!(windows) {
        outputln!(ctx, "execution isolation: process-tree/cpu/memory enforced by Windows Job Objects; network/filesystem-write unenforced");
    } else {
        outputln!(
            ctx,
            "execution isolation: process-tree/cpu/memory/network/filesystem-write unenforced"
        );
    }
    if matches!(command, "status.repository" | "inspect.repository")
        && summary.git_working_tree_changed
    {
        outputln!(ctx, "warning[git_working_tree_changed]: the Git working tree changed outside Sunlight; native Sunlight state remains authoritative");
    }
    if matches!(command, "status.repository" | "inspect.repository")
        && summary.worktree_state == "dirty"
    {
        outputln!(ctx, "warning[uncaptured_worktree_changes]: call sun compat diff --worktree, then capture the edits if they should enter Sunlight");
    } else if matches!(command, "status.repository" | "inspect.repository")
        && summary.worktree_state == "unavailable"
    {
        outputln!(ctx, "warning[worktree_scan_unavailable]: call sun compat diff --worktree for exact scan diagnostics");
    }
    if summary.recovery_rollback_evidence {
        outputln!(ctx, "warning[state_recovery_rolled_back]: inspect .sunlight/local/recovery/native-state before the next successful state publication cleans the evidence");
    }
    if summary.quarantined_count > 0 {
        outputln!(
            ctx,
            "warning[legacy_ingest_quarantine]: run sun init with the current build to migrate a clean state, or follow its preservation instructions when authored history exists"
        );
    }
    if matches!(command, "status.repository" | "inspect.repository") && summary.conflict_count > 0 {
        outputln!(
            ctx,
            "warning[resolver_conflicts]: inspect and resolve conflicting topic operations"
        );
    }
    if matches!(command, "status.repository" | "inspect.repository") && summary.staleness_count > 0
    {
        outputln!(
            ctx,
            "warning[resolver_staleness]: refresh or reselect stale topic dependencies"
        );
    }
    if summary.execution_failed > 0 || summary.execution_timeouts > 0 {
        outputln!(
            ctx,
            "warning[executions_need_attention]: inspect failed or timed-out executions"
        );
    }
    if summary.pending_promotions > 0 {
        outputln!(
            ctx,
            "warning[pending_promotions]: promote or discard execution outputs"
        );
    }
    if !summary.dirty_compat_projection_ids.is_empty() {
        outputln!(ctx, "warning[dirty_compatibility_projections]: review compatibility diffs and import or discard them");
    }
    if !summary.invalid_projection_ids.is_empty() {
        outputln!(
            ctx,
            "warning[invalid_projection_records]: inspect or recreate unreadable projections"
        );
    }
    if !state.operations.is_empty() && state.checkpoints.is_empty() {
        outputln!(
            ctx,
            "warning[checkpoint_missing]: create a checkpoint for the authored resolved view"
        );
    }
    if !summary.unexported_checkpoint_ids.is_empty() {
        outputln!(ctx, "warning[checkpoint_not_exported]: validate and export ready checkpoints when Git delivery is required");
    }
    if summary.policy.invalid_count > 0 || summary.policy.missing_export_report_count > 0 {
        outputln!(
            ctx,
            "warning[policy_report_integrity]: rerun policy checks for missing or invalid reports"
        );
    }
}

fn real_status_envelope(
    state: &RealRepoState,
    repo_root: &Path,
    command: &str,
    topic: Option<&RealTopicRecord>,
    session: Option<&RealSessionRecord>,
    summary: Option<&RealOperationalSummary>,
) -> String {
    let include_summary = summary.is_some();
    let fallback_summary;
    let summary = match summary {
        Some(summary) => summary,
        None => {
            fallback_summary = real_operational_summary(repo_root, state);
            &fallback_summary
        }
    };
    let warnings = real_operational_warnings_json(state, summary, command);
    let summary_extension = if include_summary {
        format!(
            ",\"operational_summary\":{}",
            operational_summary_json(state, summary)
        )
    } else {
        String::new()
    };
    let topic_id = topic.map(|topic| topic.topic_id.as_str());
    let topic_json = topic
        .map(real_topic_metadata_json)
        .unwrap_or_else(|| "{\"topic_id\":null,\"head_revision_id\":null}".to_string());
    let handoff_json = topic
        .map(|topic| real_topic_handoff_json(repo_root, state, topic))
        .unwrap_or_else(|| "null".to_string());
    let session_id = session.map(|session| session.session_id.as_str());
    let fallback_view = real_view(state);
    let session_generation_id = session
        .map(|session| session.session_generation_id.as_str())
        .unwrap_or(fallback_view.session_generation_id.as_str());
    let view = session
        .map(|session| real_session_view(state, session))
        .unwrap_or_else(|| real_view(state));
    let compatibility_projections = session
        .map(|session| {
            state
                .projections
                .iter()
                .filter(|projection| {
                    projection.purpose == "compatibility"
                        && projection.session_id.as_deref() == Some(session.session_id.as_str())
                })
                .map(|projection| {
                    format!(
                        "{{\"projection_id\":\"{}\",\"baseline_resolved_view_id\":\"{}\",\"baseline_session_generation_id\":{},\"last_import_operation_id\":{}}}",
                        json_escape(&projection.projection_id),
                        json_escape(&projection.resolved_view_id),
                        optional_string_json(projection.session_generation_id.as_deref()),
                        optional_string_json(projection.last_import_operation_id.as_deref()),
                    )
                })
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    let session_context = session
        .map(|session| {
            format!(
                ",\"refresh_policy\":\"{}\",\"topic_frontier\":{},\"available_newer_topic_heads\":{}",
                json_escape(&session.refresh_policy),
                string_map_object_json(&session.topic_frontier),
                string_map_object_json(&available_newer_topic_heads(state, session)),
            )
        })
        .unwrap_or_default();
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"{}\",\"repository_id\":\"{}\",\"ids\":{{\"repository_id\":\"{}\",\"session_id\":{},\"topic_id\":{},\"resolved_view_id\":\"{}\"}},\"view\":{},\"repository\":{{\"artifact_count\":{},\"tree_hash\":\"{}\",\"base_checkpoint_id\":\"{}\",\"recommended_start\":{},\"automatic_secret_detection\":false,\"legacy_quarantine_excluded_count\":{},\"ignore_policy\":{{\"gitignore\":\"git_semantics\",\"sunignore\":\".sunignore\",\"sunignore_owner\":\"human_worktree\",\"sunignore_change_command\":\"sun init\",\"intrinsic\":[\".git/\",\".sunlight/\"]}},\"worktree\":{}}},\"topic\":{},\"handoff\":{},\"session\":{{\"session_id\":{},\"session_generation_id\":\"{}\",\"compatibility_projections\":[{}]{}}}{}}},\"warnings\":{}}}",
        command,
        json_escape(&state.repository_id),
        json_escape(&state.repository_id),
        optional_string_json(session_id),
        optional_string_json(topic_id),
        json_escape(&view.resolved_view_id),
        view_json(&view),
        state.entries.iter().filter(|entry| !entry.tombstone).count(),
        json_escape(&state.tree_hash),
        json_escape(&state.base_checkpoint_id),
        real_recommended_start_json(state),
        state.quarantine.len(),
        real_worktree_status_json(state, summary),
        topic_json,
        handoff_json,
        optional_string_json(session_id),
        json_escape(session_generation_id),
        compatibility_projections,
        session_context,
        summary_extension,
        warnings,
    )
}

fn real_worktree_status_json(state: &RealRepoState, summary: &RealOperationalSummary) -> String {
    let next_action = match summary.worktree_state.as_str() {
        "dirty" => optional_string_json(Some(
            "Call worktree_diff, then worktree_capture if these edits should enter Sunlight.",
        )),
        "unavailable" => optional_string_json(Some(
            "Call worktree_diff to inspect the scan failure before changing native state.",
        )),
        _ => "null".to_string(),
    };
    let candidate_counts = summary
        .worktree_candidate_counts
        .iter()
        .map(|(kind, count)| format!("\"{}\":{}", json_escape(kind), count))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"state\":\"{}\",\"anchor\":{},\"candidate_count\":{},\"candidate_counts\":{{{}}},\"worktree_diff_id\":{},\"scan_error\":{},\"next_action\":{}}}",
        json_escape(&summary.worktree_state),
        real_worktree_anchor_summary_json(&state.worktree_anchor),
        summary.worktree_candidate_count,
        candidate_counts,
        optional_string_json(summary.worktree_diff_id.as_deref()),
        optional_string_json(summary.worktree_scan_error.as_deref()),
        next_action,
    )
}

fn available_newer_topic_heads(
    state: &RealRepoState,
    session: &RealSessionRecord,
) -> BTreeMap<String, String> {
    session
        .topic_frontier
        .iter()
        .filter(|(topic_id, _)| *topic_id != &session.write_topic_id)
        .filter_map(|(topic_id, revision_id)| {
            state
                .topics
                .iter()
                .find(|topic| topic.topic_id == *topic_id)
                .and_then(|topic| topic.head_revision_id.as_ref())
                .filter(|head| *head != revision_id)
                .map(|head| (topic_id.clone(), head.clone()))
        })
        .collect()
}

fn real_view_envelope(command: &str, state: &RealRepoState, view: &ResolvedViewResult) -> String {
    format!(
        "{{\"ok\":true,\"data\":{{\"command\":\"{}\",\"repository_id\":\"{}\",\"ids\":{{\"resolved_view_id\":\"{}\"}},\"view\":{},\"resolved_view\":{}}},\"warnings\":[]}}",
        json_escape(command),
        json_escape(&state.repository_id),
        json_escape(&view.resolved_view_id),
        view_resolve_view_json(view),
        resolved_view_record_json(view),
    )
}

fn real_inspect_envelope(
    repo_root: &Path,
    state: &RealRepoState,
    selector: &str,
) -> Result<String, CliError> {
    if let Some(path) = selector
        .strip_prefix("artifact:")
        .or_else(|| selector.strip_prefix("path:"))
    {
        let resolved = state.resolve_head_view();
        let entry = resolved
            .entries
            .iter()
            .find(|entry| !entry.tombstone && (entry.path == path || entry.artifact_id == path))
            .ok_or_else(|| object_not_found("artifact", path))?;
        let latest_operation = state.operations.iter().rev().find(|operation| {
            operation
                .artifact_effects()
                .iter()
                .any(|effect| effect.artifact_id == entry.artifact_id)
        });
        let compat_provenance = latest_operation
            .filter(|operation| operation.compat_projection_id.is_some())
            .map(real_compat_provenance_json)
            .unwrap_or_else(|| "null".to_string());
        let worktree_provenance = latest_operation
            .filter(|operation| !operation.worktree_candidate_delta_ids.is_empty())
            .map(real_worktree_provenance_json)
            .unwrap_or_else(|| "null".to_string());
        return Ok(format!(
            "{{\"ok\":true,\"data\":{{\"command\":\"inspect.artifact\",\"repository_id\":\"{}\",\"ids\":{{\"artifact_id\":\"{}\"}},\"view\":{},\"artifact\":{},\"path_history\":{},\"latest_operation_id\":{},\"compat_import_provenance\":{},\"worktree_capture_provenance\":{}}},\"warnings\":[]}}",
            json_escape(&state.repository_id),
            json_escape(&entry.artifact_id),
            view_resolve_view_json(&resolved.result),
            artifact_json(&real_artifact_view(entry)),
            real_artifact_path_history_json(state, &entry.artifact_id),
            optional_string_json(
                latest_operation.map(|operation| operation.operation_transaction_id.as_str())
            ),
            compat_provenance,
            worktree_provenance,
        ));
    }
    if selector == format!("repository:{}", state.repository_id) || selector == "repository" {
        return Ok(real_status_envelope(
            state,
            repo_root,
            "inspect.repository",
            None,
            None,
            Some(&real_operational_summary(repo_root, state)),
        ));
    }
    if let Some(topic) = selector.strip_prefix("topic:") {
        let topic = state
            .topic_by_id_or_slug(topic)
            .ok_or_else(|| CliError::new("object_not_found", "Sunlight object was not found"))?;
        return Ok(real_status_envelope(
            state,
            repo_root,
            "inspect.topic",
            Some(topic),
            None,
            None,
        ));
    }
    if let Some(session) = selector.strip_prefix("session:") {
        let session = state
            .session_by_id(session)
            .ok_or_else(|| CliError::new("object_not_found", "Sunlight object was not found"))?;
        let topic = state
            .topics
            .iter()
            .find(|topic| topic.topic_id == session.write_topic_id);
        return Ok(real_status_envelope(
            state,
            repo_root,
            "inspect.session",
            topic,
            Some(session),
            None,
        ));
    }
    if let Some(projection) = selector.strip_prefix("projection:") {
        let projection = state
            .projections
            .iter()
            .find(|candidate| candidate.projection_id == projection)
            .ok_or_else(|| object_not_found("projection", projection))?;
        let candidates = if projection.purpose == "compatibility" {
            let config = require_repository_config(repo_root)?;
            let policy = resolve_projection_policy(repo_root, &config).map_err(|error| {
                invalid_request(error.to_string())
                    .with_detail("projection_id", projection.projection_id.clone())
            })?;
            Some(
                diff_real_compat_projection(repo_root, &policy.managed_root, projection)
                    .map_err(compat_import_error)?
                    .candidates,
            )
        } else {
            None
        };
        return Ok(real_projection_inspect_envelope(
            state,
            projection,
            candidates.as_deref(),
        ));
    }
    if let Some(operation_id) = selector
        .strip_prefix("compat-import:")
        .or_else(|| selector.strip_prefix("operation:"))
    {
        let operation = state
            .operations
            .iter()
            .find(|operation| operation.operation_transaction_id == operation_id)
            .ok_or_else(|| object_not_found("operation", operation_id))?;
        let command = if selector.starts_with("compat-import:") {
            "inspect.compat-import"
        } else {
            "inspect.operation"
        };
        return Ok(real_compat_operation_inspect_envelope(
            state, operation, command,
        ));
    }
    if let Some(checkpoint) = selector.strip_prefix("checkpoint:") {
        let checkpoint = state
            .checkpoints
            .iter()
            .find(|candidate| candidate.checkpoint_id == checkpoint)
            .ok_or_else(|| object_not_found("checkpoint", checkpoint))?;
        return Ok(real_checkpoint_inspect_envelope(state, checkpoint));
    }
    if let Some(export_map) = selector
        .strip_prefix("export_map:")
        .or_else(|| selector.strip_prefix("export:"))
    {
        let export_map = state
            .export_maps
            .iter()
            .find(|candidate| candidate.export_map_id == export_map)
            .ok_or_else(|| object_not_found("export_map", export_map))?;
        return Ok(real_export_map_inspect_envelope(state, export_map));
    }
    if let Some(view) = selector.strip_prefix("view:") {
        let resolved = real_resolve_view_by_id(repo_root, state, view)?;
        return Ok(real_view_envelope("inspect.view", state, &resolved.result));
    }
    if let Some(conflict) = selector.strip_prefix("conflict:") {
        let resolved = state.resolve_head_view();
        if let Some(record) = resolved
            .result
            .records
            .iter()
            .find(|record| record.id == conflict)
        {
            return Ok(format!(
                "{{\"ok\":true,\"data\":{{\"command\":\"inspect.conflict\",\"repository_id\":\"{}\",\"ids\":{{\"conflict_id\":\"{}\",\"resolved_view_id\":\"{}\"}},\"conflict\":{}}},\"warnings\":[]}}",
                json_escape(&state.repository_id),
                json_escape(&record.id),
                json_escape(&record.resolved_view_id),
                resolver_record_json(record),
            ));
        }
        let path = repo_root
            .join(".sunlight")
            .join("conflicts")
            .join(format!("{conflict}.json"));
        if let Ok(record) = fs::read_to_string(path) {
            let record = record.trim();
            parse_json_record(record.as_bytes()).map_err(|error| {
                invalid_request(format!("persisted resolver evidence is invalid: {error}"))
            })?;
            return Ok(format!(
                "{{\"ok\":true,\"data\":{{\"command\":\"inspect.conflict\",\"repository_id\":\"{}\",\"ids\":{{\"conflict_id\":\"{}\"}},\"conflict\":{}}},\"warnings\":[]}}",
                json_escape(&state.repository_id),
                json_escape(conflict),
                record,
            ));
        }
    }
    Err(CliError::new(
        "object_not_found",
        "Sunlight object was not found",
    ))
}

fn real_artifact_path_history_json(state: &RealRepoState, artifact_id: &str) -> String {
    let mut history = state
        .base_entries
        .iter()
        .filter(|entry| entry.artifact_id == artifact_id)
        .map(|entry| {
            format!(
                "{{\"path\":\"{}\",\"state\":\"{}\",\"operation_transaction_id\":null}}",
                json_escape(&entry.path),
                if entry.tombstone {
                    "tombstone"
                } else {
                    "active"
                }
            )
        })
        .collect::<Vec<_>>();
    for operation in &state.operations {
        for effect in operation
            .artifact_effects()
            .into_iter()
            .filter(|effect| effect.artifact_id == artifact_id)
        {
            history.push(format!(
                "{{\"path\":\"{}\",\"state\":\"{}\",\"operation_transaction_id\":\"{}\"}}",
                json_escape(&effect.path),
                if effect.tombstone {
                    "tombstone"
                } else {
                    "active"
                },
                json_escape(&operation.operation_transaction_id),
            ));
        }
    }
    format!("[{}]", history.join(","))
}

#[derive(Debug)]
struct StatusOptions {
    fixture: String,
    scope: StatusScope,
    projection_root: Option<PathBuf>,
    integrity_fixture: Option<StoreIntegrityFixture>,
    promoted: bool,
}

#[derive(Debug)]
enum StatusScope {
    Repository,
    Session(String),
    Topic(String),
    View(String),
    Projection(String),
    Checkpoint(String),
    ExportMap(String),
    Git(String),
    CompatImport(String),
    Execution(String),
}

#[derive(Debug)]
struct InspectOptions {
    fixture: String,
    selector: String,
    session_id: Option<String>,
    projection_root: Option<PathBuf>,
    integrity_fixture: Option<StoreIntegrityFixture>,
    promoted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StoreIntegrityFixture {
    ScanMissingBlob,
    StoreMismatch,
    Verified,
}

impl StoreIntegrityFixture {
    fn as_str(self) -> &'static str {
        match self {
            Self::ScanMissingBlob => "scan-missing-blob",
            Self::StoreMismatch => "store-mismatch",
            Self::Verified => "verified",
        }
    }
}

#[derive(Debug)]
struct ViewResolveOptions {
    fixture: Option<String>,
    include: Vec<TopicRevisionSelection>,
    base_checkpoint_id: Option<String>,
}

#[derive(Debug)]
struct ExecutionRunOptions {
    fixture: Option<String>,
    view_id: String,
    command_argv: Vec<String>,
    cwd: String,
    integrity_fixture: Option<StoreIntegrityFixture>,
    network_policy: Option<ExecutionNetworkPolicy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExecutionNetworkPolicy {
    Disabled,
    NotEnforced,
}

impl ExecutionNetworkPolicy {
    fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            DISABLED_NETWORK_POLICY => Ok(Self::Disabled),
            NOT_ENFORCED_NETWORK_POLICY => Ok(Self::NotEnforced),
            _ => Err(
                invalid_request("sun run --network must be disabled or not_enforced")
                    .with_detail("network", value),
            ),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => DISABLED_NETWORK_POLICY,
            Self::NotEnforced => NOT_ENFORCED_NETWORK_POLICY,
        }
    }
}

#[derive(Debug)]
struct CheckpointCreateOptions {
    fixture: Option<String>,
    view_id: String,
    execution_id: Option<String>,
}

#[derive(Debug)]
struct PolicyCheckExportOptions {
    fixture: Option<String>,
    checkpoint_id: String,
    git_ref: Option<String>,
}

#[derive(Debug)]
struct PolicyCheckCommitOptions {
    paths: Vec<String>,
}

#[derive(Debug)]
struct PolicyExplainOptions {
    validation_report_id: String,
}

#[derive(Debug)]
struct GitExportOptions {
    fixture: Option<String>,
    checkpoint_id: String,
    git_ref: String,
    write_plan: bool,
    execute_fixture: Option<GitExportExecutionFixtureMode>,
    execute_local: bool,
    repo: Option<PathBuf>,
    simulate_export_map_write_failure: bool,
}

#[derive(Debug, Clone, Copy)]
enum GitExportExecutionFixtureMode {
    Success,
    RefUpdateFailure,
    ExportMapFailure,
}

impl From<GitExportExecutionFixtureMode> for GitExportExecutionFixture {
    fn from(mode: GitExportExecutionFixtureMode) -> Self {
        let mut fixture = GitExportExecutionFixture::success();
        match mode {
            GitExportExecutionFixtureMode::Success => {}
            GitExportExecutionFixtureMode::RefUpdateFailure => {
                fixture.ref_update =
                    GitExportExecutionStepFixture::fail("fixture ref update failed");
            }
            GitExportExecutionFixtureMode::ExportMapFailure => {
                fixture.export_map_write =
                    GitExportExecutionStepFixture::fail("fixture export map write failed");
            }
        }
        fixture
    }
}

#[derive(Debug)]
struct FixtureGitExport {
    checkpoint: CheckpointRecord,
    response: GitExportResponse,
}

#[derive(Debug)]
struct ProjectMaterializeOptions {
    fixture: Option<String>,
    view_id: String,
    purpose: ProjectionPurpose,
    strategy: Option<ProjectionStrategy>,
    fallback_to_copy: bool,
    projection_root: Option<PathBuf>,
}

#[derive(Debug)]
struct ProjectionQuarantineCleanupOptions {
    fixture: String,
    projection_id: String,
    projection_root: PathBuf,
}

#[derive(Debug)]
struct CompatProjectOptions {
    fixture: Option<String>,
    session_id: String,
}

#[derive(Debug)]
struct CompatDiffOptions {
    fixture: Option<String>,
    projection_id: Option<String>,
    worktree: bool,
}

#[derive(Debug)]
struct CompatImportOptions {
    fixture: Option<String>,
    projection_id: String,
    session_generation_id: Option<String>,
    candidate_delta_ids: Vec<String>,
}

#[derive(Debug)]
struct WorktreeCaptureOptions {
    topic_slug: String,
    actor_id: String,
    anchor_generation: Option<u64>,
    candidate_delta_ids: Vec<String>,
    paths: Vec<String>,
}

fn parse_compat_project_options(ctx: &CommandContext) -> Result<CompatProjectOptions, CliError> {
    let mut fixture = None;
    let mut session_id = None;
    let mut args = ctx.args.iter().skip(2);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun compat project requires --fixture basic-app")
                })?;
                fixture = Some(value.clone());
            }
            "--session" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun compat project requires --session <session-id>")
                })?;
                session_id = Some(value.clone());
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun compat project"
                )));
            }
            value => {
                return Err(invalid_request(format!(
                    "unexpected compat project argument `{value}`"
                )));
            }
        }
    }

    let usage = if fixture.is_some() {
        "usage: sun compat project --session <session-id> --fixture basic-app"
    } else {
        "usage: sun compat project --session <session-id> [--fixture basic-app]"
    };
    let session_id = session_id.ok_or_else(|| invalid_request(usage))?;

    Ok(CompatProjectOptions {
        fixture,
        session_id,
    })
}

fn parse_compat_diff_options(ctx: &CommandContext) -> Result<CompatDiffOptions, CliError> {
    let mut fixture = None;
    let mut projection_id = None;
    let mut worktree = false;
    let mut args = ctx.args.iter().skip(2);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun compat diff requires --fixture basic-app")
                })?;
                fixture = Some(value.clone());
            }
            "--projection" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun compat diff requires --projection <projection-id>")
                })?;
                projection_id = Some(value.clone());
            }
            "--worktree" => worktree = true,
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun compat diff"
                )));
            }
            value => {
                return Err(invalid_request(format!(
                    "unexpected compat diff argument `{value}`"
                )));
            }
        }
    }

    let usage =
        "usage: sun compat diff (--projection <projection-id> [--fixture basic-app] | --worktree)";
    if worktree == projection_id.is_some() || (worktree && fixture.is_some()) {
        return Err(invalid_request(usage));
    }

    Ok(CompatDiffOptions {
        fixture,
        projection_id,
        worktree,
    })
}

fn parse_worktree_capture_options(
    ctx: &CommandContext,
) -> Result<WorktreeCaptureOptions, CliError> {
    let mut worktree = false;
    let mut topic_slug = None;
    let mut actor_id = None;
    let mut anchor_generation = None;
    let mut candidate_delta_ids = Vec::new();
    let mut paths = Vec::new();
    let mut args = ctx.args.iter().skip(2);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--worktree" => worktree = true,
            "--topic" => {
                topic_slug = Some(
                    args.next()
                        .ok_or_else(|| {
                            invalid_request("sun compat capture requires --topic <slug>")
                        })?
                        .clone(),
                );
            }
            "--actor" => {
                actor_id = Some(
                    args.next()
                        .ok_or_else(|| {
                            invalid_request("sun compat capture requires --actor <actor-id>")
                        })?
                        .clone(),
                );
            }
            "--anchor-generation" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "sun compat capture --anchor-generation requires a positive integer",
                    )
                })?;
                anchor_generation = Some(value.parse::<u64>().map_err(|_| {
                    invalid_request("worktree anchor generation must be a positive integer")
                })?);
            }
            "--candidate" => {
                candidate_delta_ids.push(
                    args.next()
                        .ok_or_else(|| {
                            invalid_request(
                                "sun compat capture --candidate requires a candidate ID",
                            )
                        })?
                        .clone(),
                );
            }
            "--path" => {
                paths.push(
                    args.next()
                        .ok_or_else(|| {
                            invalid_request("sun compat capture --path requires a repository path")
                        })?
                        .clone(),
                );
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun compat capture"
                )));
            }
            value => {
                return Err(invalid_request(format!(
                    "unexpected compat capture argument `{value}`"
                )));
            }
        }
    }
    let usage = "usage: sun compat capture --worktree --topic <slug> --actor <actor-id> [--anchor-generation <number>] [--candidate <candidate-id> ... | --path <path> ...]";
    if !worktree || topic_slug.is_none() || actor_id.is_none() || anchor_generation == Some(0) {
        return Err(invalid_request(usage));
    }
    if !candidate_delta_ids.is_empty() && !paths.is_empty() {
        return Err(invalid_request(
            "select worktree changes with candidate IDs or paths, not both",
        ));
    }
    Ok(WorktreeCaptureOptions {
        topic_slug: topic_slug.expect("validated topic"),
        actor_id: actor_id.expect("validated actor"),
        anchor_generation,
        candidate_delta_ids,
        paths,
    })
}

fn parse_compat_import_options(ctx: &CommandContext) -> Result<CompatImportOptions, CliError> {
    let mut fixture = None;
    let mut projection_id = None;
    let mut session_generation_id = None;
    let mut candidate_delta_ids = Vec::new();
    let mut args = ctx.args.iter().skip(2);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun compat import requires --fixture basic-app")
                })?;
                fixture = Some(value.clone());
            }
            "--projection" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun compat import requires --projection <projection-id>",
                    )
                })?;
                projection_id = Some(value.clone());
            }
            "--session-generation" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun compat import requires --session-generation <generation-id>",
                    )
                })?;
                session_generation_id = Some(value.clone());
            }
            "--candidate" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun compat import requires --candidate <candidate-id>")
                })?;
                candidate_delta_ids.push(value.clone());
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun compat import"
                )));
            }
            value => {
                return Err(invalid_request(format!(
                    "unexpected compat import argument `{value}`"
                )));
            }
        }
    }

    let projection_id = projection_id.ok_or_else(|| {
        invalid_request(
            "usage: sun compat import --projection <projection-id> --candidate <candidate-id> [--session-generation <generation-id>] [--fixture basic-app]",
        )
    })?;

    Ok(CompatImportOptions {
        fixture,
        projection_id,
        session_generation_id,
        candidate_delta_ids,
    })
}

fn parse_projection_quarantine_cleanup_options(
    ctx: &CommandContext,
) -> Result<ProjectionQuarantineCleanupOptions, CliError> {
    let mut fixture = None;
    let mut projection_id = None;
    let mut projection_root = None;
    let mut args = ctx.args.iter().skip(2);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun projection quarantine-cleanup requires --fixture basic-app",
                    )
                })?;
                fixture = Some(value.clone());
            }
            "--projection" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun projection quarantine-cleanup requires --projection <projection-id>",
                    )
                })?;
                projection_id = Some(value.clone());
            }
            "--projection-root" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun projection quarantine-cleanup requires --projection-root <path>",
                    )
                })?;
                projection_root = Some(PathBuf::from(value));
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun projection quarantine-cleanup"
                )));
            }
            value => {
                return Err(invalid_request(format!(
                    "unexpected projection quarantine-cleanup argument `{value}`"
                )));
            }
        }
    }

    let usage = "usage: sun projection quarantine-cleanup --projection <projection-id> --projection-root <path> --fixture basic-app";
    let fixture = fixture.ok_or_else(|| invalid_request(usage))?;
    let projection_id = projection_id.ok_or_else(|| invalid_request(usage))?;
    let projection_root = projection_root.ok_or_else(|| invalid_request(usage))?;

    Ok(ProjectionQuarantineCleanupOptions {
        fixture,
        projection_id,
        projection_root,
    })
}

fn parse_project_materialize_options(
    ctx: &CommandContext,
) -> Result<ProjectMaterializeOptions, CliError> {
    let mut fixture = None;
    let mut view_id = None;
    let mut purpose = ProjectionPurpose::Execution;
    let mut strategy = None;
    let mut fallback_to_copy = true;
    let mut projection_root = None;
    let mut args = ctx.args.iter().skip(2);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun project materialize requires --fixture basic-app")
                })?;
                fixture = Some(value.clone());
            }
            "--view" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun project materialize requires --view <resolved-view-id>",
                    )
                })?;
                view_id = Some(value.clone());
            }
            "--purpose" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun project materialize --purpose <purpose>")
                })?;
                purpose = parse_projection_purpose(value)?;
            }
            "--strategy" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun project materialize --strategy <strategy>")
                })?;
                strategy = Some(parse_projection_strategy(value)?);
            }
            "--projection-root" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun project materialize --projection-root <empty-path>")
                })?;
                projection_root = Some(PathBuf::from(value));
            }
            "--no-copy-fallback" => {
                fallback_to_copy = false;
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun project materialize"
                )));
            }
            value if view_id.is_none() => view_id = Some(value.to_string()),
            value => {
                return Err(invalid_request(format!(
                    "unexpected project materialize argument `{value}`"
                )));
            }
        }
    }

    let view_id = view_id.ok_or_else(|| {
        invalid_request(
            "usage: sun project materialize --view <resolved-view-id> --purpose execution|compatibility|inspection|export [--fixture basic-app]",
        )
    })?;

    Ok(ProjectMaterializeOptions {
        fixture,
        view_id,
        purpose,
        strategy,
        fallback_to_copy,
        projection_root,
    })
}

fn parse_projection_purpose(value: &str) -> Result<ProjectionPurpose, CliError> {
    match value {
        "execution" => Ok(ProjectionPurpose::Execution),
        "compatibility" => Ok(ProjectionPurpose::Compatibility),
        "inspection" => Ok(ProjectionPurpose::Inspection),
        "export" => Ok(ProjectionPurpose::Export),
        _ => Err(
            invalid_request(format!("unknown projection purpose `{value}`"))
                .with_detail("purpose", value),
        ),
    }
}

fn parse_projection_strategy(value: &str) -> Result<ProjectionStrategy, CliError> {
    match value {
        "copy" => Ok(ProjectionStrategy::Copy),
        "reflink" => Ok(ProjectionStrategy::Reflink),
        "hardlink_readonly" => Ok(ProjectionStrategy::HardlinkReadonly),
        "overlay_copyup" => Ok(ProjectionStrategy::OverlayCopyup),
        _ => Err(invalid_request(format!(
            "unknown projection materialization strategy `{value}`"
        ))
        .with_detail("strategy", value)),
    }
}

fn parse_checkpoint_create_options(
    ctx: &CommandContext,
) -> Result<CheckpointCreateOptions, CliError> {
    let mut fixture = None;
    let mut view_id = None;
    let mut execution_id = None;
    let mut args = ctx.args.iter().skip(2);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun checkpoint create requires --fixture basic-app")
                })?;
                fixture = Some(value.clone());
            }
            "--view" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun checkpoint create requires --view <resolved-view-id>",
                    )
                })?;
                view_id = Some(value.clone());
            }
            "--execution" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun checkpoint create --execution requires <execution-id>",
                    )
                })?;
                execution_id = Some(value.clone());
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun checkpoint create"
                )));
            }
            value if view_id.is_none() => view_id = Some(value.to_string()),
            value => {
                return Err(invalid_request(format!(
                    "unexpected checkpoint create argument `{value}`"
                )));
            }
        }
    }

    let view_id = view_id.ok_or_else(|| {
        invalid_request(
            "usage: sun checkpoint create --view <resolved-view-id> [--execution <execution-id>] [--fixture basic-app]",
        )
    })?;

    Ok(CheckpointCreateOptions {
        fixture,
        view_id,
        execution_id,
    })
}

fn parse_git_export_options(ctx: &CommandContext) -> Result<GitExportOptions, CliError> {
    let mut fixture = None;
    let mut checkpoint_id = None;
    let mut git_ref = None;
    let mut write_plan = false;
    let mut execute_fixture = None;
    let mut execute_local = false;
    let mut repo = None;
    let mut simulate_export_map_write_failure = false;
    let mut args = ctx.args.iter().skip(2);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun git export requires --fixture basic-app")
                })?;
                fixture = Some(value.clone());
            }
            "--checkpoint" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun git export requires --checkpoint <checkpoint-id>")
                })?;
                checkpoint_id = Some(value.clone());
            }
            "--branch" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun git export requires --branch <git-ref>")
                })?;
                git_ref = Some(value.clone());
            }
            "--write-plan" => {
                write_plan = true;
            }
            "--execute-fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun git export --execute-fixture requires success|ref-update-failure|export-map-failure",
                    )
                })?;
                execute_fixture = Some(parse_git_export_execution_fixture(value)?);
            }
            "--execute-local" => {
                execute_local = true;
            }
            "--repo" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun git export --repo requires <path>")
                })?;
                repo = Some(PathBuf::from(value));
            }
            "--simulate-export-map-write-failure" => {
                simulate_export_map_write_failure = true;
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun git export"
                )));
            }
            value => {
                return Err(invalid_request(format!(
                    "unexpected git export argument `{value}`"
                )));
            }
        }
    }

    let checkpoint_id = checkpoint_id.ok_or_else(|| {
        invalid_request(
            "usage: sun git export --checkpoint <checkpoint-id> --branch <git-ref> [--fixture basic-app]",
        )
    })?;
    let git_ref = git_ref.ok_or_else(|| {
        invalid_request(
            "usage: sun git export --checkpoint <checkpoint-id> --branch <git-ref> [--fixture basic-app]",
        )
    })?;
    if write_plan && execute_fixture.is_some() {
        return Err(invalid_request(
            "sun git export cannot use --write-plan and --execute-fixture together",
        ));
    }
    if write_plan && execute_local {
        return Err(invalid_request(
            "sun git export cannot use --write-plan and --execute-local together",
        ));
    }
    if execute_fixture.is_some() && execute_local {
        return Err(invalid_request(
            "sun git export cannot use --execute-fixture and --execute-local together",
        ));
    }
    if simulate_export_map_write_failure && !execute_local {
        return Err(invalid_request(
            "sun git export --simulate-export-map-write-failure requires --execute-local",
        ));
    }

    Ok(GitExportOptions {
        fixture,
        checkpoint_id,
        git_ref,
        write_plan,
        execute_fixture,
        execute_local,
        repo,
        simulate_export_map_write_failure,
    })
}

fn parse_policy_check_export_options(
    ctx: &CommandContext,
) -> Result<PolicyCheckExportOptions, CliError> {
    let mut fixture = None;
    let mut checkpoint_id = None;
    let mut git_ref = None;
    let mut args = ctx.args.iter().skip(2);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun policy check-export requires --fixture basic-app")
                        .with_detail("missing", "fixture")
                })?;
                fixture = Some(value.clone());
            }
            "--checkpoint" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun policy check-export requires --checkpoint <checkpoint-id>",
                    )
                    .with_detail("missing", "checkpoint")
                })?;
                checkpoint_id = Some(value.clone());
            }
            "--branch" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun policy check-export --branch requires <git-ref>")
                })?;
                git_ref = Some(value.clone());
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun policy check-export"
                )));
            }
            value => {
                return Err(invalid_request(format!(
                    "unexpected policy check-export argument `{value}`"
                )));
            }
        }
    }

    let checkpoint_id = checkpoint_id.ok_or_else(|| {
        invalid_request(
            "usage: sun policy check-export --checkpoint <checkpoint-id> [--branch <git-ref>] [--fixture basic-app]",
        )
        .with_detail("missing", "checkpoint")
    })?;

    Ok(PolicyCheckExportOptions {
        fixture,
        checkpoint_id,
        git_ref,
    })
}

fn parse_policy_check_commit_options(
    ctx: &CommandContext,
) -> Result<PolicyCheckCommitOptions, CliError> {
    let mut paths = Vec::new();
    let mut args = ctx.args.iter().skip(2).peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--paths" => {
                let mut saw_path = false;
                while let Some(value) = args.peek() {
                    if value.starts_with("--") {
                        break;
                    }
                    paths.push((*value).clone());
                    saw_path = true;
                    args.next();
                }
                if !saw_path {
                    return Err(invalid_request(
                        "usage: sun policy check-commit --paths requires <path>...",
                    )
                    .with_detail("missing", "paths"));
                }
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun policy check-commit"
                )));
            }
            value => {
                return Err(invalid_request(format!(
                    "unexpected policy check-commit argument `{value}`"
                )));
            }
        }
    }

    Ok(PolicyCheckCommitOptions { paths })
}

fn parse_policy_explain_options(ctx: &CommandContext) -> Result<PolicyExplainOptions, CliError> {
    let mut validation_report_id = None;
    let args = ctx.args.iter().skip(2);

    for arg in args {
        match arg.as_str() {
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun policy explain"
                )));
            }
            value => {
                if validation_report_id.is_some() {
                    return Err(invalid_request(format!(
                        "unexpected policy explain argument `{value}`"
                    )));
                }
                validation_report_id = Some(value.to_string());
            }
        }
    }

    let validation_report_id = validation_report_id.ok_or_else(|| {
        invalid_request("usage: sun policy explain <validation-report-id> --json")
            .with_detail("missing", "validation_report_id")
    })?;

    Ok(PolicyExplainOptions {
        validation_report_id,
    })
}

fn parse_git_export_execution_fixture(
    value: &str,
) -> Result<GitExportExecutionFixtureMode, CliError> {
    match value {
        "success" => Ok(GitExportExecutionFixtureMode::Success),
        "ref-update-failure" => Ok(GitExportExecutionFixtureMode::RefUpdateFailure),
        "export-map-failure" => Ok(GitExportExecutionFixtureMode::ExportMapFailure),
        _ => Err(
            invalid_request(format!("unknown git export execution fixture `{value}`"))
                .with_detail("execute_fixture", value),
        ),
    }
}

fn parse_view_resolve_options(ctx: &CommandContext) -> Result<ViewResolveOptions, CliError> {
    let mut fixture = None;
    let mut include = Vec::new();
    let mut base_checkpoint_id = None;
    let mut args = ctx.args.iter().skip(2);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun view resolve requires --fixture basic-app")
                })?;
                fixture = Some(value.clone());
            }
            "--include" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun view resolve requires --include topic:revision[,topic:revision]",
                    )
                })?;
                include.extend(parse_view_include(value)?);
            }
            "--base" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun view resolve --base <checkpoint>")
                })?;
                base_checkpoint_id = Some(value.clone());
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun view resolve"
                )));
            }
            value => {
                return Err(invalid_request(format!(
                    "unexpected view resolve argument `{value}`"
                )));
            }
        }
    }

    Ok(ViewResolveOptions {
        fixture,
        include,
        base_checkpoint_id,
    })
}

fn parse_view_include(value: &str) -> Result<Vec<TopicRevisionSelection>, CliError> {
    if value.trim().is_empty() {
        return Err(invalid_request(
            "usage: sun view resolve requires --include topic:revision[,topic:revision]",
        ));
    }

    value
        .split(',')
        .map(|selection| {
            let (topic_id, revision_id) = selection.split_once(':').ok_or_else(|| {
                invalid_request(
                    "view resolve include entries must use topic:revision fixture selectors",
                )
                .with_detail("selector", selection)
            })?;
            if topic_id.is_empty() || revision_id.is_empty() {
                return Err(invalid_request(
                    "view resolve include entries must use topic:revision fixture selectors",
                )
                .with_detail("selector", selection));
            }
            Ok(TopicRevisionSelection {
                topic_id: topic_id.to_string(),
                revision_id: revision_id.to_string(),
            })
        })
        .collect()
}

fn parse_execution_run_options(ctx: &CommandContext) -> Result<ExecutionRunOptions, CliError> {
    let mut fixture = None;
    let mut view_id = None;
    let mut cwd = ".".to_string();
    let mut integrity_fixture = None;
    let mut network_policy = None;
    let mut command_argv = Vec::new();
    let mut args = ctx.args.iter().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun run requires --fixture basic-app")
                })?;
                fixture = Some(value.clone());
            }
            "--view" => {
                let value = args
                    .next()
                    .ok_or_else(|| invalid_request("usage: sun run requires --view <view>"))?;
                view_id = Some(value.clone());
            }
            "--cwd" => {
                let value = args
                    .next()
                    .ok_or_else(|| invalid_request("usage: sun run --cwd <repo-relative-path>"))?;
                if fixture.is_some() && value != "." {
                    return Err(invalid_request("fixture execution supports only --cwd .")
                        .with_detail("cwd", value.clone()));
                }
                cwd = value.clone();
            }
            "--timeout" => {
                let value = args
                    .next()
                    .ok_or_else(|| invalid_request("usage: sun run --timeout <duration>"))?;
                if fixture.is_some() && value != "fixture" {
                    return Err(invalid_request(
                        "fixture execution accepts only --timeout fixture",
                    )
                    .with_detail("timeout", value.clone()));
                }
            }
            "--integrity-fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun run --integrity-fixture store-mismatch|scan-missing-blob|verified",
                    )
                })?;
                integrity_fixture = Some(parse_store_integrity_fixture(value)?);
            }
            "--network" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun run --network disabled|not_enforced")
                })?;
                network_policy = Some(ExecutionNetworkPolicy::parse(value)?);
            }
            "--" => {
                command_argv.extend(args.cloned());
                break;
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun run"
                )));
            }
            value => {
                return Err(invalid_request(format!(
                    "unexpected run argument `{value}`; put commands after --"
                )));
            }
        }
    }

    let view_id =
        view_id.ok_or_else(|| invalid_request("usage: sun run requires --view <view>"))?;
    if command_argv.is_empty() {
        return Err(invalid_request(
            "usage: sun run --view <view> [--fixture basic-app] -- <command> [args...]",
        ));
    }

    Ok(ExecutionRunOptions {
        fixture,
        view_id,
        command_argv,
        cwd,
        integrity_fixture,
        network_policy,
    })
}

fn parse_execution_promote_output_options(
    ctx: &CommandContext,
) -> Result<ExecutionPromoteOutputOptions, CliError> {
    let mut execution_id = None;
    let mut fixture = None;
    let mut path = None;
    let mut session_id = None;
    let mut classification = None;
    let mut topic = None;
    let mut args = ctx.args.iter().skip(2);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun execution promote-output requires --fixture basic-app",
                    )
                })?;
                fixture = Some(value.clone());
            }
            "--path" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun execution promote-output requires --path <path>")
                })?;
                path = Some(value.clone());
            }
            "--session" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun execution promote-output requires --session <session>",
                    )
                })?;
                session_id = Some(value.clone());
            }
            "--classification" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun execution promote-output requires --classification source_like_delta|generated_artifact",
                    )
                })?;
                classification = Some(value.clone());
            }
            "--topic" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun execution promote-output --topic <topic>")
                })?;
                if fixture.is_some() && value != FIXTURE_WRITE_TOPIC_ID {
                    return Err(CliError::new(
                        "promotion_topic_not_found",
                        "promotion target topic was not found",
                    )
                    .with_detail("topic_id", value.clone()));
                }
                topic = Some(value.clone());
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun execution promote-output"
                )));
            }
            value => {
                if execution_id.is_some() {
                    return Err(invalid_request(format!(
                        "unexpected execution promote-output argument `{value}`"
                    )));
                }
                execution_id = Some(value.to_string());
            }
        }
    }

    let execution_id = execution_id.ok_or_else(|| {
        invalid_request(
            "usage: sun execution promote-output <execution-id> --path <path> --session <session> --classification source_like_delta|generated_artifact --fixture basic-app",
        )
    })?;

    Ok(ExecutionPromoteOutputOptions {
        execution_id,
        fixture,
        path,
        session_id,
        classification,
        topic,
    })
}

fn parse_status_options(ctx: &CommandContext) -> Result<Option<StatusOptions>, CliError> {
    let mut fixture = None;
    let mut scope = StatusScope::Repository;
    let mut projection_root = None;
    let mut integrity_fixture = None;
    let mut promoted = false;
    let mut args = ctx.args.iter().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun status requires --fixture basic-app")
                })?;
                fixture = Some(value.clone());
            }
            "--session" => {
                let value = args
                    .next()
                    .ok_or_else(|| invalid_request("usage: sun status --session <session>"))?;
                scope = StatusScope::Session(value.clone());
            }
            "--topic" => {
                let value = args
                    .next()
                    .ok_or_else(|| invalid_request("usage: sun status --topic <topic>"))?;
                scope = StatusScope::Topic(value.clone());
            }
            "--view" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun status --view <resolved-view-id>")
                })?;
                scope = StatusScope::View(value.clone());
            }
            "--projection" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun status --projection <projection-id>")
                })?;
                scope = StatusScope::Projection(value.clone());
            }
            "--projection-root" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun status --projection-root <local-path>")
                })?;
                projection_root = Some(PathBuf::from(value));
            }
            "--integrity-fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun status --integrity-fixture store-mismatch|scan-missing-blob|verified",
                    )
                })?;
                integrity_fixture = Some(parse_store_integrity_fixture(value)?);
            }
            "--checkpoint" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun status --checkpoint <checkpoint>")
                })?;
                scope = StatusScope::Checkpoint(value.clone());
            }
            "--export-map" | "--export" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun status --export-map <export-map-id>")
                })?;
                scope = StatusScope::ExportMap(value.clone());
            }
            "--git" => {
                let value = args
                    .next()
                    .ok_or_else(|| invalid_request("usage: sun status --git <commit-or-ref>"))?;
                scope = StatusScope::Git(value.clone());
            }
            "--compat-import" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun status --compat-import <operation-id>")
                })?;
                scope = StatusScope::CompatImport(value.clone());
            }
            "--execution" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun status --execution <execution-id>")
                })?;
                scope = StatusScope::Execution(value.clone());
            }
            "--promoted" => {
                promoted = true;
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun status"
                )));
            }
            value => {
                return Err(invalid_request(format!(
                    "unexpected status argument `{value}`"
                )));
            }
        }
    }

    if integrity_fixture.is_some() && !matches!(scope, StatusScope::Projection(_)) {
        return Err(invalid_request(
            "sun status --integrity-fixture applies only with --projection",
        ));
    }
    if promoted && !matches!(scope, StatusScope::Execution(_)) {
        return Err(invalid_request(
            "sun status --promoted applies only with --execution",
        ));
    }

    Ok(fixture.map(|fixture| StatusOptions {
        fixture,
        scope,
        projection_root,
        integrity_fixture,
        promoted,
    }))
}

fn parse_inspect_options(ctx: &CommandContext) -> Result<Option<InspectOptions>, CliError> {
    let mut fixture = None;
    let mut session_id = None;
    let mut projection_root = None;
    let mut integrity_fixture = None;
    let mut promoted = false;
    let mut selectors = Vec::new();
    let mut args = ctx.args.iter().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun inspect requires --fixture basic-app")
                })?;
                fixture = Some(value.clone());
            }
            "--session" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun inspect requires --session <session>")
                })?;
                session_id = Some(value.clone());
            }
            "--projection-root" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun inspect --projection-root <local-path>")
                })?;
                projection_root = Some(PathBuf::from(value));
            }
            "--integrity-fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun inspect --integrity-fixture store-mismatch|scan-missing-blob|verified",
                    )
                })?;
                integrity_fixture = Some(parse_store_integrity_fixture(value)?);
            }
            "--promoted" => {
                promoted = true;
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun inspect"
                )));
            }
            value => selectors.push(value.to_string()),
        }
    }

    if fixture.is_none() {
        return Ok(None);
    }
    if selectors.len() != 1 {
        return Err(invalid_request(
            "usage: sun inspect <selector> --fixture basic-app [--session <session>]",
        ));
    }
    if integrity_fixture.is_some() && !selectors[0].starts_with("projection:") {
        return Err(invalid_request(
            "sun inspect --integrity-fixture applies only to projection selectors",
        ));
    }
    if promoted && !selectors[0].starts_with("execution:") {
        return Err(invalid_request(
            "sun inspect --promoted applies only to execution selectors",
        ));
    }

    Ok(Some(InspectOptions {
        fixture: fixture.unwrap(),
        selector: selectors.remove(0),
        session_id,
        projection_root,
        integrity_fixture,
        promoted,
    }))
}

fn parse_store_integrity_fixture(value: &str) -> Result<StoreIntegrityFixture, CliError> {
    match value {
        "scan-missing-blob" => Ok(StoreIntegrityFixture::ScanMissingBlob),
        "store-mismatch" => Ok(StoreIntegrityFixture::StoreMismatch),
        "verified" | "store-verified" => Ok(StoreIntegrityFixture::Verified),
        _ => Err(
            invalid_request(format!("unknown integrity fixture `{value}`"))
                .with_detail("integrity_fixture", value),
        ),
    }
}

fn ensure_fixture_session(session_id: &str) -> Result<(), CliError> {
    if session_id == FIXTURE_SESSION_ID {
        Ok(())
    } else {
        Err(CliError::new(
            "session_not_found",
            format!("session `{session_id}` was not found"),
        )
        .with_detail("session_id", session_id))
    }
}

fn ensure_fixture_topic(topic: &str) -> Result<(), CliError> {
    if topic == "auth-nullability" || topic == FIXTURE_WRITE_TOPIC_ID {
        Ok(())
    } else {
        Err(
            CliError::new("topic_not_found", format!("topic `{topic}` was not found"))
                .with_detail("topic", topic),
        )
    }
}

fn ensure_fixture_checkpoint(checkpoint_id: &str) -> Result<(), CliError> {
    if fixture_checkpoint().map(|checkpoint| checkpoint.id == checkpoint_id)? {
        Ok(())
    } else {
        Err(object_not_found("checkpoint", checkpoint_id))
    }
}

fn ensure_fixture_export_map(export_map_id: &str) -> Result<FixtureGitExport, CliError> {
    let export = fixture_git_export_response()?;
    if export.response.export_map.id == export_map_id {
        Ok(export)
    } else {
        Err(object_not_found("export_map", export_map_id))
    }
}

fn ensure_fixture_git_export_by_selector(selector: &str) -> Result<FixtureGitExport, CliError> {
    let export = fixture_git_export_response()?;
    let response = &export.response;
    if response.git_ref == selector
        || response
            .git_commit_ids
            .iter()
            .any(|commit_id| commit_id == selector)
    {
        Ok(export)
    } else {
        Err(object_not_found("git", selector))
    }
}

fn fixture_execution_by_id(execution_id: &str) -> Result<ExecutionRecord, CliError> {
    if execution_id != FIXTURE_PASSING_EXECUTION_ID {
        return Err(object_not_found("execution", execution_id));
    }
    let view = fixture_resolved_view(vec![fixture_auth_revision(), fixture_profile_revision()]);
    fixture_passing_execution_from_resolved_view(&view).map_err(execution_error)
}

fn fixture_execution_promotion_record(
    promoted: bool,
) -> Result<Option<ExecutionOutputPromotionRecord>, CliError> {
    if !promoted {
        return Ok(None);
    }

    let execution = fixture_execution_by_id(FIXTURE_PASSING_EXECUTION_ID)?;
    let candidate = fixture_promotion_candidate_provenance(&execution);
    let fixture_record = fixture_execution_output_promotion_record(&candidate);
    let response = fixture_promotion_mutation_response(&candidate)?;
    let response_record =
        execution_output_promotion_record_from_mutation_response(&candidate, &response);
    debug_assert_eq!(fixture_record, response_record);
    Ok(Some(response_record))
}

fn fixture_promotion_mutation_response(
    candidate: &PromotionCandidateProvenance,
) -> Result<MutationResponse, CliError> {
    let mut store = fixture_store("basic-app")?;
    let mut response = store
        .write(WriteRequest {
            session_id: FIXTURE_SESSION_ID.to_string(),
            path: candidate.output_path.clone(),
            expected_hash: ExpectedHash::New,
            content: fixture_promoted_generated_auth_bytes(),
            classification: "source".to_string(),
            executable: false,
            media_type: "text/typescript; charset=utf-8".to_string(),
        })
        .map_err(artifact_error)?;
    normalize_promotion_mutation_response(&mut response, candidate);
    Ok(response)
}

fn fixture_inspect(options: &InspectOptions, json: bool) -> Result<String, CliError> {
    let selector = options.selector.as_str();
    if let Some(session_id) = &options.session_id {
        ensure_fixture_session(session_id)?;
    }

    if let Some(operation_id) = selector.strip_prefix("operation:") {
        if operation_id == FIXTURE_COMPAT_IMPORT_OPERATION_ID {
            let response = fixture_compat_import_response_by_operation_id(operation_id)?;
            return Ok(if json {
                fixture_inspect_compat_operation_json(&response)
            } else {
                format!("operation {} compat_import", response.operation_id)
            });
        }
        if matches!(
            operation_id,
            "op_auth_trim_guard_0001"
                | "op_profile_auth_null_guard_0001"
                | "op_auth_move_0001"
                | "op_auth_delete_0001"
                | "op_auth_metadata_0001"
        ) {
            return Ok(if json {
                fixture_inspect_operation_json(operation_id)?
            } else {
                format!("operation {operation_id} patch src/auth.ts")
            });
        }
        return Err(object_not_found("operation", operation_id));
    }
    if let Some(repository_id) = selector.strip_prefix("repository:") {
        if repository_id != FIXTURE_REPOSITORY_ID {
            return Err(object_not_found("repository", repository_id));
        }
        return Ok(if json {
            fixture_inspect_repository_json()
        } else {
            format!("repository {FIXTURE_REPOSITORY_ID} initialized")
        });
    }
    if let Some(operation_id) = selector
        .strip_prefix("compat_import:")
        .or_else(|| selector.strip_prefix("compat-import:"))
    {
        let response = fixture_compat_import_response_by_operation_id(operation_id)?;
        return Ok(if json {
            fixture_inspect_compat_import_json(&response)
        } else {
            format!("compat_import {}", response.operation_id)
        });
    }
    if let Some(session_id) = selector.strip_prefix("session:") {
        ensure_fixture_session(session_id)?;
        return Ok(if json {
            fixture_inspect_session_json()
        } else {
            fixture_status_session_text()
        });
    }
    if let Some(revision_id) = selector.strip_prefix("revision:") {
        if revision_id == "rev_auth_nullability_0001" {
            return Ok(if json {
                fixture_inspect_revision_json()
            } else {
                "revision rev_auth_nullability_0001 patch src/auth.ts".to_string()
            });
        }
        return Err(object_not_found("revision", revision_id));
    }
    if let Some(topic) = selector.strip_prefix("topic:") {
        ensure_fixture_topic(topic)?;
        return Ok(if json {
            fixture_inspect_topic_json()
        } else {
            fixture_status_topic_text()
        });
    }
    if let Some(view_id) = selector.strip_prefix("view:") {
        let view = fixture_resolved_view_by_id(view_id)
            .ok_or_else(|| object_not_found("resolved_view", view_id))?;
        return Ok(if json {
            fixture_inspect_view_json(&view)
        } else {
            format!(
                "view {} {}",
                view.resolved_view_id,
                resolved_view_lifecycle_state(&view)
            )
        });
    }
    if let Some(conflict_id) = selector.strip_prefix("conflict:") {
        let record = fixture_resolver_record_by_id(conflict_id)
            .ok_or_else(|| object_not_found("conflict_staleness", conflict_id))?;
        return Ok(if json {
            fixture_inspect_conflict_json(&record)
        } else {
            format!("conflict {} {}", record.id, record.kind.as_str())
        });
    }
    if let Some(checkpoint_id) = selector.strip_prefix("checkpoint:") {
        ensure_fixture_checkpoint(checkpoint_id)?;
        return Ok(if json {
            fixture_inspect_checkpoint_json()?
        } else {
            format!("checkpoint {checkpoint_id}")
        });
    }
    if let Some(export_map_id) = selector
        .strip_prefix("export_map:")
        .or_else(|| selector.strip_prefix("export:"))
    {
        let export = ensure_fixture_export_map(export_map_id)?;
        return Ok(if json {
            fixture_inspect_export_map_json(&export)
        } else {
            format!("export_map {}", export.response.export_map.id)
        });
    }
    if let Some(git_selector) = selector.strip_prefix("git:") {
        let export = ensure_fixture_git_export_by_selector(git_selector)?;
        return Ok(if json {
            fixture_inspect_git_json(&export)
        } else {
            format!("git {}", export.response.export_map.id)
        });
    }
    if let Some(projection_id) = selector.strip_prefix("projection:") {
        let projection = fixture_projection_by_id(projection_id)
            .ok_or_else(|| object_not_found("projection", projection_id))?
            .map_err(projection_error)?;
        ensure_store_integrity_fixture_scope(&projection, options.integrity_fixture)?;
        return Ok(if json {
            fixture_inspect_projection_json(
                &projection,
                options.projection_root.as_deref(),
                options.integrity_fixture,
            )?
        } else {
            format!("projection {}", projection.id)
        });
    }
    if let Some(execution_id) = selector.strip_prefix("execution:") {
        let execution = fixture_execution_by_id(execution_id)?;
        let promotion = fixture_execution_promotion_record(options.promoted)?;
        return Ok(if json {
            fixture_inspect_execution_json(&execution, promotion.as_ref())
        } else {
            format!(
                "execution {} {} promotion_status={}",
                execution.id,
                execution.result.status.as_str(),
                fixture_execution_promotion_status(promotion.as_ref())
            )
        });
    }

    let session_id = options.session_id.as_deref().ok_or_else(|| {
        invalid_request("bare path inspect requires --session <session> for fixture views")
    })?;
    ensure_fixture_session(session_id)?;
    fixture_inspect_artifact(selector, json)
}

fn fixture_inspect_artifact(selector: &str, json: bool) -> Result<String, CliError> {
    let artifact = fixture_artifact_after_patch(selector)
        .or_else(|| fixture_artifact_after_patch_by_id(selector))
        .ok_or_else(|| {
            CliError::new("path_not_found", format!("path `{selector}` was not found"))
                .with_detail("path", selector)
                .with_detail("session_generation_id", "gen_agent_a_0002")
        })?;

    if json {
        Ok(fixture_inspect_artifact_json(artifact))
    } else {
        Ok(format!("{} {}", artifact.artifact_id, artifact.path))
    }
}

fn fixture_resolver_record_by_id(conflict_id: &str) -> Option<ResolverConflictOrStalenessRecord> {
    fixture_known_resolved_views()
        .into_iter()
        .flat_map(|view| view.records.into_iter())
        .find(|record| record.id == conflict_id)
}

fn object_not_found(kind: &'static str, selector: &str) -> CliError {
    CliError::new("object_not_found", "Sunlight object was not found")
        .with_detail("selector", selector)
        .with_detail("object_type", kind)
}

#[derive(Clone, Copy)]
struct FixtureArtifact {
    artifact_id: &'static str,
    path: &'static str,
    content_hash: &'static str,
    byte_length: usize,
    classification: &'static str,
    executable: bool,
    created_by_operation_id: &'static str,
    latest_operation_id: Option<&'static str>,
    before_hash: Option<&'static str>,
    after_hash: Option<&'static str>,
}

fn fixture_artifact_after_patch(path: &str) -> Option<FixtureArtifact> {
    match path {
        "README.md" => Some(FixtureArtifact {
            artifact_id: "artifact_readme_md",
            path: "README.md",
            content_hash: "sha256:readme_base",
            byte_length: 48,
            classification: "source",
            executable: false,
            created_by_operation_id: "op_import_base_0001",
            latest_operation_id: None,
            before_hash: None,
            after_hash: None,
        }),
        "docs/guide.md" => Some(FixtureArtifact {
            artifact_id: "artifact_docs_guide_md",
            path: "docs/guide.md",
            content_hash: "sha256:guide_base",
            byte_length: 25,
            classification: "source",
            executable: false,
            created_by_operation_id: "op_import_base_0001",
            latest_operation_id: None,
            before_hash: None,
            after_hash: None,
        }),
        "scripts/build.sh" => Some(FixtureArtifact {
            artifact_id: "artifact_scripts_build_sh",
            path: "scripts/build.sh",
            content_hash: "sha256:build_base",
            byte_length: 28,
            classification: "source",
            executable: true,
            created_by_operation_id: "op_import_base_0001",
            latest_operation_id: None,
            before_hash: None,
            after_hash: None,
        }),
        "src/auth.ts" => Some(FixtureArtifact {
            artifact_id: "artifact_src_auth_ts",
            path: "src/auth.ts",
            content_hash: "sha256:auth_trim_guard",
            byte_length: 103,
            classification: "source",
            executable: false,
            created_by_operation_id: "op_import_base_0001",
            latest_operation_id: Some("op_auth_trim_guard_0001"),
            before_hash: Some("sha256:auth_base"),
            after_hash: Some("sha256:auth_trim_guard"),
        }),
        "src/profile.ts" => Some(FixtureArtifact {
            artifact_id: "artifact_src_profile_ts",
            path: "src/profile.ts",
            content_hash: "sha256:profile_base",
            byte_length: 41,
            classification: "source",
            executable: false,
            created_by_operation_id: "op_import_base_0001",
            latest_operation_id: None,
            before_hash: None,
            after_hash: None,
        }),
        _ => None,
    }
}

fn fixture_artifact_after_patch_by_id(artifact_id: &str) -> Option<FixtureArtifact> {
    [
        "README.md",
        "docs/guide.md",
        "scripts/build.sh",
        "src/auth.ts",
        "src/profile.ts",
    ]
    .iter()
    .filter_map(|path| fixture_artifact_after_patch(path))
    .find(|artifact| artifact.artifact_id == artifact_id)
}

fn fixture_status_repository_text() -> String {
    format!(
        "repository {FIXTURE_REPOSITORY_ID}\ntopic auth-nullability rev_auth_nullability_0001\nsession {FIXTURE_SESSION_ID} gen_agent_a_0002"
    )
}

fn fixture_status_session_text() -> String {
    format!("{FIXTURE_SESSION_ID} gen_agent_a_0002 view_agent_a_after_patch_0001")
}

fn fixture_status_topic_text() -> String {
    "auth-nullability rev_auth_nullability_0001".to_string()
}

fn fixture_status_repository_json() -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"status.repository\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"base_checkpoint_id\":\"checkpoint_base_0001\"}},",
            "\"view\":null,",
            "\"repository\":{{",
            "\"initialized\":true,",
            "\"storage_schema_version\":{},",
            "\"path_policy_id\":\"{}\",",
            "\"operation_semantics_version\":\"{}\",",
            "\"git_interop_policy\":\"default_local_mvp\"",
            "}},",
            "\"topics\":[{}],",
            "\"sessions\":[{}],",
            "\"native_errors\":[],",
            "\"pending_work\":[]",
            "}},\"warnings\":[]}}"
        ),
        FIXTURE_REPOSITORY_ID,
        CURRENT_STORAGE_SCHEMA_VERSION,
        POSIX_CASE_SENSITIVE_PATH_POLICY_ID,
        FILE_OPERATION_SEMANTICS_VERSION,
        fixture_topic_summary_json(),
        fixture_session_summary_json(),
    )
}

fn fixture_inspect_repository_json() -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.repository\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"repository_id\":\"{}\"}},",
            "\"view\":null,",
            "\"repository\":{{",
            "\"id\":\"{}\",",
            "\"record_type\":\"repository\",",
            "\"lifecycle_state\":\"initialized\",",
            "\"initialized\":true,",
            "\"storage_schema_version\":{},",
            "\"path_policy_id\":\"{}\",",
            "\"path_policy\":{{",
            "\"policy_id\":\"{}\",",
            "\"case_sensitive\":true,",
            "\"separator\":\"/\"",
            "}},",
            "\"operation_semantics_version\":\"{}\",",
            "\"projection_policy\":{{",
            "\"default_strategy\":\"copy\",",
            "\"writable_default\":false,",
            "\"local_root_privacy\":\"local_only\"",
            "}},",
            "\"git_interop_policy\":\"default_local_mvp\",",
            "\"git_policy\":{{",
            "\"interop_policy\":\"default_local_mvp\",",
            "\"export_shape\":\"single_checkpoint_commit\",",
            "\"moving_refs_require_validation\":true",
            "}},",
            "\"base_checkpoint_refs\":[\"checkpoint_base_0001\"],",
            "\"storage_health\":{{",
            "\"status\":\"ok\",",
            "\"native_errors\":[],",
            "\"quarantine_refs\":[]",
            "}},",
            "\"privacy_export_defaults\":{{",
            "\"commit_default\":\"source\",",
            "\"local_only_paths_excluded\":true,",
            "\"generated_output_requires_promotion\":true",
            "}}",
            "}}",
            "}},\"warnings\":[]}}"
        ),
        FIXTURE_REPOSITORY_ID,
        FIXTURE_REPOSITORY_ID,
        FIXTURE_REPOSITORY_ID,
        CURRENT_STORAGE_SCHEMA_VERSION,
        POSIX_CASE_SENSITIVE_PATH_POLICY_ID,
        POSIX_CASE_SENSITIVE_PATH_POLICY_ID,
        FILE_OPERATION_SEMANTICS_VERSION,
    )
}

fn fixture_status_session_json() -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"status.session\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"session_id\":\"{}\",\"write_topic_id\":\"{}\"}},",
            "\"view\":{},",
            "\"session\":{{",
            "\"actor_id\":\"{}\",",
            "\"base_resolved_view_id\":\"{}\",",
            "\"write_topic_id\":\"{}\",",
            "\"capabilities\":[\"read\",\"list\",\"search\",\"inspect\",\"patch\",\"write\",\"move\",\"delete\",\"metadata\"]",
            "}},",
            "\"topic_head\":{{",
            "\"topic_id\":\"{}\",",
            "\"head_revision_id\":\"rev_auth_nullability_0001\",",
            "\"revision_number\":1",
            "}},",
            "\"changed_artifacts\":[{}],",
            "\"last_operation_id\":\"op_auth_trim_guard_0001\",",
            "\"compatibility_imports\":{}",
            "}},\"warnings\":[]}}"
        ),
        FIXTURE_REPOSITORY_ID,
        FIXTURE_SESSION_ID,
        FIXTURE_WRITE_TOPIC_ID,
        fixture_post_patch_view_json(),
        FIXTURE_ACTOR_ID,
        FIXTURE_RESOLVED_VIEW_ID,
        FIXTURE_WRITE_TOPIC_ID,
        FIXTURE_WRITE_TOPIC_ID,
        fixture_changed_artifact_json(),
        fixture_session_compatibility_imports_json(),
    )
}

fn fixture_session_compatibility_imports_json() -> String {
    let projection = fixture_compat_import_projection_by_id(FIXTURE_COMPATIBILITY_PROJECTION_ID)
        .expect("fixture compatibility projection should exist")
        .expect("fixture compatibility projection should validate");
    let import = fixture_compat_import_response_by_operation_id(FIXTURE_COMPAT_IMPORT_OPERATION_ID)
        .expect("fixture compatibility import should exist");
    let candidates = fixture_basic_app_candidate_deltas();

    format!(
        concat!(
            "{{",
            "\"recent_projections\":[{{",
            "\"projection_id\":\"{}\",",
            "\"purpose\":\"{}\",",
            "\"baseline\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"selected_candidate_delta_ids\":{},",
            "\"candidate_summary\":{{",
            "\"candidate_counts\":{},",
            "\"quarantine_refs\":{}",
            "}}",
            "}}],",
            "\"last_import\":{{",
            "\"compat_import_operation_id\":\"{}\",",
            "\"operation_transaction_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"candidate_delta_ids\":{}",
            "}}",
            "}}"
        ),
        json_escape(&projection.id),
        projection.purpose.as_str(),
        json_escape(&projection.resolved_view_id),
        single_repo_tree_json(&projection.tree_identity),
        compat_projection_selected_candidate_ids_json(),
        compat_candidate_counts_json(&candidates),
        compat_quarantine_refs_json(&candidates),
        json_escape(&import.operation_id),
        json_escape(&import.operation_id),
        json_escape(&import.projection_id),
        json_escape(&import.session_generation_id),
        json_escape(&import.resolved_view_id),
        compat_import_candidate_delta_ids_json(&import),
    )
}

fn projection_quarantine_cleanup_success_envelope(
    cleanup: &ProjectionQuarantineLocalCleanup,
) -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"projection.quarantine_cleanup\",",
            "\"cleanup\":{{",
            "\"projection_id\":\"{}\",",
            "\"quarantine_dir\":{{\"path\":\"{}\",\"local_only\":true}},",
            "\"existed\":{},",
            "\"removed_records\":{},",
            "\"removed_dirs\":{},",
            "\"local_only\":{},",
            "\"retention_state_after\":\"{}\"",
            "}}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&cleanup.projection_id),
        json_escape(&cleanup.quarantine_dir.display().to_string()),
        cleanup.existed,
        cleanup_paths_json(cleanup, &cleanup.removed_files),
        cleanup_paths_json(cleanup, &cleanup.removed_dirs),
        cleanup.local_only,
        cleanup.retention_state_after.as_str(),
    )
}

fn cleanup_paths_json(cleanup: &ProjectionQuarantineLocalCleanup, paths: &[PathBuf]) -> String {
    let root = cleanup
        .quarantine_dir
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .and_then(|path| path.parent());
    let values = paths
        .iter()
        .map(|path| {
            let value = root
                .and_then(|root| path.strip_prefix(root).ok())
                .map(|relative| {
                    format!(
                        "local://{}",
                        relative.display().to_string().replace('\\', "/")
                    )
                })
                .unwrap_or_else(|| path.display().to_string());
            format!("\"{}\"", json_escape(&value))
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{values}]")
}

fn fixture_status_topic_json() -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"status.topic\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"topic_id\":\"{}\",\"head_revision_id\":\"rev_auth_nullability_0001\"}},",
            "\"view\":null,",
            "\"topic\":{{",
            "\"slug\":\"auth-nullability\",",
            "\"display_name\":\"Auth nullability\",",
            "\"status\":\"open\",",
            "\"owner_actor_id\":\"{}\",",
            "\"base_checkpoint_id\":\"checkpoint_base_0001\",",
            "\"revision_count\":1",
            "}},",
            "\"head\":{{",
            "\"topic_revision_id\":\"rev_auth_nullability_0001\",",
            "\"revision_number\":1,",
            "\"operation_transaction_id\":\"op_auth_trim_guard_0001\",",
            "\"parent_revision_id\":null",
            "}},",
            "\"changed_artifacts\":[{}],",
            "\"sessions\":[{{",
            "\"session_id\":\"{}\",",
            "\"session_generation_id\":\"gen_agent_a_0002\",",
            "\"resolved_view_id\":\"view_agent_a_after_patch_0001\"",
            "}}]",
            "}},\"warnings\":[]}}"
        ),
        FIXTURE_REPOSITORY_ID,
        FIXTURE_WRITE_TOPIC_ID,
        FIXTURE_ACTOR_ID,
        fixture_changed_artifact_json(),
        FIXTURE_SESSION_ID,
    )
}

fn fixture_status_projection_json(
    projection: &ProjectionRecord,
    projection_root: Option<&std::path::Path>,
    integrity_fixture: Option<StoreIntegrityFixture>,
) -> Result<String, CliError> {
    ensure_store_integrity_fixture_scope(projection, integrity_fixture)?;
    let lifecycle_state = match integrity_fixture {
        Some(StoreIntegrityFixture::ScanMissingBlob) => "quarantined",
        Some(StoreIntegrityFixture::StoreMismatch) => "quarantined",
        Some(StoreIntegrityFixture::Verified) | None => {
            projection_lifecycle_state(projection, projection_root)
        }
    };
    let manifest = fixture_local_projection_manifest(projection)?;
    let verification = local_projection_root_verification(projection, &manifest, projection_root);
    let verification_json =
        local_projection_root_verification_json_from_verification(projection_root, &verification);
    let store_integrity =
        fixture_projection_store_integrity_result(projection, &manifest, integrity_fixture);
    persist_projection_quarantine_record_if_available(projection_root, &store_integrity)?;
    let integrity_status = store_integrity.integrity_status.as_str();
    let retention_state = store_integrity
        .quarantine
        .as_ref()
        .map(|quarantine| quarantine.state.as_str())
        .unwrap_or_else(|| projection.retention_state.as_str());
    let quarantine_json = projection_quarantine_json(&store_integrity);
    let native_errors_json =
        projection_native_errors_json(projection, &manifest, &store_integrity, integrity_fixture);
    Ok(format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"status.projection\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"projection_id\":\"{}\",\"resolved_view_id\":\"{}\"}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"projection\":{{",
            "\"lifecycle_state\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"purpose\":\"{}\",",
            "\"strategy\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{},",
            "\"retention_state\":\"{}\",",
            "\"integrity_status\":\"{}\",",
            "\"root_ref\":{},",
            "\"cache_key\":\"{}\",",
            "\"local_projection_manifest\":{},",
            "\"local_store_integrity\":{},",
            "\"quarantine\":{},",
            "\"dirty_local\":{},",
            "\"local_root_verification\":{}{}",
            "}},",
            "\"native_errors\":{}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&projection.repository_id),
        json_escape(&projection.id),
        json_escape(&projection.resolved_view_id),
        json_escape(&projection.resolved_view_id),
        single_repo_tree_json(&projection.tree_identity),
        lifecycle_state,
        json_escape(&projection.id),
        projection.purpose.as_str(),
        projection.strategy.as_str(),
        json_escape(&projection.resolved_view_id),
        single_repo_tree_json(&projection.tree_identity),
        retention_state,
        integrity_status,
        projection_root_ref_json(projection),
        json_escape(&projection.cache_key.stable_string()),
        local_projection_manifest_json(&manifest),
        projection_local_store_integrity_json(&store_integrity),
        quarantine_json,
        verification.dirty_local_json(),
        verification_json,
        compat_projection_status_extension_json(projection),
        native_errors_json,
    ))
}

fn fixture_status_checkpoint_json() -> Result<String, CliError> {
    let checkpoint = fixture_checkpoint()?;
    Ok(format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"status.checkpoint\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"checkpoint_id\":\"{}\",\"resolved_view_id\":\"{}\"}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"topic_frontier\":{},",
            "\"tree_identity\":{}",
            "}},",
            "\"checkpoint\":{},",
            "\"conflict_free\":{},",
            "\"evidence_ready\":{},",
            "\"export_ready\":{},",
            "\"validation_report\":null,",
            "\"export_refs\":{}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&checkpoint.repository_id),
        json_escape(&checkpoint.id),
        json_escape(&checkpoint.resolved_view_id),
        json_escape(&checkpoint.resolved_view_id),
        checkpoint_topic_frontier_json(&checkpoint),
        single_repo_tree_json(&checkpoint.tree_identity),
        checkpoint_json(&checkpoint),
        checkpoint.conflict_free,
        !checkpoint.evidence_refs.is_empty(),
        checkpoint.conflict_free,
        export_refs_json(&checkpoint),
    ))
}

fn fixture_status_export_map_json(export: &FixtureGitExport) -> String {
    let response = &export.response;
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"status.export_map\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"export_map_id\":\"{}\",",
            "\"checkpoint_id\":\"{}\",",
            "\"validation_report_id\":\"{}\"",
            "}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"git_export\":{{",
            "\"lifecycle_state\":\"exported\",",
            "\"export_map_id\":\"{}\",",
            "\"checkpoint_id\":\"{}\",",
            "\"validation_report_id\":\"{}\",",
            "\"git_ref\":\"{}\",",
            "\"git_commit_ids\":{},",
            "\"partial_failure_marker\":null",
            "}},",
            "\"validation_report\":{},",
            "\"export_map\":{}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&response.export_map.repository_id),
        json_escape(&response.export_map.id),
        json_escape(&response.checkpoint_id),
        json_escape(&response.validation_report.id),
        json_escape(&export.checkpoint.resolved_view_id),
        single_repo_tree_json(&response.export_map.tree_identity),
        json_escape(&response.export_map.id),
        json_escape(&response.checkpoint_id),
        json_escape(&response.validation_report.id),
        json_escape(&response.git_ref),
        string_array_json(response.git_commit_ids.iter().map(String::as_str)),
        git_export_validation_report_json(&response.validation_report),
        git_export_map_json(&response.export_map),
    )
}

fn fixture_status_git_json(export: &FixtureGitExport) -> String {
    let response = &export.response;
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"status.git\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"git_ref\":\"{}\",",
            "\"export_map_id\":\"{}\",",
            "\"checkpoint_id\":\"{}\",",
            "\"validation_report_id\":\"{}\"",
            "}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"git_export\":{{",
            "\"lifecycle_state\":\"exported\",",
            "\"mapping_state\":\"resolved\",",
            "\"export_map_id\":\"{}\",",
            "\"checkpoint_id\":\"{}\",",
            "\"validation_report_id\":\"{}\",",
            "\"git_ref\":\"{}\",",
            "\"git_commit_ids\":{},",
            "\"partial_failure_marker\":null",
            "}},",
            "\"validation_report\":{},",
            "\"export_map\":{}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&response.export_map.repository_id),
        json_escape(&response.git_ref),
        json_escape(&response.export_map.id),
        json_escape(&response.checkpoint_id),
        json_escape(&response.validation_report.id),
        json_escape(&export.checkpoint.resolved_view_id),
        single_repo_tree_json(&response.export_map.tree_identity),
        json_escape(&response.export_map.id),
        json_escape(&response.checkpoint_id),
        json_escape(&response.validation_report.id),
        json_escape(&response.git_ref),
        string_array_json(response.git_commit_ids.iter().map(String::as_str)),
        git_export_validation_report_json(&response.validation_report),
        git_export_map_json(&response.export_map),
    )
}

fn fixture_inspect_artifact_json(artifact: FixtureArtifact) -> String {
    let provenance = if let Some(operation_id) = artifact.latest_operation_id {
        format!(
            concat!(
                "{{",
                "\"latest_operation_id\":\"{}\",",
                "\"topic_id\":\"{}\",",
                "\"topic_revision_id\":\"rev_auth_nullability_0001\",",
                "\"session_id\":\"{}\",",
                "\"session_generation_id\":\"gen_agent_a_0002\"",
                "}}"
            ),
            operation_id, FIXTURE_WRITE_TOPIC_ID, FIXTURE_SESSION_ID
        )
    } else {
        "null".to_string()
    };
    let before_refs = artifact
        .before_hash
        .map(|hash| {
            format!(
                "[{{\"operation_transaction_id\":\"op_auth_trim_guard_0001\",\"content_hash\":\"{}\",\"tree_hash\":\"{}\"}}]",
                hash, FIXTURE_TREE_HASH
            )
        })
        .unwrap_or_else(|| "[]".to_string());
    let after_refs = artifact
        .after_hash
        .map(|hash| {
            format!(
                "[{{\"operation_transaction_id\":\"op_auth_trim_guard_0001\",\"content_hash\":\"{}\",\"tree_hash\":\"tree_after_auth_patch_0001\"}}]",
                hash
            )
        })
        .unwrap_or_else(|| "[]".to_string());
    let checkpoint_export_trace = fixture_artifact_checkpoint_export_trace_json(artifact);

    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.artifact\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"session_id\":\"{}\",\"artifact_id\":\"{}\"}},",
            "\"view\":{},",
            "\"artifact\":{{",
            "\"artifact_id\":\"{}\",",
            "\"artifact_kind\":\"file\",",
            "\"path\":\"{}\",",
            "\"path_state\":\"active\",",
            "\"content_hash\":\"{}\",",
            "\"byte_length\":{},",
            "\"classification\":\"{}\",",
            "\"executable\":{},",
            "\"created_by_operation_id\":\"{}\"",
            "}},",
            "\"path_history\":[{{",
            "\"path\":\"{}\",",
            "\"state\":\"active\",",
            "\"introduced_by_operation_id\":\"op_import_base_0001\"",
            "}}],",
            "\"provenance\":{},",
            "\"compatibility_import\":{},",
            "\"before_refs\":{},",
            "\"after_refs\":{},",
            "\"checkpoint_export_trace\":{}",
            "}},\"warnings\":[]}}"
        ),
        FIXTURE_REPOSITORY_ID,
        FIXTURE_SESSION_ID,
        artifact.artifact_id,
        fixture_post_patch_view_json(),
        artifact.artifact_id,
        json_escape(artifact.path),
        artifact.content_hash,
        artifact.byte_length,
        artifact.classification,
        artifact.executable,
        artifact.created_by_operation_id,
        json_escape(artifact.path),
        provenance,
        fixture_artifact_compat_import_provenance_json(artifact),
        before_refs,
        after_refs,
        checkpoint_export_trace,
    )
}

fn fixture_artifact_compat_import_provenance_json(artifact: FixtureArtifact) -> String {
    if artifact.artifact_id != "artifact_src_auth_ts" || artifact.path != "src/auth.ts" {
        return "null".to_string();
    }

    let response =
        fixture_compat_import_response_by_operation_id(FIXTURE_COMPAT_IMPORT_OPERATION_ID)
            .expect("fixture compatibility import should exist");
    let imported_artifact = response
        .imported_artifacts
        .iter()
        .find(|imported_artifact| {
            imported_artifact.artifact_id == artifact.artifact_id
                && imported_artifact.path == artifact.path
        })
        .expect("fixture compatibility import should include artifact_src_auth_ts");

    format!(
        concat!(
            "{{",
            "\"kind\":\"compat_import\",",
            "\"operation_transaction_id\":\"{}\",",
            "\"topic_revision_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"candidate_delta_ids\":{},",
            "\"session_generation_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"imported_artifact\":{}",
            "}}"
        ),
        json_escape(&response.operation_id),
        json_escape(&response.topic_revision_id),
        json_escape(&response.projection_id),
        compat_import_candidate_delta_ids_json(&response),
        json_escape(&response.session_generation_id),
        json_escape(&response.resolved_view_id),
        compat_imported_artifact_json(imported_artifact),
    )
}

fn fixture_artifact_checkpoint_export_trace_json(artifact: FixtureArtifact) -> String {
    if artifact.latest_operation_id != Some("op_auth_trim_guard_0001") {
        return "null".to_string();
    }

    format!(
        concat!(
            "{{",
            "\"operation_id\":\"{}\",",
            "\"topic_revision_id\":\"rev_auth_nullability_0001\",",
            "\"resolved_view_id\":\"{}\",",
            "\"execution_evidence_id\":\"{}\",",
            "\"execution_result\":\"pass\",",
            "\"checkpoint_id\":\"{}\",",
            "\"export_map_id\":\"{}\",",
            "\"git_ref\":\"{}\",",
            "\"git_commit_ids\":[\"{}\"]",
            "}}"
        ),
        artifact.latest_operation_id.unwrap(),
        FIXTURE_RESOLVED_VIEW_ID,
        FIXTURE_PASSING_EXECUTION_ID,
        FIXTURE_CHECKPOINT_ID,
        FIXTURE_EXPORT_MAP_ID,
        FIXTURE_EXPORTED_GIT_REF,
        FIXTURE_GIT_COMMIT_ID,
    )
}

fn fixture_inspect_checkpoint_json() -> Result<String, CliError> {
    let checkpoint = fixture_checkpoint()?;
    Ok(format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.checkpoint\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"checkpoint_id\":\"{}\",\"resolved_view_id\":\"{}\"}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"topic_frontier\":{},",
            "\"tree_identity\":{}",
            "}},",
            "\"checkpoint\":{}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&checkpoint.repository_id),
        json_escape(&checkpoint.id),
        json_escape(&checkpoint.resolved_view_id),
        json_escape(&checkpoint.resolved_view_id),
        checkpoint_topic_frontier_json(&checkpoint),
        single_repo_tree_json(&checkpoint.tree_identity),
        checkpoint_json(&checkpoint),
    ))
}

fn fixture_inspect_export_map_json(export: &FixtureGitExport) -> String {
    let response = &export.response;
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.export_map\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"export_map_id\":\"{}\",",
            "\"checkpoint_id\":\"{}\",",
            "\"validation_report_id\":\"{}\"",
            "}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"export_map\":{},",
            "\"validation_report\":{}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&response.export_map.repository_id),
        json_escape(&response.export_map.id),
        json_escape(&response.checkpoint_id),
        json_escape(&response.validation_report.id),
        json_escape(&export.checkpoint.resolved_view_id),
        single_repo_tree_json(&response.export_map.tree_identity),
        git_export_map_json(&response.export_map),
        git_export_validation_report_json(&response.validation_report),
    )
}

fn fixture_inspect_git_json(export: &FixtureGitExport) -> String {
    let response = &export.response;
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.git\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"git_ref\":\"{}\",",
            "\"export_map_id\":\"{}\",",
            "\"checkpoint_id\":\"{}\",",
            "\"validation_report_id\":\"{}\"",
            "}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"git_mapping\":{{",
            "\"git_ref\":\"{}\",",
            "\"git_commit_ids\":{},",
            "\"export_map_id\":\"{}\",",
            "\"checkpoint_id\":\"{}\"",
            "}},",
            "\"export_map\":{},",
            "\"validation_report\":{}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&response.export_map.repository_id),
        json_escape(&response.git_ref),
        json_escape(&response.export_map.id),
        json_escape(&response.checkpoint_id),
        json_escape(&response.validation_report.id),
        json_escape(&export.checkpoint.resolved_view_id),
        single_repo_tree_json(&response.export_map.tree_identity),
        json_escape(&response.git_ref),
        string_array_json(response.git_commit_ids.iter().map(String::as_str)),
        json_escape(&response.export_map.id),
        json_escape(&response.checkpoint_id),
        git_export_map_json(&response.export_map),
        git_export_validation_report_json(&response.validation_report),
    )
}

fn fixture_inspect_operation_json(operation_id: &str) -> Result<String, CliError> {
    if let Some(response) = fixture_structural_mutation_response_by_operation_id(operation_id)? {
        return Ok(fixture_inspect_mutation_operation_json(&response));
    }

    let (topic_id, topic_revision_id, actor_id, authored_context_id, after_hash) =
        match operation_id {
            "op_auth_trim_guard_0001" => (
                FIXTURE_WRITE_TOPIC_ID,
                "rev_auth_nullability_0001",
                FIXTURE_ACTOR_ID,
                "ctx_agent_a_gen_0001",
                "sha256:auth_trim_guard",
            ),
            "op_profile_auth_null_guard_0001" => (
                "topic_profile_ui",
                "rev_profile_auth_overlap_0001",
                "agent_b",
                "ctx_agent_b_gen_0001",
                "sha256:auth_null_guard",
            ),
            _ => return Err(object_not_found("operation", operation_id)),
        };

    Ok(format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.operation\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"operation_transaction_id\":\"{}\",",
            "\"topic_id\":\"{}\",",
            "\"session_id\":\"{}\",",
            "\"topic_revision_id\":\"{}\"",
            "}},",
            "\"view\":{},",
            "\"operation\":{{",
            "\"mutation\":\"patch\",",
            "\"actor_id\":\"{}\",",
            "\"authored_context_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"classification\":\"source\",",
            "\"privacy_class\":\"policy_gated\",",
            "\"preconditions\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"expected_path\":\"src/auth.ts\",",
            "\"expected_hash\":\"sha256:auth_base\"",
            "}},",
            "\"write_set\":[{{\"artifact_id\":\"artifact_src_auth_ts\",\"path\":\"src/auth.ts\",\"mutation\":\"patch\"}}],",
            "\"before_refs\":{{\"content_hash\":\"sha256:auth_base\",\"tree_hash\":\"{}\"}},",
            "\"after_refs\":{{\"content_hash\":\"{}\",\"tree_hash\":\"tree_after_auth_patch_0001\"}}",
            "}},",
            "\"created_revision\":{{",
            "\"topic_revision_id\":\"{}\",",
            "\"revision_number\":1,",
            "\"parent_revision_id\":null",
            "}},",
            "\"resolver_impacts\":{}",
            "}},\"warnings\":[]}}"
        ),
        FIXTURE_REPOSITORY_ID,
        operation_id,
        topic_id,
        FIXTURE_SESSION_ID,
        topic_revision_id,
        fixture_base_view_json(),
        actor_id,
        authored_context_id,
        FIXTURE_SESSION_GENERATION_ID,
        FIXTURE_RESOLVED_VIEW_ID,
        FIXTURE_SESSION_GENERATION_ID,
        FIXTURE_TREE_HASH,
        after_hash,
        topic_revision_id,
        operation_resolver_impacts_json(operation_id),
    ))
}

fn fixture_structural_mutation_response_by_operation_id(
    operation_id: &str,
) -> Result<Option<MutationResponse>, CliError> {
    let mut store = InMemoryArtifactStore::fixture_basic_app();
    let response = match operation_id {
        "op_auth_move_0001" => store.move_path(MoveRequest {
            session_id: FIXTURE_SESSION_ID.to_string(),
            source_path: "src/auth.ts".to_string(),
            target_path: "src/auth.renamed.ts".to_string(),
            expected_hash: "sha256:auth_base".to_string(),
        }),
        "op_auth_delete_0001" => store.delete_path(DeleteRequest {
            session_id: FIXTURE_SESSION_ID.to_string(),
            path: "src/auth.ts".to_string(),
            expected_hash: "sha256:auth_base".to_string(),
        }),
        "op_auth_metadata_0001" => store.metadata_set(MetadataSetRequest {
            session_id: FIXTURE_SESSION_ID.to_string(),
            path: "src/auth.ts".to_string(),
            expected_hash: "sha256:auth_base".to_string(),
            classification: "generated".to_string(),
        }),
        _ => return Ok(None),
    }
    .map_err(artifact_error)?;

    Ok(Some(response))
}

fn fixture_inspect_mutation_operation_json(response: &MutationResponse) -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.operation\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"operation_transaction_id\":\"{}\",",
            "\"topic_id\":\"{}\",",
            "\"session_id\":\"{}\",",
            "\"topic_revision_id\":\"{}\",",
            "\"session_generation_id\":\"{}\"",
            "}},",
            "\"view\":{},",
            "\"operation\":{},",
            "\"created_revision\":{},",
            "\"session_generation\":{}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&response.repository_id),
        json_escape(&response.operation.id),
        json_escape(&response.operation.topic_id),
        json_escape(&response.operation.session_id),
        json_escape(&response.topic_revision.id),
        json_escape(&response.session_generation.id),
        view_json(&response.view),
        operation_json(response),
        topic_revision_json(response),
        session_generation_json(response),
    )
}

fn operation_resolver_impacts_json(operation_id: &str) -> String {
    let records = fixture_known_resolved_views()
        .into_iter()
        .flat_map(|view| view.records.into_iter())
        .filter(|record| record.operation_ids.iter().any(|id| id == operation_id))
        .collect::<Vec<_>>();

    format!(
        concat!(
            "{{",
            "\"conflict_ids\":{},",
            "\"staleness_ids\":{},",
            "\"records\":[{}]",
            "}}"
        ),
        string_array_json(
            records
                .iter()
                .filter(|record| record.kind.is_conflict())
                .map(|record| record.id.as_str())
        ),
        string_array_json(
            records
                .iter()
                .filter(|record| record.kind.is_staleness())
                .map(|record| record.id.as_str())
        ),
        records
            .iter()
            .map(resolver_record_json)
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn fixture_status_view_json(view: &ResolvedViewResult) -> String {
    let conflict_ids = view
        .conflicts()
        .map(|record| record.id.as_str())
        .collect::<Vec<_>>();
    let staleness_ids = view
        .staleness()
        .map(|record| record.id.as_str())
        .collect::<Vec<_>>();

    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"status.view\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"resolved_view_id\":\"{}\"}},",
            "\"view\":{},",
            "\"resolved_view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"lifecycle_state\":\"{}\",",
            "\"base_checkpoint_ids\":{},",
            "\"topic_frontier\":{},",
            "\"dependency_closure\":{},",
            "\"resolver_order\":{},",
            "\"conflict_count\":{},",
            "\"staleness_count\":{},",
            "\"conflict_ids\":{},",
            "\"staleness_ids\":{},",
            "\"tree_identity\":{},",
            "\"missing_tree_reason\":{}",
            "}}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&view.repository_id),
        json_escape(&view.resolved_view_id),
        view_resolve_view_json(view),
        json_escape(&view.resolved_view_id),
        json_escape(&view.repository_id),
        resolved_view_lifecycle_state(view),
        string_array_json(view.base_checkpoint_ids.iter().map(String::as_str)),
        topic_frontier_json(view),
        dependency_closure_json(&view.dependency_closure),
        resolver_order_json(&view.resolver_order),
        conflict_ids.len(),
        staleness_ids.len(),
        string_array_json(conflict_ids.iter().copied()),
        string_array_json(staleness_ids.iter().copied()),
        optional_single_repo_tree_json(view.tree_identity.as_ref()),
        optional_string_json(resolved_view_missing_tree_reason(view)),
    )
}

fn fixture_inspect_view_json(view: &ResolvedViewResult) -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.view\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"resolved_view_id\":\"{}\"}},",
            "\"view\":{},",
            "\"resolved_view\":{},",
            "\"resolver_inputs\":{{",
            "\"base_checkpoint_ids\":{},",
            "\"topic_frontier\":{},",
            "\"dependency_closure\":{},",
            "\"resolver_order\":{}",
            "}},",
            "\"conflict_refs\":[{}],",
            "\"staleness_refs\":[{}],",
            "\"tree_identity\":{},",
            "\"missing_tree_reason\":{},",
            "\"lifecycle_state\":\"{}\"",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&view.repository_id),
        json_escape(&view.resolved_view_id),
        view_resolve_view_json(view),
        resolved_view_record_json(view),
        string_array_json(view.base_checkpoint_ids.iter().map(String::as_str)),
        topic_frontier_json(view),
        dependency_closure_json(&view.dependency_closure),
        resolver_order_json(&view.resolver_order),
        view.conflicts()
            .map(resolver_ref_json)
            .collect::<Vec<_>>()
            .join(","),
        view.staleness()
            .map(resolver_ref_json)
            .collect::<Vec<_>>()
            .join(","),
        optional_single_repo_tree_json(view.tree_identity.as_ref()),
        optional_string_json(resolved_view_missing_tree_reason(view)),
        resolved_view_lifecycle_state(view),
    )
}

fn fixture_inspect_conflict_json(record: &ResolverConflictOrStalenessRecord) -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.conflict\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"conflict_id\":\"{}\",\"resolved_view_id\":\"{}\"}},",
            "\"view\":null,",
            "\"conflict_staleness\":{},",
            "\"competing_operation_ids\":{},",
            "\"path_refs\":[{}],",
            "\"artifact_ids\":{},",
            "\"authored_context_ids\":{},",
            "\"policy_reason\":\"{}\"",
            "}},\"warnings\":[]}}"
        ),
        FIXTURE_REPOSITORY_ID,
        json_escape(&record.id),
        json_escape(&record.resolved_view_id),
        resolver_record_json(record),
        string_array_json(record.operation_ids.iter().map(String::as_str)),
        record
            .path_refs
            .iter()
            .map(|path_ref| {
                format!(
                    "{{\"path\":\"{}\",\"path_state\":\"{}\"}}",
                    json_escape(&path_ref.path),
                    json_escape(&path_ref.path_state)
                )
            })
            .collect::<Vec<_>>()
            .join(","),
        string_array_json(record.artifact_ids.iter().map(String::as_str)),
        string_array_json(record.authored_context_ids.iter().map(String::as_str)),
        json_escape(&record.policy_reason),
    )
}

fn resolved_view_record_json(view: &ResolvedViewResult) -> String {
    let conflict_ids = view
        .conflicts()
        .map(|record| record.id.as_str())
        .collect::<Vec<_>>();
    let staleness_ids = view
        .staleness()
        .map(|record| record.id.as_str())
        .collect::<Vec<_>>();

    format!(
        concat!(
            "{{",
            "\"schema_version\":1,",
            "\"record_type\":\"resolved_view\",",
            "\"id\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"base_checkpoint_ids\":{},",
            "\"topic_frontier\":{},",
            "\"dependency_closure\":{},",
            "\"operation_semantics_version\":\"{}\",",
            "\"path_policy_id\":\"{}\",",
            "\"resolver_order\":{},",
            "\"conflict_ids\":{},",
            "\"staleness_ids\":{},",
            "\"tree_identity\":{},",
            "\"lifecycle_state\":\"{}\"",
            "}}"
        ),
        json_escape(&view.resolved_view_id),
        json_escape(&view.repository_id),
        string_array_json(view.base_checkpoint_ids.iter().map(String::as_str)),
        topic_frontier_json(view),
        dependency_closure_json(&view.dependency_closure),
        json_escape(&view.operation_semantics_version),
        json_escape(&view.path_policy_id),
        resolver_order_json(&view.resolver_order),
        string_array_json(conflict_ids.iter().copied()),
        string_array_json(staleness_ids.iter().copied()),
        optional_single_repo_tree_json(view.tree_identity.as_ref()),
        resolved_view_lifecycle_state(view),
    )
}

fn resolver_ref_json(record: &ResolverConflictOrStalenessRecord) -> String {
    format!(
        "{{\"id\":\"{}\",\"kind\":\"{}\"}}",
        json_escape(&record.id),
        record.kind.as_str(),
    )
}

fn resolved_view_lifecycle_state(view: &ResolvedViewResult) -> &'static str {
    if view.conflicts().next().is_some() {
        "conflicted"
    } else if view.staleness().next().is_some() {
        "stale"
    } else if view.tree_identity.is_none() {
        "missing_tree"
    } else {
        "resolved"
    }
}

fn resolved_view_missing_tree_reason(view: &ResolvedViewResult) -> Option<&'static str> {
    if view.tree_identity.is_some() {
        None
    } else if view.conflicts().next().is_some() {
        Some("blocked_by_conflict")
    } else if view.staleness().next().is_some() {
        Some("blocked_by_staleness")
    } else {
        Some("tree_identity_unavailable")
    }
}

fn fixture_inspect_session_json() -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.session\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"session_id\":\"{}\",\"write_topic_id\":\"{}\",\"session_generation_id\":\"gen_agent_a_0002\"}},",
            "\"view\":{},",
            "\"session\":{{",
            "\"actor_id\":\"{}\",",
            "\"base_resolved_view_id\":\"{}\",",
            "\"write_topic_id\":\"{}\",",
            "\"current_generation_number\":2,",
            "\"created_by\":{{\"kind\":\"session_start\",\"id\":\"{}\"}}",
            "}},",
            "\"generations\":[",
            "{{\"session_generation_id\":\"{}\",\"generation_number\":1,\"resolved_view_id\":\"{}\",\"created_by\":{{\"kind\":\"session_start\",\"id\":\"{}\"}}}},",
            "{{\"session_generation_id\":\"gen_agent_a_0002\",\"generation_number\":2,\"resolved_view_id\":\"view_agent_a_after_patch_0001\",\"created_by\":{{\"kind\":\"operation_transaction\",\"id\":\"op_auth_trim_guard_0001\"}}}}",
            "]",
            "}},\"warnings\":[]}}"
        ),
        FIXTURE_REPOSITORY_ID,
        FIXTURE_SESSION_ID,
        FIXTURE_WRITE_TOPIC_ID,
        fixture_post_patch_view_json(),
        FIXTURE_ACTOR_ID,
        FIXTURE_RESOLVED_VIEW_ID,
        FIXTURE_WRITE_TOPIC_ID,
        FIXTURE_SESSION_ID,
        FIXTURE_SESSION_GENERATION_ID,
        FIXTURE_RESOLVED_VIEW_ID,
        FIXTURE_SESSION_ID,
    )
}

fn fixture_inspect_topic_json() -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.topic\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"topic_id\":\"{}\",\"head_revision_id\":\"rev_auth_nullability_0001\"}},",
            "\"view\":null,",
            "\"topic\":{{",
            "\"slug\":\"auth-nullability\",",
            "\"display_name\":\"Auth nullability\",",
            "\"owner_actor_id\":\"{}\",",
            "\"base_checkpoint_id\":\"checkpoint_base_0001\",",
            "\"status\":\"open\",",
            "\"visibility\":\"local\"",
            "}},",
            "\"revisions\":[{{",
            "\"topic_revision_id\":\"rev_auth_nullability_0001\",",
            "\"revision_number\":1,",
            "\"parent_revision_id\":null,",
            "\"operation_transaction_id\":\"op_auth_trim_guard_0001\",",
            "\"changed_artifacts\":[{}]",
            "}}]",
            "}},\"warnings\":[]}}"
        ),
        FIXTURE_REPOSITORY_ID,
        FIXTURE_WRITE_TOPIC_ID,
        FIXTURE_ACTOR_ID,
        fixture_changed_artifact_revision_json(),
    )
}

fn fixture_inspect_revision_json() -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.revision\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"topic_id\":\"{}\",",
            "\"topic_revision_id\":\"rev_auth_nullability_0001\",",
            "\"operation_transaction_id\":\"op_auth_trim_guard_0001\"",
            "}},",
            "\"view\":null,",
            "\"revision\":{{",
            "\"revision_number\":1,",
            "\"parent_revision_id\":null,",
            "\"tree_delta_ref\":\"delta_auth_trim_guard_0001\",",
            "\"dependency_revision_ids\":[],",
            "\"privacy_class\":\"commit_default\"",
            "}},",
            "\"operation\":{{",
            "\"mutation\":\"patch\",",
            "\"session_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"authored_context_id\":\"ctx_agent_a_gen_0001\"",
            "}},",
            "\"changed_artifacts\":[{}]",
            "}},\"warnings\":[]}}"
        ),
        FIXTURE_REPOSITORY_ID,
        FIXTURE_WRITE_TOPIC_ID,
        FIXTURE_SESSION_ID,
        FIXTURE_SESSION_GENERATION_ID,
        fixture_changed_artifact_revision_json(),
    )
}

fn fixture_topic_summary_json() -> String {
    format!(
        concat!(
            "{{",
            "\"topic_id\":\"{}\",",
            "\"slug\":\"auth-nullability\",",
            "\"status\":\"open\",",
            "\"base_checkpoint_id\":\"checkpoint_base_0001\",",
            "\"head_revision_id\":\"rev_auth_nullability_0001\",",
            "\"revision_count\":1,",
            "\"changed_artifact_count\":1",
            "}}"
        ),
        FIXTURE_WRITE_TOPIC_ID
    )
}

fn fixture_session_summary_json() -> String {
    format!(
        concat!(
            "{{",
            "\"session_id\":\"{}\",",
            "\"actor_id\":\"{}\",",
            "\"write_topic_id\":\"{}\",",
            "\"session_generation_id\":\"gen_agent_a_0002\",",
            "\"resolved_view_id\":\"view_agent_a_after_patch_0001\",",
            "\"refresh_policy\":\"pinned_except_own_topic\"",
            "}}"
        ),
        FIXTURE_SESSION_ID, FIXTURE_ACTOR_ID, FIXTURE_WRITE_TOPIC_ID
    )
}

fn fixture_changed_artifact_json() -> &'static str {
    concat!(
        "{\"artifact_id\":\"artifact_src_auth_ts\",",
        "\"path\":\"src/auth.ts\",",
        "\"kind\":\"file\",",
        "\"path_state\":\"active\",",
        "\"before_hash\":\"sha256:auth_base\",",
        "\"after_hash\":\"sha256:auth_trim_guard\",",
        "\"classification\":\"source\",",
        "\"executable\":false,",
        "\"tombstone\":false}"
    )
}

fn fixture_changed_artifact_revision_json() -> &'static str {
    concat!(
        "{\"artifact_id\":\"artifact_src_auth_ts\",",
        "\"path\":\"src/auth.ts\",",
        "\"mutation\":\"patch\",",
        "\"before_hash\":\"sha256:auth_base\",",
        "\"after_hash\":\"sha256:auth_trim_guard\"}"
    )
}

fn fixture_base_view_json() -> String {
    format!(
        concat!(
            "{{",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"refresh_policy\":\"pinned_except_own_topic\",",
            "\"topic_frontier\":{{}},",
            "\"tree_identity\":{{\"kind\":\"SingleRepoTree\",\"repository_id\":\"{}\",\"tree_hash\":\"{}\"}}",
            "}}"
        ),
        FIXTURE_RESOLVED_VIEW_ID,
        FIXTURE_SESSION_GENERATION_ID,
        FIXTURE_REPOSITORY_ID,
        FIXTURE_TREE_HASH,
    )
}

fn fixture_post_patch_view_json() -> String {
    format!(
        concat!(
            "{{",
            "\"resolved_view_id\":\"view_agent_a_after_patch_0001\",",
            "\"session_generation_id\":\"gen_agent_a_0002\",",
            "\"refresh_policy\":\"pinned_except_own_topic\",",
            "\"topic_frontier\":{{\"{}\":\"rev_auth_nullability_0001\"}},",
            "\"tree_identity\":{{\"kind\":\"SingleRepoTree\",\"repository_id\":\"{}\",\"tree_hash\":\"tree_after_auth_patch_0001\"}}",
            "}}"
        ),
        FIXTURE_WRITE_TOPIC_ID, FIXTURE_REPOSITORY_ID
    )
}

fn require_repository_config(repo_root: impl Into<PathBuf>) -> Result<RepositoryConfig, CliError> {
    let repo_root = repo_root.into();
    let config_path = repo_root.join(".sunlight").join("config.toml");
    if !config_path.is_file() {
        return Err(CliError::new(
            "not_initialized",
            "Sunlight repository is not initialized",
        ));
    }

    let body = fs::read_to_string(&config_path).map_err(|error| {
        CliError::new("not_initialized", "Sunlight repository is not initialized")
            .with_detail("path", config_path.display().to_string())
            .with_detail("source", error.to_string())
    })?;

    RepositoryConfig::from_toml(&body, config_path.clone()).map_err(|error| {
        CliError::new(
            "invalid_repository_config",
            "Sunlight repository config is invalid",
        )
        .with_detail("path", config_path.display().to_string())
        .with_detail("source", error.to_string())
    })
}

fn require_projection_policy(
    repo_root: impl Into<PathBuf>,
) -> Result<ResolvedProjectionPolicy, CliError> {
    let repo_root = repo_root.into();
    let config = require_repository_config(repo_root.clone())?;
    resolve_required_projection_policy(&repo_root, &config)
}

fn resolve_required_projection_policy(
    repo_root: &Path,
    config: &RepositoryConfig,
) -> Result<ResolvedProjectionPolicy, CliError> {
    resolve_projection_policy(&repo_root, &config).map_err(|error| {
        CliError::new(
            "invalid_repository_config",
            "Sunlight repository projection/path policy is invalid",
        )
        .with_detail(
            "path",
            repo_root
                .join(".sunlight")
                .join("config.toml")
                .display()
                .to_string(),
        )
        .with_detail("source", error.to_string())
    })
}

fn validate_execution_projection_binding(
    policy: &ResolvedProjectionPolicy,
    projection_id: &str,
    persisted_root: &Path,
) -> Result<PathBuf, CliError> {
    if fs::symlink_metadata(persisted_root)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err(CliError::new(
            "execution_projection_invalid",
            "execution projection root cannot be a symlink",
        )
        .with_detail("projection_id", projection_id));
    }
    let actual = fs::canonicalize(persisted_root).map_err(|error| {
        CliError::new(
            "execution_projection_invalid",
            "execution projection root was not found",
        )
        .with_detail("projection_id", projection_id)
        .with_detail("source", error.to_string())
    })?;
    let expected_root = policy.execution_root(projection_id);
    let expected = fs::canonicalize(&expected_root).map_err(|error| {
        CliError::new(
            "execution_projection_invalid",
            "configured execution projection root was not found",
        )
        .with_detail("projection_id", projection_id)
        .with_detail("source", error.to_string())
    })?;
    if actual != expected || !actual.starts_with(&policy.managed_root) {
        return Err(CliError::new(
            "execution_projection_invalid",
            "execution projection root does not match its configured managed subtree",
        )
        .with_detail("projection_id", projection_id)
        .with_detail("actual_root", actual.display().to_string())
        .with_detail("expected_root", expected.display().to_string()));
    }
    Ok(actual)
}

fn invalid_request(message: impl Into<String>) -> CliError {
    CliError::new("invalid_request", message)
}

fn unimplemented_command(command: &'static str, message: impl Into<String>) -> CliError {
    CliError::new("invalid_request", message).with_detail("command", command)
}

#[derive(Debug)]
struct TopicCreateOptions {
    slug: String,
    display_name: String,
    owner_actor_id: String,
    visibility: String,
    acceptance_criteria: Vec<String>,
    fixture: Option<String>,
}

#[derive(Debug)]
struct TopicCompleteOptions {
    topic_id: String,
    revision_id: String,
    session_id: String,
    summary: Option<String>,
}

#[derive(Debug)]
struct SessionStartOptions {
    topic: String,
    view_id: String,
    actor_id: String,
    fixture: Option<String>,
}

#[derive(Debug)]
struct SessionRefreshOptions {
    session_id: String,
    policy: String,
}

#[derive(Debug)]
struct ArtifactCommandOptions {
    session_id: Option<String>,
    view_id: Option<String>,
    fixture: Option<String>,
    operands: Vec<String>,
}

#[derive(Debug)]
struct MutationCommandOptions {
    session_id: String,
    fixture: Option<String>,
    operands: Vec<String>,
    expect_hash: Option<String>,
    patch_file: Option<String>,
    content_file: Option<String>,
    classification: Option<String>,
}

#[derive(Debug)]
struct ExecutionPromoteOutputOptions {
    execution_id: String,
    fixture: Option<String>,
    path: Option<String>,
    session_id: Option<String>,
    classification: Option<String>,
    topic: Option<String>,
}

fn parse_topic_create_options(ctx: &CommandContext) -> Result<TopicCreateOptions, CliError> {
    let mut display_name = None;
    let mut owner_actor_id = None;
    let mut visibility = None;
    let mut acceptance_criteria = Vec::new();
    let mut fixture = None;
    let mut operands = Vec::new();
    let mut args = ctx.args.iter().skip(2);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--display-name" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun topic create requires --display-name <name>")
                })?;
                display_name = Some(value.clone());
            }
            "--owner" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun topic create requires --owner <actor>")
                })?;
                owner_actor_id = Some(value.clone());
            }
            "--visibility" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun topic create requires --visibility local|private")
                })?;
                visibility = Some(value.clone());
            }
            "--acceptance-criterion" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(
                        "usage: sun topic create requires --acceptance-criterion <text>",
                    )
                })?;
                acceptance_criteria.push(value.clone());
            }
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun topic create requires --fixture basic-app")
                })?;
                fixture = Some(value.clone());
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun topic create"
                )));
            }
            value => operands.push(value.to_string()),
        }
    }

    if operands.len() != 1 {
        return Err(invalid_request(
            "usage: sun topic create <slug> --display-name <name> [--fixture basic-app]",
        ));
    }

    Ok(TopicCreateOptions {
        slug: operands.remove(0),
        display_name: display_name.ok_or_else(|| {
            invalid_request("usage: sun topic create requires --display-name <name>")
        })?,
        owner_actor_id: owner_actor_id.unwrap_or_else(|| "local".to_string()),
        visibility: visibility.unwrap_or_else(|| "local".to_string()),
        acceptance_criteria,
        fixture,
    })
}

fn parse_topic_complete_options(ctx: &CommandContext) -> Result<TopicCompleteOptions, CliError> {
    let mut topic_id = None;
    let mut revision_id = None;
    let mut session_id = None;
    let mut summary = None;
    let mut args = ctx.args.iter().skip(2);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--topic" => {
                topic_id = Some(
                    args.next()
                        .ok_or_else(|| {
                            invalid_request("sun topic complete requires --topic <topic-id>")
                        })?
                        .clone(),
                );
            }
            "--revision" => {
                revision_id = Some(
                    args.next()
                        .ok_or_else(|| {
                            invalid_request(
                                "sun topic complete requires --revision <topic-revision-id>",
                            )
                        })?
                        .clone(),
                );
            }
            "--session" => {
                session_id = Some(
                    args.next()
                        .ok_or_else(|| {
                            invalid_request("sun topic complete requires --session <session-id>")
                        })?
                        .clone(),
                );
            }
            "--summary" => {
                summary = Some(
                    args.next()
                        .ok_or_else(|| {
                            invalid_request("sun topic complete --summary requires text")
                        })?
                        .clone(),
                );
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun topic complete"
                )));
            }
            value => {
                return Err(invalid_request(format!(
                    "unexpected topic complete argument `{value}`"
                )));
            }
        }
    }
    Ok(TopicCompleteOptions {
        topic_id: topic_id
            .ok_or_else(|| invalid_request("sun topic complete requires --topic <topic-id>"))?,
        revision_id: revision_id.ok_or_else(|| {
            invalid_request("sun topic complete requires --revision <topic-revision-id>")
        })?,
        session_id: session_id
            .ok_or_else(|| invalid_request("sun topic complete requires --session <session-id>"))?,
        summary,
    })
}

fn parse_session_start_options(ctx: &CommandContext) -> Result<SessionStartOptions, CliError> {
    let mut topic = None;
    let mut view_id = None;
    let mut actor_id = None;
    let mut fixture = None;
    let mut args = ctx.args.iter().skip(2);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--topic" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun session start requires --topic <topic>")
                })?;
                topic = Some(value.clone());
            }
            "--view" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun session start requires --view <view>")
                })?;
                view_id = Some(value.clone());
            }
            "--actor" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun session start requires --actor <actor-id>")
                })?;
                actor_id = Some(value.clone());
            }
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun session start requires --fixture basic-app")
                })?;
                fixture = Some(value.clone());
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun session start"
                )));
            }
            value => {
                return Err(invalid_request(format!(
                    "unexpected session start argument `{value}`"
                )));
            }
        }
    }

    Ok(SessionStartOptions {
        topic: topic
            .ok_or_else(|| invalid_request("usage: sun session start requires --topic <topic>"))?,
        view_id: view_id
            .ok_or_else(|| invalid_request("usage: sun session start requires --view <view>"))?,
        actor_id: actor_id.ok_or_else(|| {
            invalid_request("usage: sun session start requires --actor <actor-id>")
        })?,
        fixture,
    })
}

fn parse_session_refresh_options(ctx: &CommandContext) -> Result<SessionRefreshOptions, CliError> {
    let mut session_id = None;
    let mut policy = None;
    let mut args = ctx.args.iter().skip(2);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--policy" => {
                policy = Some(
                    args.next()
                        .ok_or_else(|| {
                            invalid_request(
                                "usage: sun session refresh <session> --policy manual|follow|none",
                            )
                        })?
                        .clone(),
                );
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun session refresh"
                )));
            }
            value if session_id.is_none() => session_id = Some(value.to_string()),
            value => {
                return Err(invalid_request(format!(
                    "unexpected session refresh argument `{value}`"
                )));
            }
        }
    }
    let policy = policy.ok_or_else(|| {
        invalid_request("usage: sun session refresh <session> --policy manual|follow|none")
    })?;
    if !matches!(policy.as_str(), "manual" | "follow" | "none") {
        return Err(
            invalid_request("session refresh policy must be manual, follow, or none")
                .with_detail("policy", policy),
        );
    }
    Ok(SessionRefreshOptions {
        session_id: session_id.ok_or_else(|| {
            invalid_request("usage: sun session refresh <session> --policy manual|follow|none")
        })?,
        policy,
    })
}

fn parse_artifact_options(
    ctx: &CommandContext,
    command: &'static str,
    min_operands: usize,
    max_operands: usize,
) -> Result<ArtifactCommandOptions, CliError> {
    let mut session_id = None;
    let mut view_id = None;
    let mut fixture = None;
    let mut operands = Vec::new();
    let mut args = ctx.args.iter().skip(1);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--session" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(format!("usage: sun {command} requires --session <session>"))
                })?;
                session_id = Some(value.clone());
            }
            "--view" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(format!(
                        "usage: sun {command} requires --view <resolved-view>"
                    ))
                })?;
                view_id = Some(value.clone());
            }
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(format!("usage: sun {command} requires --fixture basic-app"))
                })?;
                fixture = Some(value.clone());
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun {command}"
                )));
            }
            value => operands.push(value.to_string()),
        }
    }

    if operands.len() < min_operands || operands.len() > max_operands {
        return Err(invalid_request(artifact_usage(command)));
    }

    if session_id.is_some() == view_id.is_some() {
        return Err(CliError::new(
            "artifact_read_scope_invalid",
            format!("sun {command} requires exactly one of --session or --view"),
        )
        .with_detail("session_supplied", session_id.is_some().to_string())
        .with_detail("view_supplied", view_id.is_some().to_string()));
    }
    if fixture.is_some() && view_id.is_some() {
        return Err(invalid_request(
            "fixture artifact reads require --session; direct --view reads are repository-backed",
        ));
    }
    Ok(ArtifactCommandOptions {
        session_id,
        view_id,
        fixture,
        operands,
    })
}

fn artifact_usage(command: &str) -> String {
    match command {
        "read" => "usage: sun read <path-or-artifact-id> (--session <session> | --view <resolved-view>) [--fixture basic-app]",
        "list" => "usage: sun list [path-prefix] (--session <session> | --view <resolved-view>) [--fixture basic-app]",
        "search" => "usage: sun search <query> (--session <session> | --view <resolved-view>) [--fixture basic-app]",
        "patch" => {
            "usage: sun patch <path> --session <session> [--fixture basic-app] --expect-hash <hash> --patch-file <file>"
        }
        "write" => {
            "usage: sun write <path> --session <session> [--fixture basic-app] --expect-hash <hash-or-new> --content-file <file> --classification source|generated"
        }
        "move" => {
            "usage: sun move <from> <to> --session <session> [--fixture basic-app] --expect-hash <hash>"
        }
        "delete" => {
            "usage: sun delete <path> --session <session> [--fixture basic-app] --expect-hash <hash>"
        }
        "metadata set" => {
            "usage: sun metadata set <path> --session <session> [--fixture basic-app] --expect-hash <hash> --classification source|generated"
        }
        _ => "usage: sun <artifact-command> --session <session> --fixture basic-app",
    }
    .to_string()
}

fn parse_mutation_options(
    ctx: &CommandContext,
    command: &'static str,
    operand_count: usize,
) -> Result<MutationCommandOptions, CliError> {
    parse_mutation_options_with_skip(ctx, command, operand_count, 1)
}

fn parse_mutation_options_with_skip(
    ctx: &CommandContext,
    command: &'static str,
    operand_count: usize,
    skip_args: usize,
) -> Result<MutationCommandOptions, CliError> {
    let mut session_id = None;
    let mut fixture = None;
    let mut expect_hash = None;
    let mut patch_file = None;
    let mut content_file = None;
    let mut classification = None;
    let mut operands = Vec::new();
    let mut args = ctx.args.iter().skip(skip_args);

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--session" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(format!("usage: sun {command} requires --session <session>"))
                })?;
                session_id = Some(value.clone());
            }
            "--fixture" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(format!("usage: sun {command} requires --fixture basic-app"))
                })?;
                fixture = Some(value.clone());
            }
            "--expect-hash" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(format!(
                        "usage: sun {command} requires --expect-hash <hash-or-new>"
                    ))
                })?;
                expect_hash = Some(value.clone());
            }
            "--patch-file" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun patch requires --patch-file <file>")
                })?;
                patch_file = Some(value.clone());
            }
            "--content-file" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun write requires --content-file <file>")
                })?;
                content_file = Some(value.clone());
            }
            "--classification" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request(format!(
                        "usage: sun {command} requires --classification source|generated"
                    ))
                })?;
                classification = Some(value.clone());
            }
            flag if flag.starts_with("--") => {
                return Err(invalid_request(format!(
                    "unknown flag `{flag}` for sun {command}"
                )));
            }
            value => operands.push(value.to_string()),
        }
    }

    if operands.len() != operand_count {
        return Err(invalid_request(artifact_usage(command)));
    }

    let session_id = session_id.ok_or_else(|| {
        invalid_request(format!("usage: sun {command} requires --session <session>"))
    })?;
    Ok(MutationCommandOptions {
        session_id,
        fixture,
        operands,
        expect_hash,
        patch_file,
        content_file,
        classification,
    })
}

fn fixture_store(fixture: &str) -> Result<InMemoryArtifactStore, CliError> {
    match fixture {
        "basic-app" => Ok(InMemoryArtifactStore::fixture_basic_app()),
        _ => Err(invalid_request(format!("unknown fixture `{fixture}`"))
            .with_detail("fixture", fixture.to_string())),
    }
}

fn ensure_basic_app_fixture(fixture: &str) -> Result<(), CliError> {
    if fixture == "basic-app" {
        Ok(())
    } else {
        Err(invalid_request(format!("unknown fixture `{fixture}`"))
            .with_detail("fixture", fixture.to_string()))
    }
}

fn fixture_resolver_revisions() -> Vec<TopicRevisionRef> {
    vec![
        fixture_auth_revision(),
        fixture_profile_revision(),
        fixture_overlapping_auth_revision(),
        fixture_profile_revision_missing_auth_dependency(),
    ]
}

fn fixture_resolved_view_by_id(view_id: &str) -> Option<ResolvedViewResult> {
    if view_id == FIXTURE_BASE_RESOLVED_VIEW_ID {
        return Some(fixture_base_resolved_content_view());
    }

    fixture_known_resolved_views()
        .into_iter()
        .find(|view| view.resolved_view_id == view_id)
}

fn fixture_base_resolved_content_view() -> ResolvedViewResult {
    let store = InMemoryArtifactStore::fixture_basic_app();
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
        topic_frontier: Default::default(),
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

fn fixture_known_resolved_views() -> Vec<ResolvedViewResult> {
    vec![
        fixture_resolved_view(vec![fixture_auth_revision(), fixture_profile_revision()]),
        fixture_resolved_view(vec![
            fixture_auth_revision(),
            fixture_overlapping_auth_revision(),
        ]),
        fixture_resolved_view(vec![fixture_profile_revision_missing_auth_dependency()]),
    ]
}

fn fixture_resolved_view(revisions: Vec<TopicRevisionRef>) -> ResolvedViewResult {
    let frontier = revisions
        .iter()
        .map(|revision| TopicRevisionSelection {
            topic_id: revision.topic_id.clone(),
            revision_id: revision.revision_id.clone(),
        })
        .collect();
    let mut available_revisions = fixture_resolver_revisions();
    for revision in revisions {
        if !available_revisions
            .iter()
            .any(|candidate| candidate.revision_id == revision.revision_id)
        {
            available_revisions.push(revision);
        }
    }
    resolve_fixture_view(
        fixture_resolver_input(frontier),
        fixture_base_entries(),
        available_revisions,
    )
}

fn fixture_checkpoint() -> Result<CheckpointRecord, CliError> {
    let view = fixture_resolved_view(vec![fixture_auth_revision(), fixture_profile_revision()]);
    let execution = fixture_passing_execution_from_resolved_view(&view).map_err(execution_error)?;
    fixture_checkpoint_from_resolved_view(&view, Some(&execution)).map_err(checkpoint_error)
}

fn fixture_git_export_response() -> Result<FixtureGitExport, CliError> {
    let checkpoint = fixture_checkpoint()?;
    let response = git_export_checkpoint(GitExportRequest::from_checkpoint(&checkpoint))
        .map_err(git_export_error)?;
    Ok(FixtureGitExport {
        checkpoint,
        response,
    })
}

fn apply_fixture_generated_output_export_gate(request: &mut GitExportRequest) {
    if !is_fixture_generated_output_export_ref(&request.git_ref) {
        return;
    }

    request
        .generated_output_requirements
        .push(GeneratedOutputExportRequirement {
            path: "src/generated/auth.generated.ts".to_string(),
            provenance_requirement: "promotion_operation_id".to_string(),
        });
}

fn is_fixture_generated_output_export_ref(git_ref: &str) -> bool {
    git_ref == "refs/heads/sunlight/unpromoted-generated-output"
}

fn fixture_git_export_writer_input(request: GitExportRequest) -> GitExportWriterInput {
    let mut validation_report = sunlight_core::git_export::validate_git_export_request(&request);
    if request.git_ref == "refs/heads/sunlight/stale-validation" {
        validation_report.git_ref = "refs/heads/sunlight/auth-profile-ready".to_string();
    }

    let mut repository = GitExportRepositoryState {
        repository_id: request.checkpoint.repository_id.clone(),
        git_root: "/repo/basic-app".to_string(),
        sunlight_repo_root: "/repo/basic-app".to_string(),
        reachable_commit_ids: vec![fixture_base_git_commit_id()],
        refs: vec![GitRefState {
            git_ref: request.git_ref.clone(),
            commit_id: fixture_base_git_commit_id(),
        }],
    };

    if request.git_ref == "refs/heads/sunlight/ref-conflict" {
        repository.refs = vec![GitRefState {
            git_ref: request.git_ref.clone(),
            commit_id: "git_sha1_unrelated_ref_tip_0001".to_string(),
        }];
    }

    if request.git_ref == "refs/heads/sunlight/invalid-repository" {
        repository.git_root = "/repo/other".to_string();
    }

    GitExportWriterInput {
        base_checkpoint_ids: vec![FIXTURE_BASE_CHECKPOINT_ID.to_string()],
        imported_base_commits: vec![ImportedBaseGitCommit {
            checkpoint_id: FIXTURE_BASE_CHECKPOINT_ID.to_string(),
            git_commit_id: fixture_base_git_commit_id(),
        }],
        prior_export_maps: Vec::new(),
        planned_commit_id: FIXTURE_GIT_COMMIT_ID.to_string(),
        export_map_id: FIXTURE_EXPORT_MAP_ID.to_string(),
        exported_at: FIXTURE_CREATED_AT.to_string(),
        request,
        validation_report,
        repository,
    }
}

fn fixture_policy_explain_validation_report(
    validation_report_id: &str,
) -> Result<GitExportValidationReport, CliError> {
    if validation_report_id != FIXTURE_VALIDATION_REPORT_ID {
        return Err(
            object_not_found("validation_report", validation_report_id).with_detail(
                "available_fixture_validation_report_id",
                FIXTURE_VALIDATION_REPORT_ID,
            ),
        );
    }

    let checkpoint = fixture_checkpoint()?;
    let request = GitExportRequest::from_checkpoint(&checkpoint);
    Ok(sunlight_core::git_export::validate_git_export_request(
        &request,
    ))
}

fn local_fixture_git_export_writer_input(
    options: &GitExportOptions,
    request: GitExportRequest,
) -> Result<GitExportWriterInput, CliError> {
    let repo_root = options.repo.clone().unwrap_or_else(|| PathBuf::from("."));
    let repo_root = fs::canonicalize(&repo_root).map_err(|error| {
        invalid_request(format!(
            "failed to resolve local Git repository path `{}`: {error}",
            repo_root.display()
        ))
    })?;
    let repo_root_string = repo_root.display().to_string();
    let base_commit_id = run_git_capture(&repo_root, &["rev-parse", "--verify", "HEAD^{commit}"])
        .map_err(|message| {
            invalid_request(format!(
                "sun git export --execute-local requires a local Git repository with HEAD: {message}"
            ))
        })?
        .trim()
        .to_string();

    let target_ref_commit_id =
        run_git_capture(&repo_root, &["rev-parse", "--verify", &options.git_ref])
            .ok()
            .map(|commit_id| commit_id.trim().to_string())
            .filter(|commit_id| !commit_id.is_empty());
    let refs = target_ref_commit_id
        .map(|commit_id| GitRefState {
            git_ref: options.git_ref.clone(),
            commit_id,
        })
        .into_iter()
        .collect();

    let mut validation_report = sunlight_core::git_export::validate_git_export_request(&request);
    validation_report.git_ref = options.git_ref.clone();

    Ok(GitExportWriterInput {
        base_checkpoint_ids: vec![FIXTURE_BASE_CHECKPOINT_ID.to_string()],
        imported_base_commits: vec![ImportedBaseGitCommit {
            checkpoint_id: FIXTURE_BASE_CHECKPOINT_ID.to_string(),
            git_commit_id: base_commit_id.clone(),
        }],
        prior_export_maps: Vec::new(),
        planned_commit_id: "planned_commit_id_replaced_by_real_git".to_string(),
        export_map_id: FIXTURE_EXPORT_MAP_ID.to_string(),
        exported_at: FIXTURE_CREATED_AT.to_string(),
        request,
        validation_report,
        repository: GitExportRepositoryState {
            repository_id: FIXTURE_REPOSITORY_ID.to_string(),
            git_root: repo_root_string.clone(),
            sunlight_repo_root: repo_root_string,
            reachable_commit_ids: vec![base_commit_id],
            refs,
        },
    })
}

fn fixture_git_export_content_files() -> Vec<GitExportContentFile> {
    vec![
        git_export_content_file("src/auth.rs", b"pub fn auth() {}\n", false),
        git_export_content_file("src/profile.rs", b"pub fn profile() {}\n", false),
        git_export_content_file("bin/run-auth-check", b"#!/bin/sh\n", true),
        git_export_content_file(
            ".sunlight/export-manifest.json",
            b"{\"policy\":\"approved_manifest_only\"}\n",
            false,
        ),
    ]
}

fn git_export_content_file(path: &str, bytes: &[u8], executable: bool) -> GitExportContentFile {
    GitExportContentFile {
        path: path.to_string(),
        bytes: bytes.to_vec(),
        executable,
    }
}

#[derive(Debug)]
struct FailingGitExportMapStore;

impl GitExportMapStore for FailingGitExportMapStore {
    fn persist_git_export_map(
        &mut self,
        _export_map: GitExportMapRecord,
    ) -> Result<PersistedGitExportMap, String> {
        Err("fixture export map write failed".to_string())
    }
}

fn run_git_capture(repo_root: &PathBuf, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to start git {}: {error}", args.join(" ")))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("git {} failed: {}", args.join(" "), stderr.trim()))
    }
}

fn git_working_tree_changed_outside_sunlight(repo_root: &Path) -> bool {
    Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(repo_root)
        .args([
            "status",
            "--porcelain=v1",
            "--untracked-files=normal",
            "--ignore-submodules=none",
            "--",
            ".",
            ":(top,glob,exclude).sunlight/**",
        ])
        .stdin(Stdio::null())
        .output()
        .is_ok_and(|output| output.status.success() && !output.stdout.is_empty())
}

fn fixture_base_git_commit_id() -> String {
    "git_sha1_base_parent_0001".to_string()
}

fn fixture_projection_materialization_request(
    options: &ProjectMaterializeOptions,
) -> ProjectionMaterializationRequest {
    let strategy_preference = options
        .strategy
        .map(|strategy| vec![strategy])
        .unwrap_or_else(|| vec![ProjectionStrategy::Copy]);

    ProjectionMaterializationRequest {
        purpose: options.purpose,
        projection_id: fixture_projection_id_for_purpose(options.purpose).to_string(),
        session_generation_id: fixture_projection_session_generation_id(options.purpose),
        strategy_preference,
        fallback_to_copy: options.fallback_to_copy,
        capabilities: fixture_projection_capabilities(),
    }
}

fn fixture_projection_capabilities() -> ProjectionMaterializationCapabilities {
    let mut capabilities = ProjectionMaterializationCapabilities::copy_only();
    capabilities.reflink_supported = true;
    capabilities.reflink_writes_are_private = true;
    capabilities
}

fn fixture_projection_id_for_purpose(purpose: ProjectionPurpose) -> &'static str {
    match purpose {
        ProjectionPurpose::Execution => FIXTURE_EXECUTION_PROJECTION_ID,
        ProjectionPurpose::Compatibility => FIXTURE_COMPATIBILITY_PROJECTION_ID,
        ProjectionPurpose::Inspection => FIXTURE_INSPECTION_PROJECTION_ID,
        ProjectionPurpose::Export => FIXTURE_EXPORT_PROJECTION_ID,
    }
}

fn fixture_projection_session_generation_id(purpose: ProjectionPurpose) -> Option<String> {
    (purpose == ProjectionPurpose::Compatibility).then(|| "gen_agent_a_0001".to_string())
}

fn fixture_projection_by_id(
    projection_id: &str,
) -> Option<Result<ProjectionRecord, ProjectionValidationError>> {
    let view = fixture_base_resolved_content_view();
    match projection_id {
        FIXTURE_EXECUTION_PROJECTION_ID => {
            Some(fixture_execution_projection_from_resolved_view(&view))
        }
        FIXTURE_COMPATIBILITY_PROJECTION_ID => Some(
            fixture_compatibility_projection_from_resolved_view(&view, "gen_agent_a_0001"),
        ),
        FIXTURE_INSPECTION_PROJECTION_ID => {
            Some(fixture_inspection_projection_from_resolved_view(&view))
        }
        FIXTURE_EXPORT_PROJECTION_ID => Some(fixture_export_projection_from_resolved_view(&view)),
        _ => None,
    }
}

fn fixture_compat_import_projection_by_id(
    projection_id: &str,
) -> Option<Result<ProjectionRecord, ProjectionValidationError>> {
    if projection_id == FIXTURE_COMPATIBILITY_PROJECTION_ID {
        let view = fixture_base_resolved_content_view();
        Some(fixture_compatibility_projection_from_resolved_view(
            &view,
            "gen_agent_a_0001",
        ))
    } else if projection_id == FIXTURE_STALE_COMPATIBILITY_PROJECTION_ID {
        let view = fixture_base_resolved_content_view();
        Some(
            fixture_compatibility_projection_from_resolved_view(&view, "gen_agent_a_0001").map(
                |mut projection| {
                    projection.id = FIXTURE_STALE_COMPATIBILITY_PROJECTION_ID.to_string();
                    projection.tree_identity.tree_hash =
                        "tree_stale_compat_projection_baseline_0001".to_string();
                    projection
                },
            ),
        )
    } else {
        fixture_projection_by_id(projection_id)
    }
}

fn fixture_compat_import_view_for_projection(projection: &ProjectionRecord) -> ResolvedViewResult {
    if projection.id == FIXTURE_COMPATIBILITY_PROJECTION_ID {
        fixture_base_resolved_content_view()
    } else {
        fixture_resolved_view_by_id(&projection.resolved_view_id)
            .unwrap_or_else(|| fixture_resolved_view(Vec::new()))
    }
}

fn fixture_compat_import_response_by_operation_id(
    operation_id: &str,
) -> Result<CompatImportResponse, CliError> {
    if operation_id != FIXTURE_COMPAT_IMPORT_OPERATION_ID {
        return Err(object_not_found("compat_import", operation_id));
    }

    let projection = fixture_compat_import_projection_by_id(FIXTURE_COMPATIBILITY_PROJECTION_ID)
        .ok_or_else(|| object_not_found("projection", FIXTURE_COMPATIBILITY_PROJECTION_ID))?
        .map_err(projection_error)?;
    let current_view = fixture_compat_import_view_for_projection(&projection);

    plan_fixture_basic_app_import(
        &projection,
        &current_view,
        CompatImportRequest {
            projection_id: FIXTURE_COMPATIBILITY_PROJECTION_ID.to_string(),
            session_id: FIXTURE_SESSION_ID.to_string(),
            session_generation_id: FIXTURE_SESSION_GENERATION_ID.to_string(),
            resolved_view_id: current_view.resolved_view_id.clone(),
            write_topic_id: FIXTURE_WRITE_TOPIC_ID.to_string(),
            parent_topic_revision_id: None,
            selected_candidate_delta_ids: vec!["compat_delta_src_auth_ts_0001".to_string()],
        },
        &fixture_basic_app_candidate_deltas(),
    )
    .map_err(compat_import_error)
}

fn artifact_error(error: ArtifactIoError) -> CliError {
    let message = match &error {
        ArtifactIoError::PathPolicyViolation { .. } => {
            "path is rejected by repository path policy".to_string()
        }
        ArtifactIoError::PathNotFound { path, .. } => format!("path `{path}` was not found"),
        ArtifactIoError::SessionNotFound { session_id } => {
            format!("session `{session_id}` was not found")
        }
        _ => error.to_string(),
    };

    let mut cli_error = CliError::new(error.code(), message);
    match error {
        ArtifactIoError::PathPolicyViolation {
            path,
            policy_id,
            reason,
            session_generation_id,
        } => {
            cli_error = cli_error
                .with_detail("path", path)
                .with_detail("policy_id", policy_id)
                .with_detail("reason", reason.as_str())
                .with_detail("session_generation_id", session_generation_id);
        }
        ArtifactIoError::PathNotFound {
            path,
            session_generation_id,
        } => {
            cli_error = cli_error
                .with_detail("path", path)
                .with_detail("session_generation_id", session_generation_id);
        }
        ArtifactIoError::SessionNotFound { session_id } => {
            cli_error = cli_error.with_detail("session_id", session_id);
        }
        ArtifactIoError::MissingContent { content_ref } => {
            cli_error = cli_error.with_detail("content_ref", content_ref);
        }
        ArtifactIoError::NonUtf8Content { path } => {
            cli_error = cli_error.with_detail("path", path);
        }
        ArtifactIoError::PreconditionFailed {
            failed_precondition,
            path,
            artifact_id,
            expected,
            actual,
            session_generation_id,
            resolved_view_id,
        } => {
            cli_error = cli_error
                .with_detail("failed_precondition", failed_precondition)
                .with_detail("path", path)
                .with_detail("expected", expected)
                .with_detail("session_generation_id", session_generation_id)
                .with_detail("resolved_view_id", resolved_view_id);
            if let Some(artifact_id) = artifact_id {
                cli_error = cli_error.with_detail("artifact_id", artifact_id);
            }
            if let Some(actual) = actual {
                cli_error = cli_error.with_detail("actual", actual);
            }
        }
        ArtifactIoError::PatchApplyFailed {
            path,
            artifact_id,
            content_hash,
            failed_hunk,
            session_generation_id,
            resolved_view_id,
        } => {
            cli_error = cli_error
                .with_detail("path", path)
                .with_detail("artifact_id", artifact_id)
                .with_detail("content_hash", content_hash)
                .with_detail("failed_hunk", failed_hunk.to_string())
                .with_detail("session_generation_id", session_generation_id)
                .with_detail("resolved_view_id", resolved_view_id);
        }
    }
    cli_error
}

fn checkpoint_error(error: CheckpointValidationError) -> CliError {
    let message = match error.code.as_str() {
        "checkpoint_conflicted_view" => "resolved view has conflicts and cannot be checkpointed",
        "checkpoint_stale_view" => "resolved view has staleness and cannot be checkpointed",
        "checkpoint_missing_tree" => "resolved view has no checkpointable tree identity",
        "checkpoint_evidence_failed" => "checkpoint evidence did not pass",
        "checkpoint_evidence_view_mismatch" => "checkpoint evidence references a different view",
        "checkpoint_evidence_tree_mismatch" => "checkpoint evidence references a different tree",
        _ => "checkpoint fixture could not be prepared",
    };
    CliError::new(error.code.as_str(), message).with_raw_details_json(format!(
        concat!(
            "{{",
            "\"resolved_view_id\":\"{}\",",
            "\"conflict_ids\":{},",
            "\"staleness_ids\":{},",
            "\"checkpoint_id\":null,",
            "\"execution_id\":{},",
            "\"expected_tree_identity\":{},",
            "\"actual_tree_identity\":{}",
            "}}"
        ),
        json_escape(&error.resolved_view_id),
        string_array_json(error.conflict_ids.iter().map(String::as_str)),
        string_array_json(error.staleness_ids.iter().map(String::as_str)),
        optional_string_json(error.execution_id.as_deref()),
        optional_single_repo_tree_json(error.expected_tree_identity.as_ref()),
        optional_single_repo_tree_json(error.actual_tree_identity.as_ref()),
    ))
}

fn projection_error(error: ProjectionValidationError) -> CliError {
    let message = match error.code.as_str() {
        "projection_conflicted_view" => "resolved view has conflicts and cannot be projected",
        "projection_stale_view" => "resolved view has staleness and cannot be projected",
        "projection_conflicted_and_stale_view" => {
            "resolved view has conflicts and staleness and cannot be projected"
        }
        "projection_missing_tree" => "resolved view has no projectable tree identity",
        _ => "projection fixture could not be prepared",
    };
    CliError::new(error.code.as_str(), message).with_raw_details_json(format!(
        concat!(
            "{{",
            "\"resolved_view_id\":\"{}\",",
            "\"conflict_ids\":{},",
            "\"staleness_ids\":{},",
            "\"projection_id\":null",
            "}}"
        ),
        json_escape(&error.resolved_view_id),
        string_array_json(error.conflict_ids.iter().map(String::as_str)),
        string_array_json(error.staleness_ids.iter().map(String::as_str)),
    ))
}

fn projection_materialization_error(error: ProjectionMaterializationError) -> CliError {
    if error.code == ProjectionMaterializationErrorCode::ProjectionValidationFailed {
        if let Some(validation_error) = error.validation_error {
            return projection_error(validation_error);
        }
    }

    let message = match error.code.as_str() {
        "projection_materialization_copy_unavailable" => {
            "copy materialization is unavailable for this fixture"
        }
        "projection_materialization_reflink_unsupported" => {
            "reflink materialization is unsupported for this fixture"
        }
        "projection_materialization_reflink_unsafe_for_writes" => {
            "reflink materialization is unsafe for writable projections"
        }
        "projection_materialization_hardlink_readonly_unsupported" => {
            "read-only hardlink materialization is unsupported for this fixture"
        }
        "projection_materialization_hardlink_readonly_requires_read_only_policy" => {
            "read-only hardlink materialization requires a read-only projection policy"
        }
        "projection_materialization_hardlink_readonly_unsafe_for_store" => {
            "read-only hardlink materialization cannot protect store integrity"
        }
        "projection_materialization_overlay_copyup_unsupported" => {
            "overlay copy-up materialization is unsupported for this fixture"
        }
        "projection_materialization_overlay_copyup_unsafe_for_writes" => {
            "overlay copy-up materialization is unsafe for writable projections"
        }
        "projection_materialization_metadata_policy_unsupported" => {
            "materialization strategy does not preserve required metadata policy"
        }
        "projection_materialization_no_eligible_strategy" => {
            "no eligible projection materialization strategy was found"
        }
        "projection_materialization_content_tree_mismatch" => {
            "resolved view does not match fixture content tree"
        }
        "projection_materialization_missing_content_blob" => {
            "fixture content tree references a missing content blob"
        }
        "projection_materialization_unsupported_content_entry_kind" => {
            "fixture content tree contains an unsupported entry kind"
        }
        "projection_materialization_projection_root_unavailable" => {
            "projection root must be an empty directory or a creatable path"
        }
        "projection_materialization_write_failed" => "projection files could not be written",
        _ => "projection materialization could not be planned",
    };
    CliError::new(error.code.as_str(), message).with_raw_details_json(format!(
        concat!(
            "{{",
            "\"resolved_view_id\":\"{}\",",
            "\"strategy\":{},",
            "\"projection_id\":null",
            "}}"
        ),
        json_escape(&error.resolved_view_id),
        optional_projection_strategy_json(error.strategy),
    ))
}

fn execution_error(error: ExecutionFoundationError) -> CliError {
    let message = match error.code.as_str() {
        "execution_conflicted_view" => {
            "resolved view has conflicts or staleness and cannot be executed"
        }
        "execution_missing_tree" => "resolved view has no executable tree identity",
        _ => "execution fixture could not be prepared",
    };
    CliError::new(error.code.as_str(), message).with_raw_details_json(format!(
        concat!(
            "{{",
            "\"resolved_view_id\":\"{}\",",
            "\"conflict_ids\":{},",
            "\"staleness_ids\":{},",
            "\"projection_id\":null,",
            "\"execution_id\":null",
            "}}"
        ),
        json_escape(&error.resolved_view_id),
        string_array_json(error.conflict_ids.iter().map(String::as_str)),
        string_array_json(error.staleness_ids.iter().map(String::as_str)),
    ))
}

fn execution_store_integrity_error(
    projection: &ProjectionRecord,
    integrity: &ProjectionStoreIntegrityResult,
    integrity_fixture: StoreIntegrityFixture,
) -> CliError {
    let reason_code = integrity
        .reason_code
        .unwrap_or(ProjectionStoreIntegrityReasonCode::ExecutionStoreIntegrityFailed);
    let quarantine_refs = integrity
        .quarantine
        .as_ref()
        .map(|quarantine| {
            format!(
                concat!(
                    "{{",
                    "\"projection\":\"{}\",",
                    "\"cache\":\"{}\",",
                    "\"native_error\":\"{}\"",
                    "}}"
                ),
                json_escape(&quarantine.quarantine_refs.projection),
                json_escape(&quarantine.quarantine_refs.cache),
                json_escape(&quarantine.quarantine_refs.native_error),
            )
        })
        .unwrap_or_else(|| "null".to_string());

    CliError::new(
        "execution_store_integrity_failed",
        format!(
            "projection store integrity verification failed for fixture {}",
            integrity_fixture.as_str()
        ),
    )
    .with_raw_details_json(format!(
        concat!(
            "{{",
            "\"resolved_view_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"execution_id\":null,",
            "\"integrity_fixture\":\"{}\",",
            "\"integrity_status\":\"{}\",",
            "\"quarantine_reason\":\"{}\",",
            "\"reason_code\":\"{}\",",
            "\"manifest_ref\":\"{}\",",
            "\"manifest_digest\":\"{}\",",
            "\"cache_key\":\"{}\",",
            "\"quarantine_refs\":{},",
            "\"durable_record\":{},",
            "\"cache_reuse_allowed\":{},",
            "\"cache_invalidation_reason\":\"{}\",",
            "\"local_store_integrity\":{},",
            "\"local_quarantine\":{}",
            "}}"
        ),
        json_escape(&projection.resolved_view_id),
        json_escape(&projection.id),
        json_escape(integrity_fixture.as_str()),
        integrity.integrity_status.as_str(),
        reason_code.reason(),
        reason_code.as_str(),
        json_escape(integrity.manifest_ref.as_deref().unwrap_or("")),
        json_escape(integrity.manifest_digest.as_deref().unwrap_or("")),
        json_escape(&integrity.cache_key),
        quarantine_refs,
        projection_quarantine_durable_record_json(integrity),
        projection_quarantine_cache_reuse_allowed_json(integrity),
        json_escape(&projection_quarantine_cache_invalidation_reason(integrity)),
        projection_local_store_integrity_json(integrity),
        projection_quarantine_json(integrity),
    ))
}

fn promotion_error(
    code: &'static str,
    message: &'static str,
    execution_id: &str,
    path: Option<&str>,
    session_id: Option<&str>,
    classification: Option<&str>,
) -> CliError {
    CliError::new(code, message).with_raw_details_json(format!(
        concat!(
            "{{",
            "\"execution_id\":\"{}\",",
            "\"path\":{},",
            "\"session_id\":{},",
            "\"classification\":{},",
            "\"operation_transaction_id\":null,",
            "\"topic_revision_id\":null",
            "}}"
        ),
        json_escape(execution_id),
        optional_string_json(path),
        optional_string_json(session_id),
        optional_string_json(classification),
    ))
}

fn promotion_classification_error(
    execution_id: &str,
    path: Option<&str>,
    session_id: Option<&str>,
    provided_classification: Option<&str>,
    expected_classification: &str,
) -> CliError {
    CliError::new(
        "promotion_policy_failed",
        "execution output promotion classification does not match the candidate",
    )
    .with_raw_details_json(format!(
        concat!(
            "{{",
            "\"execution_id\":\"{}\",",
            "\"path\":{},",
            "\"session_id\":{},",
            "\"classification\":{},",
            "\"provided_classification\":{},",
            "\"expected_classification\":\"{}\",",
            "\"operation_transaction_id\":null,",
            "\"topic_revision_id\":null",
            "}}"
        ),
        json_escape(execution_id),
        optional_string_json(path),
        optional_string_json(session_id),
        optional_string_json(provided_classification),
        optional_string_json(provided_classification),
        json_escape(expected_classification),
    ))
}

fn oversized_promotion_error(
    execution_id: &str,
    path: &str,
    session_id: &str,
    classification: &str,
    observed_bytes: u64,
) -> CliError {
    CliError::new(
        "promotion_policy_failed",
        "execution output exceeds the source-promotion size limit",
    )
    .with_raw_details_json(format!(
        concat!(
            "{{",
            "\"execution_id\":\"{}\",",
            "\"path\":\"{}\",",
            "\"session_id\":\"{}\",",
            "\"classification\":\"{}\",",
            "\"observed_bytes\":{},",
            "\"maximum_promotable_bytes\":{},",
            "\"operation_transaction_id\":null,",
            "\"topic_revision_id\":null",
            "}}"
        ),
        json_escape(execution_id),
        json_escape(path),
        json_escape(session_id),
        json_escape(classification),
        observed_bytes,
        MAX_PROMOTABLE_EXECUTION_OUTPUT_BYTES,
    ))
}

fn git_export_error(error: GitExportError) -> CliError {
    let message = match error.code.as_str() {
        "export_policy_failed" => "checkpoint failed Git export validation",
        "export_parent_not_found" => "Git export parent checkpoint was not found",
        "export_git_failed" => "Git export failed",
        "export_map_write_failed" => "Git export map could not be written",
        _ => "Git export failed",
    };
    CliError::new(error.code.as_str(), message).with_raw_details_json(format!(
        "{{\"validation_report\":{}}}",
        git_export_validation_report_json(&error.validation_report),
    ))
}

fn policy_check_export_error(report: &GitExportValidationReport) -> CliError {
    CliError::new(
        "export_policy_failed",
        "checkpoint failed Git export validation",
    )
    .with_raw_details_json(format!(
        "{{\"validation_report\":{}}}",
        git_export_validation_report_json(report),
    ))
}

fn validation_report_write_error(
    validation_report_id: &str,
    error: GitExportValidationReportStoreError,
) -> CliError {
    validation_report_store_error(
        "validation_report_write_failed",
        validation_report_id,
        error,
    )
}

fn validation_report_load_error(
    validation_report_id: &str,
    error: GitExportValidationReportStoreError,
) -> CliError {
    match error {
        GitExportValidationReportStoreError::NotFound { .. } => {
            object_not_found("validation_report", validation_report_id)
        }
        error => validation_report_store_error(
            "validation_report_integrity_failed",
            validation_report_id,
            error,
        ),
    }
}

fn validation_report_store_error(
    code: &'static str,
    validation_report_id: &str,
    error: GitExportValidationReportStoreError,
) -> CliError {
    let path = match &error {
        GitExportValidationReportStoreError::NotFound { path }
        | GitExportValidationReportStoreError::Invalid { path, .. }
        | GitExportValidationReportStoreError::Io { path, .. } => path.display().to_string(),
    };
    CliError::new(code, error.to_string())
        .with_detail("validation_report_id", validation_report_id)
        .with_detail("path", path)
}

fn real_git_export_policy_error(report: &GitExportValidationReport) -> CliError {
    CliError::new(
        "export_policy_failed",
        "checkpoint failed Git export validation",
    )
    .with_raw_details_json(format!(
        "{{\"validation_report\":{},\"git_write\":{{\"commit_created\":false,\"ref_updated\":false,\"export_map_written\":false}}}}",
        git_export_validation_report_json(report),
    ))
}

fn policy_check_commit_error(
    report: &ValidationReport,
    candidate_paths_checked: usize,
) -> CliError {
    CliError::new(
        "commit_policy_failed",
        "repository failed commit policy validation",
    )
    .with_raw_details_json(format!(
        "{{\"validation_report\":{}}}",
        policy_check_commit_validation_report_json(report, candidate_paths_checked),
    ))
}

fn generated_output_git_export_error(error: GitExportError) -> CliError {
    CliError::new(
        error.code.as_str(),
        "checkpoint failed Git export validation",
    )
    .with_raw_details_json(format!(
        concat!(
            "{{",
            "\"validation_report\":{},",
            "\"git_write\":{{",
            "\"commit_created\":false,",
            "\"ref_updated\":false,",
            "\"export_map_written\":false",
            "}}",
            "}}"
        ),
        git_export_validation_report_json(&error.validation_report),
    ))
}

fn git_export_planning_error(error: GitExportPlanningError) -> CliError {
    CliError::new(error.code.as_str(), error.message.clone()).with_raw_details_json(format!(
        concat!(
            "{{",
            "\"checkpoint_id\":{},",
            "\"validation_report_id\":{},",
            "\"target_ref\":{},",
            "\"parent_commit_id\":{},",
            "\"created_commit_id\":{}",
            "}}"
        ),
        optional_string_json(error.checkpoint_id.as_deref()),
        optional_string_json(error.validation_report_id.as_deref()),
        optional_string_json(error.target_ref.as_deref()),
        optional_string_json(error.parent_commit_id.as_deref()),
        optional_string_json(error.created_commit_id.as_deref()),
    ))
}

fn compat_import_error(error: CompatImportValidationError) -> CliError {
    let message = match error.code {
        CompatImportErrorCode::NoSelectedChanges => "no compatibility import candidates selected",
        CompatImportErrorCode::DiffFailed => "selected compatibility candidate was not found",
        CompatImportErrorCode::SecretDetected => {
            "selected compatibility candidate contains secrets"
        }
        CompatImportErrorCode::CacheBlocked => {
            "selected compatibility candidate is cache, build, or ignored path"
        }
        CompatImportErrorCode::ProjectionNotFound => "compatibility projection was not found",
        CompatImportErrorCode::ProjectionInvalid => {
            "projection is not valid for compatibility import"
        }
        CompatImportErrorCode::ProjectionStale => "compatibility projection is stale",
        CompatImportErrorCode::ProjectionIntegrityFailed => {
            "compatibility projection integrity check failed"
        }
        CompatImportErrorCode::PathPolicyFailed => {
            "selected compatibility candidate failed path policy"
        }
        CompatImportErrorCode::PreconditionFailed => "compatibility import precondition failed",
        CompatImportErrorCode::ConflictedDelta => "selected compatibility candidate is conflicted",
        CompatImportErrorCode::AmbiguousRename => {
            "selected compatibility candidate has ambiguous rename identity"
        }
        CompatImportErrorCode::PolicyFailed => {
            "selected compatibility candidate failed import policy"
        }
        CompatImportErrorCode::PartialWriteBlocked => {
            "compatibility import partial write is blocked"
        }
    };

    CliError::new(error.code.as_str(), message).with_raw_details_json(format!(
        concat!(
            "{{",
            "\"projection_id\":\"{}\",",
            "\"session_id\":\"{}\",",
            "\"candidate_delta_ids\":{},",
            "\"imported_artifacts\":[],",
            "\"operation_transaction_id\":null,",
            "\"topic_revision_id\":null,",
            "\"session_generation_id\":null,",
            "\"reason\":\"{}\"",
            "}}"
        ),
        json_escape(&error.projection_id),
        json_escape(&error.session_id),
        string_array_json(error.candidate_delta_ids.iter().map(String::as_str)),
        json_escape(&error.message),
    ))
}

fn worktree_capture_error(error: CompatImportValidationError) -> CliError {
    let code = match error.code {
        CompatImportErrorCode::NoSelectedChanges => "worktree_no_selected_changes",
        CompatImportErrorCode::DiffFailed => "worktree_diff_failed",
        CompatImportErrorCode::SecretDetected => "worktree_secret_detected",
        CompatImportErrorCode::CacheBlocked => "worktree_candidate_blocked",
        CompatImportErrorCode::PathPolicyFailed => "worktree_path_policy_failed",
        CompatImportErrorCode::PreconditionFailed => "worktree_capture_precondition_failed",
        CompatImportErrorCode::ConflictedDelta => "worktree_candidate_conflicted",
        CompatImportErrorCode::AmbiguousRename => "worktree_rename_ambiguous",
        CompatImportErrorCode::PolicyFailed => "worktree_candidate_policy_failed",
        CompatImportErrorCode::PartialWriteBlocked => "worktree_partial_capture_blocked",
        CompatImportErrorCode::ProjectionNotFound
        | CompatImportErrorCode::ProjectionInvalid
        | CompatImportErrorCode::ProjectionStale
        | CompatImportErrorCode::ProjectionIntegrityFailed => "worktree_anchor_invalid",
    };
    CliError::new(code, error.message).with_raw_details_json(format!(
        "{{\"worktree_anchor_id\":\"{}\",\"candidate_delta_ids\":{},\"native_operation_ids\":[],\"native_revision_ids\":[]}}",
        json_escape(&error.projection_id),
        string_array_json(error.candidate_delta_ids.iter().map(String::as_str)),
    ))
}

fn init_success_envelope(
    repository_id: &str,
    repo_root: &str,
    sunlight_dir: &str,
    created_config: bool,
    created_gitignore: bool,
    created_directories: usize,
    _quarantined_count: usize,
    legacy_quarantine_migrated: bool,
    sunignore_policy_refreshed: bool,
) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"repository.init\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"repository_id\":\"{}\"}},",
            "\"view\":null,",
            "\"repository\":{{",
            "\"initialized\":true,",
            "\"storage_schema_version\":{},",
            "\"path_policy_id\":\"path_policy_posix_case_sensitive_v1\",",
            "\"operation_semantics_version\":\"file_ops_v1\",",
            "\"git_interop_policy\":\"default_local_mvp\"",
            "}},",
            "\"init\":{{",
            "\"repo_root\":\"{}\",",
            "\"sunlight_dir\":\"{}\",",
            "\"created_config\":{},",
            "\"created_gitignore\":{},",
            "\"created_directories\":{},",
            "\"automatic_secret_detection\":false,",
            "\"legacy_quarantine_migrated\":{},",
            "\"sunignore_policy_refreshed\":{},",
            "\"ignore_policy\":{{\"gitignore\":\"git_semantics\",\"sunignore\":\".sunignore\",\"sunignore_owner\":\"human_worktree\",\"sunignore_change_command\":\"sun init\",\"intrinsic\":[\".git/\",\".sunlight/\"]}}",
            "}}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(repository_id),
        json_escape(repository_id),
        CURRENT_STORAGE_SCHEMA_VERSION,
        json_escape(repo_root),
        json_escape(sunlight_dir),
        created_config,
        created_gitignore,
        created_directories,
        legacy_quarantine_migrated,
        sunignore_policy_refreshed,
    )
}

fn topic_create_success_envelope(display_name: &str) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"topic.create\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"topic_id\":\"{}\",",
            "\"base_checkpoint_id\":\"{}\",",
            "\"head_revision_id\":null",
            "}},",
            "\"view\":null,",
            "\"topic\":{{",
            "\"topic_id\":\"{}\",",
            "\"slug\":\"auth-nullability\",",
            "\"display_name\":\"{}\",",
            "\"status\":\"open\",",
            "\"lifecycle\":\"open\",",
            "\"base_checkpoint_id\":\"{}\",",
            "\"head_revision_id\":null,",
            "\"owner_actor_id\":\"{}\",",
            "\"visibility\":\"local\"",
            "}}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        FIXTURE_REPOSITORY_ID,
        FIXTURE_WRITE_TOPIC_ID,
        FIXTURE_BASE_CHECKPOINT_ID,
        FIXTURE_WRITE_TOPIC_ID,
        json_escape(display_name),
        FIXTURE_BASE_CHECKPOINT_ID,
        FIXTURE_ACTOR_ID,
    )
}

fn session_start_success_envelope(resolved_view_id: &str) -> String {
    let view = SessionView {
        resolved_view_id: resolved_view_id.to_string(),
        session_generation_id: FIXTURE_SESSION_GENERATION_ID.to_string(),
        tree_identity: TreeIdentityView {
            kind: "SingleRepoTree".to_string(),
            repository_id: FIXTURE_REPOSITORY_ID.to_string(),
            tree_hash: FIXTURE_TREE_HASH.to_string(),
        },
    };
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"session.start\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"topic_id\":\"{}\",",
            "\"session_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":\"{}\"",
            "}},",
            "\"view\":{},",
            "\"session\":{{",
            "\"session_id\":\"{}\",",
            "\"actor_id\":\"{}\",",
            "\"write_topic_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"refresh_policy\":\"pinned_except_own_topic\",",
            "\"capabilities\":{}",
            "}},",
            "\"topic_frontier\":[{{",
            "\"topic_id\":\"{}\",",
            "\"revision_id\":null,",
            "\"mode\":\"write\"",
            "}}]",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        FIXTURE_REPOSITORY_ID,
        FIXTURE_WRITE_TOPIC_ID,
        FIXTURE_SESSION_ID,
        json_escape(resolved_view_id),
        FIXTURE_SESSION_GENERATION_ID,
        view_json(&view),
        FIXTURE_SESSION_ID,
        FIXTURE_ACTOR_ID,
        FIXTURE_WRITE_TOPIC_ID,
        json_escape(resolved_view_id),
        FIXTURE_SESSION_GENERATION_ID,
        phase1_capabilities_json(),
        FIXTURE_WRITE_TOPIC_ID,
    )
}

fn phase1_capabilities_json() -> String {
    format!(
        "[{}]",
        PHASE1_SESSION_CAPABILITIES
            .iter()
            .map(|capability| format!("\"{}\"", json_escape(capability.as_str())))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn checkpoint_create_success_envelope(checkpoint: &CheckpointRecord) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"checkpoint.create\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"checkpoint_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\"",
            "}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"topic_frontier\":{},",
            "\"tree_identity\":{}",
            "}},",
            "\"checkpoint\":{},",
            "\"checkpoint_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{},",
            "\"topic_frontier\":{},",
            "\"evidence_refs\":[{}],",
            "\"export_refs\":{},",
            "\"export_ready\":true,",
            "\"handoff\":{}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(&checkpoint.repository_id),
        json_escape(&checkpoint.id),
        json_escape(&checkpoint.resolved_view_id),
        json_escape(&checkpoint.resolved_view_id),
        checkpoint_topic_frontier_json(checkpoint),
        single_repo_tree_json(&checkpoint.tree_identity),
        checkpoint_json(checkpoint),
        json_escape(&checkpoint.id),
        json_escape(&checkpoint.resolved_view_id),
        single_repo_tree_json(&checkpoint.tree_identity),
        checkpoint_topic_frontier_json(checkpoint),
        checkpoint
            .evidence_refs
            .iter()
            .map(evidence_ref_json)
            .collect::<Vec<_>>()
            .join(","),
        export_refs_json(checkpoint),
        checkpoint_handoff_json(checkpoint),
    )
}

fn checkpoint_handoff_json(checkpoint: &CheckpointRecord) -> String {
    let execution_ids = checkpoint
        .evidence_refs
        .iter()
        .map(|evidence| match evidence {
            EvidenceRef::Execution(execution) => execution.execution_id.as_str(),
        })
        .collect::<Vec<_>>();
    format!(
        "{{\"exact_ids\":{{\"checkpoint_id\":\"{}\",\"resolved_view_id\":\"{}\",\"tree_hash\":\"{}\",\"execution_ids\":{}}},\"topic_frontier\":{},\"export_ready\":true}}",
        json_escape(&checkpoint.id),
        json_escape(&checkpoint.resolved_view_id),
        json_escape(&checkpoint.tree_identity.tree_hash),
        string_array_json(execution_ids.iter().copied()),
        checkpoint_topic_frontier_json(checkpoint),
    )
}

fn git_export_success_envelope(response: &GitExportResponse) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"{}\",",
            "\"checkpoint_id\":\"{}\",",
            "\"ids\":{{",
            "\"checkpoint_id\":\"{}\",",
            "\"export_map_id\":\"{}\",",
            "\"validation_report_id\":\"{}\"",
            "}},",
            "\"validation_report\":{},",
            "\"git_ref\":\"{}\",",
            "\"git_commit_ids\":{},",
            "\"export_map\":{}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(&response.command),
        json_escape(&response.checkpoint_id),
        json_escape(&response.checkpoint_id),
        json_escape(&response.export_map.id),
        json_escape(&response.validation_report.id),
        git_export_validation_report_json(&response.validation_report),
        json_escape(&response.git_ref),
        string_array_json(response.git_commit_ids.iter().map(String::as_str)),
        git_export_map_json(&response.export_map),
    )
}

fn policy_check_export_success_envelope(report: &GitExportValidationReport) -> String {
    policy_check_export_success_envelope_inner(None, report)
}

fn policy_check_export_success_envelope_with_repository(
    repository_id: &str,
    report: &GitExportValidationReport,
) -> String {
    policy_check_export_success_envelope_inner(Some(repository_id), report)
}

fn policy_check_export_success_envelope_inner(
    repository_id: Option<&str>,
    report: &GitExportValidationReport,
) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"policy.check-export\",",
            "\"repository_id\":{},",
            "\"checkpoint_id\":\"{}\",",
            "\"validation_report_id\":\"{}\",",
            "\"ids\":{{",
            "\"checkpoint_id\":\"{}\",",
            "\"validation_report_id\":\"{}\"",
            "}},",
            "\"validation_report\":{}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        optional_string_json(repository_id),
        json_escape(&report.checkpoint_id),
        json_escape(&report.id),
        json_escape(&report.checkpoint_id),
        json_escape(&report.id),
        git_export_validation_report_json(report),
    )
}

fn policy_check_commit_success_envelope(
    repository_id: &str,
    report: &ValidationReport,
    candidate_paths_checked: usize,
) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"policy.check-commit\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"repository_id\":\"{}\"}},",
            "\"validation_report\":{}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(repository_id),
        json_escape(repository_id),
        policy_check_commit_validation_report_json(report, candidate_paths_checked),
    )
}

fn policy_explain_success_envelope(
    repository_id: Option<&str>,
    report: &GitExportValidationReport,
) -> String {
    let repository = repository_id
        .map(|repository_id| format!("\"repository_id\":\"{}\",", json_escape(repository_id)))
        .unwrap_or_default();
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"policy.explain\",",
            "{}",
            "\"validation_report_id\":\"{}\",",
            "\"ids\":{{",
            "\"validation_report_id\":\"{}\"",
            "}},",
            "\"validation_report\":{}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        repository,
        json_escape(&report.id),
        json_escape(&report.id),
        git_export_validation_report_json(report),
    )
}

fn git_export_write_plan_success_envelope(plan: &GitExportWriterPlan) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"git.export.write_plan\",",
            "\"checkpoint_id\":\"{}\",",
            "\"ids\":{{",
            "\"checkpoint_id\":\"{}\",",
            "\"export_map_id\":\"{}\",",
            "\"validation_report_id\":\"{}\"",
            "}},",
            "\"parent_commit\":{{",
            "\"checkpoint_id\":\"{}\",",
            "\"commit_id\":\"{}\"",
            "}},",
            "\"planned_commit\":{},",
            "\"ref_update\":{},",
            "\"export_map\":{}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(&plan.commit.checkpoint_id),
        json_escape(&plan.commit.checkpoint_id),
        json_escape(&plan.export_map.id),
        json_escape(&plan.commit.validation_report_id),
        json_escape(&plan.parent.checkpoint_id),
        json_escape(&plan.parent.commit_id),
        git_export_commit_plan_json(&plan.commit),
        git_export_ref_update_plan_json(&plan.ref_update),
        git_export_map_json(&plan.export_map),
    )
}

fn git_export_execute_fixture_success_envelope(result: &GitExportExecutionResult) -> String {
    git_export_execution_success_envelope(result, "git.export.execute_fixture")
}

fn git_export_execute_success_envelope(result: &GitExportExecutionResult) -> String {
    git_export_execution_success_envelope(result, "git.export.execute")
}

fn git_export_execution_success_envelope(
    result: &GitExportExecutionResult,
    command: &str,
) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"{}\",",
            "\"checkpoint_id\":\"{}\",",
            "\"ids\":{{",
            "\"checkpoint_id\":\"{}\",",
            "\"validation_report_id\":\"{}\"",
            "}},",
            "\"lifecycle_state\":\"{}\",",
            "\"target_ref\":\"{}\",",
            "\"parent_commit_id\":\"{}\",",
            "\"created_commit_id\":{},",
            "\"summary\":{},",
            "\"error\":{},",
            "\"export_map\":{}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(command),
        json_escape(&result.checkpoint_id),
        json_escape(&result.checkpoint_id),
        json_escape(&result.validation_report_id),
        result.lifecycle_state.as_str(),
        json_escape(&result.target_ref),
        json_escape(&result.parent_commit_id),
        optional_string_json(result.created_commit_id.as_deref()),
        git_export_execution_summary_json(&result.summary),
        git_export_execution_error_json(result.error.as_ref()),
        optional_git_export_map_json(result.export_map.as_ref()),
    )
}

fn compat_project_success_envelope(projection: &ProjectionRecord, session_id: &str) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"compat.project\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"projection_id\":\"{}\",",
            "\"session_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":\"{}\"",
            "}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"projection_id\":\"{}\",",
            "\"session_id\":\"{}\",",
            "\"baseline\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"tree_identity\":{},",
            "\"manifest_ref\":{},",
            "\"manifest_digest\":\"{}\"",
            "}},",
            "\"purpose\":\"{}\",",
            "\"root_ref\":{},",
            "\"strategy\":\"{}\",",
            "\"baseline_manifest_ref\":{},",
            "\"baseline_manifest_digest\":\"{}\",",
            "\"retention_state\":\"{}\",",
            "\"privacy_class\":\"{}\",",
            "\"path_policy\":{},",
            "\"projection\":{}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(&projection.repository_id),
        json_escape(&projection.id),
        json_escape(session_id),
        json_escape(&projection.resolved_view_id),
        json_escape(
            projection
                .session_generation_id
                .as_deref()
                .unwrap_or(FIXTURE_SESSION_GENERATION_ID)
        ),
        json_escape(&projection.resolved_view_id),
        json_escape(
            projection
                .session_generation_id
                .as_deref()
                .unwrap_or(FIXTURE_SESSION_GENERATION_ID)
        ),
        single_repo_tree_json(&projection.tree_identity),
        json_escape(&projection.id),
        json_escape(session_id),
        json_escape(&projection.resolved_view_id),
        json_escape(
            projection
                .session_generation_id
                .as_deref()
                .unwrap_or(FIXTURE_SESSION_GENERATION_ID)
        ),
        single_repo_tree_json(&projection.tree_identity),
        optional_string_json(projection.baseline_manifest_ref.as_deref()),
        json_escape(FIXTURE_COMPAT_BASELINE_MANIFEST_DIGEST),
        projection.purpose.as_str(),
        projection_root_ref_json(projection),
        projection.strategy.as_str(),
        optional_string_json(projection.baseline_manifest_ref.as_deref()),
        json_escape(FIXTURE_COMPAT_BASELINE_MANIFEST_DIGEST),
        projection.retention_state.as_str(),
        projection.privacy_class.as_str(),
        compat_path_policy_json(projection),
        projection_record_json(projection),
    )
}

fn compat_diff_success_envelope(
    projection: &ProjectionRecord,
    current_view: &ResolvedViewResult,
    candidates: &[CompatCandidateDelta],
) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"compat.diff\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"projection_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":{}",
            "}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"projection_id\":\"{}\",",
            "\"session_generation_id\":{},",
            "\"baseline\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":{},",
            "\"tree_identity\":{},",
            "\"manifest_ref\":{},",
            "\"manifest_digest\":\"{}\"",
            "}},",
            "\"candidate_counts\":{},",
            "\"selected_candidate_delta_ids\":{},",
            "\"selected_safe_default_candidate\":{},",
            "\"quarantine_refs\":{},",
            "\"candidates\":[{}],",
            "\"native_operation_ids\":[],",
            "\"native_revision_ids\":[]",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(&projection.repository_id),
        json_escape(&projection.id),
        json_escape(&projection.resolved_view_id),
        optional_string_json(projection.session_generation_id.as_deref()),
        json_escape(&current_view.resolved_view_id),
        optional_single_repo_tree_json(current_view.tree_identity.as_ref()),
        json_escape(&projection.id),
        optional_string_json(projection.session_generation_id.as_deref()),
        json_escape(&projection.resolved_view_id),
        optional_string_json(projection.session_generation_id.as_deref()),
        single_repo_tree_json(&projection.tree_identity),
        optional_string_json(projection.baseline_manifest_ref.as_deref()),
        json_escape(FIXTURE_COMPAT_BASELINE_MANIFEST_DIGEST),
        compat_candidate_counts_json(candidates),
        string_array_json(["compat_delta_src_auth_ts_0001"].into_iter()),
        compat_candidate_json(
            candidates
                .iter()
                .find(|candidate| candidate.candidate_delta_id == "compat_delta_src_auth_ts_0001")
                .expect("fixture safe default candidate should exist"),
        ),
        string_array_json(
            candidates
                .iter()
                .filter_map(|candidate| { candidate.quarantine_ref.as_deref() })
        ),
        candidates
            .iter()
            .map(compat_candidate_json)
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn compat_import_success_envelope(response: &CompatImportResponse) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"projection_id\":\"{}\",",
            "\"session_id\":\"{}\",",
            "\"operation_transaction_id\":\"{}\",",
            "\"topic_revision_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\"",
            "}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"projection_id\":\"{}\",",
            "\"operation_transaction_id\":\"{}\",",
            "\"topic_revision_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{},",
            "\"selected_delta_count\":{},",
            "\"candidate_delta_ids\":{},",
            "\"imported_artifacts\":[{}],",
            "\"ignored_candidate_delta_ids\":{},",
            "\"quarantine_refs\":{},",
            "\"operation\":{},",
            "\"topic_revision\":{},",
            "\"session_generation\":{}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(response.command),
        json_escape(&response.repository_id),
        json_escape(&response.projection_id),
        json_escape(&response.session_id),
        json_escape(&response.operation_id),
        json_escape(&response.topic_revision_id),
        json_escape(&response.session_generation_id),
        json_escape(&response.resolved_view_id),
        json_escape(&response.resolved_view_id),
        json_escape(&response.session_generation_id),
        single_repo_tree_json(&response.tree_identity),
        json_escape(&response.projection_id),
        json_escape(&response.operation_id),
        json_escape(&response.topic_revision_id),
        json_escape(&response.session_generation_id),
        json_escape(&response.resolved_view_id),
        single_repo_tree_json(&response.tree_identity),
        response
            .plan
            .operation
            .mutation_payload
            .selected_deltas
            .len(),
        compat_import_candidate_delta_ids_json(response),
        response
            .imported_artifacts
            .iter()
            .map(compat_imported_artifact_json)
            .collect::<Vec<_>>()
            .join(","),
        string_array_json(
            response
                .ignored_candidate_delta_ids
                .iter()
                .map(String::as_str)
        ),
        string_array_json(response.quarantine_refs.iter().map(String::as_str)),
        compat_operation_json(response),
        compat_topic_revision_json(response),
        compat_session_generation_json(response),
    )
}

fn fixture_status_compat_import_json(response: &CompatImportResponse) -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"status.compat_import\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"compat_import_operation_id\":\"{}\",",
            "\"operation_transaction_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"topic_revision_id\":\"{}\",",
            "\"session_generation_id\":\"{}\"",
            "}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"lifecycle_state\":\"imported\",",
            "\"projection_id\":\"{}\",",
            "\"operation_transaction_id\":\"{}\",",
            "\"topic_revision_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"imported_artifact_count\":{},",
            "\"selected_delta_count\":{},",
            "\"quarantine_count\":{},",
            "\"candidate_delta_ids\":{},",
            "\"imported_artifacts\":[{}],",
            "\"selected_deltas\":[{}],",
            "\"operation_plan\":{},",
            "\"topic_revision\":{},",
            "\"session_generation\":{}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&response.repository_id),
        json_escape(&response.operation_id),
        json_escape(&response.operation_id),
        json_escape(&response.projection_id),
        json_escape(&response.topic_revision_id),
        json_escape(&response.session_generation_id),
        json_escape(&response.resolved_view_id),
        json_escape(&response.session_generation_id),
        single_repo_tree_json(&response.tree_identity),
        json_escape(&response.projection_id),
        json_escape(&response.operation_id),
        json_escape(&response.topic_revision_id),
        json_escape(&response.session_generation_id),
        response.imported_artifacts.len(),
        response
            .plan
            .operation
            .mutation_payload
            .selected_deltas
            .len(),
        response.quarantine_refs.len(),
        compat_import_candidate_delta_ids_json(response),
        response
            .imported_artifacts
            .iter()
            .map(compat_imported_artifact_json)
            .collect::<Vec<_>>()
            .join(","),
        compat_selected_deltas_json(response),
        compat_operation_json(response),
        compat_topic_revision_json(response),
        compat_session_generation_json(response),
    )
}

fn fixture_inspect_compat_import_json(response: &CompatImportResponse) -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.compat_import\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"compat_import_operation_id\":\"{}\",",
            "\"operation_transaction_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"topic_revision_id\":\"{}\",",
            "\"session_generation_id\":\"{}\"",
            "}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"import_provenance\":{{",
            "\"projection_id\":\"{}\",",
            "\"operation_transaction_id\":\"{}\",",
            "\"topic_revision_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"candidate_delta_ids\":{}",
            "}},",
            "\"imported_artifacts\":[{}],",
            "\"selected_deltas\":[{}],",
            "\"ignored_candidate_delta_ids\":{},",
            "\"quarantine_refs\":{},",
            "\"operation_plan\":{},",
            "\"topic_revision\":{},",
            "\"session_generation\":{}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&response.repository_id),
        json_escape(&response.operation_id),
        json_escape(&response.operation_id),
        json_escape(&response.projection_id),
        json_escape(&response.topic_revision_id),
        json_escape(&response.session_generation_id),
        json_escape(&response.resolved_view_id),
        json_escape(&response.session_generation_id),
        single_repo_tree_json(&response.tree_identity),
        json_escape(&response.projection_id),
        json_escape(&response.operation_id),
        json_escape(&response.topic_revision_id),
        json_escape(&response.session_generation_id),
        json_escape(&response.resolved_view_id),
        compat_import_candidate_delta_ids_json(response),
        response
            .imported_artifacts
            .iter()
            .map(compat_imported_artifact_json)
            .collect::<Vec<_>>()
            .join(","),
        compat_selected_deltas_json(response),
        string_array_json(
            response
                .ignored_candidate_delta_ids
                .iter()
                .map(String::as_str)
        ),
        string_array_json(response.quarantine_refs.iter().map(String::as_str)),
        compat_operation_json(response),
        compat_topic_revision_json(response),
        compat_session_generation_json(response),
    )
}

fn fixture_inspect_compat_operation_json(response: &CompatImportResponse) -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.operation\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"operation_transaction_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"topic_revision_id\":\"{}\",",
            "\"session_generation_id\":\"{}\"",
            "}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"operation\":{},",
            "\"projection_provenance\":{{",
            "\"projection_id\":\"{}\",",
            "\"baseline_manifest_digest\":\"{}\",",
            "\"selected_candidate_delta_ids\":{}",
            "}},",
            "\"imported_artifacts\":[{}],",
            "\"selected_deltas\":[{}],",
            "\"created_revision\":{},",
            "\"session_generation\":{}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&response.repository_id),
        json_escape(&response.operation_id),
        json_escape(&response.projection_id),
        json_escape(&response.topic_revision_id),
        json_escape(&response.session_generation_id),
        json_escape(&response.resolved_view_id),
        json_escape(&response.session_generation_id),
        single_repo_tree_json(&response.tree_identity),
        compat_operation_json(response),
        json_escape(&response.projection_id),
        json_escape(
            &response
                .plan
                .operation
                .mutation_payload
                .baseline_manifest_digest
        ),
        string_array_json(
            response
                .plan
                .operation
                .preconditions
                .selected_candidate_delta_ids
                .iter()
                .map(String::as_str)
        ),
        response
            .imported_artifacts
            .iter()
            .map(compat_imported_artifact_json)
            .collect::<Vec<_>>()
            .join(","),
        compat_selected_deltas_json(response),
        compat_topic_revision_json(response),
        compat_session_generation_json(response),
    )
}

fn compat_path_policy_json(projection: &ProjectionRecord) -> String {
    format!(
        concat!(
            "{{",
            "\"path_policy_id\":\"{}\",",
            "\"operation_semantics_version\":\"{}\",",
            "\"case_sensitive\":true,",
            "\"root_ref_privacy\":\"{}\"",
            "}}"
        ),
        json_escape(&projection.path_policy_id),
        json_escape(&projection.operation_semantics_version),
        projection.root_ref.privacy.as_str(),
    )
}

fn compat_candidate_counts_json(candidates: &[CompatCandidateDelta]) -> String {
    let mut by_classification = BTreeMap::<&str, usize>::new();
    let mut by_kind = BTreeMap::<&str, usize>::new();
    for candidate in candidates {
        *by_classification
            .entry(candidate.classification.as_str())
            .or_default() += 1;
        *by_kind.entry(candidate.kind.as_str()).or_default() += 1;
    }

    format!(
        concat!(
            "{{",
            "\"total\":{},",
            "\"by_classification\":{},",
            "\"by_kind\":{},",
            "\"safe_default\":{}",
            "}}"
        ),
        candidates.len(),
        usize_map_json(&by_classification),
        usize_map_json(&by_kind),
        candidates
            .iter()
            .filter(|candidate| is_safe_default_compat_candidate(candidate))
            .count(),
    )
}

fn compat_projection_status_extension_json(projection: &ProjectionRecord) -> String {
    if projection.purpose != ProjectionPurpose::Compatibility {
        return String::new();
    }

    let candidates = fixture_basic_app_candidate_deltas();
    let last_import_attempt = compat_projection_last_import_attempt_json(projection);
    format!(
        concat!(
            ",\"candidate_counts\":{}",
            ",\"selected_candidate_delta_ids\":{}",
            ",\"quarantine_refs\":{}",
            ",\"last_import_attempt\":{}",
            ",\"local_projection_refs\":{}"
        ),
        compat_candidate_counts_json(&candidates),
        compat_projection_selected_candidate_ids_json(),
        compat_quarantine_refs_json(&candidates),
        last_import_attempt,
        compat_projection_local_refs_json(projection, &candidates),
    )
}

fn compat_projection_inspect_extension_json(projection: &ProjectionRecord) -> String {
    if projection.purpose != ProjectionPurpose::Compatibility {
        return String::new();
    }

    let candidates = fixture_basic_app_candidate_deltas();
    let import = compat_projection_last_import_response(projection);
    let last_import_attempt = import
        .as_ref()
        .map(compat_import_attempt_json)
        .unwrap_or_else(|| "null".to_string());
    let native_operation_ids = import
        .as_ref()
        .map(|response| string_array_json([response.operation_id.as_str()].into_iter()))
        .unwrap_or_else(|| "[]".to_string());
    let native_revision_ids = import
        .as_ref()
        .map(|response| string_array_json([response.topic_revision_id.as_str()].into_iter()))
        .unwrap_or_else(|| "[]".to_string());
    format!(
        concat!(
            ",\"compatibility_projection\":{{",
            "\"baseline\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{},",
            "\"manifest_ref\":{},",
            "\"manifest_digest\":\"{}\"",
            "}},",
            "\"path_policy\":{},",
            "\"writable_import_policy\":{{",
            "\"writable_policy\":\"{}\",",
            "\"import_required\":true,",
            "\"store_integrity_policy\":\"{}\",",
            "\"operation_semantics_version\":\"{}\"",
            "}},",
            "\"candidate_summary\":{{",
            "\"candidate_counts\":{},",
            "\"selected_candidate_delta_ids\":{},",
            "\"quarantine_refs\":{},",
            "\"summary_ref\":\"{}\"",
            "}},",
            "\"candidate_detail_refs\":{},",
            "\"local_projection_refs\":{},",
            "\"last_import_attempt\":{},",
            "\"native_operation_ids\":{},",
            "\"native_revision_ids\":{}",
            "}}"
        ),
        json_escape(&projection.resolved_view_id),
        single_repo_tree_json(&projection.tree_identity),
        optional_string_json(projection.baseline_manifest_ref.as_deref()),
        json_escape(FIXTURE_COMPAT_BASELINE_MANIFEST_DIGEST),
        compat_path_policy_json(projection),
        projection.writable_policy.as_str(),
        projection.store_integrity_policy.as_str(),
        json_escape(&projection.operation_semantics_version),
        compat_candidate_counts_json(&candidates),
        compat_projection_selected_candidate_ids_json(),
        compat_quarantine_refs_json(&candidates),
        json_escape(&compat_candidate_summary_ref(projection)),
        compat_candidate_detail_refs_json(projection, &candidates),
        compat_projection_local_refs_json(projection, &candidates),
        last_import_attempt,
        native_operation_ids,
        native_revision_ids,
    )
}

fn compat_projection_last_import_attempt_json(projection: &ProjectionRecord) -> String {
    compat_projection_last_import_response(projection)
        .as_ref()
        .map(compat_import_attempt_json)
        .unwrap_or_else(|| "null".to_string())
}

fn compat_projection_last_import_response(
    projection: &ProjectionRecord,
) -> Option<CompatImportResponse> {
    if projection.id != FIXTURE_COMPATIBILITY_PROJECTION_ID {
        return None;
    }

    fixture_compat_import_response_by_operation_id(FIXTURE_COMPAT_IMPORT_OPERATION_ID).ok()
}

fn compat_import_attempt_json(response: &CompatImportResponse) -> String {
    format!(
        concat!(
            "{{",
            "\"compat_import_operation_id\":\"{}\",",
            "\"operation_transaction_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"topic_revision_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"candidate_delta_ids\":{},",
            "\"selected_deltas\":[{}],",
            "\"topic_revision\":{},",
            "\"session_generation\":{}",
            "}}"
        ),
        json_escape(&response.operation_id),
        json_escape(&response.operation_id),
        json_escape(&response.projection_id),
        json_escape(&response.topic_revision_id),
        json_escape(&response.session_generation_id),
        json_escape(&response.resolved_view_id),
        compat_import_candidate_delta_ids_json(response),
        compat_selected_deltas_json(response),
        compat_topic_revision_json(response),
        compat_session_generation_json(response),
    )
}

fn compat_projection_selected_candidate_ids_json() -> String {
    string_array_json(["compat_delta_src_auth_ts_0001"].into_iter())
}

fn compat_quarantine_refs_json(candidates: &[CompatCandidateDelta]) -> String {
    string_array_json(
        candidates
            .iter()
            .filter_map(|candidate| candidate.quarantine_ref.as_deref()),
    )
}

fn compat_projection_local_refs_json(
    projection: &ProjectionRecord,
    candidates: &[CompatCandidateDelta],
) -> String {
    format!(
        concat!(
            "{{",
            "\"root_ref\":{},",
            "\"baseline_manifest_ref\":{},",
            "\"candidate_summary_ref\":\"{}\",",
            "\"candidate_detail_ref\":\"{}\",",
            "\"quarantine_refs\":{}",
            "}}"
        ),
        projection_root_ref_json(projection),
        optional_string_json(projection.baseline_manifest_ref.as_deref()),
        json_escape(&compat_candidate_summary_ref(projection)),
        json_escape(&compat_candidate_detail_ref(projection)),
        compat_quarantine_refs_json(candidates),
    )
}

fn compat_candidate_detail_refs_json(
    projection: &ProjectionRecord,
    candidates: &[CompatCandidateDelta],
) -> String {
    let items = candidates
        .iter()
        .map(|candidate| {
            format!(
                concat!(
                    "{{",
                    "\"candidate_delta_id\":\"{}\",",
                    "\"detail_ref\":\"{}\"",
                    "}}"
                ),
                json_escape(&candidate.candidate_delta_id),
                json_escape(&format!(
                    "{}/{}",
                    compat_candidate_detail_ref(projection),
                    candidate.candidate_delta_id
                )),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{items}]")
}

fn compat_candidate_summary_ref(projection: &ProjectionRecord) -> String {
    format!(
        "local://.sunlight/projections/compatibility/{}/compat-diff-summary.json",
        projection.id
    )
}

fn compat_candidate_detail_ref(projection: &ProjectionRecord) -> String {
    format!(
        "local://.sunlight/projections/compatibility/{}/candidate-deltas",
        projection.id
    )
}

fn compat_candidate_json(candidate: &CompatCandidateDelta) -> String {
    format!(
        concat!(
            "{{",
            "\"candidate_delta_id\":\"{}\",",
            "\"kind\":\"{}\",",
            "\"operation_kind\":\"{}\",",
            "\"artifact_id\":{},",
            "\"path\":\"{}\",",
            "\"source_path\":{},",
            "\"before_hash\":{},",
            "\"after_hash\":{},",
            "\"byte_length\":{},",
            "\"executable\":{},",
            "\"media_type\":\"{}\",",
            "\"classification\":\"{}\",",
            "\"privacy_class\":\"{}\",",
            "\"path_policy_result\":{},",
            "\"quarantine_ref\":{},",
            "\"safe_default\":{}",
            "}}"
        ),
        json_escape(&candidate.candidate_delta_id),
        candidate.kind.as_str(),
        candidate.operation_kind.as_str(),
        optional_string_json(candidate.artifact_id.as_deref()),
        json_escape(&candidate.path),
        optional_string_json(candidate.source_path.as_deref()),
        optional_string_json(candidate.before_hash.as_deref()),
        optional_string_json(candidate.after_hash.as_deref()),
        candidate.byte_length,
        candidate.executable,
        json_escape(&candidate.media_type),
        json_escape(&candidate.classification),
        candidate.privacy_class.as_str(),
        compat_path_policy_result_json(candidate),
        optional_string_json(candidate.quarantine_ref.as_deref()),
        is_safe_default_compat_candidate(candidate),
    )
}

fn compat_path_policy_result_json(candidate: &CompatCandidateDelta) -> String {
    format!(
        concat!(
            "{{",
            "\"allowed\":{},",
            "\"normalized_path\":{},",
            "\"reason\":{}",
            "}}"
        ),
        candidate.path_policy_result.allowed,
        optional_string_json(candidate.path_policy_result.normalized_path.as_deref()),
        optional_string_json(candidate.path_policy_result.reason.as_deref()),
    )
}

fn is_safe_default_compat_candidate(candidate: &CompatCandidateDelta) -> bool {
    (matches!(
        candidate.kind,
        CompatCandidateKind::ModifiedSource
            | CompatCandidateKind::CreatedSource
            | CompatCandidateKind::DeletedSource
            | CompatCandidateKind::MetadataChanged
    ) || (candidate.kind == CompatCandidateKind::MovedOrRenamed
        && candidate.source_path.is_some()
        && candidate.before_hash == candidate.after_hash))
        && candidate.classification == "source"
        && candidate.path_policy_result.allowed
        && candidate.quarantine_ref.is_none()
}

fn compat_imported_artifact_json(artifact: &CompatImportedArtifact) -> String {
    format!(
        concat!(
            "{{",
            "\"candidate_delta_id\":\"{}\",",
            "\"artifact_id\":\"{}\",",
            "\"path\":\"{}\",",
            "\"operation_kind\":\"{}\",",
            "\"before_hash\":{},",
            "\"after_hash\":{},",
            "\"classification\":\"{}\",",
            "\"privacy_class\":\"{}\"",
            "}}"
        ),
        json_escape(&artifact.candidate_delta_id),
        json_escape(&artifact.artifact_id),
        json_escape(&artifact.path),
        artifact.operation_kind.as_str(),
        optional_string_json(artifact.before_hash.as_deref()),
        optional_string_json(artifact.after_hash.as_deref()),
        json_escape(&artifact.classification),
        artifact.privacy_class.as_str(),
    )
}

fn compat_import_candidate_delta_ids_json(response: &CompatImportResponse) -> String {
    string_array_json(
        response
            .plan
            .operation
            .mutation_payload
            .selected_deltas
            .iter()
            .map(|delta| delta.candidate_delta_id.as_str()),
    )
}

fn compat_selected_deltas_json(response: &CompatImportResponse) -> String {
    response
        .plan
        .operation
        .mutation_payload
        .selected_deltas
        .iter()
        .map(|delta| {
            format!(
                concat!(
                    "{{",
                    "\"candidate_delta_id\":\"{}\",",
                    "\"operation_kind\":\"{}\",",
                    "\"path\":\"{}\",",
                    "\"patch_digest\":{},",
                    "\"base_content_hash\":{},",
                    "\"result_content_hash\":{},",
                    "\"operations\":[{}],",
                    "\"classification\":\"{}\",",
                    "\"privacy_class\":\"{}\"",
                    "}}"
                ),
                json_escape(&delta.candidate_delta_id),
                delta.operation_kind.as_str(),
                json_escape(&delta.path),
                optional_string_json(delta.patch_digest.as_deref()),
                optional_string_json(delta.base_content_hash.as_deref()),
                optional_string_json(delta.result_content_hash.as_deref()),
                compat_selected_delta_operations_json(delta),
                json_escape(&delta.classification),
                delta.privacy_class.as_str(),
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn compat_selected_delta_operations_json(
    delta: &sunlight_core::compat_import::CompatSelectedDeltaPlan,
) -> String {
    delta
        .operations
        .iter()
        .map(|operation| {
            format!(
                concat!(
                    "{{",
                    "\"operation_kind\":\"{}\",",
                    "\"source_path\":{},",
                    "\"target_path\":\"{}\",",
                    "\"base_content_hash\":{},",
                    "\"result_content_hash\":{},",
                    "\"patch_digest\":{}",
                    "}}"
                ),
                operation.operation_kind.as_str(),
                optional_string_json(operation.source_path.as_deref()),
                json_escape(&operation.target_path),
                optional_string_json(operation.base_content_hash.as_deref()),
                optional_string_json(operation.result_content_hash.as_deref()),
                optional_string_json(operation.patch_digest.as_deref()),
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn compat_operation_json(response: &CompatImportResponse) -> String {
    let operation = &response.plan.operation;
    format!(
        concat!(
            "{{",
            "\"operation_transaction_id\":\"{}\",",
            "\"topic_id\":\"{}\",",
            "\"session_id\":\"{}\",",
            "\"actor_id\":\"{}\",",
            "\"authored_context_id\":\"{}\",",
            "\"mutation\":\"compat_import\",",
            "\"session_generation_id\":\"{}\",",
            "\"classification\":\"{}\",",
            "\"parent_topic_revision_id\":{},",
            "\"next_topic_revision_number\":{},",
            "\"parents\":{},",
            "\"preconditions\":{{",
            "\"projection_id\":\"{}\",",
            "\"projection_purpose\":\"{}\",",
            "\"projection_baseline_resolved_view_id\":\"{}\",",
            "\"projection_baseline_tree_identity\":{},",
            "\"session_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"write_topic_id\":\"{}\",",
            "\"parent_topic_revision_id\":{},",
            "\"path_policy_id\":\"{}\",",
            "\"operation_semantics_version\":\"{}\",",
            "\"selected_candidate_delta_ids\":{}",
            "}},",
            "\"read_set\":{{\"mode\":\"{}\",\"resolved_view_id\":\"{}\",\"projection_id\":\"{}\"}},",
            "\"write_set\":[{}],",
            "\"payload\":{},",
            "\"before_refs\":{},",
            "\"after_refs\":{}",
            "}}"
        ),
        json_escape(&operation.id),
        json_escape(&operation.topic_id),
        json_escape(&operation.session_id),
        json_escape(&operation.actor_id),
        json_escape(&operation.authored_context_id),
        json_escape(&operation.session_generation_id),
        json_escape(&operation.classification),
        optional_string_json(operation.parent_topic_revision_id.as_deref()),
        operation.next_topic_revision_number,
        string_array_json(operation.parents.iter().map(String::as_str)),
        json_escape(&operation.preconditions.projection_id),
        operation.preconditions.projection_purpose.as_str(),
        json_escape(&operation.preconditions.projection_baseline_resolved_view_id),
        single_repo_tree_json(&operation.preconditions.projection_baseline_tree_identity),
        json_escape(&operation.preconditions.session_id),
        json_escape(&operation.preconditions.session_generation_id),
        json_escape(&operation.preconditions.resolved_view_id),
        json_escape(&operation.preconditions.write_topic_id),
        optional_string_json(operation.preconditions.parent_topic_revision_id.as_deref()),
        json_escape(&operation.preconditions.path_policy_id),
        json_escape(&operation.preconditions.operation_semantics_version),
        string_array_json(
            operation
                .preconditions
                .selected_candidate_delta_ids
                .iter()
                .map(String::as_str)
        ),
        json_escape(&operation.read_set.mode),
        json_escape(&operation.read_set.resolved_view_id),
        json_escape(&operation.read_set.projection_id),
        operation
            .write_set
            .iter()
            .map(|entry| {
                format!(
                    "{{\"artifact_id\":\"{}\",\"path\":\"{}\",\"mutation\":\"{}\"}}",
                    json_escape(&entry.artifact_id),
                    json_escape(&entry.path),
                    entry.mutation.as_str(),
                )
            })
            .collect::<Vec<_>>()
            .join(","),
        compat_import_payload_json(response),
        refs_json(&operation.before_refs),
        refs_json(&operation.after_refs),
    )
}

fn compat_import_payload_json(response: &CompatImportResponse) -> String {
    let payload = &response.plan.operation.mutation_payload;
    format!(
        concat!(
            "{{",
            "\"kind\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"baseline_manifest_digest\":\"{}\",",
            "\"selected_deltas\":[{}]",
            "}}"
        ),
        json_escape(&payload.kind),
        json_escape(&payload.projection_id),
        json_escape(&payload.baseline_manifest_digest),
        payload
            .selected_deltas
            .iter()
            .map(|delta| {
                format!(
                    concat!(
                        "{{",
                        "\"candidate_delta_id\":\"{}\",",
                        "\"operation_kind\":\"{}\",",
                        "\"path\":\"{}\",",
                        "\"patch_digest\":{},",
                        "\"base_content_hash\":{},",
                        "\"result_content_hash\":{},",
                        "\"operations\":[{}],",
                        "\"classification\":\"{}\",",
                        "\"privacy_class\":\"{}\"",
                        "}}"
                    ),
                    json_escape(&delta.candidate_delta_id),
                    delta.operation_kind.as_str(),
                    json_escape(&delta.path),
                    optional_string_json(delta.patch_digest.as_deref()),
                    optional_string_json(delta.base_content_hash.as_deref()),
                    optional_string_json(delta.result_content_hash.as_deref()),
                    compat_selected_delta_operations_json(delta),
                    json_escape(&delta.classification),
                    delta.privacy_class.as_str(),
                )
            })
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn compat_topic_revision_json(response: &CompatImportResponse) -> String {
    let revision = &response.plan.topic_revision;
    format!(
        concat!(
            "{{",
            "\"topic_revision_id\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"topic_id\":\"{}\",",
            "\"revision_number\":{},",
            "\"parent_revision_id\":{},",
            "\"operation_transaction_id\":\"{}\",",
            "\"tree_delta_ref\":\"{}\",",
            "\"dependency_revision_ids\":{}",
            "}}"
        ),
        json_escape(&revision.id),
        json_escape(&revision.repository_id),
        json_escape(&revision.topic_id),
        revision.revision_number,
        optional_string_json(revision.parent_revision_id.as_deref()),
        json_escape(&revision.operation_transaction_id),
        json_escape(&revision.tree_delta_ref),
        string_array_json(revision.dependency_revision_ids.iter().map(String::as_str)),
    )
}

fn compat_session_generation_json(response: &CompatImportResponse) -> String {
    let generation = &response.plan.session_generation;
    let topic_frontier = generation
        .topic_frontier
        .iter()
        .map(|(topic_id, revision_id)| {
            format!(
                "\"{}\":\"{}\"",
                json_escape(topic_id),
                json_escape(revision_id)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{",
            "\"session_generation_id\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"session_id\":\"{}\",",
            "\"write_topic_id\":\"{}\",",
            "\"base_resolved_view_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"topic_frontier\":{{{}}},",
            "\"generation_number\":{},",
            "\"refresh_policy\":\"{}\",",
            "\"created_by_operation_id\":\"{}\"",
            "}}"
        ),
        json_escape(&generation.id),
        json_escape(&generation.repository_id),
        json_escape(&generation.session_id),
        json_escape(&generation.write_topic_id),
        json_escape(&generation.base_resolved_view_id),
        json_escape(&generation.resolved_view_id),
        topic_frontier,
        generation.generation_number,
        json_escape(&generation.refresh_policy),
        json_escape(&generation.created_by_operation_id),
    )
}

fn git_export_validation_report_json(report: &GitExportValidationReport) -> String {
    format!(
        concat!(
            "{{",
            "\"id\":\"{}\",",
            "\"policy_id\":\"{}\",",
            "\"checkpoint_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{},",
            "\"git_ref\":\"{}\",",
            "\"ok\":{},",
            "\"summary\":{{",
            "\"records_checked\":{},",
            "\"payloads_checked\":{},",
            "\"warnings\":{},",
            "\"blocked\":{}",
            "}},",
            "\"failures\":[{}]",
            "}}"
        ),
        json_escape(&report.id),
        json_escape(&report.policy_id),
        json_escape(&report.checkpoint_id),
        json_escape(&report.resolved_view_id),
        single_repo_tree_json(&report.tree_identity),
        json_escape(&report.git_ref),
        report.ok,
        report.summary.records_checked,
        report.summary.payloads_checked,
        report.summary.warnings,
        report.summary.blocked,
        report
            .failures
            .iter()
            .map(git_export_validation_failure_json)
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn git_export_validation_failure_json(failure: &GitExportValidationFailure) -> String {
    format!(
        concat!(
            "{{",
            "\"check\":\"{}\",",
            "\"code\":\"{}\",",
            "\"field\":{},",
            "\"value\":{},",
            "\"reason\":\"{}\"",
            "}}"
        ),
        failure.check.as_str(),
        failure.code.as_str(),
        optional_string_json(failure.field.as_deref()),
        optional_string_json(failure.value.as_deref()),
        json_escape(&failure.reason),
    )
}

fn combine_policy_check_commit_reports(
    ignore_report: ValidationReport,
    path_report: ValidationReport,
) -> ValidationReport {
    let mut failures = ignore_report.failures;
    failures.extend(path_report.failures);
    ValidationReport::from_failures(failures)
}

fn policy_check_commit_validation_report_json(
    report: &ValidationReport,
    candidate_paths_checked: usize,
) -> String {
    format!(
        concat!(
            "{{",
            "\"ok\":{},",
            "\"summary\":{{",
            "\"managed_ignore_blocks_checked\":1,",
            "\"candidate_paths_checked\":{},",
            "\"warnings\":0,",
            "\"blocked\":{}",
            "}},",
            "\"failures\":[{}]",
            "}}"
        ),
        report.ok,
        candidate_paths_checked,
        report.failures.len(),
        report
            .failures
            .iter()
            .map(policy_check_commit_validation_failure_json)
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn policy_check_commit_validation_failure_json(failure: &ValidationFailure) -> String {
    format!(
        concat!(
            "{{",
            "\"check\":\"{}\",",
            "\"code\":\"{}\",",
            "\"path\":{},",
            "\"reason\":\"{}\"",
            "}}"
        ),
        failure.check.as_str(),
        failure.code.as_str(),
        optional_string_json(failure.path.as_deref()),
        json_escape(&failure.reason),
    )
}

fn git_export_execution_summary_json(summary: &GitExportExecutionSummary) -> String {
    format!(
        concat!(
            "{{",
            "\"commit_created\":{},",
            "\"ref_updated\":{},",
            "\"export_map_written\":{},",
            "\"completed_steps\":{}",
            "}}"
        ),
        summary.commit_created,
        summary.ref_updated,
        summary.export_map_written,
        git_export_execution_steps_json(&summary.completed_steps),
    )
}

fn git_export_execution_steps_json(steps: &[GitExportExecutionStep]) -> String {
    let items = steps
        .iter()
        .map(|step| format!("\"{}\"", step.as_str()))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{items}]")
}

fn git_export_execution_error_json(error: Option<&GitExportExecutionError>) -> String {
    error
        .map(|error| {
            format!(
                concat!(
                    "{{",
                    "\"code\":\"{}\",",
                    "\"failed_step\":\"{}\",",
                    "\"checkpoint_id\":\"{}\",",
                    "\"validation_report_id\":\"{}\",",
                    "\"target_ref\":\"{}\",",
                    "\"parent_commit_id\":\"{}\",",
                    "\"created_commit_id\":{},",
                    "\"message\":\"{}\"",
                    "}}"
                ),
                error.code.as_str(),
                error.failed_step.as_str(),
                json_escape(&error.checkpoint_id),
                json_escape(&error.validation_report_id),
                json_escape(&error.target_ref),
                json_escape(&error.parent_commit_id),
                optional_string_json(error.created_commit_id.as_deref()),
                json_escape(&error.message),
            )
        })
        .unwrap_or_else(|| "null".to_string())
}

fn optional_git_export_map_json(export_map: Option<&GitExportMapRecord>) -> String {
    export_map
        .map(git_export_map_json)
        .unwrap_or_else(|| "null".to_string())
}

fn git_export_map_json(export_map: &GitExportMapRecord) -> String {
    format!(
        concat!(
            "{{",
            "\"record_type\":\"git_export_map\",",
            "\"id\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"checkpoint_id\":\"{}\",",
            "\"tree_identity\":{},",
            "\"git_remote\":{},",
            "\"git_ref\":\"{}\",",
            "\"git_commit_ids\":{},",
            "\"export_shape\":{},",
            "\"validation_report_id\":\"{}\",",
            "\"exported_at\":\"{}\",",
            "\"privacy_class\":\"{}\"",
            "}}"
        ),
        json_escape(&export_map.id),
        json_escape(&export_map.repository_id),
        json_escape(&export_map.checkpoint_id),
        single_repo_tree_json(&export_map.tree_identity),
        optional_string_json(export_map.git_remote.as_deref()),
        json_escape(&export_map.git_ref),
        string_array_json(export_map.git_commit_ids.iter().map(String::as_str)),
        export_shape_json(export_map),
        json_escape(&export_map.validation_report_id),
        json_escape(&export_map.exported_at),
        export_map.privacy_class.as_str(),
    )
}

fn export_shape_json(export_map: &GitExportMapRecord) -> String {
    format!(
        concat!(
            "{{",
            "\"kind\":\"{}\",",
            "\"parent_policy\":\"{}\",",
            "\"include_sunlight_metadata\":\"{}\"",
            "}}"
        ),
        export_map.export_shape.kind.as_str(),
        json_escape(&export_map.export_shape.parent_policy),
        json_escape(&export_map.export_shape.include_sunlight_metadata),
    )
}

fn git_export_commit_plan_json(commit: &GitExportCommitPlan) -> String {
    format!(
        concat!(
            "{{",
            "\"checkpoint_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{},",
            "\"parent_commit_id\":\"{}\",",
            "\"planned_commit_id\":\"{}\",",
            "\"export_shape\":{},",
            "\"validation_report_id\":\"{}\",",
            "\"message\":\"{}\"",
            "}}"
        ),
        json_escape(&commit.checkpoint_id),
        json_escape(&commit.resolved_view_id),
        single_repo_tree_json(&commit.tree_identity),
        json_escape(&commit.parent_commit_id),
        json_escape(&commit.planned_commit_id),
        export_shape_value_json(
            commit.export_shape.kind.as_str(),
            &commit.export_shape.parent_policy,
            &commit.export_shape.include_sunlight_metadata,
        ),
        json_escape(&commit.validation_report_id),
        json_escape(&commit.message),
    )
}

fn git_export_ref_update_plan_json(ref_update: &GitExportRefUpdatePlan) -> String {
    format!(
        concat!(
            "{{",
            "\"git_ref\":\"{}\",",
            "\"expected_old_commit_id\":{},",
            "\"new_commit_id\":\"{}\",",
            "\"allowed_reason\":\"{}\"",
            "}}"
        ),
        json_escape(&ref_update.git_ref),
        optional_string_json(ref_update.expected_old_commit_id.as_deref()),
        json_escape(&ref_update.new_commit_id),
        ref_update.allowed_reason.as_str(),
    )
}

fn export_shape_value_json(
    kind: &str,
    parent_policy: &str,
    include_sunlight_metadata: &str,
) -> String {
    format!(
        concat!(
            "{{",
            "\"kind\":\"{}\",",
            "\"parent_policy\":\"{}\",",
            "\"include_sunlight_metadata\":\"{}\"",
            "}}"
        ),
        json_escape(kind),
        json_escape(parent_policy),
        json_escape(include_sunlight_metadata),
    )
}

fn projection_materialize_success_envelope(plan: &ProjectionMaterializationPlan) -> String {
    let projection = &plan.projection;
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"projection.materialize\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"projection_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\"",
            "}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"projection_id\":\"{}\",",
            "\"purpose\":\"{}\",",
            "\"selected_strategy\":\"{}\",",
            "\"strategy\":\"{}\",",
            "\"root_ref\":{},",
            "\"tree_identity\":{},",
            "\"cache_key\":\"{}\",",
            "\"source\":\"{}\",",
            "\"local_materialization\":{},",
            "\"retention_state\":\"{}\",",
            "\"policy\":{{",
            "\"path_policy_id\":\"{}\",",
            "\"operation_semantics_version\":\"{}\",",
            "\"writable_policy\":\"{}\",",
            "\"store_integrity_policy\":\"{}\",",
            "\"privacy_class\":\"{}\"",
            "}},",
            "\"projection\":{}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(&projection.repository_id),
        json_escape(&projection.id),
        json_escape(&projection.resolved_view_id),
        json_escape(&projection.resolved_view_id),
        single_repo_tree_json(&projection.tree_identity),
        json_escape(&projection.id),
        projection.purpose.as_str(),
        projection.strategy.as_str(),
        projection.strategy.as_str(),
        projection_root_ref_json(projection),
        single_repo_tree_json(&projection.tree_identity),
        json_escape(&projection.cache_key.stable_string()),
        plan.source.as_str(),
        projection_materialization_local_metadata_json(&plan.local_metadata),
        projection.retention_state.as_str(),
        json_escape(&projection.path_policy_id),
        json_escape(&projection.operation_semantics_version),
        projection.writable_policy.as_str(),
        projection.store_integrity_policy.as_str(),
        projection.privacy_class.as_str(),
        projection_record_json(projection),
    )
}

fn projection_filesystem_materialize_success_envelope(
    materialization: &ProjectionFilesystemMaterialization,
) -> Result<String, CliError> {
    let plan = &materialization.plan;
    let projection = &plan.projection;
    let manifest = fixture_local_projection_manifest(projection)?;
    Ok(format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"projection.materialize\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"projection_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\"",
            "}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"projection_id\":\"{}\",",
            "\"purpose\":\"{}\",",
            "\"selected_strategy\":\"{}\",",
            "\"strategy\":\"{}\",",
            "\"root_ref\":{},",
            "\"tree_identity\":{},",
            "\"cache_key\":\"{}\",",
            "\"source\":\"{}\",",
            "\"local_materialization\":{},",
            "\"local_projection_manifest\":{},",
            "\"projection_root\":{},",
            "\"files_written\":{},",
            "\"directories_created\":{},",
            "\"bytes_written\":{},",
            "\"executable_files\":{},",
            "\"cleanup\":{},",
            "\"retention_state\":\"{}\",",
            "\"policy\":{{",
            "\"path_policy_id\":\"{}\",",
            "\"operation_semantics_version\":\"{}\",",
            "\"writable_policy\":\"{}\",",
            "\"store_integrity_policy\":\"{}\",",
            "\"privacy_class\":\"{}\"",
            "}},",
            "\"projection\":{}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(&projection.repository_id),
        json_escape(&projection.id),
        json_escape(&projection.resolved_view_id),
        json_escape(&projection.resolved_view_id),
        single_repo_tree_json(&projection.tree_identity),
        json_escape(&projection.id),
        projection.purpose.as_str(),
        projection.strategy.as_str(),
        projection.strategy.as_str(),
        projection_root_ref_json(projection),
        single_repo_tree_json(&projection.tree_identity),
        json_escape(&projection.cache_key.stable_string()),
        plan.source.as_str(),
        projection_materialization_local_metadata_json(&plan.local_metadata),
        local_projection_manifest_json(&manifest),
        local_projection_root_json(&materialization.projection_root),
        materialization.files_written,
        materialization.directories_created,
        materialization.bytes_written,
        materialization.executable_files,
        projection_cleanup_check_json(materialization),
        projection.retention_state.as_str(),
        json_escape(&projection.path_policy_id),
        json_escape(&projection.operation_semantics_version),
        projection.writable_policy.as_str(),
        projection.store_integrity_policy.as_str(),
        projection.privacy_class.as_str(),
        projection_record_json(projection),
    ))
}

fn projection_materialization_local_metadata_json(
    metadata: &ProjectionMaterializationLocalMetadata,
) -> String {
    format!(
        concat!(
            "{{",
            "\"privacy_class\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{},",
            "\"strategy\":\"{}\",",
            "\"cache_key\":\"{}\",",
            "\"root_ref\":{},",
            "\"writable_policy\":\"{}\",",
            "\"store_integrity_policy\":\"{}\",",
            "\"source\":\"{}\"",
            "}}"
        ),
        metadata.privacy_class.as_str(),
        json_escape(&metadata.projection_id),
        json_escape(&metadata.resolved_view_id),
        single_repo_tree_json(&metadata.tree_identity),
        metadata.strategy.as_str(),
        json_escape(&metadata.cache_key),
        projection_root_ref_value_json(&metadata.root_ref),
        metadata.writable_policy.as_str(),
        metadata.store_integrity_policy.as_str(),
        metadata.source.as_str(),
    )
}

fn local_projection_root_json(path: &std::path::Path) -> String {
    format!(
        concat!(
            "{{",
            "\"path\":\"{}\",",
            "\"privacy\":\"local_only_path\",",
            "\"privacy_class\":\"local_only\"",
            "}}"
        ),
        json_escape(&path.display().to_string()),
    )
}

fn projection_cleanup_check_json(materialization: &ProjectionFilesystemMaterialization) -> String {
    format!(
        concat!(
            "{{",
            "\"projection_root\":{},",
            "\"exists\":{},",
            "\"local_only\":{}",
            "}}"
        ),
        local_projection_root_json(&materialization.cleanup.projection_root),
        materialization.cleanup.exists,
        materialization.cleanup.local_only,
    )
}

fn fixture_local_projection_manifest(
    projection: &ProjectionRecord,
) -> Result<ProjectionManifestRecord, CliError> {
    let store = InMemoryArtifactStore::fixture_basic_app();
    let view = fixture_resolved_view_by_id(&projection.resolved_view_id)
        .ok_or_else(|| object_not_found("resolved_view", &projection.resolved_view_id))?;

    fixture_projection_manifest_from_content_tree(
        projection,
        &view,
        store.tree(),
        store.content_blobs(),
    )
    .map_err(projection_materialization_error)
}

fn fixture_execution_projection_manifest_for_view(
    projection: &ProjectionRecord,
    view: &ResolvedViewResult,
) -> Result<(ProjectionManifestRecord, BTreeMap<String, ContentBlob>), CliError> {
    let store = InMemoryArtifactStore::fixture_basic_app();
    let mut blobs = store.content_blobs().clone();
    let mut entries = view
        .tree_entries
        .values()
        .map(|entry| {
            if !blobs.contains_key(&entry.content_hash) {
                blobs.insert(
                    entry.content_hash.clone(),
                    ContentBlob {
                        id: format!("blob_fixture_{}", entry.content_hash.replace(':', "_")),
                        repository_id: view.repository_id.clone(),
                        digest: entry.content_hash.clone(),
                        bytes: entry.content_hash.as_bytes().to_vec(),
                        media_type: "application/octet-stream".to_string(),
                        classification: "source".to_string(),
                        storage_ref: format!(
                            "objects/blobs/fixture/{}",
                            entry.content_hash.replace(':', "/")
                        ),
                        privacy_class: "policy_gated".to_string(),
                        created_at: FIXTURE_CREATED_AT.to_string(),
                    },
                );
            }

            let executable = store
                .tree()
                .entries
                .iter()
                .find(|candidate| candidate.path == entry.path)
                .map(|candidate| candidate.executable)
                .unwrap_or(false);

            TreeEntry {
                path: entry.path.clone(),
                artifact_id: entry.artifact_id.clone(),
                content_ref: entry.content_hash.clone(),
                kind: ArtifactKind::File,
                executable,
                tombstone: false,
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.path.cmp(&right.path));

    let tree_identity = view
        .tree_identity
        .as_ref()
        .ok_or_else(|| object_not_found("resolved_view", &view.resolved_view_id))?;
    let content_tree = ContentTree {
        id: tree_identity.tree_hash.clone(),
        repository_id: view.repository_id.clone(),
        tree_hash: tree_identity.tree_hash.clone(),
        path_policy_id: view.path_policy_id.clone(),
        entries,
        privacy_class: "policy_gated".to_string(),
        created_at: FIXTURE_CREATED_AT.to_string(),
    };

    let manifest =
        fixture_projection_manifest_from_content_tree(projection, view, &content_tree, &blobs)
            .map_err(projection_materialization_error)?;
    Ok((manifest, blobs))
}

fn local_projection_manifest_json(manifest: &ProjectionManifestRecord) -> String {
    format!(
        concat!(
            "{{",
            "\"privacy_class\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"id\":\"{}\",",
            "\"manifest_ref\":\"{}\",",
            "\"manifest_digest\":\"{}\",",
            "\"entry_count\":{},",
            "\"summary\":{},",
            "\"manifest\":{}",
            "}}"
        ),
        manifest.privacy_class.as_str(),
        json_escape(&manifest.projection_id),
        json_escape(&manifest.id),
        json_escape(&projection_manifest_ref(manifest)),
        json_escape(&manifest.manifest_digest),
        manifest.entries.len(),
        projection_manifest_summary_json(manifest),
        projection_manifest_record_json(manifest),
    )
}

fn projection_manifest_record_json(manifest: &ProjectionManifestRecord) -> String {
    let bytes = canonical_json_bytes(&manifest.to_json_value())
        .expect("projection manifest JSON should serialize canonically");
    String::from_utf8(bytes).expect("projection manifest JSON should be valid UTF-8")
}

fn projection_manifest_summary_json(manifest: &ProjectionManifestRecord) -> String {
    format!(
        concat!(
            "{{",
            "\"directories\":{},",
            "\"files\":{},",
            "\"bytes\":{},",
            "\"executable_files\":{}",
            "}}"
        ),
        manifest.summary.directories,
        manifest.summary.files,
        manifest.summary.bytes,
        manifest.summary.executable_files,
    )
}

fn fixture_inspect_projection_json(
    projection: &ProjectionRecord,
    projection_root: Option<&std::path::Path>,
    integrity_fixture: Option<StoreIntegrityFixture>,
) -> Result<String, CliError> {
    ensure_store_integrity_fixture_scope(projection, integrity_fixture)?;
    let manifest = fixture_local_projection_manifest(projection)?;
    let store_integrity =
        fixture_projection_store_integrity_result(projection, &manifest, integrity_fixture);
    persist_projection_quarantine_record_if_available(projection_root, &store_integrity)?;
    Ok(format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.projection\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"projection_id\":\"{}\",\"resolved_view_id\":\"{}\"}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"projection\":{},",
            "\"local_projection_manifest\":{},",
            "\"local_store_integrity\":{},",
            "\"local_quarantine\":{},",
            "\"local_root_verification\":{}{}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&projection.repository_id),
        json_escape(&projection.id),
        json_escape(&projection.resolved_view_id),
        json_escape(&projection.resolved_view_id),
        single_repo_tree_json(&projection.tree_identity),
        projection_record_json(projection),
        local_projection_manifest_json(&manifest),
        projection_local_store_integrity_json(&store_integrity),
        projection_quarantine_json(&store_integrity),
        local_projection_root_verification_json(projection, &manifest, projection_root),
        compat_projection_inspect_extension_json(projection),
    ))
}

fn ensure_store_integrity_fixture_scope(
    projection: &ProjectionRecord,
    integrity_fixture: Option<StoreIntegrityFixture>,
) -> Result<(), CliError> {
    if integrity_fixture.is_some() && projection.id != FIXTURE_EXECUTION_PROJECTION_ID {
        let fixture = integrity_fixture
            .map(StoreIntegrityFixture::as_str)
            .unwrap_or("unknown");
        return Err(invalid_request(
            "store integrity fixture applies only to the basic-app execution projection",
        )
        .with_detail("projection_id", projection.id.clone())
        .with_detail("integrity_fixture", fixture));
    }
    Ok(())
}

fn fixture_projection_store_integrity_result(
    projection: &ProjectionRecord,
    manifest: &ProjectionManifestRecord,
    integrity_fixture: Option<StoreIntegrityFixture>,
) -> ProjectionStoreIntegrityResult {
    match integrity_fixture {
        Some(StoreIntegrityFixture::ScanMissingBlob) => {
            let store = InMemoryArtifactStore::fixture_basic_app();
            let mut blobs = store.content_blobs().clone();
            if let Some(entry) = manifest.entries.iter().find(|entry| !entry.tombstone) {
                blobs.remove(&entry.content_hash);
            }
            projection_store_integrity_from_manifest_scan(projection, manifest, &blobs)
        }
        Some(StoreIntegrityFixture::StoreMismatch) => {
            projection_store_integrity_failed_quarantined(
                projection,
                manifest,
                ProjectionStoreIntegrityReasonCode::ExecutionStoreIntegrityFailed,
            )
        }
        Some(StoreIntegrityFixture::Verified) => {
            let store = InMemoryArtifactStore::fixture_basic_app();
            projection_store_integrity_from_manifest_scan(
                projection,
                manifest,
                store.content_blobs(),
            )
        }
        None => projection_store_integrity_not_checked(projection),
    }
}

fn persist_projection_quarantine_record_if_available(
    projection_root: Option<&std::path::Path>,
    integrity: &ProjectionStoreIntegrityResult,
) -> Result<(), CliError> {
    let Some(projection_root) = projection_root else {
        return Ok(());
    };
    let Some(quarantine) = integrity.quarantine.as_ref() else {
        return Ok(());
    };
    persist_projection_quarantine_local_record(projection_root, quarantine).map_err(|error| {
        CliError::new(
            "projection_quarantine_record_write_failed",
            "failed to write projection quarantine local record",
        )
        .with_detail("projection_root", projection_root.display().to_string())
        .with_detail("projection_id", quarantine.projection_id.clone())
        .with_detail("reason_code", quarantine.reason_code.as_str())
        .with_detail("error", error.to_string())
    })?;
    Ok(())
}

fn projection_local_store_integrity_json(integrity: &ProjectionStoreIntegrityResult) -> String {
    match integrity.integrity_status {
        ProjectionStoreIntegrityStatus::Failed => {
            let reason_code = integrity
                .reason_code
                .unwrap_or(ProjectionStoreIntegrityReasonCode::ExecutionStoreIntegrityFailed);
            format!(
                concat!(
                    "{{",
                    "\"privacy_class\":\"local_only\",",
                    "\"integrity_status\":\"failed\",",
                    "\"policy\":\"{}\",",
                    "\"reason\":\"{}\",",
                    "\"reason_code\":\"execution_store_integrity_failed\",",
                    "\"projection_id\":\"{}\",",
                    "\"resolved_view_id\":\"{}\",",
                    "\"tree_identity\":{},",
                    "\"root_ref\":{},",
                    "\"cache_key\":\"{}\",",
                    "\"manifest_ref\":\"{}\",",
                    "\"manifest_digest\":\"{}\",",
                    "\"source_truth\":\"immutable_store_manifest\",",
                    "\"local_filesystem_source_truth\":false",
                    "}}"
                ),
                integrity.policy.as_str(),
                reason_code.reason(),
                json_escape(&integrity.projection_id),
                json_escape(&integrity.resolved_view_id),
                single_repo_tree_json(&integrity.tree_identity),
                projection_root_ref_value_json(&integrity.root_ref),
                json_escape(&integrity.cache_key),
                json_escape(integrity.manifest_ref.as_deref().unwrap_or("")),
                json_escape(integrity.manifest_digest.as_deref().unwrap_or("")),
            )
        }
        ProjectionStoreIntegrityStatus::Verified => format!(
            concat!(
                "{{",
                "\"privacy_class\":\"local_only\",",
                "\"integrity_status\":\"verified\",",
                "\"policy\":\"{}\",",
                "\"projection_id\":\"{}\",",
                "\"resolved_view_id\":\"{}\",",
                "\"tree_identity\":{},",
                "\"root_ref\":{},",
                "\"cache_key\":\"{}\",",
                "\"manifest_ref\":\"{}\",",
                "\"manifest_digest\":\"{}\",",
                "\"source_truth\":\"{}\",",
                "\"local_filesystem_source_truth\":false",
                "}}"
            ),
            integrity.policy.as_str(),
            json_escape(&integrity.projection_id),
            json_escape(&integrity.resolved_view_id),
            single_repo_tree_json(&integrity.tree_identity),
            projection_root_ref_value_json(&integrity.root_ref),
            json_escape(&integrity.cache_key),
            json_escape(integrity.manifest_ref.as_deref().unwrap_or("")),
            json_escape(integrity.manifest_digest.as_deref().unwrap_or("")),
            integrity.source_truth.as_str(),
        ),
        ProjectionStoreIntegrityStatus::NotChecked => format!(
            concat!(
                "{{",
                "\"privacy_class\":\"local_only\",",
                "\"integrity_status\":\"not_checked\",",
                "\"policy\":\"{}\",",
                "\"projection_id\":\"{}\",",
                "\"source_truth\":\"not_checked\",",
                "\"local_filesystem_source_truth\":false",
                "}}"
            ),
            integrity.policy.as_str(),
            json_escape(&integrity.projection_id),
        ),
    }
}

fn projection_quarantine_json(integrity: &ProjectionStoreIntegrityResult) -> String {
    match &integrity.quarantine {
        Some(quarantine) => format!(
            concat!(
                "{{",
                "\"privacy_class\":\"local_only\",",
                "\"state\":\"quarantined\",",
                "\"reason\":\"{}\",",
                "\"reason_code\":\"execution_store_integrity_failed\",",
                "\"projection_id\":\"{}\",",
                "\"resolved_view_id\":\"{}\",",
                "\"root_ref\":{},",
                "\"cache_key\":\"{}\",",
                "\"manifest_ref\":\"{}\",",
                "\"manifest_digest\":\"{}\",",
                "\"quarantine_refs\":{{",
                "\"projection\":\"{}\",",
                "\"cache\":\"{}\",",
                "\"native_error\":\"{}\"",
                "}},",
                "\"provenance\":{{",
                "\"repository_id\":\"{}\",",
                "\"resolved_view_id\":\"{}\",",
                "\"tree_identity\":{},",
                "\"created_from_content_tree\":\"{}\",",
                "\"store_integrity_policy\":\"{}\"",
                "}},",
                "\"source_truth\":\"immutable_store_manifest\",",
                "\"local_filesystem_source_truth\":false,",
                "\"durable_record\":{},",
                "\"cache_reuse_allowed\":{},",
                "\"cache_invalidation_reason\":\"{}\"",
                "}}"
            ),
            quarantine.reason_code.reason(),
            json_escape(&quarantine.projection_id),
            json_escape(&quarantine.resolved_view_id),
            projection_root_ref_value_json(&quarantine.root_ref),
            json_escape(&quarantine.cache_key),
            json_escape(quarantine.manifest_ref.as_deref().unwrap_or("")),
            json_escape(quarantine.manifest_digest.as_deref().unwrap_or("")),
            json_escape(&quarantine.quarantine_refs.projection),
            json_escape(&quarantine.quarantine_refs.cache),
            json_escape(&quarantine.quarantine_refs.native_error),
            json_escape(&quarantine.provenance.repository_id),
            json_escape(&quarantine.provenance.resolved_view_id),
            single_repo_tree_json(&quarantine.provenance.tree_identity),
            json_escape(&quarantine.provenance.created_from_content_tree),
            quarantine.provenance.store_integrity_policy.as_str(),
            optional_string_json(quarantine.durable_record.as_deref()),
            quarantine.cache_reuse_allowed,
            quarantine.cache_invalidation_reason.as_str(),
        ),
        None => "null".to_string(),
    }
}

fn projection_quarantine_durable_record_json(integrity: &ProjectionStoreIntegrityResult) -> String {
    optional_string_json(
        integrity
            .quarantine
            .as_ref()
            .and_then(|quarantine| quarantine.durable_record.as_deref()),
    )
}

fn projection_quarantine_cache_reuse_allowed_json(
    integrity: &ProjectionStoreIntegrityResult,
) -> bool {
    integrity
        .quarantine
        .as_ref()
        .map(|quarantine| quarantine.cache_reuse_allowed)
        .unwrap_or(true)
}

fn projection_quarantine_cache_invalidation_reason(
    integrity: &ProjectionStoreIntegrityResult,
) -> String {
    integrity
        .quarantine
        .as_ref()
        .map(|quarantine| quarantine.cache_invalidation_reason.as_str())
        .unwrap_or("none")
        .to_string()
}

fn projection_native_errors_json(
    projection: &ProjectionRecord,
    manifest: &ProjectionManifestRecord,
    integrity: &ProjectionStoreIntegrityResult,
    integrity_fixture: Option<StoreIntegrityFixture>,
) -> String {
    match integrity_fixture {
        Some(StoreIntegrityFixture::ScanMissingBlob)
        | Some(StoreIntegrityFixture::StoreMismatch) => {
            format!(
                concat!(
                    "[{{",
                    "\"code\":\"execution_store_integrity_failed\",",
                    "\"message\":\"projection store integrity verification failed for fixture {}\",",
                    "\"projection_id\":\"{}\",",
                    "\"resolved_view_id\":\"{}\",",
                    "\"integrity_status\":\"failed\",",
                    "\"quarantine_reason\":\"store_integrity_mismatch\",",
                    "\"root_ref\":{},",
                    "\"cache_key\":\"{}\",",
                    "\"manifest_ref\":\"{}\",",
                    "\"manifest_digest\":\"{}\",",
                    "\"durable_record\":{},",
                    "\"cache_reuse_allowed\":{},",
                    "\"cache_invalidation_reason\":\"{}\",",
                    "\"privacy_class\":\"local_only\"",
                    "}}]"
                ),
                integrity_fixture
                    .map(StoreIntegrityFixture::as_str)
                    .unwrap_or("unknown"),
                json_escape(&projection.id),
                json_escape(&projection.resolved_view_id),
                projection_root_ref_json(projection),
                json_escape(&projection.cache_key.stable_string()),
                json_escape(&projection_manifest_ref(manifest)),
                json_escape(&manifest.manifest_digest),
                projection_quarantine_durable_record_json(integrity),
                projection_quarantine_cache_reuse_allowed_json(integrity),
                json_escape(&projection_quarantine_cache_invalidation_reason(integrity)),
            )
        }
        Some(StoreIntegrityFixture::Verified) | None => "[]".to_string(),
    }
}

fn projection_lifecycle_state(
    projection: &ProjectionRecord,
    projection_root: Option<&std::path::Path>,
) -> &'static str {
    if matches!(
        projection.retention_state,
        sunlight_core::projection::ProjectionRetentionState::Quarantined
    ) {
        return "quarantined";
    }
    if let Some(root) = projection_root {
        if !root.exists() {
            return "removed";
        }
    }
    "materialized"
}

#[derive(Debug, Default)]
struct LocalProjectionRootScan {
    exists: bool,
    is_dir: bool,
    directories: usize,
    files: usize,
    bytes: u64,
    executable_files: usize,
    sample_paths: Vec<String>,
    all_file_paths: Vec<String>,
    scan_error: Option<String>,
}

#[derive(Debug)]
struct LocalProjectionRootVerification {
    scan: LocalProjectionRootScan,
    verification_state: &'static str,
    content_verification: &'static str,
    manifest_ref: String,
    manifest_digest: String,
    dirty_local: Option<bool>,
    mismatched_files: usize,
    missing_files: usize,
    extra_files: usize,
    metadata_mismatches: usize,
    verification_errors: Vec<String>,
}

impl LocalProjectionRootVerification {
    fn dirty_local_json(&self) -> String {
        optional_bool_json(self.dirty_local)
    }
}

#[derive(Debug)]
enum PersistedLocalManifestBinding {
    Unavailable,
    Invalid,
    Available { normalized_root_ref: String },
}

fn local_projection_root_verification_json(
    projection: &ProjectionRecord,
    manifest: &ProjectionManifestRecord,
    projection_root: Option<&std::path::Path>,
) -> String {
    let verification = local_projection_root_verification(projection, manifest, projection_root);
    local_projection_root_verification_json_from_verification(projection_root, &verification)
}

fn local_projection_root_verification_json_from_verification(
    projection_root: Option<&std::path::Path>,
    verification: &LocalProjectionRootVerification,
) -> String {
    let Some(root) = projection_root else {
        return "null".to_string();
    };
    let scan = &verification.scan;
    format!(
        concat!(
            "{{",
            "\"projection_root\":{},",
            "\"verification_state\":\"{}\",",
            "\"content_verification\":\"{}\",",
            "\"manifest_ref\":\"{}\",",
            "\"manifest_digest\":\"{}\",",
            "\"exists\":{},",
            "\"is_dir\":{},",
            "\"directories\":{},",
            "\"files\":{},",
            "\"bytes\":{},",
            "\"executable_files\":{},",
            "\"dirty_local\":{},",
            "\"mismatched_files\":{},",
            "\"missing_files\":{},",
            "\"extra_files\":{},",
            "\"metadata_mismatches\":{},",
            "\"verification_errors\":{},",
            "\"sample_paths\":{},",
            "\"scan_error\":{}",
            "}}"
        ),
        local_projection_root_json(root),
        verification.verification_state,
        verification.content_verification,
        json_escape(&verification.manifest_ref),
        json_escape(&verification.manifest_digest),
        scan.exists,
        scan.is_dir,
        scan.directories,
        scan.files,
        scan.bytes,
        scan.executable_files,
        verification.dirty_local_json(),
        verification.mismatched_files,
        verification.missing_files,
        verification.extra_files,
        verification.metadata_mismatches,
        string_array_json(verification.verification_errors.iter().map(String::as_str)),
        string_array_json(scan.sample_paths.iter().map(String::as_str)),
        optional_string_json(scan.scan_error.as_deref()),
    )
}

fn local_projection_root_verification(
    projection: &ProjectionRecord,
    manifest: &ProjectionManifestRecord,
    projection_root: Option<&std::path::Path>,
) -> LocalProjectionRootVerification {
    let Some(root) = projection_root else {
        return LocalProjectionRootVerification {
            scan: LocalProjectionRootScan::default(),
            verification_state: "not_supplied",
            content_verification: "verification_error",
            manifest_ref: projection_manifest_ref(manifest),
            manifest_digest: manifest.manifest_digest.clone(),
            dirty_local: None,
            mismatched_files: 0,
            missing_files: 0,
            extra_files: 0,
            metadata_mismatches: 0,
            verification_errors: vec!["projection_root_not_supplied".to_string()],
        };
    };
    let scan = scan_local_projection_root(root);
    let state = if !scan.exists {
        "missing"
    } else if !scan.is_dir {
        "not_directory"
    } else if scan.scan_error.is_some() {
        "scan_failed"
    } else {
        "present"
    };
    if state != "present" {
        let error = match state {
            "missing" => "projection_root_missing",
            "not_directory" => "projection_root_not_directory",
            "scan_failed" => "projection_root_scan_failed",
            _ => "projection_root_unverified",
        };
        return LocalProjectionRootVerification {
            scan,
            verification_state: state,
            content_verification: "verification_error",
            manifest_ref: projection_manifest_ref(manifest),
            manifest_digest: manifest.manifest_digest.clone(),
            dirty_local: None,
            mismatched_files: 0,
            missing_files: 0,
            extra_files: 0,
            metadata_mismatches: 0,
            verification_errors: vec![error.to_string()],
        };
    }

    match persisted_local_manifest_binding(root, projection, manifest) {
        PersistedLocalManifestBinding::Available {
            normalized_root_ref,
        } if normalized_root_ref != projection.root_ref.value => {
            return LocalProjectionRootVerification {
                scan,
                verification_state: state,
                content_verification: "root_mismatch",
                manifest_ref: projection_manifest_ref(manifest),
                manifest_digest: manifest.manifest_digest.clone(),
                dirty_local: None,
                mismatched_files: 0,
                missing_files: 0,
                extra_files: 0,
                metadata_mismatches: 0,
                verification_errors: Vec::new(),
            };
        }
        PersistedLocalManifestBinding::Invalid => {
            return LocalProjectionRootVerification {
                scan,
                verification_state: state,
                content_verification: "manifest_invalid",
                manifest_ref: projection_manifest_ref(manifest),
                manifest_digest: manifest.manifest_digest.clone(),
                dirty_local: None,
                mismatched_files: 0,
                missing_files: 0,
                extra_files: 0,
                metadata_mismatches: 0,
                verification_errors: vec!["projection_manifest_local_invalid".to_string()],
            };
        }
        PersistedLocalManifestBinding::Available { .. }
        | PersistedLocalManifestBinding::Unavailable => {}
    }

    let fixture_store = InMemoryArtifactStore::fixture_basic_app();
    let expected_paths = manifest
        .entries
        .iter()
        .map(|entry| entry.path.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let local_paths = scan
        .all_file_paths
        .iter()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    let missing_files = expected_paths.difference(&local_paths).count();
    let extra_files = local_paths.difference(&expected_paths).count();
    let mut mismatched_files = 0;
    #[cfg(unix)]
    let mut metadata_mismatches = 0;
    #[cfg(not(unix))]
    let metadata_mismatches = 0;
    let mut verification_errors = Vec::new();

    for entry in &manifest.entries {
        let local_path = root.join(&entry.path);
        if !local_path.is_file() {
            continue;
        }
        match fixture_store.content_blobs().get(&entry.content_hash) {
            Some(blob) => match fs::read(&local_path) {
                Ok(bytes) if bytes == blob.bytes => {}
                Ok(_) => mismatched_files += 1,
                Err(_) => {
                    mismatched_files += 1;
                    verification_errors.push(format!("read_failed:{}", entry.path));
                }
            },
            None => {
                verification_errors.push(format!("missing_fixture_blob:{}", entry.content_hash))
            }
        }

        #[cfg(unix)]
        {
            if let Ok(metadata) = fs::symlink_metadata(&local_path) {
                if local_file_is_executable(&metadata) != entry.executable {
                    metadata_mismatches += 1;
                }
            }
        }
    }

    let verified = missing_files == 0
        && extra_files == 0
        && mismatched_files == 0
        && metadata_mismatches == 0
        && verification_errors.is_empty();
    LocalProjectionRootVerification {
        scan,
        verification_state: state,
        content_verification: if verified { "verified" } else { "dirty" },
        manifest_ref: projection_manifest_ref(manifest),
        manifest_digest: manifest.manifest_digest.clone(),
        dirty_local: Some(!verified),
        mismatched_files,
        missing_files,
        extra_files,
        metadata_mismatches,
        verification_errors,
    }
}

fn persisted_local_manifest_binding(
    projection_root: &std::path::Path,
    projection: &ProjectionRecord,
    manifest: &ProjectionManifestRecord,
) -> PersistedLocalManifestBinding {
    let path = projection_manifest_local_record_path(projection_root, projection);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return PersistedLocalManifestBinding::Unavailable;
        }
        Err(_) => return PersistedLocalManifestBinding::Invalid,
    };
    let Ok(record) = parse_json_record(&bytes) else {
        return PersistedLocalManifestBinding::Invalid;
    };
    parse_persisted_local_manifest_binding(&record, projection, manifest)
}

fn parse_persisted_local_manifest_binding(
    record: &JsonValue,
    projection: &ProjectionRecord,
    expected_manifest: &ProjectionManifestRecord,
) -> PersistedLocalManifestBinding {
    let JsonValue::Object(envelope) = record else {
        return PersistedLocalManifestBinding::Invalid;
    };
    match envelope.get("privacy_class") {
        Some(JsonValue::String(value)) if value == "local_only" => {}
        _ => return PersistedLocalManifestBinding::Invalid,
    }
    let Some(JsonValue::Object(manifest)) = envelope.get("manifest") else {
        return PersistedLocalManifestBinding::Invalid;
    };
    if !manifest_string_field_matches(
        manifest,
        "manifest_digest",
        &expected_manifest.manifest_digest,
    ) || !manifest_string_field_matches(manifest, "projection_id", &projection.id)
        || !manifest_string_field_matches(
            manifest,
            "resolved_view_id",
            &projection.resolved_view_id,
        )
        || !manifest_number_field_matches(
            manifest,
            "materialization_generation",
            expected_manifest.materialization_generation,
        )
    {
        return PersistedLocalManifestBinding::Invalid;
    }
    let Some(JsonValue::Object(manifest_root_ref)) = manifest.get("root_ref") else {
        return PersistedLocalManifestBinding::Invalid;
    };
    if !root_ref_matches(manifest_root_ref, &projection.root_ref.value) {
        return PersistedLocalManifestBinding::Invalid;
    }
    let Some(JsonValue::Object(root_binding)) = envelope.get("root_binding") else {
        return PersistedLocalManifestBinding::Invalid;
    };
    match root_binding.get("normalization") {
        Some(JsonValue::String(value)) if value == "local_uri_relative_v1" => {}
        _ => return PersistedLocalManifestBinding::Invalid,
    }
    match root_binding.get("privacy_class") {
        Some(JsonValue::String(value)) if value == "local_only" => {}
        _ => return PersistedLocalManifestBinding::Invalid,
    }
    let Some(JsonValue::Object(root_ref)) = root_binding.get("normalized_root_ref") else {
        return PersistedLocalManifestBinding::Invalid;
    };
    match root_ref.get("privacy") {
        Some(JsonValue::String(value)) if value == "local_only_path" => {}
        _ => return PersistedLocalManifestBinding::Invalid,
    }
    let Some(JsonValue::String(value)) = root_ref.get("value") else {
        return PersistedLocalManifestBinding::Invalid;
    };
    if !value.starts_with("local://.sunlight/projections/") {
        return PersistedLocalManifestBinding::Invalid;
    }
    PersistedLocalManifestBinding::Available {
        normalized_root_ref: value.clone(),
    }
}

fn manifest_string_field_matches(
    manifest: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
    expected: &str,
) -> bool {
    matches!(manifest.get(field), Some(JsonValue::String(value)) if value == expected)
}

fn manifest_number_field_matches(
    manifest: &std::collections::BTreeMap<String, JsonValue>,
    field: &str,
    expected: u64,
) -> bool {
    matches!(manifest.get(field), Some(JsonValue::Number(value)) if value == &expected.to_string())
}

fn root_ref_matches(
    root_ref: &std::collections::BTreeMap<String, JsonValue>,
    expected: &str,
) -> bool {
    matches!(root_ref.get("value"), Some(JsonValue::String(value)) if value == expected)
        && matches!(
            root_ref.get("privacy"),
            Some(JsonValue::String(value)) if value == "local_only_path"
        )
        && matches!(
            root_ref.get("privacy_class"),
            Some(JsonValue::String(value)) if value == "local_only"
        )
}

fn scan_local_projection_root(root: &std::path::Path) -> LocalProjectionRootScan {
    let root_metadata = fs::symlink_metadata(root);
    let mut scan = LocalProjectionRootScan {
        exists: root_metadata.is_ok(),
        is_dir: root_metadata
            .as_ref()
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false),
        ..LocalProjectionRootScan::default()
    };
    if !scan.exists || !scan.is_dir {
        return scan;
    }
    scan.directories = 1;
    if let Err(error) = scan_local_projection_root_inner(root, root, &mut scan) {
        scan.scan_error = Some(error.to_string());
    }
    scan.sample_paths.sort();
    if scan.sample_paths.len() > 8 {
        scan.sample_paths.truncate(8);
    }
    scan
}

fn scan_local_projection_root_inner(
    root: &std::path::Path,
    current: &std::path::Path,
    scan: &mut LocalProjectionRootScan,
) -> std::io::Result<()> {
    let mut entries = fs::read_dir(current)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if is_projection_local_metadata_path(root, &path) {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.is_dir() {
            let directories_before = scan.directories;
            let files_before = scan.files;
            scan_local_projection_root_inner(root, &path, scan)?;
            if !is_projection_local_metadata_parent_path(root, &path)
                || scan.directories > directories_before
                || scan.files > files_before
            {
                scan.directories += 1;
            }
        } else if metadata.is_file() {
            scan.files += 1;
            scan.bytes += metadata.len();
            scan.executable_files += usize::from(local_file_is_executable(&metadata));
            if let Ok(relative_path) = path.strip_prefix(root) {
                let relative_path = relative_path.display().to_string().replace('\\', "/");
                scan.sample_paths.push(relative_path.clone());
                scan.all_file_paths.push(relative_path);
            }
        }
    }

    Ok(())
}

#[cfg(unix)]
fn local_file_is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;

    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn local_file_is_executable(_metadata: &fs::Metadata) -> bool {
    false
}

fn projection_record_json(projection: &ProjectionRecord) -> String {
    format!(
        concat!(
            "{{",
            "\"schema_version\":{},",
            "\"record_type\":\"{}\",",
            "\"id\":\"{}\",",
            "\"repository_scope\":{{\"kind\":\"single\",\"repository_id\":\"{}\"}},",
            "\"purpose\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":{},",
            "\"tree_identity\":{},",
            "\"path_policy_id\":\"{}\",",
            "\"operation_semantics_version\":\"{}\",",
            "\"strategy\":\"{}\",",
            "\"root_ref\":{},",
            "\"created_from_content_tree\":\"{}\",",
            "\"baseline_manifest_ref\":{},",
            "\"writable_policy\":\"{}\",",
            "\"store_integrity_policy\":\"{}\",",
            "\"cache_key\":\"{}\",",
            "\"retention_state\":\"{}\",",
            "\"privacy_class\":\"{}\",",
            "\"created_at\":\"{}\"",
            "}}"
        ),
        projection.schema_version,
        projection.record_type,
        json_escape(&projection.id),
        json_escape(&projection.repository_id),
        projection.purpose.as_str(),
        json_escape(&projection.resolved_view_id),
        optional_string_json(projection.session_generation_id.as_deref()),
        single_repo_tree_json(&projection.tree_identity),
        json_escape(&projection.path_policy_id),
        json_escape(&projection.operation_semantics_version),
        projection.strategy.as_str(),
        projection_root_ref_json(projection),
        json_escape(&projection.created_from_content_tree),
        optional_string_json(projection.baseline_manifest_ref.as_deref()),
        projection.writable_policy.as_str(),
        projection.store_integrity_policy.as_str(),
        json_escape(&projection.cache_key.stable_string()),
        projection.retention_state.as_str(),
        projection.privacy_class.as_str(),
        json_escape(&projection.created_at),
    )
}

fn projection_root_ref_json(projection: &ProjectionRecord) -> String {
    projection_root_ref_value_json(&projection.root_ref)
}

fn projection_root_ref_value_json(root_ref: &ProjectionRootRef) -> String {
    format!(
        concat!(
            "{{",
            "\"value\":\"{}\",",
            "\"privacy\":\"{}\",",
            "\"privacy_class\":\"{}\"",
            "}}"
        ),
        json_escape(&root_ref.value),
        root_ref.privacy.as_str(),
        root_ref.privacy.privacy_class().as_str(),
    )
}

fn read_success_envelope(response: &ReadResponse) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"session_id\":\"{}\"}},",
            "\"view\":{},",
            "\"artifacts\":[{}],",
            "\"content\":{{\"encoding\":\"{}\",\"bytes\":\"{}\"}}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(response.command),
        json_escape(&response.repository_id),
        json_escape(&response.session_id),
        view_json(&response.view),
        artifact_json(&response.artifact),
        json_escape(&response.content.encoding),
        json_escape(&response.content.bytes),
    )
}

fn list_success_envelope(response: &ListResponse) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"session_id\":\"{}\"}},",
            "\"view\":{},",
            "\"artifacts\":[{}]",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(response.command),
        json_escape(&response.repository_id),
        json_escape(&response.session_id),
        view_json(&response.view),
        response
            .artifacts
            .iter()
            .map(artifact_json)
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn search_success_envelope(response: &SearchResponse) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"session_id\":\"{}\"}},",
            "\"view\":{},",
            "\"matches\":[{}]",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(response.command),
        json_escape(&response.repository_id),
        json_escape(&response.session_id),
        view_json(&response.view),
        response
            .matches
            .iter()
            .map(|item| {
                format!(
                    concat!(
                        "{{",
                        "\"artifact_id\":\"{}\",",
                        "\"path\":\"{}\",",
                        "\"content_hash\":\"{}\",",
                        "\"line\":{},",
                        "\"snippet\":\"{}\"",
                        "}}"
                    ),
                    json_escape(&item.artifact_id),
                    json_escape(&item.path),
                    json_escape(&item.content_hash),
                    item.line,
                    json_escape(&item.snippet),
                )
            })
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn view_read_success_envelope(
    state: &RealRepoState,
    view: &ResolvedViewResult,
    entry: &RealArtifactEntry,
    bytes: &str,
) -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"artifact.read\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"resolved_view_id\":\"{}\"}},",
            "\"access_mode\":\"read_only_view\",",
            "\"view\":{},",
            "\"artifacts\":[{}],",
            "\"content\":{{\"encoding\":\"utf-8\",\"bytes\":\"{}\"}}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&state.repository_id),
        json_escape(&view.resolved_view_id),
        view_resolve_view_json(view),
        artifact_json(&real_artifact_view(entry)),
        json_escape(bytes),
    )
}

fn view_list_success_envelope(
    state: &RealRepoState,
    view: &ResolvedViewResult,
    artifacts: &[SessionVisibleArtifactView],
) -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"artifact.list\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"resolved_view_id\":\"{}\"}},",
            "\"access_mode\":\"read_only_view\",",
            "\"view\":{},",
            "\"artifacts\":[{}]",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&state.repository_id),
        json_escape(&view.resolved_view_id),
        view_resolve_view_json(view),
        artifacts
            .iter()
            .map(artifact_json)
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn view_search_success_envelope(
    state: &RealRepoState,
    view: &ResolvedViewResult,
    matches: &[sunlight_core::artifacts::SearchMatch],
) -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"artifact.search\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"resolved_view_id\":\"{}\"}},",
            "\"access_mode\":\"read_only_view\",",
            "\"view\":{},",
            "\"matches\":[{}]",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&state.repository_id),
        json_escape(&view.resolved_view_id),
        view_resolve_view_json(view),
        matches
            .iter()
            .map(|item| {
                format!(
                    "{{\"artifact_id\":\"{}\",\"path\":\"{}\",\"content_hash\":\"{}\",\"line\":{},\"snippet\":\"{}\"}}",
                    json_escape(&item.artifact_id),
                    json_escape(&item.path),
                    json_escape(&item.content_hash),
                    item.line,
                    json_escape(&item.snippet),
                )
            })
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn mutation_success_envelope(response: &MutationResponse) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"session_id\":\"{}\",",
            "\"operation_transaction_id\":\"{}\",",
            "\"topic_revision_id\":\"{}\"",
            "}},",
            "\"view\":{},",
            "\"artifacts\":[{}],",
            "\"operation\":{},",
            "\"topic_revision\":{},",
            "\"session_generation\":{}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(response.command),
        json_escape(&response.repository_id),
        json_escape(&response.session_id),
        json_escape(&response.operation.id),
        json_escape(&response.topic_revision.id),
        view_json(&response.view),
        mutation_artifact_json(&response.artifact),
        operation_json(response),
        topic_revision_json(response),
        session_generation_json(response),
    )
}

fn promotion_success_envelope(
    response: &MutationResponse,
    candidate: &PromotionCandidateProvenance,
) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"execution.promote_output\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"session_id\":\"{}\",",
            "\"execution_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"operation_transaction_id\":\"{}\",",
            "\"topic_revision_id\":\"{}\"",
            "}},",
            "\"view\":{},",
            "\"artifacts\":[{}],",
            "\"promotion_source\":{},",
            "\"operation\":{},",
            "\"topic_revision\":{},",
            "\"session_generation\":{}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(&response.repository_id),
        json_escape(&response.session_id),
        json_escape(&candidate.execution_id),
        json_escape(&candidate.projection_id),
        json_escape(&response.operation.id),
        json_escape(&response.topic_revision.id),
        view_json(&response.view),
        mutation_artifact_json(&response.artifact),
        promotion_source_json(candidate),
        promotion_operation_json(response, candidate),
        topic_revision_json(response),
        session_generation_json(response),
    )
}

fn exact_requested_frontier(
    selections: &[TopicRevisionSelection],
) -> Result<BTreeMap<String, String>, CliError> {
    let mut frontier = BTreeMap::new();
    for selection in selections {
        if let Some(previous) =
            frontier.insert(selection.topic_id.clone(), selection.revision_id.clone())
        {
            return Err(CliError::new(
                "duplicate_topic_selection",
                "an exact view may select only one revision per topic",
            )
            .with_detail("topic_id", selection.topic_id.clone())
            .with_detail("first_revision_id", previous)
            .with_detail("second_revision_id", selection.revision_id.clone()));
        }
    }
    Ok(frontier)
}

fn ensure_resolver_preserved_frontier(
    requested: &BTreeMap<String, String>,
    normalized: &BTreeMap<String, String>,
) -> Result<(), CliError> {
    if requested.is_empty() || requested == normalized {
        return Ok(());
    }
    Err(CliError::new(
        "resolver_frontier_input_lost",
        "the resolver did not preserve the exact requested topic frontier",
    )
    .with_raw_details_json(format!(
        "{{\"requested_frontier\":{},\"normalized_frontier\":{}}}",
        string_string_map_json(requested),
        string_string_map_json(normalized),
    )))
}

fn view_resolve_success_envelope(
    result: &ResolvedViewResult,
    requested_frontier: &BTreeMap<String, String>,
    starting_checkpoint_id: &str,
    starting_frontier: &BTreeMap<String, String>,
) -> String {
    let conflict_ids = result
        .conflicts()
        .map(|record| record.id.as_str())
        .collect::<Vec<_>>();
    let staleness_ids = result
        .staleness()
        .map(|record| record.id.as_str())
        .collect::<Vec<_>>();

    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"view.resolve\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"resolved_view_id\":\"{}\",\"base_checkpoint_id\":\"{}\",\"starting_checkpoint_id\":\"{}\"}},",
            "\"view\":{},",
            "\"resolved_view_id\":\"{}\",",
            "\"base_checkpoint_ids\":{},",
            "\"starting_checkpoint_id\":\"{}\",",
            "\"starting_frontier\":{},",
            "\"topic_frontier\":{},",
            "\"requested_frontier\":{},",
            "\"normalized_frontier\":{},",
            "\"dependency_closure\":{},",
            "\"operation_semantics_version\":\"{}\",",
            "\"path_policy_id\":\"{}\",",
            "\"resolver_order\":{},",
            "\"tree_identity\":{},",
            "\"conflict_ids\":{},",
            "\"staleness_ids\":{},",
            "\"conflicts\":[{}],",
            "\"staleness\":[{}]",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(&result.repository_id),
        json_escape(&result.resolved_view_id),
        json_escape(&result.base_checkpoint_ids[0]),
        json_escape(starting_checkpoint_id),
        view_resolve_view_json(result),
        json_escape(&result.resolved_view_id),
        string_array_json(result.base_checkpoint_ids.iter().map(String::as_str)),
        json_escape(starting_checkpoint_id),
        string_string_map_json(starting_frontier),
        topic_frontier_json(result),
        string_string_map_json(requested_frontier),
        string_string_map_json(&result.topic_frontier),
        dependency_closure_json(&result.dependency_closure),
        json_escape(&result.operation_semantics_version),
        json_escape(&result.path_policy_id),
        resolver_order_json(&result.resolver_order),
        optional_single_repo_tree_json(result.tree_identity.as_ref()),
        string_array_json(conflict_ids.iter().copied()),
        string_array_json(staleness_ids.iter().copied()),
        result
            .conflicts()
            .map(resolver_record_json)
            .collect::<Vec<_>>()
            .join(","),
        result
            .staleness()
            .map(resolver_record_json)
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn execution_run_success_envelope(execution: &ExecutionRecord) -> String {
    let promotion_candidates = if execution.result.status.as_str() == "pass" {
        vec![fixture_promotion_candidate_provenance(execution)]
    } else {
        Vec::new()
    };

    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"execution.run\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"execution_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\"",
            "}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"execution_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"tree_identity\":{},",
            "\"result\":{},",
            "\"output_summary_counts\":{},",
            "\"promotion_candidates\":[{}]",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(&execution.repository_id),
        json_escape(&execution.id),
        json_escape(&execution.projection_id),
        json_escape(&execution.resolved_view_id),
        json_escape(&execution.resolved_view_id),
        single_repo_tree_json(&execution.tree_identity),
        json_escape(&execution.id),
        json_escape(&execution.projection_id),
        single_repo_tree_json(&execution.tree_identity),
        execution_result_json(execution),
        output_summary_counts_json(execution),
        promotion_candidates
            .iter()
            .map(promotion_candidate_json)
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn fixture_status_execution_json(
    execution: &ExecutionRecord,
    promotion: Option<&ExecutionOutputPromotionRecord>,
) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"status.execution\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"execution_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\"",
            "}},",
            "\"execution_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"result\":{},",
            "\"output_summary_counts\":{},",
            "\"promotion_status\":\"{}\",",
            "\"promotion_candidates\":[{}],",
            "\"promotions\":[{}],",
            "\"privacy_semantics\":{}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(&execution.repository_id),
        json_escape(&execution.id),
        json_escape(&execution.projection_id),
        json_escape(&execution.resolved_view_id),
        json_escape(&execution.id),
        json_escape(&execution.projection_id),
        json_escape(&execution.resolved_view_id),
        execution_result_json(execution),
        output_summary_counts_json(execution),
        fixture_execution_promotion_status(promotion),
        fixture_execution_promotion_candidates_json(execution, promotion),
        optional_promotion_record_json(promotion),
        execution_privacy_semantics_json(execution, promotion),
    )
}

fn fixture_inspect_execution_json(
    execution: &ExecutionRecord,
    promotion: Option<&ExecutionOutputPromotionRecord>,
) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"inspect.execution\",",
            "\"repository_id\":\"{}\",",
            "\"execution\":{},",
            "\"promotion_status\":\"{}\",",
            "\"promotion_candidates\":[{}],",
            "\"promotions\":[{}],",
            "\"privacy_semantics\":{}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
        json_escape(&execution.repository_id),
        execution_record_json(execution),
        fixture_execution_promotion_status(promotion),
        fixture_execution_promotion_candidates_json(execution, promotion),
        optional_promotion_record_json(promotion),
        execution_privacy_semantics_json(execution, promotion),
    )
}

fn execution_environment_summary_json(
    summary: &sunlight_core::execution::EnvironmentSummary,
) -> String {
    let tool_hints = summary
        .tool_hints
        .iter()
        .map(|hint| {
            format!(
                "{{\"name\":\"{}\",\"version_or_digest\":\"{}\"}}",
                json_escape(&hint.name),
                json_escape(&hint.version_or_digest),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{",
            "\"id\":\"{}\",",
            "\"os\":\"{}\",",
            "\"platform_hint\":\"{}\",",
            "\"arch\":\"{}\",",
            "\"sunlight_build_id\":\"{}\",",
            "\"command_runner_version\":\"{}\",",
            "\"tool_hints\":[{}],",
            "\"env_policy\":\"{}\",",
            "\"redacted_env_allowlist_digest\":\"{}\",",
            "\"network_policy\":\"{}\",",
            "\"sandbox_writable_policy\":\"{}\",",
            "\"digest\":\"{}\"",
            "}}"
        ),
        json_escape(&summary.id),
        json_escape(&summary.os),
        json_escape(&summary.platform_hint),
        json_escape(&summary.arch),
        json_escape(&summary.sunlight_build_id),
        json_escape(&summary.command_runner_version),
        tool_hints,
        json_escape(&summary.env_policy),
        json_escape(&summary.redacted_env_allowlist_digest),
        summary.network_policy.as_str(),
        summary.sandbox_writable_policy.as_str(),
        json_escape(&summary.digest),
    )
}

fn execution_record_json(execution: &ExecutionRecord) -> String {
    format!(
        concat!(
            "{{",
            "\"schema_version\":1,",
            "\"record_type\":\"execution\",",
            "\"id\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{},",
            "\"command\":{{\"argv\":{},\"shell\":{}}},",
            "\"working_directory\":\"{}\",",
            "\"environment_summary\":{},",
            "\"projection_id\":\"{}\",",
            "\"inputs\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_hash\":\"{}\",",
            "\"path_policy_id\":\"{}\",",
            "\"operation_semantics_version\":\"{}\"",
            "}},",
            "\"result\":{},",
            "\"output_summary_counts\":{},",
            "\"started_at\":\"{}\",",
            "\"finished_at\":\"{}\",",
            "\"privacy_class\":\"{}\"",
            "}}"
        ),
        json_escape(&execution.id),
        json_escape(&execution.repository_id),
        json_escape(&execution.resolved_view_id),
        single_repo_tree_json(&execution.tree_identity),
        string_array_json(execution.command.argv.iter().map(String::as_str)),
        optional_string_json(execution.command.shell.as_deref()),
        json_escape(&execution.working_directory),
        execution_environment_summary_json(&execution.environment_summary),
        json_escape(&execution.projection_id),
        json_escape(&execution.inputs.resolved_view_id),
        json_escape(&execution.inputs.tree_hash),
        json_escape(&execution.inputs.path_policy_id),
        json_escape(&execution.inputs.operation_semantics_version),
        execution_result_json(execution),
        output_summary_counts_json(execution),
        json_escape(&execution.started_at),
        json_escape(&execution.finished_at),
        execution.privacy_class.as_str(),
    )
}

fn fixture_execution_promotion_status(
    promotion: Option<&ExecutionOutputPromotionRecord>,
) -> &'static str {
    if promotion.is_some() {
        "promoted"
    } else {
        "promotion_required"
    }
}

fn fixture_execution_promotion_candidates_json(
    execution: &ExecutionRecord,
    promotion: Option<&ExecutionOutputPromotionRecord>,
) -> String {
    if promotion.is_some() || execution.result.status.as_str() != "pass" {
        return String::new();
    }
    promotion_candidate_json(&fixture_promotion_candidate_provenance(execution))
}

fn optional_promotion_record_json(promotion: Option<&ExecutionOutputPromotionRecord>) -> String {
    promotion
        .map(promotion_record_json)
        .unwrap_or_else(String::new)
}

fn promotion_record_json(record: &ExecutionOutputPromotionRecord) -> String {
    format!(
        concat!(
            "{{",
            "\"execution_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"output_path\":\"{}\",",
            "\"target_topic_id\":\"{}\",",
            "\"classification\":\"{}\",",
            "\"before_hash\":{},",
            "\"after_hash\":\"{}\",",
            "\"operation_transaction_id\":\"{}\",",
            "\"topic_revision_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"authored_context_id\":\"{}\",",
            "\"provenance_refs\":[{}]",
            "}}"
        ),
        json_escape(&record.execution_id),
        json_escape(&record.projection_id),
        json_escape(&record.output_path),
        json_escape(&record.target_topic_id),
        record.classification.as_str(),
        optional_string_json(record.before_hash.as_deref()),
        json_escape(&record.after_hash),
        json_escape(&record.operation_transaction_id),
        json_escape(&record.topic_revision_id),
        json_escape(&record.session_generation_id),
        json_escape(&record.authored_context_id),
        record
            .provenance_refs
            .iter()
            .map(promotion_provenance_ref_json)
            .collect::<Vec<_>>()
            .join(","),
    )
}

fn promotion_provenance_ref_json(reference: &ExecutionOutputPromotionProvenanceRef) -> String {
    format!(
        "{{\"kind\":\"{}\",\"id\":\"{}\"}}",
        reference.kind.as_str(),
        json_escape(&reference.id),
    )
}

fn execution_privacy_semantics_json(
    execution: &ExecutionRecord,
    promotion: Option<&ExecutionOutputPromotionRecord>,
) -> String {
    let promotion_record = if promotion.is_some() {
        "policy_gated"
    } else {
        "not_persisted"
    };
    format!(
        concat!(
            "{{",
            "\"execution_record\":\"{}\",",
            "\"raw_outputs\":\"local_only\",",
            "\"promotion_record\":\"{}\",",
            "\"durability\":\"fixture_only_not_persisted\"",
            "}}"
        ),
        execution.privacy_class.as_str(),
        promotion_record,
    )
}

fn checkpoint_json(checkpoint: &CheckpointRecord) -> String {
    format!(
        concat!(
            "{{",
            "\"schema_version\":1,",
            "\"record_type\":\"checkpoint\",",
            "\"id\":\"{}\",",
            "\"repository_scope\":{{\"kind\":\"single\",\"repository_id\":\"{}\"}},",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{},",
            "\"topic_frontier\":{},",
            "\"evidence_refs\":[{}],",
            "\"conflict_free\":{},",
            "\"created_by\":{{\"actor_id\":\"{}\",\"command\":\"{}\"}},",
            "\"created_at\":\"{}\",",
            "\"retention_class\":\"{}\",",
            "\"export_refs\":{},",
            "\"privacy_class\":\"{}\"",
            "}}"
        ),
        json_escape(&checkpoint.id),
        json_escape(&checkpoint.repository_id),
        json_escape(&checkpoint.resolved_view_id),
        single_repo_tree_json(&checkpoint.tree_identity),
        checkpoint_topic_frontier_json(checkpoint),
        checkpoint
            .evidence_refs
            .iter()
            .map(evidence_ref_json)
            .collect::<Vec<_>>()
            .join(","),
        checkpoint.conflict_free,
        json_escape(&checkpoint.created_by.actor_id),
        json_escape(&checkpoint.created_by.command),
        json_escape(&checkpoint.created_at),
        checkpoint.retention_class.as_str(),
        export_refs_json(checkpoint),
        checkpoint.privacy_class.as_str(),
    )
}

fn checkpoint_topic_frontier_json(checkpoint: &CheckpointRecord) -> String {
    let fields = checkpoint
        .topic_frontier
        .iter()
        .map(|entry| {
            format!(
                "\"{}\":\"{}\"",
                json_escape(&entry.topic_id),
                json_escape(&entry.topic_revision_id)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{fields}}}")
}

fn evidence_ref_json(evidence: &EvidenceRef) -> String {
    match evidence {
        EvidenceRef::Execution(execution) => format!(
            concat!(
                "{{",
                "\"kind\":\"execution\",",
                "\"execution_id\":\"{}\",",
                "\"result\":\"{}\",",
                "\"resolved_view_id\":\"{}\",",
                "\"tree_identity\":{}",
                "}}"
            ),
            json_escape(&execution.execution_id),
            execution.result.as_str(),
            json_escape(&execution.resolved_view_id),
            single_repo_tree_json(&execution.tree_identity),
        ),
    }
}

fn export_refs_json(checkpoint: &CheckpointRecord) -> String {
    format!(
        "[{}]",
        checkpoint
            .export_refs
            .iter()
            .map(|export_ref| {
                format!(
                    "{{\"export_map_id\":\"{}\"}}",
                    json_escape(&export_ref.export_map_id)
                )
            })
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn execution_result_json(execution: &ExecutionRecord) -> String {
    let termination_reason = execution
        .result
        .termination_reason
        .as_deref()
        .map(|reason| format!(",\"termination_reason\":\"{}\"", json_escape(reason)))
        .unwrap_or_default();
    format!(
        concat!(
            "{{",
            "\"status\":\"{}\",",
            "\"exit_code\":{},",
            "\"timed_out\":{}{}",
            "}}"
        ),
        execution.result.status.as_str(),
        execution
            .result
            .exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "null".to_string()),
        execution.result.timed_out,
        termination_reason,
    )
}

fn output_summary_counts_json(execution: &ExecutionRecord) -> String {
    let stdout = execution
        .outputs
        .iter()
        .filter(|output| output.kind == OutputKind::StdoutSummary)
        .count();
    let stderr = execution
        .outputs
        .iter()
        .filter(|output| output.kind == OutputKind::StderrSummary)
        .count();
    let file_delta = execution
        .outputs
        .iter()
        .filter(|output| output.kind == OutputKind::FileDelta)
        .count();
    let source_like = execution
        .outputs
        .iter()
        .filter(|output| output.classification == OutputClassification::SourceLikeDelta)
        .count();
    format!(
        concat!(
            "{{",
            "\"total\":{},",
            "\"stdout_summary\":{},",
            "\"stderr_summary\":{},",
            "\"file_delta\":{},",
            "\"source_like_delta\":{}",
            "}}"
        ),
        execution.outputs.len(),
        stdout,
        stderr,
        file_delta,
        source_like,
    )
}

fn promotion_candidate_json(candidate: &PromotionCandidateProvenance) -> String {
    format!(
        concat!(
            "{{",
            "\"execution_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"output_path\":\"{}\",",
            "\"target_topic_id\":\"{}\",",
            "\"classification\":\"{}\",",
            "\"before_hash\":{},",
            "\"after_hash\":\"{}\"",
            "}}"
        ),
        json_escape(&candidate.execution_id),
        json_escape(&candidate.projection_id),
        json_escape(&candidate.output_path),
        json_escape(&candidate.target_topic_id),
        candidate.classification.as_str(),
        optional_string_json(candidate.before_hash.as_deref()),
        json_escape(&candidate.after_hash),
    )
}

fn view_resolve_view_json(result: &ResolvedViewResult) -> String {
    let Some(tree_identity) = &result.tree_identity else {
        return "null".to_string();
    };

    format!(
        concat!(
            "{{",
            "\"resolved_view_id\":\"{}\",",
            "\"topic_frontier\":{},",
            "\"tree_identity\":{}",
            "}}"
        ),
        json_escape(&result.resolved_view_id),
        topic_frontier_json(result),
        single_repo_tree_json(tree_identity),
    )
}

fn topic_frontier_json(result: &ResolvedViewResult) -> String {
    let fields = result
        .topic_frontier
        .iter()
        .map(|(topic_id, revision_id)| {
            format!(
                "\"{}\":\"{}\"",
                json_escape(topic_id),
                json_escape(revision_id)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{fields}}}")
}

fn dependency_closure_json(closure: &DependencyClosure) -> String {
    format!(
        "{{\"revision_ids\":{}}}",
        string_array_json(closure.revision_ids.iter().map(String::as_str))
    )
}

fn resolver_order_json(order: &DeterministicResolverOrder) -> String {
    format!(
        "{{\"operation_ids\":{}}}",
        string_array_json(order.operation_ids.iter().map(String::as_str))
    )
}

fn optional_single_repo_tree_json(tree_identity: Option<&SingleRepoTree>) -> String {
    tree_identity
        .map(single_repo_tree_json)
        .unwrap_or_else(|| "null".to_string())
}

fn optional_projection_strategy_json(strategy: Option<ProjectionStrategy>) -> String {
    strategy
        .map(|strategy| format!("\"{}\"", strategy.as_str()))
        .unwrap_or_else(|| "null".to_string())
}

fn single_repo_tree_json(tree_identity: &SingleRepoTree) -> String {
    format!(
        concat!(
            "{{",
            "\"kind\":\"SingleRepoTree\",",
            "\"repository_id\":\"{}\",",
            "\"tree_hash\":\"{}\"",
            "}}"
        ),
        json_escape(&tree_identity.repository_id),
        json_escape(&tree_identity.tree_hash),
    )
}

fn resolver_record_json(record: &ResolverConflictOrStalenessRecord) -> String {
    let record_type = match record.kind {
        ResolverRecordKind::SameArtifactConflict | ResolverRecordKind::FrontierInconsistent => {
            "conflict"
        }
        ResolverRecordKind::MissingDependency | ResolverRecordKind::StaleDependency => "staleness",
    };

    format!(
        concat!(
            "{{",
            "\"id\":\"{}\",",
            "\"record_type\":\"{}\",",
            "\"kind\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"artifact_ids\":{},",
            "\"path_refs\":[{}],",
            "\"operation_ids\":{},",
            "\"authored_context_ids\":{},",
            "\"policy_reason\":\"{}\",",
            "\"candidate_refs\":{},",
            "\"resolution_operation_id\":{}",
            "}}"
        ),
        json_escape(&record.id),
        record_type,
        record.kind.as_str(),
        json_escape(&record.resolved_view_id),
        string_array_json(record.artifact_ids.iter().map(String::as_str)),
        record
            .path_refs
            .iter()
            .map(|path_ref| {
                format!(
                    "{{\"path\":\"{}\",\"path_state\":\"{}\"}}",
                    json_escape(&path_ref.path),
                    json_escape(&path_ref.path_state),
                )
            })
            .collect::<Vec<_>>()
            .join(","),
        string_array_json(record.operation_ids.iter().map(String::as_str)),
        string_array_json(record.authored_context_ids.iter().map(String::as_str)),
        json_escape(&record.policy_reason),
        string_array_map_json(&record.candidate_refs),
        optional_string_json(record.resolution_operation_id.as_deref()),
    )
}

fn normalize_promotion_mutation_response(
    response: &mut MutationResponse,
    candidate: &PromotionCandidateProvenance,
) {
    response.artifact.artifact_id = FIXTURE_PROMOTION_ARTIFACT_ID.to_string();
    response.artifact.after_hash = candidate.after_hash.clone();
    response.view.resolved_view_id = FIXTURE_PROMOTION_RESOLVED_VIEW_ID.to_string();
    response.view.session_generation_id = FIXTURE_PROMOTION_SESSION_GENERATION_ID.to_string();
    response.view.tree_identity.tree_hash = FIXTURE_PROMOTION_TREE_HASH.to_string();
    response.operation.id = FIXTURE_PROMOTION_OPERATION_TRANSACTION_ID.to_string();
    response.operation.authored_context_id = promotion_authored_context_id(candidate);
    response.operation.write_set[0].artifact_id = response.artifact.artifact_id.clone();
    response.operation.preconditions.expected_path = candidate.output_path.clone();
    response.operation.before_refs.tree_identity.tree_hash = candidate
        .before_hash
        .clone()
        .unwrap_or_else(|| FIXTURE_TREE_HASH.to_string());
    response.operation.after_refs.tree_identity.tree_hash = FIXTURE_PROMOTION_TREE_HASH.to_string();
    response.operation.before_refs.artifacts[0].artifact_id = None;
    response.operation.before_refs.artifacts[0].content_hash = candidate.before_hash.clone();
    response.operation.after_refs.artifacts[0].artifact_id =
        Some(response.artifact.artifact_id.clone());
    response.operation.after_refs.artifacts[0].content_hash = Some(candidate.after_hash.clone());
    if let MutationPayload::Write { content_hash, .. } = &mut response.operation.mutation_payload {
        *content_hash = candidate.after_hash.clone();
    }
    response.topic_revision.id = FIXTURE_PROMOTION_TOPIC_REVISION_ID.to_string();
    response.topic_revision.operation_transaction_id =
        FIXTURE_PROMOTION_OPERATION_TRANSACTION_ID.to_string();
    response.topic_revision.tree_delta_ref = "delta_promote_generated_auth_0001".to_string();
    response.session_generation.id = FIXTURE_PROMOTION_SESSION_GENERATION_ID.to_string();
    response.session_generation.resolved_view_id = FIXTURE_PROMOTION_RESOLVED_VIEW_ID.to_string();
    response.session_generation.topic_frontier.insert(
        FIXTURE_WRITE_TOPIC_ID.to_string(),
        FIXTURE_PROMOTION_TOPIC_REVISION_ID.to_string(),
    );
    response.session_generation.created_by_operation_id =
        FIXTURE_PROMOTION_OPERATION_TRANSACTION_ID.to_string();
}

fn fixture_promoted_generated_auth_bytes() -> Vec<u8> {
    b"export const generatedAuthPolicy = \"strict\";\n".to_vec()
}

fn promotion_source_json(candidate: &PromotionCandidateProvenance) -> String {
    format!(
        concat!(
            "{{",
            "\"execution_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"output_path\":\"{}\",",
            "\"target_topic_id\":\"{}\",",
            "\"classification\":\"{}\",",
            "\"before_hash\":{},",
            "\"after_hash\":\"{}\"",
            "}}"
        ),
        json_escape(&candidate.execution_id),
        json_escape(&candidate.projection_id),
        json_escape(&candidate.output_path),
        json_escape(&candidate.target_topic_id),
        candidate.classification.as_str(),
        optional_string_json(candidate.before_hash.as_deref()),
        json_escape(&candidate.after_hash),
    )
}

fn promotion_operation_json(
    response: &MutationResponse,
    candidate: &PromotionCandidateProvenance,
) -> String {
    let mut operation = operation_json(response);
    operation.pop();
    format!(
        concat!(
            "{},",
            "\"promotion_source\":{},",
            "\"execution_provenance\":{{",
            "\"execution_id\":\"{}\",",
            "\"projection_id\":\"{}\",",
            "\"output_path\":\"{}\",",
            "\"classification\":\"{}\"",
            "}}",
            "}}"
        ),
        operation,
        promotion_source_json(candidate),
        json_escape(&candidate.execution_id),
        json_escape(&candidate.projection_id),
        json_escape(&candidate.output_path),
        candidate.classification.as_str(),
    )
}

fn operation_json(response: &MutationResponse) -> String {
    let operation = &response.operation;
    let payload = mutation_payload_json(&operation.mutation_payload);
    let mut authored_frontier = response.session_generation.topic_frontier.clone();
    if let Some(parent) = &operation.preconditions.parent_topic_revision_id {
        authored_frontier.insert(operation.topic_id.clone(), parent.clone());
    } else {
        authored_frontier.remove(&operation.topic_id);
    }
    format!(
        concat!(
            "{{",
            "\"operation_transaction_id\":\"{}\",",
            "\"topic_id\":\"{}\",",
            "\"session_id\":\"{}\",",
            "\"actor_id\":\"{}\",",
            "\"authored_context_id\":\"{}\",",
            "\"mutation\":\"{}\",",
            "\"preconditions\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"write_topic_id\":\"{}\",",
            "\"parent_topic_revision_id\":{},",
            "\"path_policy_id\":\"{}\",",
            "\"operation_semantics_version\":\"{}\",",
            "\"expected_path\":\"{}\",",
            "\"expected_hash\":\"{}\"",
            "}},",
            "\"read_set\":{{\"mode\":\"full_authored_context\",\"resolved_view_id\":\"{}\",\"topic_frontier\":{}}},",
            "\"dependency_revision_ids\":{},",
            "\"write_set\":[{}],",
            "\"payload\":{},",
            "\"before_refs\":{},",
            "\"after_refs\":{}",
            "}}"
        ),
        json_escape(&operation.id),
        json_escape(&operation.topic_id),
        json_escape(&operation.session_id),
        json_escape(&operation.actor_id),
        json_escape(&operation.authored_context_id),
        operation.write_set[0].mutation.as_str(),
        json_escape(&operation.preconditions.resolved_view_id),
        json_escape(&operation.preconditions.session_generation_id),
        json_escape(&operation.preconditions.write_topic_id),
        optional_string_json(operation.preconditions.parent_topic_revision_id.as_deref()),
        json_escape(&operation.preconditions.path_policy_id),
        json_escape(&operation.preconditions.operation_semantics_version),
        json_escape(&operation.preconditions.expected_path),
        json_escape(operation.preconditions.expected_hash.as_str()),
        json_escape(&operation.preconditions.resolved_view_id),
        string_map_object_json(&authored_frontier),
        string_array_json(
            response
                .topic_revision
                .dependency_revision_ids
                .iter()
                .map(String::as_str)
        ),
        operation
            .write_set
            .iter()
            .map(|entry| {
                format!(
                    "{{\"artifact_id\":\"{}\",\"path\":\"{}\",\"mutation\":\"{}\"}}",
                    json_escape(&entry.artifact_id),
                    json_escape(&entry.path),
                    entry.mutation.as_str(),
                )
            })
            .collect::<Vec<_>>()
            .join(","),
        payload,
        refs_json(&operation.before_refs),
        refs_json(&operation.after_refs),
    )
}

fn mutation_payload_json(payload: &MutationPayload) -> String {
    match payload {
        MutationPayload::Patch {
            patch_digest,
            base_content_hash,
            result_content_hash,
            hunk_count,
            byte_delta,
            ..
        } => format!(
            concat!(
                "{{",
                "\"kind\":\"patch\",",
                "\"patch_digest\":\"{}\",",
                "\"base_content_hash\":\"{}\",",
                "\"result_content_hash\":\"{}\",",
                "\"hunk_count\":{},",
                "\"byte_delta\":{}",
                "}}"
            ),
            json_escape(patch_digest),
            json_escape(base_content_hash),
            json_escape(result_content_hash),
            hunk_count,
            byte_delta,
        ),
        MutationPayload::Write {
            write_mode,
            content_hash,
            byte_length,
            media_type,
            executable,
            classification,
        } => format!(
            concat!(
                "{{",
                "\"kind\":\"write\",",
                "\"write_mode\":\"{}\",",
                "\"content_hash\":\"{}\",",
                "\"byte_length\":{},",
                "\"media_type\":\"{}\",",
                "\"executable\":{},",
                "\"classification\":\"{}\"",
                "}}"
            ),
            write_mode_json(write_mode),
            json_escape(content_hash),
            byte_length,
            json_escape(media_type),
            executable,
            json_escape(classification),
        ),
        MutationPayload::Move {
            source_path,
            target_path,
            artifact_id,
            content_hash,
            source_path_state,
            target_path_state,
        } => format!(
            concat!(
                "{{",
                "\"kind\":\"move\",",
                "\"source_path\":\"{}\",",
                "\"target_path\":\"{}\",",
                "\"artifact_id\":\"{}\",",
                "\"content_hash\":\"{}\",",
                "\"path_binding_removal\":{{\"path\":\"{}\",\"state\":\"{}\"}},",
                "\"path_binding_addition\":{{\"path\":\"{}\",\"state\":\"{}\"}}",
                "}}"
            ),
            json_escape(source_path),
            json_escape(target_path),
            json_escape(artifact_id),
            json_escape(content_hash),
            json_escape(source_path),
            json_escape(source_path_state),
            json_escape(target_path),
            json_escape(target_path_state),
        ),
        MutationPayload::Delete {
            path,
            artifact_id,
            content_hash,
            path_state,
        } => format!(
            concat!(
                "{{",
                "\"kind\":\"delete\",",
                "\"path\":\"{}\",",
                "\"artifact_id\":\"{}\",",
                "\"content_hash\":\"{}\",",
                "\"path_binding_removal\":{{\"path\":\"{}\",\"state\":\"{}\"}},",
                "\"tombstone\":true",
                "}}"
            ),
            json_escape(path),
            json_escape(artifact_id),
            json_escape(content_hash),
            json_escape(path),
            json_escape(path_state),
        ),
        MutationPayload::MetadataSet {
            path,
            artifact_id,
            content_hash,
            classification_before,
            classification_after,
        } => format!(
            concat!(
                "{{",
                "\"kind\":\"metadata_set\",",
                "\"path\":\"{}\",",
                "\"artifact_id\":\"{}\",",
                "\"content_hash\":\"{}\",",
                "\"classification_before\":\"{}\",",
                "\"classification_after\":\"{}\"",
                "}}"
            ),
            json_escape(path),
            json_escape(artifact_id),
            json_escape(content_hash),
            json_escape(classification_before),
            json_escape(classification_after),
        ),
    }
}

fn topic_revision_json(response: &MutationResponse) -> String {
    let revision = &response.topic_revision;
    format!(
        concat!(
            "{{",
            "\"topic_revision_id\":\"{}\",",
            "\"topic_id\":\"{}\",",
            "\"revision_number\":{},",
            "\"parent_revision_id\":{},",
            "\"operation_transaction_id\":\"{}\",",
            "\"tree_delta_ref\":\"{}\",",
            "\"dependency_revision_ids\":[]",
            "}}"
        ),
        json_escape(&revision.id),
        json_escape(&revision.topic_id),
        revision.revision_number,
        optional_string_json(revision.parent_revision_id.as_deref()),
        json_escape(&revision.operation_transaction_id),
        json_escape(&revision.tree_delta_ref),
    )
}

fn session_generation_json(response: &MutationResponse) -> String {
    let generation = &response.session_generation;
    let topic_frontier = generation
        .topic_frontier
        .iter()
        .map(|(topic_id, revision_id)| {
            format!(
                "\"{}\":\"{}\"",
                json_escape(topic_id),
                json_escape(revision_id)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{",
            "\"session_generation_id\":\"{}\",",
            "\"session_id\":\"{}\",",
            "\"write_topic_id\":\"{}\",",
            "\"base_resolved_view_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"topic_frontier\":{{{}}},",
            "\"generation_number\":{},",
            "\"refresh_policy\":\"{}\",",
            "\"created_by_operation_id\":\"{}\"",
            "}}"
        ),
        json_escape(&generation.id),
        json_escape(&generation.session_id),
        json_escape(&generation.write_topic_id),
        json_escape(&generation.base_resolved_view_id),
        json_escape(&generation.resolved_view_id),
        topic_frontier,
        generation.generation_number,
        json_escape(&generation.refresh_policy),
        json_escape(&generation.created_by_operation_id),
    )
}

fn string_map_object_json(values: &BTreeMap<String, String>) -> String {
    format!(
        "{{{}}}",
        values
            .iter()
            .map(|(key, value)| format!("\"{}\":\"{}\"", json_escape(key), json_escape(value)))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn real_session_generation_record_json(generation: &RealSessionGenerationRecord) -> String {
    format!(
        concat!(
            "{{",
            "\"schema_version\":1,",
            "\"record_type\":\"session_generation\",",
            "\"id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"session_id\":\"{}\",",
            "\"write_topic_id\":\"{}\",",
            "\"base_resolved_view_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"topic_frontier\":{},",
            "\"generation_number\":{},",
            "\"refresh_policy\":\"{}\",",
            "\"created_by\":\"{}\",",
            "\"privacy_class\":\"local_only\"",
            "}}"
        ),
        json_escape(&generation.session_generation_id),
        json_escape(&generation.session_generation_id),
        json_escape(&generation.session_id),
        json_escape(&generation.write_topic_id),
        json_escape(&generation.base_resolved_view_id),
        json_escape(&generation.resolved_view_id),
        string_map_object_json(&generation.topic_frontier),
        generation.generation_number,
        json_escape(&generation.refresh_policy),
        json_escape(&generation.created_by),
    )
}

fn real_session_refresh_success_envelope(
    state: &RealRepoState,
    session: &RealSessionRecord,
    changed: bool,
    no_op_reason: Option<&str>,
) -> String {
    let revision_ids = session.topic_frontier.values().map(String::as_str);
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"session.refresh\",",
            "\"repository_id\":\"{}\",",
            "\"changed\":{},",
            "\"no_op_reason\":{},",
            "\"ids\":{{",
            "\"session_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"resolved_view_id\":\"{}\",",
            "\"topic_revision_ids\":{}",
            "}},",
            "\"refresh_policy\":\"{}\",",
            "\"generation_number\":{},",
            "\"topic_frontier\":{},",
            "\"available_newer_topic_heads\":{}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&state.repository_id),
        changed,
        optional_string_json(no_op_reason),
        json_escape(&session.session_id),
        json_escape(&session.session_generation_id),
        json_escape(&session.resolved_view_id),
        string_array_json(revision_ids),
        json_escape(&session.refresh_policy),
        session.generation_number,
        string_map_object_json(&session.topic_frontier),
        string_map_object_json(&available_newer_topic_heads(state, session)),
    )
}

fn refs_json(refs: &MutationRefs) -> String {
    format!(
        "{{\"artifacts\":[{}],\"tree_identity\":{}}}",
        refs.artifacts
            .iter()
            .map(|artifact| {
                format!(
                    concat!(
                        "{{",
                        "\"artifact_id\":{},",
                        "\"path\":\"{}\",",
                        "\"path_state\":\"{}\",",
                        "\"content_hash\":{},",
                        "\"executable\":{},",
                        "\"classification\":{}",
                        "}}"
                    ),
                    optional_string_json(artifact.artifact_id.as_deref()),
                    json_escape(&artifact.path),
                    json_escape(&artifact.path_state),
                    optional_string_json(artifact.content_hash.as_deref()),
                    optional_bool_json(artifact.executable),
                    optional_string_json(artifact.classification.as_deref()),
                )
            })
            .collect::<Vec<_>>()
            .join(","),
        tree_identity_json(&refs.tree_identity),
    )
}

fn view_json(view: &SessionView) -> String {
    format!(
        concat!(
            "{{",
            "\"resolved_view_id\":\"{}\",",
            "\"session_generation_id\":\"{}\",",
            "\"tree_identity\":{{",
            "\"kind\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"tree_hash\":\"{}\"",
            "}}",
            "}}"
        ),
        json_escape(&view.resolved_view_id),
        json_escape(&view.session_generation_id),
        json_escape(&view.tree_identity.kind),
        json_escape(&view.tree_identity.repository_id),
        json_escape(&view.tree_identity.tree_hash),
    )
}

fn tree_identity_json(tree_identity: &TreeIdentityView) -> String {
    format!(
        concat!(
            "{{",
            "\"kind\":\"{}\",",
            "\"repository_id\":\"{}\",",
            "\"tree_hash\":\"{}\"",
            "}}"
        ),
        json_escape(&tree_identity.kind),
        json_escape(&tree_identity.repository_id),
        json_escape(&tree_identity.tree_hash),
    )
}

fn artifact_json(artifact: &SessionVisibleArtifactView) -> String {
    format!(
        concat!(
            "{{",
            "\"artifact_id\":\"{}\",",
            "\"path\":\"{}\",",
            "\"kind\":\"{}\",",
            "\"content_hash\":\"{}\",",
            "\"byte_length\":{},",
            "\"classification\":\"{}\",",
            "\"executable\":{},",
            "\"tombstone\":{}",
            "}}"
        ),
        json_escape(&artifact.artifact_id),
        json_escape(&artifact.path),
        artifact.kind.as_str(),
        json_escape(&artifact.content_hash),
        artifact.byte_length,
        json_escape(&artifact.classification),
        artifact.executable,
        artifact.tombstone,
    )
}

fn mutation_artifact_json(artifact: &MutationArtifactView) -> String {
    format!(
        concat!(
            "{{",
            "\"artifact_id\":\"{}\",",
            "\"path\":\"{}\",",
            "\"kind\":\"{}\",",
            "\"before_hash\":{},",
            "\"after_hash\":\"{}\",",
            "\"classification\":\"{}\",",
            "\"executable\":{}",
            "}}"
        ),
        json_escape(&artifact.artifact_id),
        json_escape(&artifact.path),
        artifact.kind.as_str(),
        optional_string_json(artifact.before_hash.as_deref()),
        json_escape(&artifact.after_hash),
        json_escape(&artifact.classification),
        artifact.executable,
    )
}

fn string_string_map_json(values: &BTreeMap<String, String>) -> String {
    let fields = values
        .iter()
        .map(|(key, value)| format!("\"{}\":\"{}\"", json_escape(key), json_escape(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{fields}}}")
}

fn string_array_json<'a>(values: impl IntoIterator<Item = &'a str>) -> String {
    let items = values
        .into_iter()
        .map(|value| format!("\"{}\"", json_escape(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{items}]")
}

fn string_array_map_json(map: &std::collections::BTreeMap<String, Vec<String>>) -> String {
    let fields = map
        .iter()
        .map(|(key, values)| {
            format!(
                "\"{}\":{}",
                json_escape(key),
                string_array_json(values.iter().map(String::as_str))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{fields}}}")
}

fn usize_map_json(map: &BTreeMap<&str, usize>) -> String {
    let fields = map
        .iter()
        .map(|(key, value)| format!("\"{}\":{}", json_escape(key), value))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{fields}}}")
}

fn optional_string_json(value: Option<&str>) -> String {
    value
        .map(|value| format!("\"{}\"", json_escape(value)))
        .unwrap_or_else(|| "null".to_string())
}

fn optional_bool_json(value: Option<bool>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

fn write_mode_json(write_mode: &WriteMode) -> &'static str {
    if matches!(write_mode, WriteMode::Create) {
        "create"
    } else {
        "replace"
    }
}

fn failure_envelope(error: &CliError) -> String {
    let details = error
        .raw_details_json
        .as_deref()
        .map(str::to_string)
        .unwrap_or_else(|| details_json(&error.details));
    format!(
        "{{\"ok\":false,\"error\":{{\"code\":\"{}\",\"message\":\"{}\",\"details\":{},\"next_action\":\"{}\"}}}}",
        json_escape(error.code),
        json_escape(&error.message),
        details,
        json_escape(next_action_for_error_code(error.code)),
    )
}

fn next_action_for_error_code(code: &str) -> &'static str {
    match code {
        "not_initialized" => {
            "Call repository_init for this repository, then retry the original operation."
        }
        "sunignore_excluded" => {
            "Treat the path as outside Sunlight ownership, or have a human update .sunignore and reinitialize Sunlight before retrying."
        }
        "sunignore_policy_external" => {
            "Have a human edit .sunignore in the repository worktree, then call repository_init to adopt the policy change."
        }
        "sunignore_policy_changed" => {
            "Call repository_init. A clean state refreshes automatically; for authored history, follow the preservation instructions returned by repository_init."
        }
        "sunignore_policy_change_blocked" => {
            "Restore the prior .sunignore to continue, or preserve authored work, back up .sunlight, remove only .sunlight, and call repository_init to adopt the new policy."
        }
        "legacy_quarantine_migration_blocked" => {
            "Preserve or export needed historical work, back up .sunlight, then remove only .sunlight and rerun repository_init with this build."
        }
        "precondition_failed" | "compat_precondition_failed" | "promotion_precondition_failed" => {
            "Read the current artifact or inspect the referenced object, use the returned exact hash and IDs, then retry only after adapting the intended change."
        }
        "patch_apply_failed" | "patch_parse_failed" => {
            "Read the current artifact, rebuild the patch with exact unique context, keep the current content hash as the compare-and-swap guard, and retry."
        }
        "session_not_found" => {
            "Inspect repository status for current sessions; start a new session on the intended topic and exact view when the supplied session does not exist."
        }
        "session_view_mismatch" => {
            "Use the returned existing session at its pinned exact view, or choose a distinct actor identifier when a separate session is required."
        }
        "session_identity_mismatch" => {
            "Choose a distinct actor identifier and retry session_start; do not reuse a session owned by another actor or topic."
        }
        "topic_completed" | "topic_already_completed" => {
            "Treat the completed revision as immutable; create a dependent topic and session for any follow-up change."
        }
        "topic_session_mismatch" | "topic_head_mismatch" | "topic_conflict" => {
            "Inspect the topic and session facts, then use the session that owns the topic and its exact current revision; do not guess or reuse another writer's session."
        }
        "session_refresh_blocked" | "conflicted_view" | "execution_conflicted_view"
        | "checkpoint_conflicted_view" => {
            "Inspect the returned conflict IDs, adapt the overlapping change in its own topic, and resolve a new exact view before retrying."
        }
        "compat_projection_stale" => {
            "Create a fresh compatibility projection from the returned session's current generation, reapply the remaining filesystem change to that projection, call compat_diff, then import the selected candidates with the re-echoed session_generation_id."
        }
        "worktree_anchor_stale"
        | "worktree_changed_during_capture"
        | "worktree_capture_precondition_failed" => {
            "Call worktree_diff again, reconsider the exact current candidates and anchor generation, then retry deliberately."
        }
        "worktree_candidate_not_found"
        | "worktree_no_eligible_changes"
        | "worktree_no_selected_changes" => {
            "Call worktree_diff, inspect the returned classifications and exact candidate IDs or paths, then select only the intended eligible changes."
        }
        "worktree_candidate_blocked"
        | "worktree_path_policy_failed"
        | "worktree_secret_detected"
        | "worktree_candidate_conflicted"
        | "worktree_rename_ambiguous"
        | "worktree_candidate_policy_failed"
        | "worktree_partial_capture_blocked" => {
            "Inspect worktree_diff candidate facts, correct or exclude the blocked worktree change, then retry with an exact safe selection."
        }
        "worktree_anchor_invalid" | "worktree_diff_failed" => {
            "Inspect repository_status and the reported worktree path or anchor facts; correct the unsafe filesystem condition before retrying worktree_diff."
        }
        "checkpoint_stale_view" | "resolver_frontier_input_lost" | "persisted_view_mismatch" => {
            "Inspect the returned staleness or frontier facts, reselect exact completed revisions, and create a fresh resolved view before retrying."
        }
        "repository_writer_busy" | "concurrent_state_update" => {
            "Call repository_status to observe durable state, then retry the same idempotent operation after the bounded writer contention clears."
        }
        "request_cancelled" => {
            "Inspect repository status before retrying; an acknowledged publication may already be durable, so do not assume cancellation rolled it back."
        }
        "execution_network_isolation_incompatible_toolchain" => {
            "Use a toolchain compatible with enforced network isolation, or explicitly request not_enforced only when repository policy permits that weaker guarantee."
        }
        "execution_containment_setup_failed" | "execution_filesystem_isolation_setup_failed"
        | "execution_network_isolation_setup_failed" => {
            "Inspect the reported platform and policy facts, correct the unavailable containment prerequisite or choose an explicitly permitted policy, then retry."
        }
        "execution_network_isolation_cleanup_failed" => {
            "Inspect the recorded execution and cleanup facts, run the documented bounded cleanup or restart recovery, and do not assume isolation state was removed."
        }
        "commit_policy_failed" => {
            "Inspect the inline metadata-policy failures, then correct the supplied .sunlight metadata candidate paths or restore the managed .gitignore block before rerunning policy_check_commit."
        }
        "export_policy_failed" | "checkpoint_evidence_failed"
        | "checkpoint_evidence_view_mismatch" | "checkpoint_evidence_tree_mismatch" => {
            "Inspect the validation report and exact view/tree identities, satisfy the reported policy or evidence failures, then create or export a matching checkpoint."
        }
        "state_recovery_failed" | "publication_outbox_recovery_failed"
        | "invalid_native_state" | "invalid_repository_config" | "repository_io_failed" => {
            "Preserve .sunlight, inspect the reported recovery paths and repository status, and use the documented recovery command or backup; do not delete native state."
        }
        "projection_cache_integrity_failed" | "projection_quarantine_record_write_failed" => {
            "Inspect the reported cache or quarantine path, preserve native state, run projection quarantine cleanup, and rematerialize the exact view."
        }
        "path_outside_repository" | "path_policy_violation" => {
            "Use a portable repository-relative artifact path that does not traverse protected state, symlinks, junctions, or repository boundaries."
        }
        "shell_not_allowed" => {
            "Invoke an approved executable directly with a separate argument array; shell interpreters and shell command strings are not accepted."
        }
        "mcp_content_too_large" | "mcp_stdout_too_large" | "mcp_stderr_too_large" => {
            "Narrow the requested content or command output and retry; inspect repository status separately because durable state remains available."
        }
        "promotion_policy_failed" => {
            "Inspect the returned classification and size facts, keep denied output local-only, then correct, reduce, or split a safe source-like candidate before retrying promotion."
        }
        "artifact_read_scope_invalid" => {
            "Supply exactly one scope: a session for topic-bound authoring context or an exact resolved view for read-only access."
        }
        "invalid_request" | "unknown_tool" => {
            "Correct the request using the advertised tool schema or CLI help, then retry; do not repeat unchanged arguments."
        }
        _ if code.contains("path") || code.contains("projection") => {
            "Inspect the reported path and projection facts, keep all access inside the repository-managed boundary, and retry with a valid exact view or relative path."
        }
        _ if code.contains("view") || code.contains("resolver") => {
            "Inspect the exact view, conflict, and staleness facts; resolve a new view from exact completed revisions before retrying."
        }
        _ if code.contains("execution") => {
            "Inspect the durable execution record and policy report, correct the reported command or policy condition, and retry against the same exact view only when safe."
        }
        _ if code.contains("git") || code.contains("export") => {
            "Inspect the checkpoint, validation report, branch, and working-tree facts; correct the reported Git handoff condition without changing existing work, then retry."
        }
        _ => {
            "Inspect error details and repository_status, correct the identified IDs or preconditions, and retry only when the operation is safe and still needed."
        }
    }
}

fn details_json(details: &[(&'static str, String)]) -> String {
    if details.is_empty() {
        return "{}".to_string();
    }

    let fields = details
        .iter()
        .map(|(key, value)| format!("\"{}\":\"{}\"", json_escape(key), json_escape(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{fields}}}")
}

fn json_escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            character if character.is_control() => format!("\\u{:04x}", character as u32)
                .chars()
                .collect::<Vec<_>>(),
            character => vec![character],
        })
        .collect()
}

fn print_help(ctx: &CommandContext) {
    outputln!(ctx,
        "\
sun - local-first source artifact management

Usage:
  sun init [--repo <path>]
  sun agent install --client generic|codex|cursor [--force] [--repo <path>] [--json]
  sun agent doctor --client generic|codex|cursor [--repo <path>] [--json]
  sun topic create <slug> --display-name <name> [--owner <actor>] [--visibility local|private] [--acceptance-criterion <text>...] [--json]
  sun topic complete --topic <topic> --revision <revision> --session <session> [--summary <text>] [--json]
  sun session start --topic <topic> --view <view> --actor <actor-id> [--json]
  sun session refresh <session> --policy manual|follow|none [--json]
  sun read|list|search ... (--session <session> | --view <resolved-view>) [--json]
  sun patch|write|move|delete|metadata set ... --session <session> [--json]
  sun view resolve --base <checkpoint> [--include <topic>:<revision>] [--json]
  sun project materialize --view <view> --purpose execution|compatibility|inspection|export [--strategy copy|reflink|hardlink_readonly|overlay_copyup] [--no-copy-fallback] [--projection-root <path>] [--json]
  sun compat project --session <session> [--json]
  sun compat diff --projection <projection> [--json]
  sun compat import --projection <projection> --candidate <candidate> --session-generation <generation> [--json]
  sun compat diff --worktree [--json]
  sun compat capture --worktree --topic <slug> --actor <actor> [--candidate <candidate>...|--path <path>...] [--json]
  sun run --view <view> [runtime policy options] -- <command> [args...]
  sun execution promote-output <execution> --path <path> --session <session> --classification source_like_delta|generated_artifact [--json]
  sun checkpoint create --view <view> [--execution <execution>] [--json]
  sun policy check-export --checkpoint <checkpoint> --branch <ref> [--json]
  sun policy check-commit [--paths <.sunlight/path>...] [--json]
  sun policy explain <validation-report> [--json]
  sun git export --checkpoint <checkpoint> --branch <ref> [--execute-local] [--json]
  sun status [--topic|--session|--view|--projection|--execution|--checkpoint|--export <id>] [--json]
  sun inspect <typed-selector> [--json]
  sun mcp serve --repo <repository-directory>

Commands:
  init       Ingest the repository into persisted Sunlight native state
  agent      Install the portable skill and client MCP adapter; diagnose setup
  topic      Create and durably complete authoring topics
  session    Start and explicitly refresh topic-bound sessions over exact views
  read/list/search/inspect
             Query persisted artifacts and provenance
  patch/write/move/delete/metadata
             Author native operations with explicit preconditions
  view       Resolve persisted topic heads into an exact view or conflicts
  project    Materialize a managed tool projection; it is not source truth
  compat     Review and adopt compatibility-projection or direct worktree edits
  run        Execute a command against an exact persisted view
  execution  Promote approved execution outputs into native operations
  checkpoint Freeze a resolved view for validation and export
  policy     Validate commit/export policy and inspect persisted reports
  git        Export a checkpoint to ordinary Git history
  status     Summarize repository health and object lifecycle state
  inspect    Inspect a persisted object using a typed selector
  mcp        Serve repository-confined MCP JSON-RPC 2.0 tools over local stdio

Typical journey:
  sun init
  sun topic create auth-fix --display-name \"Auth fix\"
  sun session start --topic auth-fix --view view_base_0001 --actor agent-a
  sun read <path> --session <session-id>
  sun patch <path> --session <session-id> --expect-hash <sha256> --patch-file <file>
  sun topic complete --topic <topic-id> --revision <revision-id> --session <session-id>
  sun view resolve --base <base-checkpoint-id> --include <topic-id>:<revision-id>
  sun run --view <resolved-view-id> -- <test-program> <args...>
  sun checkpoint create --view <resolved-view-id> --execution <passing-execution-id>
  sun policy check-export --checkpoint <checkpoint-id> --branch refs/heads/sunlight/auth-fix
  sun git export --checkpoint <checkpoint-id> --branch refs/heads/sunlight/auth-fix --execute-local

If validation returns an intentional promotion candidate, promote it with the
exact returned classification, resolve its new topic revision into an exact
view, rerun validation, and checkpoint that matching view. Git handoff is done
only when export returns an export-map ID for the checkpoint and target ref.

Compatibility/testing:
  Add --fixture basic-app to supported commands only when exercising the legacy
  deterministic fixture contracts. Fixture state is not repository source truth.
"
    );
}

fn print_command_help(ctx: &CommandContext, command: &str) {
    match command {
        "agent" | "agent install" => outputln!(ctx, "sun agent install\n\nUsage:\n  sun agent install --client generic|codex|cursor [--force] [--repo <path>] [--json]\n\nInstalls the portable Sunlight Agent Skill. Codex and Cursor also receive a repository-bound local MCP entry. Generic installs the skill only; configure a client transport for sun mcp serve separately. Existing unrelated client configuration is preserved."),
        "agent doctor" => outputln!(ctx, "sun agent doctor\n\nUsage:\n  sun agent doctor --client generic|codex|cursor [--repo <path>] [--json]\n\nVerifies the portable skill and, for Codex or Cursor, the selected client MCP entry. Generic doctor does not inspect client transport or live tool visibility. Setup is complete when repository_status and artifact tools are visible in the client."),
        "status" => outputln!(ctx, "sun status\n\nUsage:\n  sun status [--json]\n  sun status --topic <topic> [--json]\n  sun status --session <session> [--json]\n  sun status --view <view> [--json]\n  sun status --projection <projection> [--json]\n  sun status --execution <execution> [--json]\n  sun status --checkpoint <checkpoint> [--json]\n  sun status --export <export-map> [--json]\n\nRepository status reports native state plus direct worktree differences relative to Sunlight's durable anchor. Git state remains a separate diagnostic."),
        "inspect" => outputln!(ctx, "sun inspect\n\nUsage:\n  sun inspect repository [--json]\n  sun inspect topic:<topic>|session:<session>|view:<view> [--json]\n  sun inspect artifact:<path>|operation:<id>|conflict:<id> [--json]\n  sun inspect projection:<id>|execution:<id>|checkpoint:<id>|export:<id> [--json]"),
        "topic" => outputln!(ctx, "sun topic\n\nUsage:\n  sun topic create <slug> --display-name <name> [options] [--json]\n  sun topic complete --topic <topic> --revision <revision> --session <session> [--summary <text>] [--json]"),
        "topic create" => outputln!(ctx, "sun topic create\n\nUsage:\n  sun topic create <slug> --display-name <name> [--owner <actor>] [--visibility local|private] [--acceptance-criterion <text>...] [--json]\n\nCreates a durable topic in the initialized repository. Defaults: owner=local, visibility=local."),
        "topic complete" => outputln!(ctx, "sun topic complete\n\nUsage:\n  sun topic complete --topic <topic> --revision <revision> --session <session> [--summary <text>] [--json]\n\nRecords an idempotent durable completion fact for the exact current topic revision; it does not assert review or quality."),
        "session" => outputln!(ctx, "sun session\n\nUsage:\n  sun session start --topic <topic> --view <view> --actor <actor-id> [--json]\n  sun session refresh <session> --policy manual|follow|none [--json]"),
        "session start" => outputln!(ctx, "sun session start\n\nUsage:\n  sun session start --topic <topic> --view <view> --actor <actor-id> [--json]\n\nThe exact view may be the repository base, current head, or a persisted checkpoint view."),
        "session refresh" => outputln!(ctx, "sun session refresh\n\nUsage:\n  sun session refresh <session> --policy manual|follow|none [--json]\n\nmanual and follow immediately advance already-selected non-write topics to current heads; none pins them. The write topic always remains at the session revision. An unchanged policy/frontier is an idempotent no-op."),
        "compat" | "compat project" | "compat diff" | "compat import" | "compat capture" => outputln!(ctx, "sun compat\n\nUsage:\n  sun compat project --session <session> [--json]\n  sun compat diff --projection <projection> [--json]\n  sun compat import --projection <projection> --candidate <candidate> --session-generation <generation> [--json]\n  sun compat diff --worktree [--json]\n  sun compat capture --worktree --topic <slug> --actor <actor> [--anchor-generation <number>] [--candidate <candidate>...|--path <path>...] [--json]\n\nProjection imports enter an existing session. Direct worktree capture creates one completed topic and advances the durable worktree anchor without rewriting repository files."),
        "policy" | "policy check-export" | "policy check-commit" | "policy explain" => outputln!(ctx, "sun policy\n\nUsage:\n  sun policy check-export --checkpoint <checkpoint> --branch <ref> [--json]\n  sun policy check-commit [--paths <.sunlight/path>...] [--json]\n  sun policy explain <validation-report> [--json]\n\ncheck-commit validates only Sunlight metadata or, with no paths, the managed .gitignore block. It returns an inline report. Source-artifact safety uses a checkpoint plus check-export; that persisted export report can be passed to policy explain."),
        "git" | "git export" => outputln!(ctx, "sun git export\n\nUsage:\n  sun git export --checkpoint <checkpoint> --branch <ref> [--write-plan|--execute-local] [--repo <path>] [--json]"),
        "checkpoint" | "checkpoint create" => outputln!(ctx, "sun checkpoint create\n\nUsage:\n  sun checkpoint create --view <resolved-view-id> [--execution <passing-execution-id>] [--json]\n\nWhen supplied, execution evidence must pass and exactly match the resolved view and tree."),
        "run" => outputln!(ctx, "sun run\n\nUsage:\n  sun run --view <resolved-view-id> [--network disabled|not_enforced] -- <command> [args...]\n\nRuntime bounds come from [execution_policy] in .sunlight/config.toml. Windows runs always use restricted low-integrity filesystem isolation and a dedicated Job Object. Network disabled adds a capability-less per-execution AppContainer; not_enforced retains compatibility with ordinary user toolchains."),
        "read" | "list" | "search" | "patch" | "write" | "move" | "delete" | "metadata" | "metadata set" => outputln!(ctx, "sun artifact operations\n\nUsage:\n  sun read <path> (--session <session> | --view <resolved-view>) [--json]\n  sun list [path-prefix] (--session <session> | --view <resolved-view>) [--json]\n  sun search <query> (--session <session> | --view <resolved-view>) [--json]\n  sun patch <path> --session <session> --expect-hash <hash> --patch-file <file> [--json]\n  sun write <path> --session <session> --expect-hash <hash-or-new> --content-file <file> --classification source|generated [--json]\n  sun move <from> <to> --session <session> --expect-hash <hash> [--json]\n  sun delete <path> --session <session> --expect-hash <hash> [--json]\n  sun metadata set <path> --session <session> --expect-hash <hash> --classification source|generated [--json]\n\nsource checkpoints and exports normally. generated checkpoints normally but exports only with reachable execution-output promotion provenance; relabeling does not create provenance."),
        "view" | "view resolve" => outputln!(ctx, "sun view resolve\n\nUsage:\n  sun view resolve --base <checkpoint> [--include <topic>:<revision>] [--json]\n\nStart from any exact checkpoint. Its topic frontier is included automatically, and explicit selections replace revisions for the same topic. Omitting include on a later checkpoint reproduces it exactly. Omitting include on the repository base is discovery-only and resolves moving current heads. Conflicts and staleness remain inspectable records."),
        "project" | "project materialize" | "projection" | "projection create" => outputln!(ctx, "sun project materialize\n\nUsage:\n  sun project materialize --view <resolved-view-id> --purpose execution|compatibility|inspection|export [--strategy copy|reflink|hardlink_readonly|overlay_copyup] [--no-copy-fallback] [--projection-root <path>] [--json]\n\nManaged projections adapt persisted views for filesystem tools and are not source truth. Automatic selection uses verified-cache read-only hardlinks for Windows inspection, prefers safe block cloning for writable purposes, and falls back to private copies; --no-copy-fallback makes the requested strategy required."),
        "execution" | "execution promote-output" => outputln!(ctx, "sun execution promote-output\n\nUsage:\n  sun execution promote-output <execution-id> --path <path> --session <session> --classification source_like_delta|generated_artifact [--json]\n\nPass the exact promotion-candidate classification returned by sun run."),
        "mcp" | "mcp serve" => outputln!(ctx, "sun mcp serve\n\nUsage:\n  sun mcp serve --repo <repository-directory>\n\nRuns MCP JSON-RPC 2.0 over newline-delimited stdio. The server is bound to one canonical directory, which may be uninitialized so repository_init can perform first ingest; stdout contains protocol messages only."),
        _ => print_help(ctx),
    }
}

fn print_init_help(ctx: &CommandContext) {
    outputln!(
        ctx,
        "\
sun init

Usage:
  sun init [--repo <path>]

Creates .sunlight/config.toml, the initial repository layout, and a conservative
.sunlight/.gitignore fragment. Existing config and policy files are preserved.
"
    );
}

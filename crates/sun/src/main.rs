use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

use sunlight_core::artifacts::{
    ArtifactIoError, ArtifactKind, ContentBlob, ContentTree, DeleteRequest, ExpectedHash,
    InMemoryArtifactStore, ListResponse, MetadataSetRequest, MoveRequest, MutationArtifactView,
    MutationPayload, MutationRefs, MutationResponse, PatchRequest, ReadResponse, SearchResponse,
    SessionView, SessionVisibleArtifactView, TreeEntry, TreeIdentityView, WriteMode, WriteRequest,
    FILE_OPERATION_SEMANTICS_VERSION, FIXTURE_ACTOR_ID, FIXTURE_REPOSITORY_ID,
    FIXTURE_RESOLVED_VIEW_ID, FIXTURE_SESSION_GENERATION_ID, FIXTURE_SESSION_ID, FIXTURE_TREE_HASH,
    FIXTURE_WRITE_TOPIC_ID, POSIX_CASE_SENSITIVE_PATH_POLICY_ID,
};
use sunlight_core::checkpoint::{
    fixture_checkpoint_from_resolved_view, CheckpointRecord, CheckpointValidationError,
    EvidenceRef, GitExportMapRecord, FIXTURE_CHECKPOINT_ID, FIXTURE_CREATED_AT,
    FIXTURE_EXPORTED_GIT_REF, FIXTURE_EXPORT_MAP_ID, FIXTURE_GIT_COMMIT_ID,
    FIXTURE_VALIDATION_REPORT_ID,
};
use sunlight_core::compat_import::{
    fixture_basic_app_candidate_deltas, plan_fixture_basic_app_import, CompatCandidateDelta,
    CompatCandidateKind, CompatImportErrorCode, CompatImportRequest, CompatImportResponse,
    CompatImportValidationError, CompatImportedArtifact, FIXTURE_COMPAT_BASELINE_MANIFEST_DIGEST,
    FIXTURE_COMPAT_IMPORT_OPERATION_ID,
};
use sunlight_core::execution::{
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
    plan_git_export_writer, GeneratedOutputExportRequirement, GitExportCommitPlan,
    GitExportContentFile, GitExportError, GitExportExecutionError, GitExportExecutionFixture,
    GitExportExecutionResult, GitExportExecutionStep, GitExportExecutionStepFixture,
    GitExportExecutionSummary, GitExportMapStore, GitExportPlanningError, GitExportRefUpdatePlan,
    GitExportRepositoryState, GitExportRequest, GitExportResponse, GitExportValidationFailure,
    GitExportValidationReport, GitExportWriterInput, GitExportWriterPlan, GitRefState,
    ImportedBaseGitCommit, InMemoryGitExportMapStore, PersistedGitExportMap,
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
    ProjectionValidationError, FIXTURE_COMPATIBILITY_PROJECTION_ID,
    FIXTURE_EXECUTION_PROJECTION_ID, FIXTURE_EXPORT_PROJECTION_ID,
    FIXTURE_INSPECTION_PROJECTION_ID,
};
use sunlight_core::records::{canonical_json_bytes, parse_json_record, JsonValue};
use sunlight_core::repository::{
    init_repository, RepositoryConfig, CURRENT_STORAGE_SCHEMA_VERSION,
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

const FIXTURE_STALE_COMPATIBILITY_PROJECTION_ID: &str =
    "projection_compat_agent_a_stale_baseline_0001";

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    let json = args.iter().any(|arg| arg == "--json");

    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if json {
                println!("{}", failure_envelope(&error));
            } else {
                eprintln!("sun: {}", error.message);
            }
            ExitCode::FAILURE
        }
    }
}

#[derive(Debug, Clone)]
struct CommandContext {
    json: bool,
    args: Vec<String>,
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

fn run(args: Vec<String>) -> Result<(), CliError> {
    let json = args.iter().any(|arg| arg == "--json");
    let args = args
        .into_iter()
        .filter(|arg| arg != "--json")
        .collect::<Vec<_>>();
    let ctx = CommandContext { json, args };

    match ctx.args.as_slice() {
        [] => {
            print_help();
            Ok(())
        }
        [flag] if flag == "--help" || flag == "-h" => {
            print_help();
            Ok(())
        }
        [command] if command == "init" => init(&ctx, PathBuf::from(".")),
        [command, flag, path] if command == "init" && flag == "--repo" => {
            init(&ctx, PathBuf::from(path))
        }
        [command, flag] if command == "init" && flag == "--help" => {
            print_init_help();
            Ok(())
        }
        [command, ..] if command == "init" => {
            Err(invalid_request("usage: sun init [--repo <path>]"))
        }
        [scope, command, ..] if scope == "topic" && command == "create" => topic_create(&ctx),
        [scope, command, ..] if scope == "session" && command == "start" => session_start(&ctx),
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

fn init(ctx: &CommandContext, repo_root: PathBuf) -> Result<(), CliError> {
    let report = init_repository(&repo_root).map_err(|error| {
        invalid_request(error.to_string()).with_detail("command", "repository.init")
    })?;

    if ctx.json {
        println!(
            "{}",
            init_success_envelope(
                &report.repository_id,
                &report.repo_root.display().to_string(),
                &report.sunlight_dir.display().to_string(),
                report.created_config,
                report.created_gitignore,
                report.created_directories.len(),
            )
        );
    } else {
        println!("initialized Sunlight repository");
        println!("repo_root = {}", report.repo_root.display());
        println!("sunlight_dir = {}", report.sunlight_dir.display());
        println!("repository_id = {}", report.repository_id);
        println!("created_config = {}", report.created_config);
        println!("created_gitignore = {}", report.created_gitignore);
        println!("created_directories = {}", report.created_directories.len());
    }

    Ok(())
}

fn topic_create(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_topic_create_options(ctx)?;
    ensure_basic_app_fixture(&options.fixture)?;
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
        println!("{}", topic_create_success_envelope(&options.display_name));
    } else {
        println!("created topic {}", FIXTURE_WRITE_TOPIC_ID);
    }

    Ok(())
}

fn session_start(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_session_start_options(ctx)?;
    ensure_basic_app_fixture(&options.fixture)?;
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
        println!("{}", session_start_success_envelope(&options.view_id));
    } else {
        println!("started session {}", FIXTURE_SESSION_ID);
    }

    Ok(())
}

fn artifact_read(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_artifact_options(ctx, "read", 1, 1)?;
    let store = fixture_store(&options.fixture)?;
    let response = store
        .read(&options.session_id, &options.operands[0])
        .map_err(artifact_error)?;

    if ctx.json {
        println!("{}", read_success_envelope(&response));
    } else {
        print!("{}", response.content.bytes);
    }

    Ok(())
}

fn artifact_list(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_artifact_options(ctx, "list", 0, 1)?;
    let prefix = options.operands.first().map(String::as_str).unwrap_or("");
    let store = fixture_store(&options.fixture)?;
    let response = store
        .list(&options.session_id, prefix)
        .map_err(artifact_error)?;

    if ctx.json {
        println!("{}", list_success_envelope(&response));
    } else {
        for artifact in response.artifacts {
            println!("{}", artifact.path);
        }
    }

    Ok(())
}

fn artifact_search(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_artifact_options(ctx, "search", 1, 1)?;
    let store = fixture_store(&options.fixture)?;
    let response = store
        .search(&options.session_id, &options.operands[0])
        .map_err(artifact_error)?;

    if ctx.json {
        println!("{}", search_success_envelope(&response));
    } else {
        for item in response.matches {
            println!("{}:{}:{}", item.path, item.line, item.snippet);
        }
    }

    Ok(())
}

fn artifact_patch(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_mutation_options(ctx, "patch", 1)?;
    let expect_hash = options
        .expect_hash
        .ok_or_else(|| invalid_request("usage: sun patch requires --expect-hash <hash>"))?;
    let patch_file = options
        .patch_file
        .ok_or_else(|| invalid_request("usage: sun patch requires --patch-file <file>"))?;
    let patch = fs::read_to_string(&patch_file).map_err(|error| {
        invalid_request(format!("failed to read patch file `{patch_file}`"))
            .with_detail("source", error.to_string())
    })?;
    let mut store = fixture_store(&options.fixture)?;
    let response = store
        .patch(PatchRequest {
            session_id: options.session_id,
            path: options.operands[0].clone(),
            expected_hash: expect_hash,
            patch,
        })
        .map_err(artifact_error)?;

    if ctx.json {
        println!("{}", mutation_success_envelope(&response));
    } else {
        println!(
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
        .ok_or_else(|| invalid_request("usage: sun write requires --expect-hash <hash-or-new>"))?;
    let content_file = options
        .content_file
        .ok_or_else(|| invalid_request("usage: sun write requires --content-file <file>"))?;
    let classification = options
        .classification
        .ok_or_else(|| invalid_request("usage: sun write requires --classification <class>"))?;
    let content = fs::read(&content_file).map_err(|error| {
        invalid_request(format!("failed to read content file `{content_file}`"))
            .with_detail("source", error.to_string())
    })?;
    let expected_hash = if expect_hash == "new" {
        ExpectedHash::New
    } else {
        ExpectedHash::Existing(expect_hash)
    };
    let mut store = fixture_store(&options.fixture)?;
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
        println!("{}", mutation_success_envelope(&response));
    } else {
        println!(
            "wrote {} {}",
            response.artifact.path, response.artifact.after_hash
        );
    }

    Ok(())
}

fn artifact_move(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_mutation_options(ctx, "move", 2)?;
    let expect_hash = options
        .expect_hash
        .ok_or_else(|| invalid_request("usage: sun move requires --expect-hash <hash>"))?;
    let mut store = fixture_store(&options.fixture)?;
    let response = store
        .move_path(MoveRequest {
            session_id: options.session_id,
            source_path: options.operands[0].clone(),
            target_path: options.operands[1].clone(),
            expected_hash: expect_hash,
        })
        .map_err(artifact_error)?;

    if ctx.json {
        println!("{}", mutation_success_envelope(&response));
    } else {
        println!("moved {}", response.artifact.path);
    }

    Ok(())
}

fn artifact_delete(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_mutation_options(ctx, "delete", 1)?;
    let expect_hash = options
        .expect_hash
        .ok_or_else(|| invalid_request("usage: sun delete requires --expect-hash <hash>"))?;
    let mut store = fixture_store(&options.fixture)?;
    let response = store
        .delete_path(DeleteRequest {
            session_id: options.session_id,
            path: options.operands[0].clone(),
            expected_hash: expect_hash,
        })
        .map_err(artifact_error)?;

    if ctx.json {
        println!("{}", mutation_success_envelope(&response));
    } else {
        println!("deleted {}", response.artifact.path);
    }

    Ok(())
}

fn artifact_metadata_set(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_mutation_options_with_skip(ctx, "metadata set", 1, 2)?;
    let expect_hash = options
        .expect_hash
        .ok_or_else(|| invalid_request("usage: sun metadata set requires --expect-hash <hash>"))?;
    let classification = options.classification.ok_or_else(|| {
        invalid_request("usage: sun metadata set requires --classification <class>")
    })?;
    let mut store = fixture_store(&options.fixture)?;
    let response = store
        .metadata_set(MetadataSetRequest {
            session_id: options.session_id,
            path: options.operands[0].clone(),
            expected_hash: expect_hash,
            classification,
        })
        .map_err(artifact_error)?;

    if ctx.json {
        println!("{}", mutation_success_envelope(&response));
    } else {
        println!("updated metadata {}", response.artifact.path);
    }

    Ok(())
}

fn view_resolve(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_view_resolve_options(ctx)?;
    if options.fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{}`", options.fixture))
                .with_detail("fixture", options.fixture),
        );
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

    let result = resolve_fixture_view(
        fixture_resolver_input(frontier),
        fixture_base_entries(),
        revisions,
    );

    if ctx.json {
        println!("{}", view_resolve_success_envelope(&result));
    } else if result.conflict_free() {
        let tree_hash = result
            .tree_identity
            .as_ref()
            .map(|tree| tree.tree_hash.as_str())
            .unwrap_or("unavailable");
        println!("{} {}", result.resolved_view_id, tree_hash);
    } else {
        println!("{} blocked", result.resolved_view_id);
    }

    Ok(())
}

fn execution_run(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_execution_run_options(ctx)?;
    if options.fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{}`", options.fixture))
                .with_detail("fixture", options.fixture),
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
        println!("{}", execution_run_success_envelope(&execution));
    } else {
        println!("{} {}", execution.id, execution.result.status.as_str());
    }

    Ok(())
}

fn execution_promote_output(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_execution_promote_output_options(ctx)?;
    if options.fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{}`", options.fixture))
                .with_detail("fixture", options.fixture),
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
        return Err(promotion_error(
            "promotion_policy_failed",
            "execution output promotion classification does not match the candidate",
            &options.execution_id,
            options.path.as_deref(),
            options.session_id.as_deref(),
            options.classification.as_deref(),
        ));
    }

    let response = fixture_promotion_mutation_response(&candidate)?;

    if ctx.json {
        println!("{}", promotion_success_envelope(&response, &candidate));
    } else {
        println!(
            "promoted {} {}",
            response.artifact.path, response.artifact.after_hash
        );
    }

    Ok(())
}

fn project_materialize(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_project_materialize_options(ctx)?;
    if options.fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{}`", options.fixture))
                .with_detail("fixture", options.fixture),
        );
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
            println!(
                "{}",
                projection_filesystem_materialize_success_envelope(&materialization)?
            );
        } else {
            println!(
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
        println!("{}", projection_materialize_success_envelope(&plan));
    } else {
        println!("{} {}", plan.projection.id, plan.projection.root_ref.value);
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
        println!(
            "{}",
            projection_quarantine_cleanup_success_envelope(&cleanup)
        );
    } else {
        println!(
            "{} {}",
            cleanup.projection_id,
            cleanup.retention_state_after.as_str()
        );
    }

    Ok(())
}

fn checkpoint_create(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_checkpoint_create_options(ctx)?;
    if options.fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{}`", options.fixture))
                .with_detail("fixture", options.fixture),
        );
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
        println!("{}", checkpoint_create_success_envelope(&checkpoint));
    } else {
        println!("{} {}", checkpoint.id, checkpoint.resolved_view_id);
    }

    Ok(())
}

fn git_export(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_git_export_options(ctx)?;
    if options.fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{}`", options.fixture))
                .with_detail("fixture", options.fixture),
        );
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
            println!("{}", git_export_execute_success_envelope(&result));
        } else {
            println!(
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
            println!("{}", git_export_execute_fixture_success_envelope(&result));
        } else {
            println!(
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
            println!("{}", git_export_write_plan_success_envelope(&plan));
        } else {
            println!("{} {}", plan.commit.checkpoint_id, plan.ref_update.git_ref);
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
        println!("{}", git_export_success_envelope(&response));
    } else {
        println!("{} {}", response.checkpoint_id, response.git_ref);
    }

    Ok(())
}

fn policy_check_export(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_policy_check_export_options(ctx)?;
    if options.fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{}`", options.fixture))
                .with_detail("fixture", options.fixture),
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
        println!("{}", policy_check_export_success_envelope(&report));
    } else {
        println!("{} {}", report.checkpoint_id, report.id);
    }

    Ok(())
}

fn policy_check_commit(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_policy_check_commit_options(ctx)?;
    let config = require_repository_config(".")?;
    let gitignore = fs::read_to_string(".sunlight/.gitignore").unwrap_or_default();

    let ignore_report = validate_managed_ignore_block(&gitignore);
    let path_report = validate_candidate_paths(options.paths.iter().map(String::as_str));
    let report = combine_policy_check_commit_reports(ignore_report, path_report);

    if !report.ok {
        return Err(policy_check_commit_error(&report, options.paths.len()));
    }

    if ctx.json {
        println!(
            "{}",
            policy_check_commit_success_envelope(
                &config.repository_id,
                &report,
                options.paths.len(),
            )
        );
    } else {
        println!("policy.check-commit ok");
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

    let report = fixture_policy_explain_validation_report(&options.validation_report_id)?;
    println!("{}", policy_explain_success_envelope(&report));

    Ok(())
}

fn compat_project(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_compat_project_options(ctx)?;
    if options.fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{}`", options.fixture))
                .with_detail("fixture", options.fixture),
        );
    }
    ensure_fixture_session(&options.session_id)?;

    let view = fixture_base_resolved_content_view();
    let projection =
        fixture_compatibility_projection_from_resolved_view(&view, FIXTURE_SESSION_GENERATION_ID)
            .map_err(projection_error)?;

    if ctx.json {
        println!(
            "{}",
            compat_project_success_envelope(&projection, &options.session_id)
        );
    } else {
        println!("{} {}", projection.id, projection.root_ref.value);
    }

    Ok(())
}

fn compat_diff(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_compat_diff_options(ctx)?;
    if options.fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{}`", options.fixture))
                .with_detail("fixture", options.fixture),
        );
    }

    let projection = fixture_compat_import_projection_by_id(&options.projection_id)
        .ok_or_else(|| object_not_found("projection", &options.projection_id))?
        .map_err(projection_error)?;
    if projection.id != FIXTURE_COMPATIBILITY_PROJECTION_ID {
        return Err(CliError::new(
            "compat_projection_invalid",
            "compat diff requires a compatibility projection",
        )
        .with_detail("projection_id", options.projection_id));
    }
    let current_view = fixture_compat_import_view_for_projection(&projection);
    let candidates = fixture_basic_app_candidate_deltas();

    if ctx.json {
        println!(
            "{}",
            compat_diff_success_envelope(&projection, &current_view, &candidates)
        );
    } else {
        println!("{} {} candidates", projection.id, candidates.len());
    }

    Ok(())
}

fn compat_import(ctx: &CommandContext) -> Result<(), CliError> {
    let options = parse_compat_import_options(ctx)?;
    if options.fixture != "basic-app" {
        return Err(
            invalid_request(format!("unknown fixture `{}`", options.fixture))
                .with_detail("fixture", options.fixture),
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
        println!("{}", compat_import_success_envelope(&response));
    } else {
        println!("{} {}", response.operation_id, response.topic_revision_id);
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
        println!("{output}");
        return Ok(());
    }

    let config = require_repository_config(".")?;
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
        println!("{output}");
        return Ok(());
    }

    require_repository_config(".")?;
    Err(CliError::new(
        "object_not_found",
        "Sunlight object was not found",
    ))
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
    fixture: String,
    include: Vec<TopicRevisionSelection>,
    base_checkpoint_id: Option<String>,
}

#[derive(Debug)]
struct ExecutionRunOptions {
    fixture: String,
    view_id: String,
    command_argv: Vec<String>,
    integrity_fixture: Option<StoreIntegrityFixture>,
}

#[derive(Debug)]
struct CheckpointCreateOptions {
    fixture: String,
    view_id: String,
}

#[derive(Debug)]
struct PolicyCheckExportOptions {
    fixture: String,
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
    fixture: String,
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
    fixture: String,
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
    fixture: String,
    session_id: String,
}

#[derive(Debug)]
struct CompatDiffOptions {
    fixture: String,
    projection_id: String,
}

#[derive(Debug)]
struct CompatImportOptions {
    fixture: String,
    projection_id: String,
    session_generation_id: Option<String>,
    candidate_delta_ids: Vec<String>,
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

    let usage = "usage: sun compat project --session <session-id> --fixture basic-app";
    let fixture = fixture.ok_or_else(|| invalid_request(usage))?;
    let session_id = session_id.ok_or_else(|| invalid_request(usage))?;

    Ok(CompatProjectOptions {
        fixture,
        session_id,
    })
}

fn parse_compat_diff_options(ctx: &CommandContext) -> Result<CompatDiffOptions, CliError> {
    let mut fixture = None;
    let mut projection_id = None;
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

    let usage = "usage: sun compat diff --projection <projection-id> --fixture basic-app";
    let fixture = fixture.ok_or_else(|| invalid_request(usage))?;
    let projection_id = projection_id.ok_or_else(|| invalid_request(usage))?;

    Ok(CompatDiffOptions {
        fixture,
        projection_id,
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

    let fixture = fixture.ok_or_else(|| {
        invalid_request(
            "usage: sun compat import --projection <projection-id> --candidate <candidate-id> [--session-generation <generation-id>] --fixture basic-app",
        )
    })?;
    let projection_id = projection_id.ok_or_else(|| {
        invalid_request(
            "usage: sun compat import --projection <projection-id> --candidate <candidate-id> [--session-generation <generation-id>] --fixture basic-app",
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

    let fixture = fixture.ok_or_else(|| {
        invalid_request(
            "usage: sun project materialize --view <resolved-view-id> --purpose execution|compatibility|inspection|export --fixture basic-app",
        )
    })?;
    let view_id = view_id.ok_or_else(|| {
        invalid_request(
            "usage: sun project materialize --view <resolved-view-id> --purpose execution|compatibility|inspection|export --fixture basic-app",
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

    let fixture = fixture.ok_or_else(|| {
        invalid_request(
            "usage: sun checkpoint create --view <resolved-view-id> --fixture basic-app",
        )
    })?;
    let view_id = view_id.ok_or_else(|| {
        invalid_request(
            "usage: sun checkpoint create --view <resolved-view-id> --fixture basic-app",
        )
    })?;

    Ok(CheckpointCreateOptions { fixture, view_id })
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

    let fixture = fixture.ok_or_else(|| {
        invalid_request(
            "usage: sun git export --checkpoint <checkpoint-id> --branch <git-ref> --fixture basic-app",
        )
    })?;
    let checkpoint_id = checkpoint_id.ok_or_else(|| {
        invalid_request(
            "usage: sun git export --checkpoint <checkpoint-id> --branch <git-ref> --fixture basic-app",
        )
    })?;
    let git_ref = git_ref.ok_or_else(|| {
        invalid_request(
            "usage: sun git export --checkpoint <checkpoint-id> --branch <git-ref> --fixture basic-app",
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

    let fixture = fixture.ok_or_else(|| {
        invalid_request(
            "usage: sun policy check-export --checkpoint <checkpoint-id> --fixture basic-app",
        )
        .with_detail("missing", "fixture")
    })?;
    let checkpoint_id = checkpoint_id.ok_or_else(|| {
        invalid_request(
            "usage: sun policy check-export --checkpoint <checkpoint-id> --fixture basic-app",
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
    let mut include = None;
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
                include = Some(parse_view_include(value)?);
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

    let fixture = fixture
        .ok_or_else(|| invalid_request("usage: sun view resolve requires --fixture basic-app"))?;
    let include = include.ok_or_else(|| {
        invalid_request(
            "usage: sun view resolve requires --include topic:revision[,topic:revision]",
        )
    })?;

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
    let mut integrity_fixture = None;
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
                if value != "." {
                    return Err(invalid_request("fixture execution supports only --cwd .")
                        .with_detail("cwd", value.clone()));
                }
            }
            "--timeout" => {
                let value = args
                    .next()
                    .ok_or_else(|| invalid_request("usage: sun run --timeout <duration>"))?;
                if value != "fixture" {
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

    let fixture =
        fixture.ok_or_else(|| invalid_request("usage: sun run requires --fixture basic-app"))?;
    let view_id =
        view_id.ok_or_else(|| invalid_request("usage: sun run requires --view <view>"))?;
    if command_argv.is_empty() {
        return Err(invalid_request(
            "usage: sun run --view <view> --fixture basic-app -- <command> [args...]",
        ));
    }

    Ok(ExecutionRunOptions {
        fixture,
        view_id,
        command_argv,
        integrity_fixture,
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
                        "usage: sun execution promote-output requires --classification <class>",
                    )
                })?;
                classification = Some(value.clone());
            }
            "--topic" => {
                let value = args.next().ok_or_else(|| {
                    invalid_request("usage: sun execution promote-output --topic <topic>")
                })?;
                if value != FIXTURE_WRITE_TOPIC_ID {
                    return Err(CliError::new(
                        "promotion_topic_not_found",
                        "promotion target topic was not found",
                    )
                    .with_detail("topic_id", value.clone()));
                }
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
            "usage: sun execution promote-output <execution-id> --path <path> --session <session> --classification <class> --fixture basic-app",
        )
    })?;
    let fixture = fixture.ok_or_else(|| {
        invalid_request("usage: sun execution promote-output requires --fixture basic-app")
    })?;

    Ok(ExecutionPromoteOutputOptions {
        execution_id,
        fixture,
        path,
        session_id,
        classification,
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
        CliError::new("not_initialized", "Sunlight repository is not initialized")
            .with_detail("path", config_path.display().to_string())
            .with_detail("source", error.to_string())
    })
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
    fixture: String,
}

#[derive(Debug)]
struct SessionStartOptions {
    topic: String,
    view_id: String,
    actor_id: String,
    fixture: String,
}

#[derive(Debug)]
struct ArtifactCommandOptions {
    session_id: String,
    fixture: String,
    operands: Vec<String>,
}

#[derive(Debug)]
struct MutationCommandOptions {
    session_id: String,
    fixture: String,
    operands: Vec<String>,
    expect_hash: Option<String>,
    patch_file: Option<String>,
    content_file: Option<String>,
    classification: Option<String>,
}

#[derive(Debug)]
struct ExecutionPromoteOutputOptions {
    execution_id: String,
    fixture: String,
    path: Option<String>,
    session_id: Option<String>,
    classification: Option<String>,
}

fn parse_topic_create_options(ctx: &CommandContext) -> Result<TopicCreateOptions, CliError> {
    let mut display_name = None;
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
            "usage: sun topic create <slug> --display-name <name> --fixture basic-app",
        ));
    }

    Ok(TopicCreateOptions {
        slug: operands.remove(0),
        display_name: display_name.ok_or_else(|| {
            invalid_request("usage: sun topic create requires --display-name <name>")
        })?,
        fixture: fixture.ok_or_else(|| {
            invalid_request("usage: sun topic create requires --fixture basic-app")
        })?,
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
        fixture: fixture.ok_or_else(|| {
            invalid_request("usage: sun session start requires --fixture basic-app")
        })?,
    })
}

fn parse_artifact_options(
    ctx: &CommandContext,
    command: &'static str,
    min_operands: usize,
    max_operands: usize,
) -> Result<ArtifactCommandOptions, CliError> {
    let mut session_id = None;
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

    let session_id = session_id.ok_or_else(|| {
        invalid_request(format!("usage: sun {command} requires --session <session>"))
    })?;
    let fixture = fixture.ok_or_else(|| {
        invalid_request(format!("usage: sun {command} requires --fixture basic-app"))
    })?;

    Ok(ArtifactCommandOptions {
        session_id,
        fixture,
        operands,
    })
}

fn artifact_usage(command: &str) -> String {
    match command {
        "read" => "usage: sun read <path-or-artifact-id> --session <session> --fixture basic-app",
        "list" => "usage: sun list [path-prefix] --session <session> --fixture basic-app",
        "search" => "usage: sun search <query> --session <session> --fixture basic-app",
        "patch" => {
            "usage: sun patch <path> --session <session> --fixture basic-app --expect-hash <hash> --patch-file <file>"
        }
        "write" => {
            "usage: sun write <path> --session <session> --fixture basic-app --expect-hash <hash-or-new> --content-file <file> --classification <class>"
        }
        "move" => {
            "usage: sun move <from> <to> --session <session> --fixture basic-app --expect-hash <hash>"
        }
        "delete" => {
            "usage: sun delete <path> --session <session> --fixture basic-app --expect-hash <hash>"
        }
        "metadata set" => {
            "usage: sun metadata set <path> --session <session> --fixture basic-app --expect-hash <hash> --classification <class>"
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
                        "usage: sun {command} requires --classification <class>"
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
    let fixture = fixture.ok_or_else(|| {
        invalid_request(format!("usage: sun {command} requires --fixture basic-app"))
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

fn init_success_envelope(
    repository_id: &str,
    repo_root: &str,
    sunlight_dir: &str,
    created_config: bool,
    created_gitignore: bool,
    created_directories: usize,
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
            "\"created_directories\":{}",
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
            "\"export_ready\":true",
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
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"policy.check-export\",",
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

fn policy_explain_success_envelope(report: &GitExportValidationReport) -> String {
    format!(
        concat!(
            "{{\"ok\":true,",
            "\"data\":{{",
            "\"command\":\"policy.explain\",",
            "\"validation_report_id\":\"{}\",",
            "\"ids\":{{",
            "\"validation_report_id\":\"{}\"",
            "}},",
            "\"validation_report\":{}",
            "}},",
            "\"warnings\":[]",
            "}}"
        ),
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
            "\"resolved_view_id\":\"{}\"",
            "}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"projection_id\":\"{}\",",
            "\"baseline\":{{",
            "\"resolved_view_id\":\"{}\",",
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
        json_escape(&current_view.resolved_view_id),
        optional_single_repo_tree_json(current_view.tree_identity.as_ref()),
        json_escape(&projection.id),
        json_escape(&projection.resolved_view_id),
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
    matches!(
        candidate.kind,
        CompatCandidateKind::ModifiedSource
            | CompatCandidateKind::CreatedSource
            | CompatCandidateKind::DeletedSource
            | CompatCandidateKind::MetadataChanged
    ) && candidate.classification == "source"
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
            "\"checkpoint_id\":\"{}\",",
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
        json_escape(&report.checkpoint_id),
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

fn view_resolve_success_envelope(result: &ResolvedViewResult) -> String {
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
            "\"ids\":{{\"resolved_view_id\":\"{}\",\"base_checkpoint_id\":\"{}\"}},",
            "\"view\":{},",
            "\"resolved_view_id\":\"{}\",",
            "\"base_checkpoint_ids\":{},",
            "\"topic_frontier\":{},",
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
        view_resolve_view_json(result),
        json_escape(&result.resolved_view_id),
        string_array_json(result.base_checkpoint_ids.iter().map(String::as_str)),
        topic_frontier_json(result),
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
    format!(
        concat!(
            "{{",
            "\"status\":\"{}\",",
            "\"exit_code\":{},",
            "\"timed_out\":{}",
            "}}"
        ),
        execution.result.status.as_str(),
        execution
            .result
            .exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "null".to_string()),
        execution.result.timed_out,
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
        "{{\"ok\":false,\"error\":{{\"code\":\"{}\",\"message\":\"{}\",\"details\":{}}}}}",
        json_escape(error.code),
        json_escape(&error.message),
        details,
    )
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

fn print_help() {
    println!(
        "\
sun

Usage:
  sun init [--repo <path>]
  sun topic create <slug> --display-name <name> --fixture basic-app --json
  sun session start --topic <topic> --view <view-selector> --actor <actor-id> --fixture basic-app --json
  sun read <path> --session <session> --fixture basic-app [--json]
  sun list [path-prefix] --session <session> --fixture basic-app [--json]
  sun search <query> --session <session> --fixture basic-app [--json]
  sun patch <path> --session <session> --fixture basic-app --expect-hash <hash> --patch-file <file> [--json]
  sun write <path> --session <session> --fixture basic-app --expect-hash <hash-or-new> --content-file <file> --classification <class> [--json]
  sun move <from> <to> --session <session> --fixture basic-app --expect-hash <hash> [--json]
  sun delete <path> --session <session> --fixture basic-app --expect-hash <hash> [--json]
  sun metadata set <path> --session <session> --fixture basic-app --expect-hash <hash> --classification <class> [--json]
  sun view resolve --fixture basic-app --include topic:revision[,topic:revision] [--json]
  sun project materialize --view <resolved-view-id> --purpose execution|compatibility|inspection|export --fixture basic-app [--projection-root <empty-path>] [--json]
  sun projection quarantine-cleanup --projection <projection-id> --projection-root <path> --fixture basic-app [--json]
  sun run --view <resolved-view-id> --fixture basic-app [--integrity-fixture store-mismatch|scan-missing-blob|verified] --json -- cargo test
  sun execution promote-output <execution-id> --path <path> --session <session> --classification <class> --fixture basic-app [--json]
  sun checkpoint create --view <resolved-view-id> --fixture basic-app [--json]
  sun policy check-commit [--paths <path>...] --json
  sun policy explain <validation-report-id> --json
  sun git export --checkpoint <checkpoint-id> --branch <git-ref> --fixture basic-app [--write-plan|--execute-fixture success|ref-update-failure|export-map-failure|--execute-local --repo <path>] --json
  sun status --view <resolved-view-id> --fixture basic-app [--json]
  sun status --export <export-map-id> --fixture basic-app [--json]
  sun status --git <commit-or-ref> --fixture basic-app [--json]
  sun status --projection <projection-id> --fixture basic-app [--projection-root <local-path>] [--integrity-fixture store-mismatch|scan-missing-blob|verified] [--json]
  sun status --execution <execution-id> --fixture basic-app [--promoted] [--json]
  sun inspect export:<export-map-id> --fixture basic-app [--json]
  sun inspect git:<commit-or-ref> --fixture basic-app [--json]
  sun inspect view:<resolved-view-id> --fixture basic-app [--json]
  sun inspect conflict:<conflict-id> --fixture basic-app [--json]
  sun inspect projection:<projection-id> --fixture basic-app [--projection-root <local-path>] [--integrity-fixture store-mismatch|scan-missing-blob|verified] [--json]
  sun inspect execution:<execution-id> --fixture basic-app [--promoted] [--json]

Commands:
  init       Create the conservative local .sunlight repository layout
  topic      Create fixture-backed Phase 1 topics with stable JSON envelopes
  session    Start fixture-backed Phase 1 sessions with stable JSON envelopes
  read       Read a fixture artifact by repository-relative path
  list       List fixture artifacts by optional path prefix
  search     Search fixture artifact text literally
  patch      Apply a fixture-backed unified diff to one artifact
  write      Write fixture-backed content to one artifact path
  move       Move a fixture artifact path and preserve artifact identity
  delete     Tombstone a fixture artifact path with provenance
  metadata   Set fixture artifact metadata without changing content bytes
  view       Resolve fixture topic revisions into a candidate view
  project    Materialize fixture projections for exact resolved views
  projection Clean up local-only fixture projection quarantine records
  run        Record a fixture execution for an exact resolved view
  execution  Promote a declared fixture execution output into a topic operation
  checkpoint Freeze a fixture resolved view as an in-memory checkpoint
  policy    Validate Sunlight commit and export policy checks
  git        Export a fixture checkpoint to a Git ref
  status     Show fixture object status, including projection integrity diagnostics
  inspect    Inspect fixture objects and local-only projection diagnostics
"
    );
}

fn print_init_help() {
    println!(
        "\
sun init

Usage:
  sun init [--repo <path>]

Creates .sunlight/config.toml, the initial repository layout, and a conservative
.sunlight/.gitignore fragment. Existing config and policy files are preserved.
"
    );
}

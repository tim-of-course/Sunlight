use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use sunlight_core::artifacts::{
    ArtifactIoError, ExpectedHash, InMemoryArtifactStore, ListResponse, MutationArtifactView,
    MutationPayload, MutationRefs, MutationResponse, PatchRequest, ReadResponse, SearchResponse,
    SessionView, SessionVisibleArtifactView, TreeIdentityView, WriteMode, WriteRequest,
    FILE_OPERATION_SEMANTICS_VERSION, FIXTURE_ACTOR_ID, FIXTURE_REPOSITORY_ID,
    FIXTURE_RESOLVED_VIEW_ID, FIXTURE_SESSION_GENERATION_ID, FIXTURE_SESSION_ID, FIXTURE_TREE_HASH,
    FIXTURE_WRITE_TOPIC_ID, POSIX_CASE_SENSITIVE_PATH_POLICY_ID,
};
use sunlight_core::checkpoint::{
    fixture_checkpoint_from_resolved_view, CheckpointRecord, CheckpointValidationError,
    EvidenceRef, GitExportMapRecord, FIXTURE_CREATED_AT, FIXTURE_EXPORT_MAP_ID,
    FIXTURE_GIT_COMMIT_ID,
};
use sunlight_core::compat_import::{
    fixture_basic_app_candidate_deltas, plan_fixture_basic_app_import, CompatImportErrorCode,
    CompatImportRequest, CompatImportResponse, CompatImportValidationError, CompatImportedArtifact,
};
use sunlight_core::execution::{
    fixture_failing_execution_from_resolved_view, fixture_passing_execution_from_resolved_view,
    fixture_promotion_candidate_provenance, ExecutionFoundationError, ExecutionRecord,
    OutputClassification, OutputKind, PromotionCandidateProvenance,
};
use sunlight_core::git_export::{
    git_export_checkpoint, plan_git_export_writer, GitExportCommitPlan, GitExportError,
    GitExportPlanningError, GitExportRefUpdatePlan, GitExportRepositoryState, GitExportRequest,
    GitExportResponse, GitExportValidationFailure, GitExportValidationReport, GitExportWriterInput,
    GitExportWriterPlan, GitRefState, ImportedBaseGitCommit,
};
use sunlight_core::projection::{
    fixture_compatibility_projection_from_resolved_view,
    fixture_execution_projection_from_resolved_view, fixture_export_projection_from_resolved_view,
    fixture_inspection_projection_from_resolved_view, plan_fixture_projection_materialization,
    ProjectionMaterializationCapabilities, ProjectionMaterializationError,
    ProjectionMaterializationErrorCode, ProjectionMaterializationLocalMetadata,
    ProjectionMaterializationPlan, ProjectionMaterializationRequest, ProjectionPurpose,
    ProjectionRecord, ProjectionRootRef, ProjectionStrategy, ProjectionValidationError,
    FIXTURE_COMPATIBILITY_PROJECTION_ID, FIXTURE_EXECUTION_PROJECTION_ID,
    FIXTURE_EXPORT_PROJECTION_ID, FIXTURE_INSPECTION_PROJECTION_ID,
};
use sunlight_core::repository::{
    init_repository, RepositoryConfig, CURRENT_STORAGE_SCHEMA_VERSION,
};
use sunlight_core::resolver::{
    fixture_auth_revision, fixture_base_entries, fixture_overlapping_auth_revision,
    fixture_profile_revision, fixture_profile_revision_missing_auth_dependency,
    fixture_resolver_input, resolve_fixture_view, DependencyClosure, DeterministicResolverOrder,
    ResolvedViewResult, ResolverConflictOrStalenessRecord, ResolverRecordKind, SingleRepoTree,
    TopicRevisionRef, TopicRevisionSelection, FIXTURE_BASE_CHECKPOINT_ID,
};

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
        [scope, command, ..] if scope == "topic" && command == "create" => {
            Err(unimplemented_command(
                "topic.create",
                "sun topic create is parsed, but topic records are not persisted yet",
            ))
        }
        [scope, command, ..] if scope == "session" && command == "start" => {
            Err(unimplemented_command(
                "session.start",
                "sun session start is parsed, but session records are not persisted yet",
            ))
        }
        [scope, command, ..] if scope == "view" && command == "resolve" => view_resolve(&ctx),
        [scope, command, ..] if scope == "project" && command == "materialize" => {
            project_materialize(&ctx)
        }
        [scope, command, ..] if scope == "projection" && command == "create" => {
            project_materialize(&ctx)
        }
        [scope, command, ..] if scope == "checkpoint" && command == "create" => {
            checkpoint_create(&ctx)
        }
        [scope, command, ..] if scope == "git" && command == "export" => git_export(&ctx),
        [scope, command, ..] if scope == "compat" && command == "import" => compat_import(&ctx),
        [command, ..] if command == "run" => execution_run(&ctx),
        [command, ..] if command == "read" => artifact_read(&ctx),
        [command, ..] if command == "list" => artifact_list(&ctx),
        [command, ..] if command == "search" => artifact_search(&ctx),
        [command, ..] if command == "patch" => artifact_patch(&ctx),
        [command, ..] if command == "write" => artifact_write(&ctx),
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
    let plan = plan_fixture_projection_materialization(
        &view,
        fixture_projection_materialization_request(&options),
    )
    .map_err(projection_materialization_error)?;

    if ctx.json {
        println!("{}", projection_materialize_success_envelope(&plan));
    } else {
        println!(
            "{} {}",
            plan.projection.id, plan.projection.root_ref.value
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
    request.git_ref = options.git_ref;
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

    let response = git_export_checkpoint(request).map_err(git_export_error)?;

    if ctx.json {
        println!("{}", git_export_success_envelope(&response));
    } else {
        println!("{} {}", response.checkpoint_id, response.git_ref);
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
            session_generation_id: FIXTURE_SESSION_GENERATION_ID.to_string(),
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
}

#[derive(Debug)]
enum StatusScope {
    Repository,
    Session(String),
    Topic(String),
    Checkpoint(String),
    ExportMap(String),
}

#[derive(Debug)]
struct InspectOptions {
    fixture: String,
    selector: String,
    session_id: Option<String>,
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
}

#[derive(Debug)]
struct CheckpointCreateOptions {
    fixture: String,
    view_id: String,
}

#[derive(Debug)]
struct GitExportOptions {
    fixture: String,
    checkpoint_id: String,
    git_ref: String,
    write_plan: bool,
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
}

#[derive(Debug)]
struct CompatImportOptions {
    fixture: String,
    projection_id: String,
    candidate_delta_ids: Vec<String>,
}

fn parse_compat_import_options(ctx: &CommandContext) -> Result<CompatImportOptions, CliError> {
    let mut fixture = None;
    let mut projection_id = None;
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
            "usage: sun compat import --projection <projection-id> --candidate <candidate-id> --fixture basic-app",
        )
    })?;
    let projection_id = projection_id.ok_or_else(|| {
        invalid_request(
            "usage: sun compat import --projection <projection-id> --candidate <candidate-id> --fixture basic-app",
        )
    })?;

    Ok(CompatImportOptions {
        fixture,
        projection_id,
        candidate_delta_ids,
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
        _ => Err(
            invalid_request(format!("unknown projection materialization strategy `{value}`"))
                .with_detail("strategy", value),
        ),
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

    Ok(GitExportOptions {
        fixture,
        checkpoint_id,
        git_ref,
        write_plan,
    })
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
    })
}

fn parse_status_options(ctx: &CommandContext) -> Result<Option<StatusOptions>, CliError> {
    let mut fixture = None;
    let mut scope = StatusScope::Repository;
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

    Ok(fixture.map(|fixture| StatusOptions { fixture, scope }))
}

fn parse_inspect_options(ctx: &CommandContext) -> Result<Option<InspectOptions>, CliError> {
    let mut fixture = None;
    let mut session_id = None;
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

    Ok(Some(InspectOptions {
        fixture: fixture.unwrap(),
        selector: selectors.remove(0),
        session_id,
    }))
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

fn fixture_inspect(options: &InspectOptions, json: bool) -> Result<String, CliError> {
    let selector = options.selector.as_str();
    if let Some(session_id) = &options.session_id {
        ensure_fixture_session(session_id)?;
    }

    if let Some(operation_id) = selector.strip_prefix("operation:") {
        if operation_id == "op_auth_trim_guard_0001" {
            return Ok(if json {
                fixture_inspect_operation_json()
            } else {
                "operation op_auth_trim_guard_0001 patch src/auth.ts".to_string()
            });
        }
        return Err(object_not_found("operation", operation_id));
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
    if let Some(projection_id) = selector.strip_prefix("projection:") {
        let projection = fixture_projection_by_id(projection_id)
            .ok_or_else(|| object_not_found("projection", projection_id))?
            .map_err(projection_error)?;
        return Ok(if json {
            fixture_inspect_projection_json(&projection)
        } else {
            format!("projection {}", projection.id)
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
            "\"last_operation_id\":\"op_auth_trim_guard_0001\"",
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
    )
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
            "\"before_refs\":{},",
            "\"after_refs\":{}",
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
        before_refs,
        after_refs,
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

fn fixture_inspect_operation_json() -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.operation\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{",
            "\"operation_transaction_id\":\"op_auth_trim_guard_0001\",",
            "\"topic_id\":\"{}\",",
            "\"session_id\":\"{}\",",
            "\"topic_revision_id\":\"rev_auth_nullability_0001\"",
            "}},",
            "\"view\":{},",
            "\"operation\":{{",
            "\"mutation\":\"patch\",",
            "\"actor_id\":\"{}\",",
            "\"authored_context_id\":\"ctx_agent_a_gen_0001\",",
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
            "\"after_refs\":{{\"content_hash\":\"sha256:auth_trim_guard\",\"tree_hash\":\"tree_after_auth_patch_0001\"}}",
            "}},",
            "\"created_revision\":{{",
            "\"topic_revision_id\":\"rev_auth_nullability_0001\",",
            "\"revision_number\":1,",
            "\"parent_revision_id\":null",
            "}}",
            "}},\"warnings\":[]}}"
        ),
        FIXTURE_REPOSITORY_ID,
        FIXTURE_WRITE_TOPIC_ID,
        FIXTURE_SESSION_ID,
        fixture_base_view_json(),
        FIXTURE_ACTOR_ID,
        FIXTURE_SESSION_GENERATION_ID,
        FIXTURE_RESOLVED_VIEW_ID,
        FIXTURE_SESSION_GENERATION_ID,
        FIXTURE_TREE_HASH,
    )
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
        _ => "usage: sun <artifact-command> --session <session> --fixture basic-app",
    }
    .to_string()
}

fn parse_mutation_options(
    ctx: &CommandContext,
    command: &'static str,
    operand_count: usize,
) -> Result<MutationCommandOptions, CliError> {
    let mut session_id = None;
    let mut fixture = None;
    let mut expect_hash = None;
    let mut patch_file = None;
    let mut content_file = None;
    let mut classification = None;
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
                    invalid_request("usage: sun write requires --classification <class>")
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

fn fixture_resolver_revisions() -> Vec<TopicRevisionRef> {
    vec![
        fixture_auth_revision(),
        fixture_profile_revision(),
        fixture_overlapping_auth_revision(),
        fixture_profile_revision_missing_auth_dependency(),
    ]
}

fn fixture_resolved_view_by_id(view_id: &str) -> Option<ResolvedViewResult> {
    fixture_known_resolved_views()
        .into_iter()
        .find(|view| view.resolved_view_id == view_id)
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
        capabilities: ProjectionMaterializationCapabilities::all_supported(),
    }
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
    let view = fixture_resolved_view(vec![fixture_auth_revision(), fixture_profile_revision()]);
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
        let view = fixture_resolved_view(Vec::new());
        Some(fixture_compatibility_projection_from_resolved_view(
            &view,
            "gen_agent_a_0001",
        ))
    } else {
        fixture_projection_by_id(projection_id)
    }
}

fn fixture_compat_import_view_for_projection(projection: &ProjectionRecord) -> ResolvedViewResult {
    if projection.id == FIXTURE_COMPATIBILITY_PROJECTION_ID {
        fixture_resolved_view(Vec::new())
    } else {
        fixture_resolved_view_by_id(&projection.resolved_view_id)
            .unwrap_or_else(|| fixture_resolved_view(Vec::new()))
    }
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
            "selected compatibility candidate is cache or build output"
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

fn fixture_inspect_projection_json(projection: &ProjectionRecord) -> String {
    format!(
        concat!(
            "{{\"ok\":true,\"data\":{{",
            "\"command\":\"inspect.projection\",",
            "\"repository_id\":\"{}\",",
            "\"ids\":{{\"projection_id\":\"{}\",\"resolved_view_id\":\"{}\"}},",
            "\"view\":{{",
            "\"resolved_view_id\":\"{}\",",
            "\"tree_identity\":{}",
            "}},",
            "\"projection\":{}",
            "}},\"warnings\":[]}}"
        ),
        json_escape(&projection.repository_id),
        json_escape(&projection.id),
        json_escape(&projection.resolved_view_id),
        json_escape(&projection.resolved_view_id),
        single_repo_tree_json(&projection.tree_identity),
        projection_record_json(projection),
    )
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
  sun topic create <slug> --display-name <name> --json
  sun session start --topic <topic> --view <view-selector> --actor <actor-id> --json
  sun read <path> --session <session> --fixture basic-app [--json]
  sun list [path-prefix] --session <session> --fixture basic-app [--json]
  sun search <query> --session <session> --fixture basic-app [--json]
  sun patch <path> --session <session> --fixture basic-app --expect-hash <hash> --patch-file <file> [--json]
  sun write <path> --session <session> --fixture basic-app --expect-hash <hash-or-new> --content-file <file> --classification <class> [--json]
  sun view resolve --fixture basic-app --include topic:revision[,topic:revision] [--json]
  sun project materialize --view <resolved-view-id> --purpose execution|compatibility|inspection|export --fixture basic-app [--json]
  sun run --view <resolved-view-id> --fixture basic-app --json -- cargo test
  sun checkpoint create --view <resolved-view-id> --fixture basic-app [--json]
  sun git export --checkpoint <checkpoint-id> --branch <git-ref> --fixture basic-app [--write-plan] --json

Commands:
  init       Create the conservative local .sunlight repository layout
  topic      Parse Phase 1 topic commands; persistence is not implemented yet
  session    Parse Phase 1 session commands; persistence is not implemented yet
  read       Read a fixture artifact by repository-relative path
  list       List fixture artifacts by optional path prefix
  search     Search fixture artifact text literally
  patch      Apply a fixture-only unified diff to one artifact
  write      Write fixture-only content to one artifact path
  view       Resolve fixture topic revisions into a candidate view
  project    Materialize fixture projections for exact resolved views
  run        Record a fixture execution for an exact resolved view
  checkpoint Freeze a fixture resolved view as an in-memory checkpoint
  git        Export a fixture checkpoint to a Git ref
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

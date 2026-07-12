# Sunlight Execution Projection Contract v0.1

| Field | Value |
| --- | --- |
| Status | Phase 3 planning contract |
| Date | July 3, 2026 |
| Scope | Execution sandbox materialization, `sun run`, execution/evidence records, output promotion, cache policy, status/inspect exposure, failure modes, and acceptance tests |
| Sources | `docs/sunlight_consolidated_architecture_v0_3.md`, `docs/sunlight_implementation_backlog_v0_1.md`, `docs/sunlight_schema_contracts_v0_1.md`, `docs/sunlight_native_io_phase1_spec_v0_1.md`, `docs/sunlight_resolver_conflict_fixtures_v0_1.md`, `docs/sunlight_checkpoint_git_export_v0_1.md`, `docs/sunlight_cli_status_inspect_v0_1.md` |

## Purpose

Phase 3 succeeds when Sunlight can run a command against an exact conflict-free `resolved_view`, capture enough evidence to explain and checkpoint the result, and convert approved tool-produced source changes into topic-owned operations.

Execution projections are adapters. They are not authoring sessions, not checkpoint trees, and not a source of truth. Source changes made by a tool become durable only after explicit promotion into `operation_transaction` records owned by one topic.

## Execution Flow

| Step | Required input | Required output |
| --- | --- | --- |
| `projection.create` | Exact `resolved_view_id`, purpose `execution`, cache policy, requested working directory | Isolated projection with `projection_id`, strategy, root path or handle, tree identity, and store-integrity guard |
| `execution.run` / `sun run` | Resolved view, command argv, working directory, timeout, env policy, projection options | `execution` record with command, projection ID, environment summary, input refs, output summaries, and result |
| `execution.classify_outputs` | Execution ID and sandbox output scan | Classified output records: source-like delta, generated artifact, log, cache, coverage, secret, or ignored |
| `execution.promote_output` | Execution ID, selected paths, target topic/session, classification, preconditions | Topic-owned patch/write/move/delete operation transactions with execution provenance |
| `checkpoint.create` | Conflict-free resolved view and selected evidence refs | Checkpoint evidence may reference execution summaries that match the same view and tree identity |

## Sandbox Materialization

An execution sandbox materializes bytes from a conflict-free `resolved_view.tree_identity`. It must not read source bytes from the mutable Git working tree.

Required projection metadata:

- `projection_id`
- `resolved_view_id`
- `tree_identity`
- `purpose: "execution"`
- `strategy`: one of `copy`, `reflink`, `hardlink_readonly`, or `overlay_copyup`
- `root_ref`: local-only filesystem path or opaque handle
- `created_from_content_tree`
- `writable_policy`
- `store_integrity_policy`
- `cache_key`
- `privacy_class: "local_only"`

Materialization rules:

- Reject views with `conflict_ids` or `staleness_ids`.
- Preserve path policy decisions, executable bits, symlink policy, tombstones, and artifact path bindings.
- Prefer shared-content strategies measured by the projection spike, but keep `copy` as the correctness fallback.
- Immutable store objects must be protected from command writes. Hardlinks are allowed only when the target filesystem and permission mode prevent store mutation.
- Writable tools use private copy-up/output areas. Any changed source path is an execution side effect until promoted.

## Command Runner

Minimum CLI:

```text
sun run --view <resolved-view-id> -- <command> [args...]
sun run --view <resolved-view-id> --cwd <repo-relative-path> --timeout <duration> --json -- <command> [args...]
```

The command runner stores normalized argv, not a shell string, unless the user explicitly invokes a shell. `working_directory` is repository-relative and validated by the path policy. The runner captures:

- exit code or signal
- timeout state
- started and finished timestamps
- stdout/stderr byte counts, digests, and bounded summaries
- projection ID and strategy
- environment summary digest
- declared input refs, including resolved view and tree identity
- output summaries and classified changed paths

Raw logs, sandbox directories, package caches, coverage output, and full environment dumps are `local_only` by default.

For no-fixture runs on Windows, the local MVP creates a dedicated Windows Job Object before launching the command. The root process is created with `CREATE_SUSPENDED`, assigned to the fully configured job, and resumed only after assignment succeeds. The job enables kill-on-close, an active-process limit, aggregate and per-process memory limits, and per-process/job user CPU-time limits. Timeout, runner cleanup, and resource-policy termination terminate the complete job and reap the root process. Job completion notifications distinguish `cpu_time_limit`, `process_memory_limit`, `job_memory_limit`, and `active_process_limit` from `wall_clock_timeout` and ordinary `command_exit`. If job creation, configuration, assignment, or resume fails, the run fails closed with `execution_containment_setup_failed`; it never resumes the uncontained process.

Repository `[execution_policy]` keys use integer local-MVP units: `process_memory_limit_bytes` and `job_memory_limit_bytes` are bytes, `cpu_time_limit_ms` is cumulative user CPU milliseconds, and `active_process_limit` counts the root plus all descendants. Defaults are respectively 2 GiB, 4 GiB, 300,000 ms, and 32 processes. Older configs that omit these keys receive those defaults. Memory values are validated from 16 MiB through 1 TiB, job memory must be at least process memory, CPU time from 1 through 86,400,000 ms, and process count from 1 through 1,024.

Non-Windows builds retain bounded output, wall timeout, and their existing best-effort process cleanup, but explicitly record process-tree, CPU, and memory enforcement as `not_enforced`. Network access and writes outside the managed writable projection are not isolated on any platform in this slice; execution/status/inspect surfaces continue to expose those limitations rather than claiming full sandbox enforcement.

## Execution Record

The Phase 3 `execution` record uses the v1 schema contract.

```json
{
  "schema_version": 1,
  "record_type": "execution",
  "id": "exec_auth_profile_tests_0001",
  "repository_scope": {
    "kind": "single",
    "repository_id": "repo_fixture_basic_app"
  },
  "resolved_view_id": "view_auth_profile_ready_0001",
  "tree_identity": {
    "kind": "SingleRepoTree",
    "repository_id": "repo_fixture_basic_app",
    "tree_hash": "tree_auth_profile_ready_0001"
  },
  "command": {
    "argv": ["cargo", "test"],
    "shell": null
  },
  "working_directory": ".",
  "environment_summary": {
    "id": "env_summary_wsl_rust_0001",
    "os": "linux",
    "arch": "x86_64",
    "tool_hints": {
      "cargo": "digest-or-version-if-available"
    },
    "env_policy": "default_redacted",
    "digest": "sha256:envsummary"
  },
  "projection_id": "projection_exec_auth_profile_0001",
  "inputs": {
    "resolved_view_id": "view_auth_profile_ready_0001",
    "tree_identity": "tree_auth_profile_ready_0001",
    "path_policy_id": "path_policy_posix_case_sensitive_v1",
    "operation_semantics_version": "file_ops_v1"
  },
  "outputs": [
    {
      "kind": "stdout_summary",
      "digest": "sha256:stdout",
      "byte_length": 1200,
      "privacy_class": "local_only"
    }
  ],
  "promotions": [],
  "result": {
    "status": "pass",
    "exit_code": 0,
    "timed_out": false
  },
  "started_at": "2026-07-03T00:00:00Z",
  "finished_at": "2026-07-03T00:00:05Z",
  "privacy_class": "policy_gated"
}
```

Identity inputs follow the schema contract: resolved view ID, tree identity, normalized command, working directory, environment summary digest, input refs, and projection strategy. Timestamps describe the run but should not be the only identity inputs.

## Environment Summary

Environment summaries are reproducibility hints, not full machine snapshots. Capture enough to explain common MVP failures without leaking secrets.

Required summary fields:

- OS family, kernel/platform hint, architecture
- Sunlight binary version or build ID
- command runner version
- selected tool versions or digests when cheap to obtain
- redacted environment allowlist and digest
- network policy, if enforced
- sandbox writable policy

Denied by default: raw `env`, credentials, home-directory paths unless redacted, SSH/Git tokens, package registry tokens, and absolute cache paths intended only for local debugging.

## Evidence Records And Checkpoints

Checkpoint evidence refs may point to execution summaries when:

- `execution.resolved_view_id` equals the checkpoint `resolved_view_id`.
- `execution.tree_identity` equals the checkpoint `tree_identity`.
- `result.status` is `pass` or the checkpoint policy includes an explicit waiver record.
- referenced output summaries are export-policy safe.
- raw logs and sandbox paths are not reachable from commit-default manifests.

The execution record remains separate from the checkpoint. Checkpoints select evidence; executions do not automatically make a view landable.

## Output Promotion

Tool-produced source changes are promoted with an explicit command:

```text
sun execution promote-output <execution-id> --path <sandbox-path> --session <session-id> --classification <class> --fixture basic-app --json
```

The current CLI fixture accepts only the declared passing execution candidate for `basic-app`.

Promotion rules:

- The target topic must exist and be the sole owner of the new operation transaction.
- Promotion creates normal `operation_transaction` records using patch/write/move/delete payloads and Phase 1 preconditions.
- The operation `authored_context_id` points to the execution's resolved view and includes `execution_id`, `projection_id`, selected output path, before hash, after hash, and classification.
- Generated source, formatter changes, lockfiles, migrations, and codegen outputs must be promoted before checkpoint/export can treat them as source truth.
- Promotion never mutates the original `resolved_view`; it advances the target topic to a new revision and later resolver runs include that revision.
- Secret or local-only output cannot be promoted unless policy converts it to an allowed reference or rejects it with a stable error.

Promotion should prefer patches when the changed artifact existed in the input tree and whole writes for new generated files or intentionally broad replacements.

## Cache And Local-Only Policy

Projection roots, execution sandboxes, package caches, stdout/stderr payloads, raw output scans, environment dumps, and integrity quarantine folders are `local_only`.

Allowed to persist as policy-gated summaries:

- execution record summaries
- projection strategy and cache key
- environment summary digest and redacted hints
- bounded log summaries
- output classifications and content digests
- promotion provenance links

Cache entries are reusable only when repository ID, resolved view ID, tree identity, path policy, projection strategy, and writable policy match. Cache reuse must revalidate store integrity before command execution. Failed integrity validation quarantines the cache/projection and creates an inspectable native error; it does not repair by silently trusting the sandbox.

## Status And Inspect Exposure

Phase 3 extends the existing JSON envelope.

| Command | Required exposure |
| --- | --- |
| `sun run --view <view> --json -- <command>` | Returns `command: "execution.run"`, execution ID, resolved view ID, tree identity, projection ID, result, output summary counts, and promotion candidates. |
| `sun status --view <resolved-view-id> --json` | Adds latest executions for the view, result counts, promotion-required counts, and cache/projection native errors. |
| `sun status --execution <execution-id> --json` | Shows command, result, projection strategy, environment summary ID, output classifications, and promotion status. |
| `sun inspect execution:<execution-id> --json` | Shows the execution record, resolved view/tree refs, command argv, environment summary, projection ID, output summaries, promotions, and result. |
| `sun inspect projection:<projection-id> --json` | Shows local-only projection metadata, strategy, view/tree refs, cache key, integrity status, and retention state. |
| `sun inspect operation:<operation-id> --json` | For promoted outputs, shows execution provenance in authored context and promotion source refs. |

Suggested error codes: `execution_conflicted_view`, `execution_missing_tree`, `execution_projection_failed`, `execution_command_failed`, `execution_timeout`, `execution_store_integrity_failed`, `execution_output_secret`, `promotion_no_changes`, `promotion_policy_failed`, `promotion_precondition_failed`, and `promotion_topic_not_found`.

## Failure Modes

| Failure | Required behavior |
| --- | --- |
| Conflicted or stale view | Reject before projection with stable error and inspectable conflict/staleness IDs. |
| Missing tree identity | Reject; do not synthesize from working tree files. |
| Projection materialization failure | Persist a failed execution or native error summary only if enough context exists; no command is run. |
| Store integrity failure | Quarantine projection/cache entry and return `execution_store_integrity_failed`. |
| Command non-zero exit | Persist execution record with `result.status: "fail"` and captured summaries. |
| Timeout | Stop the process tree when possible, record `timed_out: true`, and retain local-only logs according to policy. |
| Secret-like output | Mark output `secret` or quarantine; block export and promotion unless policy supplies a safe reference. |
| Promotion precondition failure | Write no operation transaction; return expected/actual hashes and unchanged topic/session state. |
| Partial promotion | Either write all selected operation transactions for one promotion request or none; expose retryable details. |

## Fixture Baseline

Use existing fixture IDs where possible:

| Object | Fixture ID |
| --- | --- |
| Repository | `repo_fixture_basic_app` |
| Base checkpoint | `checkpoint_base_0001` |
| Conflict-free view | `view_auth_profile_ready_0001` |
| Conflicted view | `view_auth_profile_conflicted_0001` |
| Tree identity | `tree_auth_profile_ready_0001` |
| Projection | `projection_exec_auth_profile_0001` |
| Passing execution | `exec_auth_profile_tests_0001` |
| Failing execution | `exec_auth_profile_tests_fail_0001` |
| Promotion operation | `op_promote_generated_auth_0001` |

## Acceptance Tests

| Test | Required assertions |
| --- | --- |
| `execution_rejects_conflicted_view` | `sun run` against a view with conflict/staleness IDs fails before projection and exposes those IDs. |
| `execution_materializes_exact_tree` | Sandbox files match `resolved_view.tree_identity`; unrelated working-tree files are absent. |
| `projection_records_strategy_and_policy` | Projection metadata stores strategy, purpose, cache key, local-only root ref, tree identity, and integrity policy. |
| `run_records_pass_result` | Passing command creates execution record with command argv, result pass, exit code 0, projection ID, environment summary, and output summaries. |
| `run_records_failure_result` | Non-zero command creates execution record with result fail and does not block inspect/status exposure. |
| `run_timeout_records_state` | Timed-out command records timeout state and bounded output summaries. |
| `store_integrity_failure_quarantines_projection` | Mutated immutable store object or failed verification quarantines projection/cache and returns a stable error. |
| `outputs_are_local_only_by_default` | Raw logs, sandbox paths, caches, and coverage outputs are not exportable commit-default records. |
| `promotion_creates_topic_operation` | Approved formatter/codegen output becomes a topic-owned operation with execution provenance and a new topic revision. |
| `promotion_precondition_failure_no_write` | Stale before hash writes no operation and leaves topic/session state unchanged. |
| `status_inspect_show_execution_links` | `status --view`, `status --execution`, `inspect execution`, `inspect projection`, and promoted `inspect operation` expose matching IDs. |
| `checkpoint_accepts_matching_execution_evidence` | Checkpoint can select passing execution evidence only when resolved view and tree identity match. |
| `checkpoint_rejects_unpromoted_source_output` | Source-like sandbox deltas block checkpoint/export until promoted or explicitly ignored by policy. |

## Implementation Boundaries

- Do not implement long-lived dev servers or watch mode in the first Phase 3 slice.
- Do not infer topic operations from sandbox diffs without an explicit promotion command.
- Do not store raw logs, sandboxes, package caches, or full environment dumps in commit-default records.
- Do not run commands from a conflicted, stale, or tree-less resolved view.
- Do not change Phase 1 operation records or Phase 2 resolver records to make execution tests pass; link execution records to them.
- Do not make Git working tree state authoritative for command inputs or promoted outputs.

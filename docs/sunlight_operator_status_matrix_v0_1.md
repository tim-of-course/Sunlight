# Sunlight Operator Status Matrix v0.1

| Field | Value |
| --- | --- |
| Status | Docs-only operator contract |
| Date | July 3, 2026 |
| Scope | Status and inspect surfaces across repository, session, resolved view, projection, execution, checkpoint, and Git export lifecycle states |
| Sources | `docs/sunlight_consolidated_architecture_v0_3.md`, `docs/sunlight_cli_status_inspect_v0_1.md`, `docs/sunlight_execution_projection_v0_1.md`, `docs/sunlight_checkpoint_git_export_v0_1.md`, `docs/sunlight_schema_contracts_v0_1.md` |

## Purpose

This contract defines the operator-facing state matrix for Sunlight lifecycle objects. It does not introduce new source-of-truth objects. It standardizes how `sun status` and `sun inspect` expose native state so an operator can answer: what exists, what is exact, what is blocked, what can be advanced, what is local-only, and what has been exported to Git.

All surfaces use the common JSON envelope from `sunlight_cli_status_inspect_v0_1.md`. Status is a compact operational snapshot. Inspect is an exact object/provenance view. Neither surface may infer truth from `git status`, mutable projections, or unpromoted execution side effects.

## State Vocabulary

Lifecycle states are stable lowercase strings. A record may also expose derived booleans such as `conflict_free`, `export_ready`, or `promotion_required`, but those booleans must be explainable from the state and referenced native records.

| Object | State field | Allowed states |
| --- | --- | --- |
| Repository | `repository.lifecycle_state` | `uninitialized`, `initialized`, `policy_blocked`, `corrupt` |
| Session | `session.lifecycle_state` | `active`, `refresh_available`, `stale`, `conflicted`, `closed` |
| Resolved view | `resolved_view.lifecycle_state` | `resolved`, `conflicted`, `stale`, `missing_tree`, `checkpointable` |
| Projection | `projection.lifecycle_state` | `materialized`, `dirty_local`, `quarantined`, `expired`, `removed` |
| Execution | `execution.lifecycle_state` | `queued`, `running`, `passed`, `failed`, `timed_out`, `promotion_required`, `promoted` |
| Checkpoint | `checkpoint.lifecycle_state` | `candidate`, `blocked`, `frozen`, `export_ready`, `exported` |
| Git export | `git_export.lifecycle_state` | `validated`, `exported`, `partial`, `failed`, `unknown` |

If multiple conditions apply, expose the most blocking state and list supporting `native_errors`, `warnings`, `conflict_ids`, `staleness_ids`, validation failures, or promotion candidates.

## Operator Matrix

| Surface | Status selector | Required status summary | Inspect selector | Required inspect detail |
| --- | --- | --- | --- | --- |
| Repository | `sun status --json` | Initialization state, schema/policy IDs, open topics, active sessions, native errors, latest checkpoint/export pointers if known | `sun inspect repository:<repository-id> --json` | Repository record, path/projection/Git policies, base checkpoint refs, storage health, privacy/export defaults |
| Session | `sun status --session <session> --json` | Lifecycle state, actor, write topic, current `session_generation_id`, current `resolved_view_id`, refresh policy, changed artifacts, last operation | `sun inspect session:<session> --json` | Session record, generation history, pinned frontier, write-topic ownership, capabilities, close reason when closed |
| Resolved view | `sun status --view <resolved-view-id> --json` | Lifecycle state, exact base checkpoints, exact topic frontier, conflict/staleness counts, tree identity, latest executions and projections | `sun inspect view:<resolved-view-id> --json` | Full resolved view record, dependency closure, resolver order inputs, conflict/staleness refs, tree identity or missing-tree reason |
| Projection | `sun status --projection <projection-id> --json` | Lifecycle state, purpose, strategy, resolved view, tree identity, retention state, integrity status, dirty-local flag | `sun inspect projection:<projection-id> --json` | Local-only projection metadata, root handle/path privacy, cache key, writable policy, store-integrity policy, quarantine reason |
| Execution | `sun status --execution <execution-id> --json` | Lifecycle state, result, command summary, resolved view, projection, output classification counts, promotion status | `sun inspect execution:<execution-id> --json` | Execution record, argv, environment summary, inputs, outputs, promotions, result, bounded log summaries only |
| Checkpoint | `sun status --checkpoint <checkpoint-id> --json` | Lifecycle state, resolved view, tree identity, evidence result summary, export readiness, export refs | `sun inspect checkpoint:<checkpoint-id> --json` | Checkpoint record, exact frontier, selected evidence refs, conflict-free flag, retention class, export refs |
| Git export | `sun status --export <export-map-id> --json` | Lifecycle state, checkpoint, validation report, Git ref, Git commit IDs, partial/failure marker | `sun inspect export:<export-map-id> --json` | Export-map record, export shape, validation report ID, Git refs/commits, exported timestamp |
| Git ref lookup | `sun status --git <commit-or-ref> --json` | Known/unknown mapping to native checkpoint/export map | `sun inspect git:<commit-or-ref> --json` | Matching export map and checkpoint when known; compatibility-only summary or `object_not_found` otherwise |

## Lifecycle Rules

| Object | Required transition rules |
| --- | --- |
| Repository | `uninitialized` becomes `initialized` only through `repository.init`. `policy_blocked` reports native policy errors without scanning Git working tree changes. `corrupt` is reserved for invalid or unreadable native records and must include inspectable repair context. |
| Session | Accepted writes advance the session to a new generation before the mutation response returns. `refresh_available` is advisory. `stale` or `conflicted` keeps the last good generation until explicit refresh/adaptation succeeds. |
| Resolved view | Exact selectors produce `resolved` only after dependency closure. Any conflict or staleness ref blocks `checkpointable`. `missing_tree` means the resolver result exists but cannot be projected, executed, checkpointed, or exported. |
| Projection | Projection state never advances native source truth. `dirty_local` means local filesystem changes exist outside native operations. `quarantined` follows integrity failure and must not auto-repair by trusting sandbox bytes. |
| Execution | Execution reads from an exact projection of a resolved view. Tool-produced source deltas move to `promotion_required` until explicit promotion creates topic-owned operation transactions. Passed executions do not make a checkpoint by themselves. |
| Checkpoint | `frozen` requires exact conflict-free resolved view, tree identity, and selected evidence. `export_ready` additionally requires passing policy validation. Export refs may be appended after Git export, but the frozen tree and evidence do not change. |
| Git export | `validated` is not exported. `exported` requires a persisted `git_export_map`. `partial` means Git artifacts were created but native mapping or ref update did not fully persist. Git commits remain compatibility artifacts, not native authorship. |

On Windows, no-fixture execution records report Job Object enforcement for process-tree cleanup, CPU time, per-process/job memory, and active-process count. `policy_blocked` executions carry a stable resource `termination_reason`; wall-clock expiry remains `timeout`, and nonzero command exit remains ordinary `fail`. A Job Object setup or assignment error returns `execution_containment_setup_failed` before command code is resumed. Network and broad filesystem-write isolation remain structured unenforced dimensions. Non-Windows status reports process-tree, CPU, and memory limits as unenforced.

Repository status and inspect do not emit an always-on multi-record durability warning. Canonical-mutating commands publish native state and all declared derived JSON mirrors through the local publication outbox. Recovery runs before status or inspect reads state, so a valid pending committed batch is completed idempotently and removed. A valid uncommitted batch is discarded without publishing its records. If a committed manifest or staged payload is missing, malformed, path-invalid, or digest-invalid, the command returns stable error `publication_outbox_recovery_failed` with the retained manifest/evidence path; operators must preserve that directory for inspection and restore only the originally declared bytes. Sunlight must not synthesize an operation, checkpoint, projection, execution, promotion, view, topic, session-generation, compatibility-import, or export-map payload during outbox recovery.

Status, inspect, recovery, and canonical save use `.sunlight/local/command-transaction.lock`; they do not race an active publication. One command shares a ten-second wait budget across its lock acquisitions and safe retries, so ordinary short publication overlap remains invisible without multiplying the timeout. Cancellation stops the wait. Exhausting the budget returns `repository_writer_busy` with the lock path and `timeout_ms: 10000`. A canonical save also compares its loaded `publication_sequence` to the current canonical sequence while holding the lock. Mismatch returns `concurrent_state_update` with `expected_sequence` and `actual_sequence`, publishes no canonical or derived bytes, and requires the command to reload and retry. Derived record IDs use only ASCII letters, digits, `_`, and `-` beginning with a letter or digit; Windows device names, ADS syntax, separators, dots, spaces, and other filename forms are rejected both when declared and when recovered from a manifest.

Interrupted canonical publication is recovered before status or inspect reads state. If the journal's intended sequence and digest match a fully valid canonical or staged candidate, recovery completes and removes staged/backup/journal debris. If the intended bytes are malformed but a fully valid canonical or backup candidate exists, recovery selects the highest sequence, preserves bounded evidence, and reports advisory warning `state_recovery_rolled_back` until the next successful state publication cleans that evidence. If no candidate is fully valid, the CLI returns stable error `state_recovery_failed` with `canonical`, `staged`, `backup`, and `journal` paths; it must not fall back to fixture output, delete evidence, or silently parse malformed bytes.

## Policy Failure Operator Rules

Commit and export policy failures are hard gates. Status and inspect surfaces
may expose warnings for advisory conditions, but `commit_policy_failed` and
`export_policy_failed` must keep the affected repository, checkpoint, or Git
export in a blocked/failed state until native inputs are corrected. Validator
report context may still name the abstract validator-layer code
`policy_validation_failed`.

For repository `policy_blocked` states caused by `policy.check-commit`, status
must expose `commit_policy_failed`, the validation report ID when available,
candidate path count, blocked path count, and the first blocking checks. Inspect
must show the exact managed ignore block status, staged/requested `.sunlight`
paths, blocked local/cache/quarantine/projection paths, raw execution paths,
relevant `privacy_class` values, and any unsafe references found inside records.
Safe operator actions are to restore the managed ignore block, remove local-only
paths from the candidate set, promote generated output, import source through
native IO, or rewrite native records through Sunlight commands.

For checkpoint or Git export failures caused by `policy.check-export`, status
must expose `export_policy_failed`, checkpoint ID, resolved view ID, tree
identity, validation report ID, export target, hard-failure count, and the first
blocking checks. Inspect must show checkpoint/view/evidence/export-map context,
generated-output promotion requirements, local-only evidence references, invalid
or moving refs, and target Git ref policy details. Safe operator actions are to
rerun promotion/import/checkpoint/export planning with exact IDs and then rerun
validation. Operators must not edit generated Git output, exported commits,
target branches, or export-map records directly to bypass the gate.

## Cross-Surface Invariants

- Every status response includes `data.command`, `data.repository_id` when initialized, `data.ids`, and `data.view` or `null`.
- Every session-scoped status or inspect response includes the exact current `session_generation_id`, `resolved_view_id`, refresh policy, topic frontier, and tree identity when available.
- Every view, projection, execution, checkpoint, and export status exposes enough IDs to round-trip through `inspect`.
- Moving selectors such as `topic@head`, `main`, and `latest` are normalized before durable view, checkpoint, evidence, or export records are created.
- Local-only paths, projection roots, sandboxes, caches, raw logs, and environment dumps stay out of commit-default status summaries. Inspect may expose local handles only with explicit privacy classification.
- Promotion, checkpoint, and export gates must report hard failures as stable error codes, not warnings.
- Git working tree state may be reported only as compatibility context for Git surfaces. It must not change native status, inspect provenance, checkpoint content, or exported source bytes.

## Minimum Acceptance Checks

| Check | Required assertion |
| --- | --- |
| `status_round_trips_to_inspect` | Every ID shown by a status selector can be inspected or returns a stable `object_not_found` after deletion/retention expiry. |
| `session_status_read_after_write` | Immediately after mutation, session status shows the new session generation, write-topic head, resolved view, and changed artifact summary. |
| `conflicted_view_blocks_downstream` | Conflicted or stale views block projection, execution, checkpoint, and Git export with inspectable conflict/staleness IDs. |
| `projection_dirty_is_not_source_truth` | Dirty local projection bytes appear only as projection state or promotion candidates, not as topic revisions or checkpoint content. |
| `execution_promotion_is_explicit` | Source-like execution output remains `promotion_required` until promotion creates topic-owned operation transactions. |
| `checkpoint_export_trace_is_exact` | Artifact inspect can trace changed source through operation, topic revision, resolved view, execution evidence, checkpoint, export map, and Git commit when exported. |
| `git_lookup_is_lossy_compatibility` | Unknown Git refs do not synthesize native topics or operations; they return `object_not_found` or a compatibility-only summary. |

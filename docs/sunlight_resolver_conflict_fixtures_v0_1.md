# Sunlight Resolver Conflict Fixtures v0.1

| Field | Value |
| --- | --- |
| Status | Phase 2 fixture plan |
| Date | July 3, 2026 |
| Scope | Deterministic multi-topic composition, resolver conflict objects, status/inspect exposure, and fixture acceptance tests |
| Sources | `docs/sunlight_consolidated_architecture_v0_3.md`, `docs/sunlight_implementation_backlog_v0_1.md`, `docs/sunlight_schema_contracts_v0_1.md`, `docs/sunlight_native_io_phase1_spec_v0_1.md`, `docs/sunlight_operation_transactions_v0_1.md`, `docs/sunlight_cli_status_inspect_v0_1.md` |

## Purpose

This document defines the first Phase 2 resolver and conflict fixture plan. It does not add implementation requirements beyond the existing Phase 2 backlog IDs P2.1-P2.6. It gives implementation agents practical fixture cases for composing exact topic revisions into deterministic resolved views or `conflict_staleness` records.

Phase 2 succeeds when `view.resolve` can take a base checkpoint and exact topic frontier, close dependencies, apply operation transactions in a safety-preserving order, and return either a `resolved_view` with `SingleRepoTree` identity or structured conflict/staleness records that are visible through status and inspect.

## Fixture Baseline

Use the same fixture naming style as Phase 1.

| Object | Fixture ID |
| --- | --- |
| Repository | `repo_fixture_basic_app` |
| Base checkpoint | `checkpoint_base_0001` |
| Base resolved view | `view_base_0001` |
| Base tree | `tree_fixture_base_0001` |
| Path policy | `path_policy_posix_case_sensitive_v1` |
| Operation semantics | `file_ops_v1` |
| Existing auth artifact | `artifact_src_auth_ts` at `src/auth.ts`, hash `sha256:auth_base` |
| Existing route artifact | `artifact_src_routes_ts` at `src/routes.ts`, hash `sha256:routes_base` |
| Existing config artifact | `artifact_package_json` at `package.json`, hash `sha256:package_base` |

Fixtures should use deterministic labels until canonical hashing is fully wired. Every operation transaction used by these fixtures must already satisfy the Phase 1 operation contract: topic ownership, authored context, preconditions, read set, write set, mutation payload, before refs, after refs, and parent revision boundary.

## Resolver Inputs

The first resolver fixture API may be expressed as a JSON fixture or a later CLI command, but it must normalize to this exact input shape before identity is computed:

```json
{
  "repository_id": "repo_fixture_basic_app",
  "base_checkpoint_ids": ["checkpoint_base_0001"],
  "topic_frontier": {
    "topic_auth_nullability": "rev_auth_nullability_0001",
    "topic_profile_ui": "rev_profile_ui_0001"
  },
  "operation_semantics_version": "file_ops_v1",
  "path_policy_id": "path_policy_posix_case_sensitive_v1",
  "policy": {
    "dependency_closure": "required",
    "same_artifact_conflicts": "block",
    "deterministic_tie_breakers": "safe_only"
  }
}
```

Moving selectors such as `topic@head` are allowed at the command boundary only for P2.1. The resolver record stores exact topic revision IDs. Unresolved selectors must not appear in `resolved_view.topic_frontier`.

## Dependency Frontiers

Dependency closure is resolved before operation ordering.

| Case | Required behavior |
| --- | --- |
| All dependencies selected | Include dependency revisions in `dependency_closure` and continue. |
| Required dependency missing | Return a `conflict_staleness` object with `kind: "missing_dependency"` and no tree identity. |
| Selected dependency is older than required | Return `kind: "stale_dependency"` with required and selected revision refs in `candidate_refs`. |
| Dependency introduces its own conflict | Return the dependency conflict in the target view conflict set; do not hide it behind the dependent topic. |
| Same topic selected twice through dependency closure | Normalize to one exact head if compatible; otherwise return `kind: "frontier_inconsistent"`. |

MVP dependency precision is conservative. A topic revision's `dependency_revision_ids` and operation `authored_context_id` are enough to report staleness even before file-level read dependency tracking is refined.

## Deterministic Ordering

Ordering has two phases:

1. Safety order: causal parent revisions, declared `dependency_revision_ids`, and explicit resolution operations.
2. Canonical order: stable tie-breakers only for operations proven independent or commutative.

The canonical tie-breaker for safe operations is:

1. `repository_id`
2. normalized primary path from `write_set`
3. `artifact_id`
4. `topic_id`
5. `topic_revision_id`
6. `operation_transaction_id`

This order is a reproducibility rule, not a merge policy. If applying two same-artifact operations in different safe candidate orders can produce different bytes, path bindings, tombstones, executable bits, or classifications, the resolver creates a conflict unless a dependency edge or explicit resolution operation chooses the order.

## Conflict Object Shape

Use the schema contract's `conflict_staleness` record for both conflicts and staleness. Conflict records are scoped to the attempted resolved view, not to a topic globally.

```json
{
  "schema_version": 1,
  "record_type": "conflict_staleness",
  "id": "conflict_auth_overlap_0001",
  "repository_scope": {
    "kind": "single",
    "repository_id": "repo_fixture_basic_app"
  },
  "kind": "same_artifact_conflict",
  "resolved_view_id": "view_auth_profile_conflicted_0001",
  "artifact_ids": ["artifact_src_auth_ts"],
  "path_refs": [
    {
      "path": "src/auth.ts",
      "path_state": "active"
    }
  ],
  "operation_ids": [
    "op_auth_trim_guard_0001",
    "op_profile_auth_formatter_0001"
  ],
  "authored_context_ids": [
    "ctx_agent_a_gen_0001",
    "ctx_agent_b_gen_0001"
  ],
  "policy_reason": "same artifact operations are not proven commutative under file_ops_v1",
  "candidate_refs": {
    "base_content_hash": "sha256:auth_base",
    "candidate_hashes": [
      "sha256:auth_order_a_then_b",
      "sha256:auth_order_b_then_a"
    ],
    "operation_semantics_version": "file_ops_v1",
    "path_policy_id": "path_policy_posix_case_sensitive_v1"
  },
  "resolution_operation_id": null,
  "privacy_class": "commit_default",
  "created_at": "2026-07-03T00:00:00Z"
}
```

Candidate bytes are policy-gated and should not be embedded in the conflict summary. Candidate hashes, operation IDs, paths, artifact IDs, and authored context IDs are enough for deterministic snapshot tests.

## Fixture Matrix

| Fixture | Topics and operations | Expected result |
| --- | --- | --- |
| `resolve_independent_files` | `topic_auth_nullability@rev_auth_nullability_0001` patches `src/auth.ts`; `topic_profile_ui@rev_profile_ui_0001` writes `src/profile.ts`. | Conflict-free `resolved_view`; `topic_frontier` has both revisions; `tree_identity.tree_hash` is stable across repeated runs. |
| `resolve_same_file_disjoint_commutative` | Two patch operations on `artifact_src_auth_ts` edit disjoint hunks anchored to `sha256:auth_base`; applying in either order produces `sha256:auth_disjoint_both`. | Conflict-free `resolved_view`; canonical operation order is recorded or reproducible; final hash is identical across reversed input frontier order. |
| `conflict_same_file_overlapping_patches` | `op_auth_trim_guard_0001` and `op_profile_auth_null_guard_0001` patch overlapping lines in `src/auth.ts`. | `resolved_view.conflict_ids` contains `conflict_auth_overlap_0001`; no successful tree identity is exposed for checkpoint/export. |
| `conflict_same_file_order_sensitive_patches` | Two patches both apply to `sha256:auth_base`, but order A then B and B then A produce different hashes. | `kind: "same_artifact_conflict"` with both candidate hashes; deterministic tie-breaker is not used to choose a winner. |
| `conflict_broad_write_vs_patch` | One topic whole-file replaces `src/auth.ts`; another topic patches `src/auth.ts` from the same base. | `kind: "non_commutative_write"` unless the write topic declares a dependency on the patch topic or an explicit resolution operation exists. |
| `conflict_same_path_create` | Two topics write new `src/session.ts` with `expected_hash: "new"`. | `kind: "path_conflict"` with both operation IDs and candidate content hashes. |
| `resolve_same_artifact_move_then_write_dependency` | `topic_rename_auth@rev_rename_auth_0001` moves `src/auth.ts` to `src/security/auth.ts`; `topic_auth_nullability@rev_auth_nullability_0002` depends on the rename and patches the moved artifact ID. | Conflict-free view; artifact ID is preserved; final active path is `src/security/auth.ts`. |
| `conflict_move_write_no_dependency` | One topic moves `artifact_src_auth_ts`; another patches `src/auth.ts` from the same base with no dependency. | `kind: "ambiguous_move_write"` unless the patch can be replayed against the moved artifact with exact authored intent. |
| `conflict_move_delete_same_artifact` | One topic moves `artifact_src_routes_ts`; another deletes the same artifact from the same base. | `kind: "move_delete_conflict"` with path refs for source, destination, and tombstone. |
| `conflict_delete_then_write_path` | One topic deletes `src/routes.ts`; another replaces `src/routes.ts` from the same base. | `kind: "delete_write_conflict"` unless dependency order says replacement intentionally revives the path. |
| `conflict_path_case_policy` | Under case-insensitive path policy, topics create `src/User.ts` and `src/user.ts`. | `kind: "path_policy_conflict"` and `policy_reason` references the path policy ID. |
| `missing_dependency_frontier` | `rev_profile_ui_0002` declares dependency on `rev_auth_nullability_0001`, but the requested frontier omits it. | `kind: "missing_dependency"`; `candidate_refs.required_revision_ids` includes `rev_auth_nullability_0001`. |
| `stale_dependency_frontier` | Requested frontier includes `rev_auth_nullability_0001`, but selected dependent revision requires `rev_auth_nullability_0002`. | `kind: "stale_dependency"`; selected and required revision IDs are machine-readable. |

## Resolved View Records

Conflict-free fixtures create a `resolved_view` record with:

- `base_checkpoint_ids`: exact base checkpoint IDs.
- `topic_frontier`: exact selected topic revisions after selector normalization and dependency closure.
- `dependency_closure`: complete list of included dependency revision IDs.
- `operation_semantics_version`: `file_ops_v1`.
- `path_policy_id`: `path_policy_posix_case_sensitive_v1`.
- `conflict_ids`: `[]`.
- `staleness_ids`: `[]`.
- `tree_identity`: `SingleRepoTree { repository_id, tree_hash }`.

Conflicted fixtures may create a resolved-view attempt record so conflicts have a stable scope. Its `conflict_ids` or `staleness_ids` must be populated, and `tree_identity` must either be absent/null under the implementation's JSON policy or explicitly marked unavailable. Checkpoint creation and Git export must reject conflicted views.

## Status And Inspect Exposure

Phase 2 extends the Phase 1 JSON envelope without changing it.

| Command | Required exposure |
| --- | --- |
| `sun view resolve --base checkpoint_base_0001 --include topic@rev,... --json` | Returns `command: "view.resolve"`, normalized `topic_frontier`, `dependency_closure`, `resolved_view_id`, `tree_identity` on success, or `conflict_ids`/`staleness_ids` on blocked resolution. |
| `sun status --json` | Adds `pending_views` or `native_errors` entries for unresolved resolver attempts, with counts and conflict IDs only. |
| `sun status --view <resolved-view-id> --json` | Shows base, exact frontier, dependency closure status, conflict/staleness counts, and tree identity when available. |
| `sun inspect view:<resolved-view-id> --json` | Shows resolved-view record fields, operation IDs in resolver order, dependency closure, conflict IDs, staleness IDs, and tree identity. |
| `sun inspect conflict:<conflict-id> --json` | Shows the `conflict_staleness` summary, competing operation IDs, path refs, artifact IDs, authored context IDs, policy reason, and candidate hashes. |
| `sun inspect operation:<operation-id> --json` | Keeps the Phase 1 authored-context view and may add `resolver_impacts` listing views or conflicts that reference the operation. |

Failure responses use the existing envelope. Suggested new resolver error codes are `resolve_conflicted`, `missing_dependency`, `stale_dependency`, `frontier_inconsistent`, and `path_policy_conflict`. A blocked resolver should still persist inspectable conflict/staleness summaries when policy allows.

## Acceptance Tests

| Test | Required assertions |
| --- | --- |
| `selector_normalization_exact_frontier` | Moving selectors normalize to exact revision IDs before `resolved_view` identity is computed; repeated runs produce the same ID. |
| `frontier_order_does_not_change_tree` | Reversing input topic order for independent or commutative operations yields the same resolver order, tree hash, and resolved view identity. |
| `independent_files_compose` | Operations on different artifacts produce a conflict-free view with both after hashes in the tree. |
| `same_file_disjoint_commutative_composes` | Reversed candidate application orders produce identical bytes; resolver records no conflict. |
| `same_file_overlap_conflicts` | Overlapping patches produce one `same_artifact_conflict` with both operation IDs and no checkpointable tree identity. |
| `order_sensitive_same_artifact_conflicts` | Candidate hashes differ; resolver returns conflict instead of picking the canonical operation order. |
| `whole_file_write_vs_patch_conflicts` | Whole-file replace plus patch on same artifact produces `non_commutative_write` unless ordered by dependency or resolution operation. |
| `same_path_create_conflicts` | Two new writes to the same path create a path conflict with both candidate hashes. |
| `move_then_write_with_dependency_resolves` | Dependency-ordered move plus patch preserves artifact ID and final path. |
| `move_delete_without_resolution_conflicts` | Move/delete same artifact creates `move_delete_conflict`; inspect exposes source, destination, and tombstone refs. |
| `dependency_closure_required` | Missing or stale dependencies create staleness records and do not silently drop dependent revisions. |
| `status_inspect_show_conflict` | `status --view`, `inspect view`, `inspect conflict`, and `inspect operation` expose the same linked IDs. |

## Implementation Boundaries

- Do not write conflict markers into source files by default.
- Do not treat deterministic topic ordering as proof that same-artifact operations are safe.
- Do not require AST or semantic merge for the MVP; file-level patch/write/move/delete semantics are sufficient.
- Do not checkpoint or Git-export a view with unresolved `conflict_ids` or `staleness_ids`.
- Do not change Phase 1 mutation records to make tests pass. Add Phase 2 fixture records around existing operation transactions.

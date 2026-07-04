# Sunlight Checkpoint and Git Export Contract v0.1

| Field | Value |
| --- | --- |
| Status | Phase 4 planning contract |
| Date | July 3, 2026 |
| Scope | Checkpoint records, frozen resolved views, evidence gates, export maps, Git projection policy, status/inspect exposure, and acceptance tests |
| Sources | `docs/sunlight_consolidated_architecture_v0_3.md`, `docs/sunlight_implementation_backlog_v0_1.md`, `docs/sunlight_schema_contracts_v0_1.md`, `docs/sunlight_policy_validation_spec_v0_1.md`, `docs/sunlight_cli_status_inspect_v0_1.md`, `docs/sunlight_resolver_conflict_fixtures_v0_1.md` |

## Purpose

Phase 4 succeeds when Sunlight can freeze a conflict-free `resolved_view` as a durable `checkpoint`, validate the exact export candidate set, project the checkpoint tree into ordinary Git history, and record the native-to-Git mapping without making Git or a working tree authoritative.

This contract adds the practical checkpoint and export rules needed after Phase 1 native operations, Phase 2 resolver records, and Phase 3 execution evidence exist. It does not require implementation code in this planning slice.

## Phase 4 Checkpoint

| Step | Required input | Required output |
| --- | --- | --- |
| `checkpoint.create` | Exact `resolved_view_id` with `conflict_ids: []`, `staleness_ids: []`, and available `tree_identity` | Immutable `checkpoint` record with selected evidence and export eligibility summary |
| `policy.check-export` | Checkpoint ID and export target policy | Stable validation report over reachable checkpoint, view, topic, operation, content, evidence, and export-map records |
| `git.export` | Checkpoint ID, validated report, export shape, target Git ref | Ordinary Git commit or branch plus `git_export_map` record |
| `status/inspect` | Checkpoint, export map, validation report, or Git ref selector | Native provenance links from artifact -> operation -> topic -> view -> execution -> checkpoint -> Git ref |

The default MVP export shape is one Git commit per checkpoint. Topic-per-commit and curated series may be added later, but the first exporter should prove the simplest boring Git artifact.

## Checkpoint Record Shape

The Phase 4 `checkpoint` record uses the v1 schema contract and remains tied to repository, view, operation, resolver, and policy records.

```json
{
  "schema_version": 1,
  "record_type": "checkpoint",
  "id": "checkpoint_auth_profile_ready_0001",
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
  "topic_frontier": {
    "topic_auth_nullability": "rev_auth_nullability_0002",
    "topic_profile_ui": "rev_profile_ui_0003"
  },
  "evidence_refs": [
    {
      "kind": "execution",
      "execution_id": "exec_auth_profile_tests_0001",
      "result": "pass",
      "resolved_view_id": "view_auth_profile_ready_0001",
      "tree_identity": {
        "kind": "SingleRepoTree",
        "repository_id": "repo_fixture_basic_app",
        "tree_hash": "tree_auth_profile_ready_0001"
      }
    }
  ],
  "conflict_free": true,
  "created_by": {
    "actor_id": "operator_1",
    "command": "checkpoint.create"
  },
  "created_at": "2026-07-03T00:00:00Z",
  "retention_class": "landable",
  "export_refs": [],
  "privacy_class": "commit_default"
}
```

Required rules:

- `resolved_view_id`, `tree_identity`, `topic_frontier`, `evidence_refs`, and `conflict_free` are identity inputs.
- `topic_frontier` stores exact `topic_revision_id` values only. Moving selectors such as `topic@head`, `main`, or `latest` are invalid inside checkpoints.
- `tree_identity` must exactly match the referenced `resolved_view.tree_identity`.
- A checkpoint over a single repository uses `SingleRepoTree`; the field shape must remain compatible with future `RepoTreeMap`.
- `export_refs` is empty until a successful Git export writes a `git_export_map`. The export map is authoritative; commit messages are not.

## Frozen Resolved View Requirements

Checkpoint creation is allowed only for a frozen resolved view with:

- Exact `base_checkpoint_ids` and exact `topic_frontier`.
- Complete `dependency_closure`.
- `operation_semantics_version` and `path_policy_id` pinned.
- `conflict_ids: []` and `staleness_ids: []`.
- Available `tree_identity` whose repository IDs match the checkpoint repository scope.
- Resolver order reproducible from the selected topic revisions and operation transactions.
- No unresolved dependency, policy, generated-output, or evidence gap that the selected export policy marks as blocking.

If the resolver persisted an attempted view with conflicts, checkpoint creation returns `resolve_conflicted` or the more specific resolver error code and includes inspectable `conflict_staleness` IDs. It must not synthesize a Git tree from conflicted candidates.

## Evidence And Conflict Gating

Checkpoint evidence is selected, not blindly attached. Each evidence ref must be exact, policy-classified, and tied to the same `resolved_view_id` and `tree_identity` unless the record explicitly describes a safe omission.

| Gate | Required behavior |
| --- | --- |
| Conflict gate | Reject any view with non-empty `conflict_ids` or `staleness_ids`. |
| Execution gate | Required test/build evidence must have `result: "pass"` for the checkpoint tree or be explicitly waived by a policy-gated review record. |
| Promotion gate | Generated source, formatter output, lockfile changes, migrations, and codegen outputs in the tree must be represented by topic-owned operation transactions, not unpromoted execution side effects. |
| Policy gate | Run `policy.check-export`; reject `secret`, `local_only`, unsafe references, missing reachability, oversize payloads, and blocked `.sunlight` paths. |
| Provenance gate | Every changed artifact in the checkpoint tree must trace to imported base content or reachable operation transactions. |
| Integrity gate | Export materialization must use checkpoint content blobs and content trees, not mutable working tree files. |

Evidence summaries may be `commit_default` or `policy_gated`; raw logs, sandboxes, caches, coverage directories, local env dumps, and unpromoted outputs remain `local_only`.

## Export-Map Record

`git_export_map` records map a native checkpoint to Git artifacts created from that checkpoint.

```json
{
  "schema_version": 1,
  "record_type": "git_export_map",
  "id": "export_map_checkpoint_auth_profile_ready_0001",
  "repository_id": "repo_fixture_basic_app",
  "checkpoint_id": "checkpoint_auth_profile_ready_0001",
  "tree_identity": {
    "kind": "SingleRepoTree",
    "repository_id": "repo_fixture_basic_app",
    "tree_hash": "tree_auth_profile_ready_0001"
  },
  "git_remote": null,
  "git_ref": "refs/heads/sunlight/auth-profile-ready",
  "git_commit_ids": [
    "git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  ],
  "export_shape": {
    "kind": "single_checkpoint_commit",
    "parent_policy": "base_checkpoint_git_parent",
    "include_sunlight_metadata": "policy_approved_manifest_only"
  },
  "validation_report_id": "validation_export_auth_profile_ready_0001",
  "exported_at": "2026-07-03T00:00:00Z",
  "privacy_class": "commit_default"
}
```

Required rules:

- Identity inputs are checkpoint ID, tree identity, export shape, Git ref, and Git commit IDs.
- `validation_report_id` is required and must refer to a passing export validation report for the same checkpoint and export target.
- `git_commit_ids` may contain one commit in the MVP. Later export shapes may store a topic-per-commit series.
- `git_remote` is nullable for local branch exports.
- The checkpoint may add the export-map ID to `export_refs` after export, but the original checkpoint tree and selected evidence do not change.

## Git Tree And Commit Projection Policy

The Git exporter projects the checkpoint tree into normal project files. It must not read source files from the mutable Git working tree except to determine Git parent/ref state.

| Area | Policy |
| --- | --- |
| Source bytes | Materialize from checkpoint `content_tree` and `content_blob` records reachable from `tree_identity`. |
| Parent commit | Default parent is the Git commit imported as the checkpoint's base checkpoint when available. If missing, fail with an actionable compatibility error rather than guessing. |
| Commit shape | Default `single_checkpoint_commit`; additional shapes are schema-versioned export policies. |
| Commit message | Include short human text plus checkpoint ID and resolved view ID, but do not store essential provenance only in the message. |
| `.sunlight` metadata | Include only policy-approved manifests such as checkpoint, resolved-view summary, conflict-free summaries, and export-map records when configured. Raw objects are gated. |
| Working tree | Untracked or modified working-tree files outside the checkpoint are ignored for export tree construction. |
| Git ref update | Create or update the requested branch/ref only after validation and Git tree creation succeed. |
| Failure | If Git commit creation succeeds but export-map persistence fails, report a partial export requiring repair; do not pretend native provenance is complete. |

The exporter may use a temporary export projection or direct Git plumbing. Either way, the exported Git tree must equal the checkpoint tree plus any explicitly allowed `.sunlight` manifest files selected by policy.

## Policy Allow/Deny Surfaces

Phase 4 relies on the policy validator defined in `sunlight_policy_validation_spec_v0_1.md`.

Allowed by default after validation:

- `checkpoint` manifests with exact refs and no private payload bytes.
- Conflict-free `resolved_view` summaries.
- `git_export_map` records.
- Sanitized topic/revision metadata needed for provenance.
- Safe execution summaries selected as evidence.

Denied by default:

- Raw execution logs, sandboxes, coverage output, package caches, projection caches, local leases, daemon state, and index files.
- `local_only` and `secret` records or payloads.
- Public manifests referencing ignored local paths or raw filesystem paths.
- Moving selectors in checkpoint, export, evidence, or export-map records.
- Unpromoted generated outputs selected into the exported project tree.

Policy-gated:

- Operation payloads, content blobs, large generated files, binaries, vendored payloads, private topic metadata, raw agent provenance, and evidence reports with potentially sensitive content.

Hard failures cannot be downgraded to warnings for `secret`, `local_only`, unsafe reference, missing reachability, raw execution inclusion, or conflicted view export.

## Import Compatibility Boundaries

Git import creates base checkpoints; Git export creates compatibility artifacts. Neither path changes native authorship rules.

- A Git commit imported as a base checkpoint is immutable source context for later operation transactions.
- Exported Git commits are lossy projections of checkpoints. Native operation, topic, view, execution, checkpoint, and export-map records remain authoritative.
- Re-importing an exported commit should detect the existing `git_export_map` when policy-approved metadata is present, or otherwise match by Git tree hash and configured base relationship.
- Import compatibility does not infer topic operations from arbitrary Git commits in Phase 4. That belongs to later compatibility import work.
- Cross-repo import/export remains out of scope, but record shapes must not assume more than `SingleRepoTree` as a specialization of the `tree_identity` union.

## Status And Inspect Exposure

Phase 4 extends the existing CLI JSON envelope without changing its success/failure shape.

| Command | Required exposure |
| --- | --- |
| `sun checkpoint create <view> --json` | Returns `command: "checkpoint.create"`, checkpoint ID, resolved view ID, tree identity, selected evidence refs, and `export_ready: true/false`. |
| `sun policy check-export --checkpoint <checkpoint-id> --json` | Returns validation report ID, candidate summary, checked record/payload counts, warnings, and hard failures. |
| `sun git export <checkpoint-id> --branch <ref> --json` | Returns `command: "git.export"`, checkpoint ID, validation report ID, Git ref, Git commit IDs, and export-map ID. |
| `sun status --checkpoint <checkpoint-id> --json` | Shows checkpoint record, conflict/evidence/export readiness, validation report summary, and export refs. |
| `sun inspect checkpoint:<checkpoint-id> --json` | Shows frozen resolved view link, exact frontier, tree identity, evidence refs, conflict-free flag, retention class, and export refs. |
| `sun inspect export:<export-map-id> --json` | Shows checkpoint ID, tree identity, Git ref, Git commit IDs, export shape, validation report ID, and exported timestamp. |
| `sun inspect git:<commit-or-ref> --json` | Resolves known Git refs/commits back to export maps when available; otherwise returns `object_not_found` or a compatibility-only summary. |

Suggested Phase 4 error codes: `checkpoint_conflicted_view`, `checkpoint_stale_view`, `checkpoint_missing_tree`, `checkpoint_evidence_failed`, `export_policy_failed`, `export_parent_not_found`, `export_git_failed`, and `export_map_write_failed`.

## Fixture Baseline

Use existing fixture IDs where possible:

| Object | Fixture ID |
| --- | --- |
| Repository | `repo_fixture_basic_app` |
| Base checkpoint | `checkpoint_base_0001` |
| Conflict-free view | `view_auth_profile_ready_0001` |
| Conflicted view | `view_auth_profile_conflicted_0001` |
| Tree identity | `tree_auth_profile_ready_0001` |
| Passing execution | `exec_auth_profile_tests_0001` |
| Checkpoint | `checkpoint_auth_profile_ready_0001` |
| Validation report | `validation_export_auth_profile_ready_0001` |
| Export map | `export_map_checkpoint_auth_profile_ready_0001` |

## Acceptance Tests

| Test | Required assertions |
| --- | --- |
| `checkpoint_freezes_conflict_free_view` | Creates checkpoint from exact resolved view; checkpoint stores resolved view ID, topic frontier, tree identity, evidence refs, and `conflict_free: true`. |
| `checkpoint_rejects_conflicted_view` | View with conflict/staleness IDs fails with a stable error and no checkpoint record. |
| `checkpoint_rejects_missing_tree` | Attempted resolved view without available tree identity cannot be checkpointed. |
| `checkpoint_evidence_must_match_view` | Execution evidence for a different view/tree is rejected unless represented by a policy-gated waiver. |
| `generated_output_requires_promotion_before_checkpoint` | Generated file in checkpoint tree without a promotion operation blocks checkpoint or export according to policy. |
| `export_validation_rejects_moving_selector` | Any checkpoint/export/evidence/export-map record with `topic@head`, `main`, or `latest` fails validation. |
| `export_validation_rejects_raw_execution_refs` | Checkpoint evidence referencing raw logs or sandbox paths fails with `unsafe_reference` or `execution_raw_exclusion`. |
| `export_uses_checkpoint_tree_not_working_tree` | Extra working-tree-only file is absent from exported Git tree. |
| `git_export_single_checkpoint_commit` | Successful export writes one Git commit on target branch and records `git_export_map` with checkpoint ID, tree identity, commit ID, ref, shape, and validation report ID. |
| `export_map_inspect_round_trip` | `inspect checkpoint`, `inspect export`, and `inspect git:<commit>` expose the same checkpoint/export-map/Git IDs. |
| `policy_denies_secret_or_local_only_payload` | Secret or local-only reachable payload blocks export and no Git ref is updated. |
| `reimport_exported_commit_matches_checkpoint` | Imported exported commit can be associated with the export map or matching tree identity without creating inferred topic operations. |

## Implementation Boundaries

- Do not checkpoint conflicted or stale views.
- Do not use the mutable Git working tree as the source of exported project bytes.
- Do not require topic-per-commit export for the MVP.
- Do not put raw logs, caches, sandboxes, private payloads, or secret bytes into Git export by default.
- Do not infer new operation transactions from exported or imported Git commits in Phase 4.
- Do not edit the manager scratchpad from this contract slice.

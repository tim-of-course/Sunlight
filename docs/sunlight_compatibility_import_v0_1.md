# Sunlight Compatibility Import Contract v0.1

| Field | Value |
| --- | --- |
| Status | Phase 6 planning contract |
| Date | July 3, 2026 |
| Scope | Compatibility projections, explicit projection diff import/capture into one topic/session, diff classification, preconditions, path policy, secret/cache quarantine, provenance, status/inspect exposure, failure modes, and acceptance tests |
| Sources | `docs/sunlight_consolidated_architecture_v0_3.md`, `docs/sunlight_implementation_backlog_v0_1.md`, `docs/sunlight_schema_contracts_v0_1.md`, `docs/sunlight_native_io_phase1_spec_v0_1.md`, `docs/sunlight_operation_transactions_v0_1.md`, `docs/sunlight_execution_projection_v0_1.md`, `docs/sunlight_checkpoint_git_export_v0_1.md`, `docs/sunlight_cli_status_inspect_v0_1.md` |

## Purpose

Compatibility import is the explicit escape hatch for humans, editors, and legacy agents that need ordinary files. It lets them edit a compatibility projection and then capture selected filesystem deltas as normal Sunlight operation transactions.

This path is deliberately second-class relative to native artifact IO. A compatibility projection is not an authoring session, not a checkpoint, not an execution sandbox, and not source truth. Source becomes durable only when `sun compat import` or `sun compat capture` validates the projection, classifies the diff, and records accepted changes into exactly one topic through exactly one session.

## Compatibility Projection Flow

| Step | Required input | Required output |
| --- | --- | --- |
| `projection.create` / `sun compat project` | Exact `resolved_view_id` or session generation, purpose `compatibility`, writable policy, retention policy | Compatibility projection with `projection_id`, root ref, base tree identity, path policy, baseline manifest, and local-only mutable state |
| Human or legacy edit | Ordinary filesystem writes inside the projection root | Dirty projection state only; no native operation, topic revision, or checkpoint changes |
| `compat.diff` / `sun compat diff` | Projection ID and optional path filters | Classified candidate deltas with before/after hashes, path policy results, and quarantine/promotion recommendations |
| `compat.import` / `sun compat import` | Projection ID, target session, selected deltas, classifications, preconditions | One topic-owned operation transaction and one new topic revision, or a stable failure with no partial write |
| `compat.capture` / watcher-assisted capture | Same as import, optionally triggered by detected changes | Same semantics as import; watcher detection is only a convenience input |
| `status/inspect` | Projection, import attempt, session, operation, or artifact selector | Native provenance links from projection baseline and selected deltas to operation -> topic -> session -> revision |

`import` and `capture` are semantic synonyms in this contract. Use `import` for an operator-initiated command over a projection and `capture` for a watcher-assisted or editor-integrated flow. Both must pass the same validation gates and write the same operation records.

## Required Invariants

- A compatibility projection is derived from an exact `resolved_view_id` and `tree_identity`; moving selectors must be normalized before projection creation.
- Projection edits do not change native state until an explicit import/capture succeeds.
- Every successful import/capture targets exactly one existing session and therefore exactly one write topic.
- One import/capture request creates exactly one `operation_transaction` and one new `topic_revision`, even when it includes multiple file deltas.
- The operation authored context records the projection ID, baseline resolved view, baseline tree identity, import command, selected deltas, and classification decisions.
- Preconditions are checked before any operation, revision, content, view, or generation record is made visible.
- Failed imports do not advance the session generation and do not create operation or revision records.
- Secret, cache, local-only, ignored, and policy-blocked files are quarantined or skipped by explicit policy; they are never silently imported as source.
- Path policy is enforced on both baseline paths and after paths before diff candidates can become operations.
- Import never reads source truth from the main Git working tree. It compares the projection root to the projection baseline manifest.

## Compatibility Projection Metadata

Compatibility projection metadata is `local_only` by default.

```json
{
  "schema_version": 1,
  "record_type": "projection",
  "id": "projection_compat_agent_a_0001",
  "repository_scope": {
    "kind": "single",
    "repository_id": "repo_fixture_basic_app"
  },
  "purpose": "compatibility",
  "resolved_view_id": "view_agent_a_base_0001",
  "session_generation_id": "gen_agent_a_0001",
  "tree_identity": {
    "kind": "SingleRepoTree",
    "repository_id": "repo_fixture_basic_app",
    "tree_hash": "tree_fixture_base_0001"
  },
  "path_policy_id": "path_policy_posix_case_sensitive_v1",
  "strategy": "copy",
  "root_ref": ".sunlight/projections/compat/projection_compat_agent_a_0001",
  "baseline_manifest_ref": "objects/projection-baselines/sha256/compat_agent_a_0001",
  "writable_policy": "writable_with_explicit_import",
  "store_integrity_policy": "verify_on_import",
  "cache_key": "compat_repo_fixture_basic_app_view_agent_a_base_0001_copy",
  "retention_state": "active",
  "privacy_class": "local_only",
  "created_at": "2026-07-03T00:00:00Z"
}
```

Required projection rules:

- `purpose` is `compatibility`, distinct from `execution`, `inspection`, `export`, and `debug`.
- `baseline_manifest_ref` records every projected path, artifact ID, content hash, executable bit, symlink target, tombstone state, classification, and path policy decision from the input view.
- The projection root may be writable, but content-store bytes remain protected. Reflinks are allowed only with copy-on-write guarantees; hardlinks require read-only or copy-up protection.
- Projection roots, watcher journals, editor temp files, local file snapshots, and quarantine directories remain ignored by Git and excluded from checkpoint/export by default.

## Diff Classification

`compat.diff` compares the projection root to the baseline manifest and emits candidate deltas. It must not infer topic ownership or write native operations.

| Candidate kind | Meaning | Default import behavior |
| --- | --- | --- |
| `modified_source` | Active source-like file changed from baseline hash | Importable as patch or write after preconditions and classification pass |
| `created_source` | New source-like path not present in baseline | Importable as whole-file write with `expected_hash: "new"` |
| `deleted_source` | Baseline active path missing from projection | Importable as delete when explicitly selected |
| `moved_or_renamed` | Baseline artifact appears at a new path with matching or related bytes | Importable as move, or move plus patch/write, when path identity is unambiguous |
| `metadata_changed` | Executable bit, symlink target, or classification changed | Importable as metadata operation if policy allows |
| `generated_source` | Generated source, lockfile, migration, or codegen output | Importable only with explicit classification and policy gates |
| `binary_or_large` | Binary or size-policy-sensitive payload | Policy-gated; may require whole-file write or rejection |
| `cache_or_build_output` | Package cache, build output, coverage, temp file, editor state | Quarantine or ignore by default |
| `secret_like` | Secret detector or policy marks content as secret | Quarantine; never durable secret bytes by default |
| `ignored_path` | Matches repository or Sunlight ignore policy | Ignore unless an explicit policy override reclassifies it |
| `path_policy_blocked` | Path escape, reserved path, invalid Unicode/case/symlink state, or platform-incompatible path | Hard failure for selected import |
| `conflicted_delta` | Projection baseline no longer matches target session generation, or path/artifact mapping is ambiguous | Hard failure until refreshed, adapted, or manually resolved |

Diff output must include enough data for deterministic review:

- `projection_id`, baseline `resolved_view_id`, baseline `tree_identity`, and target session generation if supplied.
- One entry per candidate path with before/after hashes, byte length, executable bit, symlink metadata, detected media type, classification, privacy class, and path policy result.
- A stable `candidate_delta_id` derived from projection ID, normalized path, baseline refs, after digest, metadata, and classification.
- Quarantine refs for blocked bytes without making those bytes reachable from commit-default or policy-gated public manifests.

## Import Preconditions

An import request is a mutation and uses the Phase 1 operation precondition model plus projection-specific checks.

Minimum CLI:

```text
sun compat project --session <session> --json
sun compat diff --projection <projection-id> --json
sun compat import --projection <projection-id> --session <session> --select <candidate-delta-id>... --json
```

Required common preconditions:

```json
{
  "projection_id": "projection_compat_agent_a_0001",
  "projection_purpose": "compatibility",
  "projection_baseline_resolved_view_id": "view_agent_a_base_0001",
  "projection_baseline_tree_identity": {
    "kind": "SingleRepoTree",
    "repository_id": "repo_fixture_basic_app",
    "tree_hash": "tree_fixture_base_0001"
  },
  "session_id": "session_agent_a",
  "session_generation_id": "gen_agent_a_0001",
  "resolved_view_id": "view_agent_a_base_0001",
  "write_topic_id": "topic_auth_nullability",
  "parent_topic_revision_id": null,
  "path_policy_id": "path_policy_posix_case_sensitive_v1",
  "operation_semantics_version": "file_ops_v1",
  "selected_candidate_delta_ids": [
    "compat_delta_src_auth_ts_0001"
  ]
}
```

Validation order:

1. Repository is initialized and the projection exists with `purpose: "compatibility"`.
2. Projection baseline manifest matches the stored `resolved_view_id`, `tree_identity`, path policy, and projection metadata.
3. Projection root is still under the managed projection root and has not been replaced by an unsafe symlink or external mount.
4. Session exists, targets exactly one write topic, and is allowed to import from this projection.
5. The current session generation matches the requested `session_generation_id`.
6. The session generation resolves to the expected `resolved_view_id`, unless the request explicitly uses a refresh/import mode that creates a new authored context after validation.
7. The write topic head matches `parent_topic_revision_id`.
8. Every selected candidate has a valid normalized path, allowed classification, allowed privacy class, and current after digest matching the candidate digest.
9. For existing artifacts, the session generation still has the expected artifact ID, path binding, content hash, executable bit, and classification.
10. For new paths, the session generation still has no active path binding at the normalized path.
11. Move/rename candidates preserve artifact identity and have no ambiguous source or target mapping.
12. Secret/cache/local-only candidates are rejected, quarantined, or explicitly ignored before operation records are created.
13. Store integrity verification passes for any projection strategy that could share content-store storage.

## Operation Mapping

Accepted imports become normal operation transactions. The mutation payload kind is `compat_import`, containing nested file operation payloads. Resolvers may either apply the nested operations directly or normalize them into the same internal patch/write/move/delete semantics used by Phase 1.

```json
{
  "schema_version": 1,
  "record_type": "operation_transaction",
  "id": "op_compat_import_auth_0001",
  "repository_id": "repo_fixture_basic_app",
  "topic_id": "topic_auth_nullability",
  "session_id": "session_agent_a",
  "session_generation_id": "gen_agent_a_0001",
  "actor_id": "human_editor_1",
  "authored_context_id": "ctx_compat_projection_0001",
  "preconditions": {
    "projection_id": "projection_compat_agent_a_0001",
    "resolved_view_id": "view_agent_a_base_0001",
    "session_generation_id": "gen_agent_a_0001",
    "selected_candidate_delta_ids": [
      "compat_delta_src_auth_ts_0001"
    ]
  },
  "read_set": {
    "mode": "projection_baseline",
    "resolved_view_id": "view_agent_a_base_0001",
    "projection_id": "projection_compat_agent_a_0001"
  },
  "write_set": [
    {
      "artifact_id": "artifact_src_auth_ts",
      "path": "src/auth.ts",
      "mutation": "patch",
      "classification": "source"
    }
  ],
  "mutation_payload": {
    "kind": "compat_import",
    "projection_id": "projection_compat_agent_a_0001",
    "baseline_manifest_digest": "sha256:compat_baseline",
    "selected_deltas": [
      {
        "candidate_delta_id": "compat_delta_src_auth_ts_0001",
        "operation_kind": "patch",
        "path": "src/auth.ts",
        "patch_digest": "sha256:auth_projection_patch",
        "base_content_hash": "sha256:auth_base",
        "result_content_hash": "sha256:auth_projection_after",
        "classification": "source",
        "privacy_class": "policy_gated"
      }
    ]
  },
  "before_refs": {
    "artifacts": [
      {
        "artifact_id": "artifact_src_auth_ts",
        "path": "src/auth.ts",
        "path_state": "active",
        "content_hash": "sha256:auth_base",
        "classification": "source"
      }
    ],
    "tree_identity": {
      "kind": "SingleRepoTree",
      "repository_id": "repo_fixture_basic_app",
      "tree_hash": "tree_fixture_base_0001"
    }
  },
  "after_refs": {
    "artifacts": [
      {
        "artifact_id": "artifact_src_auth_ts",
        "path": "src/auth.ts",
        "path_state": "active",
        "content_hash": "sha256:auth_projection_after",
        "classification": "source"
      }
    ],
    "tree_identity": {
      "kind": "SingleRepoTree",
      "repository_id": "repo_fixture_basic_app",
      "tree_hash": "tree_after_compat_import_0001"
    }
  },
  "classification": "source",
  "privacy_class": "policy_gated",
  "logical_time": {
    "parent_topic_revision_id": null,
    "next_topic_revision_number": 1
  },
  "parents": [],
  "created_at": "2026-07-03T00:00:00Z"
}
```

Operation mapping rules:

- Modified text files should become patch payloads when a deterministic patch can be generated against the expected before hash.
- Created files become whole-file writes with `expected_hash: "new"`.
- Deleted files become delete operations with tombstoned path bindings.
- Rename-only changes become move operations preserving artifact identity.
- Rename-plus-edit changes become one transaction containing move plus patch/write deltas, preserving the original artifact identity when unambiguous.
- Metadata changes become metadata operations when supported by the path policy and platform metadata model.
- Generated source, lockfiles, migrations, and codegen outputs must be explicitly classified. They are not source by accident.
- Broad rewrites and formatter-like deltas may be imported, but they should be classified so later resolver checks can treat them as potentially non-commutative broad writes.

## Path Policy

Compatibility import is stricter than ordinary filesystem editing because projection roots can contain editor temp files, symlinks, platform artifacts, and accidental path escapes.

Required path policy behavior:

- Normalize all paths as repository-relative paths under the projection root.
- Reject absolute paths, `..` escapes, reserved `.sunlight` paths, projection metadata paths, and paths outside the repository scope.
- Enforce the configured case-sensitivity and Unicode normalization rules before candidate identity is computed.
- Preserve executable bits only when the path policy and platform support them; otherwise classify the metadata delta as unsupported.
- Apply the symlink policy from the resolved view. Unsafe symlinks, symlink escapes, and symlink-to-secret targets are blocked.
- Treat platform-generated files such as `.DS_Store`, `Thumbs.db`, editor swap files, package caches, and coverage/build directories as ignored or cache candidates unless policy says otherwise.
- Detect path collisions introduced by case folding, Unicode normalization, rename targets, and generated files before writing any operation record.

## Secret, Cache, And Quarantine Policy

Compatibility projections are high-risk capture surfaces. The default is conservative.

| Input | Required behavior |
| --- | --- |
| Secret-like file or content | Mark candidate `secret_like`, move bytes or references to quarantine, return stable error or warning, and do not create durable content blobs unless represented by a typed vault reference policy |
| Cache/build output | Classify `cache_or_build_output`, ignore by default, and keep any retained bytes `local_only` |
| Editor/temp files | Ignore by default; expose in diff only when verbose diagnostics are requested |
| Raw projection snapshots | `local_only`; never reachable from commit-default manifests |
| Import diagnostic logs | `local_only` with bounded summaries allowed in status/inspect |
| Approved generated source | `policy_gated` or `commit_default` only after explicit classification, size checks, and secret scan |
| Quarantine entry | Inspectable by local ID, with reason, source projection, path, digest, and retention state; raw bytes stay local-only |

Policy failures for selected deltas are hard failures unless the request explicitly deselects those deltas or supplies an allowed policy conversion. Secret and local-only bytes cannot be downgraded to warnings.

## Provenance

Imported changes must be at least as explainable as native patch/write operations.

Required provenance links:

- Projection ID, purpose, strategy, root ref or opaque local handle, and baseline manifest digest.
- Baseline resolved view ID, tree identity, topic frontier, path policy, and operation semantics version.
- Target session ID, prior session generation ID, write topic ID, and parent topic revision ID.
- Actor/tool identity for the import command and optional editor/legacy agent hint if supplied.
- Selected candidate delta IDs, classification decisions, before/after refs, and quarantine/ignore decisions for unselected risky candidates.
- New operation transaction ID, topic revision ID, session generation ID, resolved view ID, and tree identity after success.

The operation's `authored_context_id` describes the projection baseline and import moment. It must not pretend that the edit was authored through native artifact IO, and it must not rewrite the projection baseline after a later refresh.

## Status And Inspect Exposure

Compatibility import extends the existing JSON envelope and dotted command naming.

| Command | Required exposure |
| --- | --- |
| `sun compat project --session <session> --json` | Returns `command: "compat.project"`, projection ID, session ID, baseline resolved view ID, tree identity, path policy, strategy, root ref, and retention state |
| `sun compat diff --projection <projection-id> --json` | Returns `command: "compat.diff"`, projection ID, baseline view/tree, candidate counts by classification, selected-safe defaults, warnings, and quarantine refs |
| `sun compat import --projection <projection-id> --session <session> --select ... --json` | Returns `command: "compat.import"`, operation ID, topic revision ID, new session generation, resolved view, tree identity, imported artifact summaries, ignored candidates, and quarantine refs |
| `sun status --projection <projection-id> --json` | Shows projection metadata, dirty candidate summary, quarantine count, last import attempt, retention state, and native errors |
| `sun status --session <session> --json` | Includes recent compatibility projections bound to the session and last import operation, if any |
| `sun inspect projection:<projection-id> --json` | Shows local-only projection metadata, baseline manifest digest, view/tree refs, path policy, strategy, dirty state, quarantine refs, and retention |
| `sun inspect compat-import:<operation-id> --json` | Shows import provenance, selected deltas, ignored/quarantined candidates, preconditions, before/after refs, and created topic revision |
| `sun inspect operation:<operation-id> --json` | For compatibility imports, includes `mutation_payload.kind: "compat_import"` and projection provenance |
| `sun inspect artifact:<artifact-id> --json` | Shows imported provenance on latest operation links when the artifact was changed by compatibility import |

Suggested error codes: `compat_projection_not_found`, `compat_projection_invalid`, `compat_projection_stale`, `compat_projection_integrity_failed`, `compat_diff_failed`, `compat_no_selected_changes`, `compat_path_policy_failed`, `compat_secret_detected`, `compat_cache_blocked`, `compat_precondition_failed`, `compat_conflicted_delta`, `compat_ambiguous_rename`, `compat_policy_failed`, and `compat_partial_write_blocked`.

## Failure Modes

| Failure | Required behavior |
| --- | --- |
| Missing or invalid projection | Return `compat_projection_not_found` or `compat_projection_invalid`; no diff/import records are written. |
| Projection baseline mismatch | Return `compat_projection_stale` with baseline and current refs; do not silently rebase projection edits. |
| Store integrity failure | Quarantine projection/cache entries and return `compat_projection_integrity_failed`; no operation is written. |
| Path policy violation | Return `compat_path_policy_failed` for selected deltas with normalized path, policy reason, and candidate IDs. |
| Secret selected for import | Return `compat_secret_detected`, quarantine local bytes, and write no operation. |
| Cache/build output selected as source | Return `compat_cache_blocked` or `compat_policy_failed` unless explicit policy reclassifies it. |
| No selected changes | Return `compat_no_selected_changes`; no operation/revision IDs. |
| Stale session generation | Return `compat_precondition_failed` with expected/actual generation and unchanged session state. |
| Ambiguous rename | Return `compat_ambiguous_rename` with source and target candidates; require explicit selection or split operation. |
| Conflicted delta | Return `compat_conflicted_delta`; require refresh, adaptation, or manual resolution before import. |
| Partial import failure | Write no operation transaction. Import is atomic across all selected deltas in one request. |

## Acceptance Tests

| Test | Required assertions |
| --- | --- |
| `compat_project_creates_exact_projection` | Projection records purpose `compatibility`, baseline resolved view, tree identity, path policy, strategy, root ref, baseline manifest, and `privacy_class: "local_only"`. |
| `compat_projection_edit_does_not_change_native_state` | Editing files in the projection changes `compat.diff` output but does not advance topic head, session generation, resolved view, or checkpoint state. |
| `compat_diff_classifies_candidates` | Modified, created, deleted, renamed, generated, cache, ignored, and secret-like files are classified with candidate IDs, before/after hashes, and policy results. |
| `compat_import_modified_file_creates_topic_operation` | Selected source modification creates one `operation_transaction`, one topic revision, new session generation, before/after refs, and projection provenance. |
| `compat_import_multiple_files_one_transaction` | Multiple selected safe deltas are imported atomically into one operation transaction owned by one topic/session. |
| `compat_import_new_file_requires_new_precondition` | New source path imports as a write with absent before ref; if the path exists in the current session, import fails with unchanged state. |
| `compat_import_delete_tombstones_path` | Selected deletion creates a delete delta, tombstones the path binding, and preserves artifact provenance. |
| `compat_import_rename_preserves_artifact_id` | Unambiguous rename imports as move and keeps artifact ID/path history. |
| `compat_import_rename_plus_edit_records_both` | Rename plus content change is one transaction with move and patch/write refs when source/target identity is unambiguous. |
| `compat_import_rejects_stale_session_generation` | If the target session moved after diff, import returns `compat_precondition_failed` and writes no records. |
| `compat_import_rejects_stale_projection_baseline` | If projection metadata no longer matches the baseline manifest or tree identity, import fails before diff bytes become durable. |
| `compat_import_rejects_path_escape` | Symlink escape, absolute path, `..`, reserved `.sunlight` path, or normalization collision fails with path policy details. |
| `compat_secret_quarantined_not_imported` | Secret-like selected delta creates quarantine metadata only, returns stable error, and no content blob/operation is reachable as source. |
| `compat_cache_ignored_by_default` | Build/cache/editor output appears as ignored or cache classification and is not imported without explicit policy. |
| `compat_import_atomic_failure_no_partial_write` | Mixed selected deltas with one blocked candidate create no operation or revision and leave session generation unchanged. |
| `status_inspect_show_compat_projection_and_import` | `status --projection`, `status --session`, `inspect projection`, `inspect compat-import`, `inspect operation`, and `inspect artifact` expose matching projection/import/provenance IDs. |
| `checkpoint_requires_imported_source_truth` | Checkpoint/export does not include projection-only edits; after successful import, the resulting topic revision can participate in resolved views and checkpoints. |
| `git_working_tree_not_authoritative_for_import` | Unrelated files in the main Git working tree do not appear in compatibility diff/import for a projection. |

## Implementation Boundaries

- Do not infer topic operations from arbitrary Git working tree diffs in this slice.
- Do not make compatibility projections the primary authoring path for native agents.
- Do not allow imports without an explicit target session and exactly one write topic.
- Do not import secret, cache, local-only, ignored, or path-policy-blocked bytes by default.
- Do not checkpoint or export projection-only edits.
- Do not silently refresh, rebase, or adapt projection edits onto a newer context. A later import against a newer context must create a new authored context and operation.
- Do not store projection roots, watcher journals, raw snapshots, quarantine bytes, or editor state in commit-default records.

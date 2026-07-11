# Sunlight CLI Status and Inspect Contract v0.1

| Field | Value |
| --- | --- |
| Status | Phase 1 response contract |
| Date | July 4, 2026 |
| Scope | CLI JSON envelope, command names, status snapshots, inspect snapshots, projection manifest contract, local-root verification, and acceptance tests |
| Sources | `docs/sunlight_consolidated_architecture_v0_3.md`, `docs/sunlight_implementation_backlog_v0_1.md`, `docs/sunlight_schema_contracts_v0_1.md`, `docs/sunlight_native_io_phase1_spec_v0_1.md`, `docs/sunlight_artifact_io_fixtures_v0_1.md`, `docs/sunlight_operation_transactions_v0_1.md` |

## Purpose

This file locks the Phase 1 JSON response contract for `sun status` and `sun inspect`. It is narrower than the native IO spec: implementers should use this as the snapshot shape for provenance and state queries after `init`, `topic create`, `session start`, `read/list/search`, `patch`, and `write`.

Phase 1 status and inspect must not rely on `git status`, filesystem projections, or inferred working tree changes. They report native Sunlight objects: repository, topic, session, session generation, resolved view, operation transaction, topic revision, and artifact records.

Projection status and inspect are operator diagnostics over a projection record plus optional caller-supplied local root verification. Local root paths are local-only metadata and are never source truth. Content verification requires a persisted projection manifest; scan summaries alone are not proof of correctness.

The v0.1 CLI fixture also accepts `--integrity-fixture store-mismatch` on projection status and inspect for `projection_exec_auth_profile_0001`. This is a narrow operator-visibility fixture for a failed immutable-store integrity check. It reports local-only quarantine and integrity metadata. With `--projection-root`, it also persists a local-only quarantine JSON record under that projection root; without `--projection-root`, it reports only the local URI reference where that record would live. The record is diagnostic metadata, not source truth, not a durable native source record, and not a general persistent quarantine database.

For the v0.1 `basic-app` compatibility projection fixture, projection status reports dirty candidate counts, selected safe defaults, quarantine refs, local-only candidate summary/detail refs, local projection refs, and the fixture-backed `last_import_attempt` for `op_compat_import_auth_0001`. Projection inspect adds a `compatibility_projection` block with baseline manifest refs, path policy, writable/import policy, candidate summary/detail refs, local-only projection refs, `last_import_attempt`, `native_operation_ids: ["op_compat_import_auth_0001"]`, and `native_revision_ids: ["rev_auth_nullability_compat_0001"]`. This fixture represents the current implemented import surface, not a pre-import projection state. A future real pre-import projection should report `last_import_attempt: null` and empty native operation/revision ID arrays until an explicit import succeeds.

## Common JSON Envelope

Every CLI command that accepts `--json` returns one of the two envelope shapes below.

### Success Envelope

```json
{
  "ok": true,
  "data": {
    "command": "status.repository",
    "repository_id": "repo_fixture_basic_app",
    "ids": {},
    "view": null
  },
  "warnings": []
}
```

Required success rules:

- `ok` is always `true`.
- `data.command` is the stable dotted command name.
- `data.repository_id` is present for initialized repositories.
- `data.ids` is present even when empty.
- `data.view` is present and either a view block or `null`.
- `warnings` is present and defaults to `[]`.

Warnings are machine-readable objects:

```json
{
  "code": "refresh_available",
  "message": "a newer allowed session generation may be available",
  "details": {
    "session_id": "session_agent_a"
  }
}
```

### Failure Envelope

```json
{
  "ok": false,
  "error": {
    "code": "not_initialized",
    "message": "Sunlight repository is not initialized",
    "details": {}
  }
}
```

Required failure rules:

- `ok` is always `false`.
- `error.code` is stable and testable.
- `error.message` is short human-readable text.
- `error.details` is present and machine-readable.
- Failure responses do not include `data` or `warnings`.
- Failed mutations include null operation/revision IDs in `details`; non-mutating status/inspect failures omit those fields unless relevant.

Required Phase 1 error codes:

| Code | Use |
| --- | --- |
| `not_initialized` | `.sunlight` is missing or invalid. |
| `invalid_request` | Required flags, selectors, or payload fields are missing or malformed. |
| `ambiguous_selector` | An inspect selector could match more than one object type or object. |
| `object_not_found` | A topic, session, operation, revision, artifact, or path selector has no match. |
| `session_not_found` | Session-specific status or inspect references an unknown session. |
| `topic_not_found` | Topic-specific status or inspect references an unknown topic. |
| `path_not_found` | A path selector is valid but absent from the selected session/view. |
| `path_policy_violation` | Path normalization, reserved path, escape, symlink, case, Unicode, or platform policy rejects the path. |
| `precondition_failed` | Mutation preconditions fail; included here so status/inspect fixtures share one failure vocabulary. |
| `patch_apply_failed` | Patch hunk application fails; included here for operation inspect tests after failed attempts confirm no operation exists. |

## Command Naming

Use stable dotted command names in `data.command`. The left side names the object or service; the right side names the action.

| CLI | `data.command` |
| --- | --- |
| `sun init --json` | `repository.init` |
| `sun topic create ... --fixture basic-app --json` | `topic.create` |
| `sun session start ... --fixture basic-app --json` | `session.start` |
| `sun read ... --json` | `artifact.read` |
| `sun list ... --json` | `artifact.list` |
| `sun search ... --json` | `artifact.search` |
| `sun patch ... --json` | `artifact.patch` |
| `sun write ... --json` | `artifact.write` |
| `sun move ... --fixture basic-app --json` | `artifact.move` |
| `sun delete ... --fixture basic-app --json` | `artifact.delete` |
| `sun metadata set ... --fixture basic-app --json` | `artifact.metadata_set` |
| `sun status --json` | `status.repository` |
| `sun status --session <session> --json` | `status.session` |
| `sun status --topic <topic> --json` | `status.topic` |
| `sun status --projection <projection> --json` | `status.projection` |
| `sun projection quarantine-cleanup --projection <projection> --projection-root <path> --fixture basic-app --json` | `projection.quarantine_cleanup` |
| `sun inspect <path-or-artifact> --session <session> --json` | `inspect.artifact` |
| `sun inspect topic:<topic> --json` | `inspect.topic` |
| `sun inspect session:<session> --json` | `inspect.session` |
| `sun inspect operation:<operation> --json` | `inspect.operation` |
| `sun inspect revision:<revision> --json` | `inspect.revision` |
| `sun inspect projection:<projection> --json` | `inspect.projection` |

Selectors should be explicit when IDs can overlap. Bare path/artifact inspect is allowed only with `--session` or another explicit view selector.

The accepted Phase 1 fixture commands above return stable JSON envelopes for `basic-app` topic/session lifecycle and structural mutation acceptance. Topic and session commands are fixture-backed lifecycle commands.

## Common Blocks

### ID Block

The `ids` block contains selected and newly created object IDs. Unknown or not-applicable IDs are omitted, not set to placeholder strings.

```json
{
  "repository_id": "repo_fixture_basic_app",
  "session_id": "session_agent_a",
  "topic_id": "topic_auth_nullability",
  "topic_revision_id": "rev_auth_nullability_0001",
  "operation_transaction_id": "op_auth_trim_guard_0001",
  "artifact_id": "artifact_src_auth_ts"
}
```

### View Block

Any session-scoped response includes the current exact view block.

```json
{
  "resolved_view_id": "view_agent_a_after_patch_0001",
  "session_generation_id": "gen_agent_a_0002",
  "refresh_policy": "pinned_except_own_topic",
  "topic_frontier": {
    "topic_auth_nullability": "rev_auth_nullability_0001"
  },
  "tree_identity": {
    "kind": "SingleRepoTree",
    "repository_id": "repo_fixture_basic_app",
    "tree_hash": "tree_after_auth_patch_0001"
  }
}
```

Repository-level status uses `view: null` unless a default current view selector is explicitly requested in a later phase.

### Local Root Verification Block

Projection status and inspect may include `local_root_verification` when the caller supplies `--projection-root`. The block verifies only the local filesystem root state and summary counts.

```json
{
  "projection_root": {
    "path": "/tmp/sun-projection-root",
    "privacy": "local_only_path",
    "privacy_class": "local_only"
  },
  "verification_state": "present",
  "content_verification": "not_available_without_persisted_manifest",
  "exists": true,
  "is_dir": true,
  "directories": 3,
  "files": 5,
  "bytes": 222,
  "executable_files": 1,
  "dirty_local": null,
  "sample_paths": ["README.md", "docs/guide.md"],
  "scan_error": null
}
```

Required rules:

- `projection_root.path` is a local-only path and must not be exported or stored in commit-default records.
- `verification_state` is `present` when the root exists and is a directory, `missing` when it does not exist, and `not_directory` when the supplied path is a non-directory.
- `content_verification` remains `not_available_without_persisted_manifest` until projection materialization records comparable manifest metadata for the local root.
- Counts and `sample_paths` are scan summaries for operator visibility, not native provenance and not proof of content correctness.
- Local root scans exclude Sunlight projection metadata under `.sunlight/projections` and projection quarantine metadata under `.sunlight/quarantine`. Other local content, including arbitrary `.sunlight/other` files, remains visible as extra local content.
- Without `--projection-root`, projection status embeds no local root verification details and projection inspect returns `local_root_verification: null`.

### Persisted Projection Manifest Contract

Projection materialization must persist a local-only projection manifest before status or inspect can turn local root verification from scan summaries into content verification. The manifest is derived from native source records at materialization time, not from a later filesystem scan.

For repo-backed projections, the persisted projection snapshot also exposes a
`materialization` object through projection status/inspect and through embedded
execution projection records. Its strategy and metrics describe the completed
staged materialization. `physical_allocation_bytes: null` means unique physical
allocation is not knowable; it must not be replaced with apparent file length.
`cache_hit: false` and `reuse: "created"` explicitly mean no reusable exact-view
cache participated.

```json
{
  "schema_version": 1,
  "record_type": "projection_manifest",
  "id": "projection_manifest_exec_auth_profile_0001",
  "manifest_digest": "sha256:projection_manifest_exec_auth_profile_0001",
  "projection_id": "projection_exec_auth_profile_0001",
  "repository_id": "repo_fixture_basic_app",
  "purpose": "execution",
  "strategy": "copy",
  "resolved_view_id": "view_base_0001",
  "session_generation_id": null,
  "tree_identity": {
    "kind": "SingleRepoTree",
    "repository_id": "repo_fixture_basic_app",
    "tree_hash": "tree_fixture_base_0001"
  },
  "path_policy_id": "path_policy_posix_case_sensitive_v1",
  "operation_semantics_version": "file_ops_v1",
  "materialization_generation": 1,
  "root_ref": {
    "value": "local://.sunlight/projections/execution/projection_exec_auth_profile_0001",
    "privacy": "local_only_path",
    "privacy_class": "local_only"
  },
  "entries": [
    {
      "path": "scripts/build.sh",
      "kind": "file",
      "artifact_id": "artifact_scripts_build_sh",
      "content_hash": "sha256:build_script",
      "byte_length": 48,
      "executable": true,
      "tombstone": false,
      "classification": "source",
      "path_policy_result": "accepted"
    }
  ],
  "summary": {
    "directories": 3,
    "files": 5,
    "bytes": 222,
    "executable_files": 1
  },
  "privacy_class": "local_only",
  "created_at": "2026-07-04T00:00:00Z"
}
```

Required identity inputs:

- `manifest_digest` is computed over the canonical manifest payload, excluding mutable storage location metadata.
- Manifest identity includes `projection_id`, `purpose`, `strategy`, `repository_id`, `resolved_view_id`, optional `session_generation_id`, `tree_identity`, `path_policy_id`, `operation_semantics_version`, and `materialization_generation`.
- The manifest is invalid for any other projection record, resolved view, session generation, tree identity, path policy, strategy, or materialization generation.

Required local root binding:

- Materialization must persist a local-only envelope beside the manifest with manifest identity fields, `manifest.root_ref`, `root_binding.normalized_root_ref`, `root_binding.normalization`, and `root_binding.privacy_class`.
- `root_binding.normalized_root_ref` is the normalized form of the materialized local root used for later status/inspect comparison. For the v0.1 local URI fixture contract, normalization is `local_uri_relative_v1`: path separators are `/`, dot segments are removed, the value is rooted under the projection-local `local://.sunlight/projections/...` namespace, and host absolute paths are not retained.
- The root binding is comparison metadata only. It is excluded from `manifest_digest`, projection manifest identity, exportable records, checkpoints, and commit-default records.
- `root_mismatch` requires a valid persisted manifest envelope whose `root_binding.normalized_root_ref` differs from the current projection `root_ref`. Status/inspect must not synthesize `root_mismatch` from scan summaries or the caller-supplied filesystem path alone.

```json
{
  "manifest": {
    "id": "projection_manifest_exec_auth_profile_0001",
    "manifest_digest": "sha256:projection_manifest_exec_auth_profile_0001",
    "projection_id": "projection_exec_auth_profile_0001",
    "resolved_view_id": "view_base_0001",
    "materialization_generation": 1,
    "root_ref": {
      "value": "local://.sunlight/projections/execution/projection_exec_auth_profile_0001",
      "privacy": "local_only_path",
      "privacy_class": "local_only"
    }
  },
  "root_binding": {
    "normalized_root_ref": {
      "value": "local://.sunlight/projections/execution/projection_exec_auth_profile_0001",
      "privacy": "local_only_path",
      "privacy_class": "local_only"
    },
    "normalization": "local_uri_relative_v1",
    "privacy_class": "local_only"
  },
  "privacy_class": "local_only"
}
```

Required path and executable metadata:

- `entries` are sorted by normalized repository-relative `path` under the projection path policy.
- Each active regular file entry records `path`, `kind: "file"`, `artifact_id`, `content_hash`, `byte_length`, `executable`, `tombstone: false`, `classification`, and `path_policy_result`.
- Tombstones, symlinks, and unsupported path states must be represented explicitly when the projection policy allows them; omitted active files are treated as verification failures.
- General permissions, owner, group, mtime, ctime, inode, and host absolute paths are excluded from manifest identity. The executable bit participates in verification when the platform supports it.

Required content hash rules:

- `content_hash` is the native artifact byte digest expected at the projected path.
- Local verification recomputes each local file byte digest and compares it with the manifest entry.
- Directory counts, byte totals, and `sample_paths` are diagnostics only; a matching summary never substitutes for per-entry hash checks.
- Extra files, missing files, mismatched hashes, mismatched executable bits, unexpected tombstones, path-policy violations, unreadable files, and symlink-policy mismatches make the root dirty or failed according to the status rules below.

Required local-only privacy rules:

- The projection manifest, `root_ref`, local filesystem root, scan details, unreadable path names, and dirty path samples are `local_only` unless a later policy explicitly promotes sanitized metadata.
- Public, commit-default, checkpoint, and export manifests must not contain host absolute paths, `file://` refs, temp paths, user home paths, raw local projection bytes, or projection scan payloads.
- Status and inspect may expose local paths only as `local_only_path` metadata in the operator's local process.

Invalidation rules:

- A manifest is stale when the projection record changes, the projection is refreshed, the input resolved view or session generation moves, the tree identity changes, the path policy changes, the strategy changes in a way that changes materialized semantics, or the materialization generation increments.
- Missing or unreadable manifest metadata sets `content_verification` to `not_available_without_persisted_manifest` or `manifest_unavailable`; it must not be treated as `root_mismatch`.
- A stale, malformed, schema-invalid, digest-mismatched, or identity-mismatched persisted envelope sets `content_verification` to `manifest_invalid`; it must not silently fall back to verified.
- A valid persisted envelope whose normalized root binding does not match the current projection `root_ref` is `root_mismatch` and must not be content-verified against that manifest.
- Verification must fail closed: any unreadable expected file, unsupported metadata check, or path traversal ambiguity prevents `verified`.

When a valid manifest exists, `local_root_verification` extends the scan block with content-verification fields:

```json
{
  "verification_state": "present",
  "content_verification": "verified",
  "manifest_ref": "objects/projection-manifests/sha256/projection_manifest_exec_auth_profile_0001",
  "manifest_digest": "sha256:projection_manifest_exec_auth_profile_0001",
  "dirty_local": false,
  "mismatched_files": 0,
  "missing_files": 0,
  "extra_files": 0,
  "metadata_mismatches": 0,
  "verification_errors": []
}
```

Allowed `content_verification` values:

| Value | Meaning |
| --- | --- |
| `not_available_without_persisted_manifest` | No manifest is recorded for this projection yet. Only scan summaries are available. |
| `verified` | The local root exists, the manifest is valid, every manifest entry matches local content and required metadata, and no disallowed extra paths are present. |
| `dirty` | The manifest is valid, but local content, required metadata, path set, or policy state differs. |
| `manifest_unavailable` | A manifest ref is expected but missing or unreadable. |
| `manifest_invalid` | The persisted manifest envelope, schema, digest, identity inputs, or entry ordering is invalid or stale. |
| `root_mismatch` | A valid persisted envelope records a root binding that does not match the current projection root. |
| `verification_error` | Verification could not complete because of an unreadable file, unsupported required metadata check, traversal ambiguity, or other local IO error. |

### Store Integrity Quarantine Fixture

For the basic-app execution projection, `sun status --projection projection_exec_auth_profile_0001 --fixture basic-app --integrity-fixture store-mismatch --json` reports the projection lifecycle and status as quarantined/failed:

```json
{
  "lifecycle_state": "quarantined",
  "projection_id": "projection_exec_auth_profile_0001",
  "retention_state": "quarantined",
  "integrity_status": "failed",
  "root_ref": {
    "value": "local://.sunlight/projections/execution/projection_exec_auth_profile_0001",
    "privacy": "local_only_path",
    "privacy_class": "local_only"
  },
  "cache_key": "projection-cache:repo_fixture_basic_app:...",
  "quarantine": {
    "privacy_class": "local_only",
    "state": "quarantined",
    "reason": "store_integrity_mismatch",
    "reason_code": "execution_store_integrity_failed",
    "projection_id": "projection_exec_auth_profile_0001",
    "source_truth": "immutable_store_manifest",
    "local_filesystem_source_truth": false,
    "durable_record": "local://.sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json"
  }
}
```

Required fixture rules:

- `store-mismatch` is accepted only for the basic-app execution projection ID.
- Status includes `native_errors[0].code: "execution_store_integrity_failed"` and stable projection, root, cache, manifest, quarantine reason, and provenance refs.
- Inspect includes matching `local_store_integrity` and `local_quarantine` blocks next to the unchanged projection record.
- With `--projection-root`, status or inspect writes local-only quarantine metadata to `.sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json` under the supplied projection root. The record's `durable_record` value is the local URI shown above.
- Without `--projection-root`, `local_root_verification` remains `null`; status and inspect report the same `durable_record` local URI reference but do not write a file. The failure path must not synthesize `content_verification: verified` from local bytes.
- The fixture is local-only diagnostic metadata. It must not add a store scanner, native source record, or broad cache/object garbage collection.

### Projection Quarantine Cleanup

The local-only cleanup command removes persisted projection quarantine metadata for one projection:

```text
sun projection quarantine-cleanup --projection projection_exec_auth_profile_0001 --projection-root <local-root> --fixture basic-app --json
```

The JSON response uses `command: "projection.quarantine_cleanup"` and reports the selected projection, local-only quarantine directory, whether records existed, removed local URI records, removed directories, and the retention state after cleanup. Cleanup is idempotent: when the selected projection has no persisted local quarantine record, it returns success with empty removal lists and `retention_state_after: "absent"`.

Required cleanup rules:

- Cleanup removes only `.sunlight/quarantine/projections/<projection-id>/execution_store_integrity_failed.json` and now-empty directories for the selected projection under the supplied projection root.
- Cleanup preserves sibling projection quarantine directories and arbitrary local content such as `.sunlight/other`.
- Cleanup is local-only maintenance. It does not change native source truth, projection manifests, execution records, caches, object stores, or unrelated retention state.

### Artifact Summary

Status and inspect use the same compact artifact summary as read/list snapshots, with before/after hashes added when the artifact changed in a topic.

```json
{
  "artifact_id": "artifact_src_auth_ts",
  "path": "src/auth.ts",
  "kind": "file",
  "path_state": "active",
  "content_hash": "sha256:auth_trim_guard",
  "before_hash": "sha256:auth_base",
  "after_hash": "sha256:auth_trim_guard",
  "classification": "source",
  "executable": false,
  "tombstone": false
}
```

## Status Contracts

### Repository Status

Command:

```text
sun status --json
```

Snapshot shape:

```json
{
  "ok": true,
  "data": {
    "command": "status.repository",
    "repository_id": "repo_fixture_basic_app",
    "ids": {
      "base_checkpoint_id": "checkpoint_base_0001"
    },
    "view": null,
    "repository": {
      "initialized": true,
      "storage_schema_version": 1,
      "path_policy_id": "path_policy_posix_case_sensitive_v1",
      "operation_semantics_version": "file_ops_v1",
      "git_interop_policy": "default_local_mvp"
    },
    "topics": [
      {
        "topic_id": "topic_auth_nullability",
        "slug": "auth-nullability",
        "status": "open",
        "base_checkpoint_id": "checkpoint_base_0001",
        "head_revision_id": "rev_auth_nullability_0001",
        "revision_count": 1,
        "changed_artifact_count": 1
      }
    ],
    "sessions": [
      {
        "session_id": "session_agent_a",
        "actor_id": "agent_a",
        "write_topic_id": "topic_auth_nullability",
        "session_generation_id": "gen_agent_a_0002",
        "resolved_view_id": "view_agent_a_after_patch_0001",
        "refresh_policy": "pinned_except_own_topic"
      }
    ],
    "native_errors": []
  },
  "warnings": []
}
```

Repository status is an operator snapshot. It should include open topics and active sessions but should not scan or summarize unrelated Git working tree changes.

### Session Status

Command:

```text
sun status --session session_agent_a --json
```

Snapshot shape:

```json
{
  "ok": true,
  "data": {
    "command": "status.session",
    "repository_id": "repo_fixture_basic_app",
    "ids": {
      "session_id": "session_agent_a",
      "write_topic_id": "topic_auth_nullability"
    },
    "view": {
      "resolved_view_id": "view_agent_a_after_patch_0001",
      "session_generation_id": "gen_agent_a_0002",
      "refresh_policy": "pinned_except_own_topic",
      "topic_frontier": {
        "topic_auth_nullability": "rev_auth_nullability_0001"
      },
      "tree_identity": {
        "kind": "SingleRepoTree",
        "repository_id": "repo_fixture_basic_app",
        "tree_hash": "tree_after_auth_patch_0001"
      }
    },
    "session": {
      "actor_id": "agent_a",
      "base_resolved_view_id": "view_base_0001",
      "write_topic_id": "topic_auth_nullability",
      "capabilities": [
        "read",
        "list",
        "search",
        "inspect",
        "patch",
        "write",
        "move",
        "delete",
        "metadata"
      ]
    },
    "topic_head": {
      "topic_id": "topic_auth_nullability",
      "head_revision_id": "rev_auth_nullability_0001",
      "revision_number": 1
    },
    "changed_artifacts": [
      {
        "artifact_id": "artifact_src_auth_ts",
        "path": "src/auth.ts",
        "kind": "file",
        "path_state": "active",
        "before_hash": "sha256:auth_base",
        "after_hash": "sha256:auth_trim_guard",
        "classification": "source",
        "executable": false,
        "tombstone": false
      }
    ],
    "last_operation_id": "op_auth_trim_guard_0001"
  },
  "warnings": []
}
```

Session status must observe read-after-write. Immediately after a successful mutation, it returns the new session generation and write-topic head.

### Topic Status

Command:

```text
sun status --topic auth-nullability --json
```

Snapshot shape:

```json
{
  "ok": true,
  "data": {
    "command": "status.topic",
    "repository_id": "repo_fixture_basic_app",
    "ids": {
      "topic_id": "topic_auth_nullability",
      "head_revision_id": "rev_auth_nullability_0001"
    },
    "view": null,
    "topic": {
      "slug": "auth-nullability",
      "display_name": "Auth nullability",
      "status": "open",
      "owner_actor_id": "agent_a",
      "base_checkpoint_id": "checkpoint_base_0001",
      "revision_count": 1
    },
    "head": {
      "topic_revision_id": "rev_auth_nullability_0001",
      "revision_number": 1,
      "operation_transaction_id": "op_auth_trim_guard_0001",
      "parent_revision_id": null
    },
    "changed_artifacts": [
      {
        "artifact_id": "artifact_src_auth_ts",
        "path": "src/auth.ts",
        "kind": "file",
        "path_state": "active",
        "before_hash": "sha256:auth_base",
        "after_hash": "sha256:auth_trim_guard",
        "classification": "source",
        "executable": false,
        "tombstone": false
      }
    ],
    "sessions": [
      {
        "session_id": "session_agent_a",
        "session_generation_id": "gen_agent_a_0002",
        "resolved_view_id": "view_agent_a_after_patch_0001"
      }
    ]
  },
  "warnings": []
}
```

Topic status is revision-oriented. It should be usable even when there is no active session.

### Projection Status

Command:

```text
sun status --projection projection_exec_auth_profile_0001 --projection-root <local-root> --json
```

Snapshot shape:

```json
{
  "ok": true,
  "data": {
    "command": "status.projection",
    "repository_id": "repo_fixture_basic_app",
    "ids": {
      "projection_id": "projection_exec_auth_profile_0001",
      "resolved_view_id": "view_base_0001"
    },
    "view": {
      "resolved_view_id": "view_base_0001",
      "tree_identity": {
        "kind": "SingleRepoTree",
        "repository_id": "repo_fixture_basic_app",
        "tree_hash": "tree_fixture_base_0001"
      }
    },
    "projection": {
      "lifecycle_state": "materialized",
      "projection_id": "projection_exec_auth_profile_0001",
      "purpose": "execution",
      "strategy": "copy",
      "resolved_view_id": "view_base_0001",
      "tree_identity": {
        "kind": "SingleRepoTree",
        "repository_id": "repo_fixture_basic_app",
        "tree_hash": "tree_fixture_base_0001"
      },
      "retention_state": "active",
      "integrity_status": "not_checked",
      "dirty_local": null,
      "root_ref": {
        "value": "local://.sunlight/projections/execution/projection_exec_auth_profile_0001",
        "privacy": "local_only_path",
        "privacy_class": "local_only"
      },
      "local_root_verification": {
        "verification_state": "present",
        "content_verification": "not_available_without_persisted_manifest",
        "files": 5,
        "bytes": 222
      }
    },
    "native_errors": []
  },
  "warnings": []
}
```

Projection status reports the projection record, lifecycle, strategy, tree refs, and optional local-root verification. It must not infer native changes from projection filesystem contents.

## Inspect Contracts

### Artifact Inspect

Command:

```text
sun inspect src/auth.ts --session session_agent_a --json
```

Snapshot shape:

```json
{
  "ok": true,
  "data": {
    "command": "inspect.artifact",
    "repository_id": "repo_fixture_basic_app",
    "ids": {
      "session_id": "session_agent_a",
      "artifact_id": "artifact_src_auth_ts"
    },
    "view": {
      "resolved_view_id": "view_agent_a_after_patch_0001",
      "session_generation_id": "gen_agent_a_0002",
      "refresh_policy": "pinned_except_own_topic",
      "topic_frontier": {
        "topic_auth_nullability": "rev_auth_nullability_0001"
      },
      "tree_identity": {
        "kind": "SingleRepoTree",
        "repository_id": "repo_fixture_basic_app",
        "tree_hash": "tree_after_auth_patch_0001"
      }
    },
    "artifact": {
      "artifact_id": "artifact_src_auth_ts",
      "artifact_kind": "file",
      "path": "src/auth.ts",
      "path_state": "active",
      "content_hash": "sha256:auth_trim_guard",
      "byte_length": 103,
      "classification": "source",
      "executable": false,
      "created_by_operation_id": "op_import_base_0001"
    },
    "path_history": [
      {
        "path": "src/auth.ts",
        "state": "active",
        "introduced_by_operation_id": "op_import_base_0001"
      }
    ],
    "provenance": {
      "latest_operation_id": "op_auth_trim_guard_0001",
      "topic_id": "topic_auth_nullability",
      "topic_revision_id": "rev_auth_nullability_0001",
      "session_id": "session_agent_a",
      "session_generation_id": "gen_agent_a_0002"
    },
    "before_refs": [
      {
        "operation_transaction_id": "op_auth_trim_guard_0001",
        "content_hash": "sha256:auth_base",
        "tree_hash": "tree_fixture_base_0001"
      }
    ],
    "after_refs": [
      {
        "operation_transaction_id": "op_auth_trim_guard_0001",
        "content_hash": "sha256:auth_trim_guard",
        "tree_hash": "tree_after_auth_patch_0001"
      }
    ]
  },
  "warnings": []
}
```

Inspect by artifact ID may return tombstoned path bindings. Inspect by path returns `path_not_found` after delete unless an explicit `--include-tombstones` option is added later.

### Operation Inspect

Command:

```text
sun inspect operation:op_auth_trim_guard_0001 --json
```

Snapshot shape:

```json
{
  "ok": true,
  "data": {
    "command": "inspect.operation",
    "repository_id": "repo_fixture_basic_app",
    "ids": {
      "operation_transaction_id": "op_auth_trim_guard_0001",
      "topic_id": "topic_auth_nullability",
      "session_id": "session_agent_a",
      "topic_revision_id": "rev_auth_nullability_0001"
    },
    "view": {
      "resolved_view_id": "view_base_0001",
      "session_generation_id": "gen_agent_a_0001",
      "refresh_policy": "pinned_except_own_topic",
      "topic_frontier": {},
      "tree_identity": {
        "kind": "SingleRepoTree",
        "repository_id": "repo_fixture_basic_app",
        "tree_hash": "tree_fixture_base_0001"
      }
    },
    "operation": {
      "mutation": "patch",
      "actor_id": "agent_a",
      "authored_context_id": "ctx_agent_a_gen_0001",
      "session_generation_id": "gen_agent_a_0001",
      "classification": "source",
      "privacy_class": "policy_gated",
      "preconditions": {
        "resolved_view_id": "view_base_0001",
        "session_generation_id": "gen_agent_a_0001",
        "expected_path": "src/auth.ts",
        "expected_hash": "sha256:auth_base"
      },
      "write_set": [
        {
          "artifact_id": "artifact_src_auth_ts",
          "path": "src/auth.ts",
          "mutation": "patch"
        }
      ],
      "before_refs": {
        "content_hash": "sha256:auth_base",
        "tree_hash": "tree_fixture_base_0001"
      },
      "after_refs": {
        "content_hash": "sha256:auth_trim_guard",
        "tree_hash": "tree_after_auth_patch_0001"
      }
    },
    "created_revision": {
      "topic_revision_id": "rev_auth_nullability_0001",
      "revision_number": 1,
      "parent_revision_id": null
    }
  },
  "warnings": []
}
```

The view block for operation inspect describes the authored prior context, not the post-write session generation.

### Topic Inspect

Command:

```text
sun inspect topic:auth-nullability --json
```

Snapshot shape:

```json
{
  "ok": true,
  "data": {
    "command": "inspect.topic",
    "repository_id": "repo_fixture_basic_app",
    "ids": {
      "topic_id": "topic_auth_nullability",
      "head_revision_id": "rev_auth_nullability_0001"
    },
    "view": null,
    "topic": {
      "slug": "auth-nullability",
      "display_name": "Auth nullability",
      "owner_actor_id": "agent_a",
      "base_checkpoint_id": "checkpoint_base_0001",
      "status": "open",
      "visibility": "local"
    },
    "revisions": [
      {
        "topic_revision_id": "rev_auth_nullability_0001",
        "revision_number": 1,
        "parent_revision_id": null,
        "operation_transaction_id": "op_auth_trim_guard_0001",
        "changed_artifacts": [
          {
            "artifact_id": "artifact_src_auth_ts",
            "path": "src/auth.ts",
            "mutation": "patch",
            "before_hash": "sha256:auth_base",
            "after_hash": "sha256:auth_trim_guard"
          }
        ]
      }
    ]
  },
  "warnings": []
}
```

### Session Inspect

Command:

```text
sun inspect session:session_agent_a --json
```

Snapshot shape:

```json
{
  "ok": true,
  "data": {
    "command": "inspect.session",
    "repository_id": "repo_fixture_basic_app",
    "ids": {
      "session_id": "session_agent_a",
      "write_topic_id": "topic_auth_nullability",
      "session_generation_id": "gen_agent_a_0002"
    },
    "view": {
      "resolved_view_id": "view_agent_a_after_patch_0001",
      "session_generation_id": "gen_agent_a_0002",
      "refresh_policy": "pinned_except_own_topic",
      "topic_frontier": {
        "topic_auth_nullability": "rev_auth_nullability_0001"
      },
      "tree_identity": {
        "kind": "SingleRepoTree",
        "repository_id": "repo_fixture_basic_app",
        "tree_hash": "tree_after_auth_patch_0001"
      }
    },
    "session": {
      "actor_id": "agent_a",
      "base_resolved_view_id": "view_base_0001",
      "write_topic_id": "topic_auth_nullability",
      "current_generation_number": 2,
      "created_by": {
        "kind": "session_start",
        "id": "session_agent_a"
      }
    },
    "generations": [
      {
        "session_generation_id": "gen_agent_a_0001",
        "generation_number": 1,
        "resolved_view_id": "view_base_0001",
        "created_by": {
          "kind": "session_start",
          "id": "session_agent_a"
        }
      },
      {
        "session_generation_id": "gen_agent_a_0002",
        "generation_number": 2,
        "resolved_view_id": "view_agent_a_after_patch_0001",
        "created_by": {
          "kind": "operation_transaction",
          "id": "op_auth_trim_guard_0001"
        }
      }
    ]
  },
  "warnings": []
}
```

### Revision Inspect

Command:

```text
sun inspect revision:rev_auth_nullability_0001 --json
```

Snapshot shape:

```json
{
  "ok": true,
  "data": {
    "command": "inspect.revision",
    "repository_id": "repo_fixture_basic_app",
    "ids": {
      "topic_id": "topic_auth_nullability",
      "topic_revision_id": "rev_auth_nullability_0001",
      "operation_transaction_id": "op_auth_trim_guard_0001"
    },
    "view": null,
    "revision": {
      "revision_number": 1,
      "parent_revision_id": null,
      "tree_delta_ref": "delta_auth_trim_guard_0001",
      "dependency_revision_ids": [],
      "privacy_class": "commit_default"
    },
    "operation": {
      "mutation": "patch",
      "session_id": "session_agent_a",
      "session_generation_id": "gen_agent_a_0001",
      "authored_context_id": "ctx_agent_a_gen_0001"
    },
    "changed_artifacts": [
      {
        "artifact_id": "artifact_src_auth_ts",
        "path": "src/auth.ts",
        "mutation": "patch",
        "before_hash": "sha256:auth_base",
        "after_hash": "sha256:auth_trim_guard"
      }
    ]
  },
  "warnings": []
}
```

Revision inspect is the stable selectable-boundary view. It should not require an active session.

### Projection Inspect

Command:

```text
sun inspect projection:projection_exec_auth_profile_0001 --projection-root <local-root> --json
```

Snapshot shape:

```json
{
  "ok": true,
  "data": {
    "command": "inspect.projection",
    "repository_id": "repo_fixture_basic_app",
    "ids": {
      "projection_id": "projection_exec_auth_profile_0001",
      "resolved_view_id": "view_base_0001"
    },
    "view": {
      "resolved_view_id": "view_base_0001",
      "tree_identity": {
        "kind": "SingleRepoTree",
        "repository_id": "repo_fixture_basic_app",
        "tree_hash": "tree_fixture_base_0001"
      }
    },
    "projection": {
      "record_type": "projection",
      "id": "projection_exec_auth_profile_0001",
      "purpose": "execution",
      "strategy": "copy",
      "root_ref": {
        "value": "local://.sunlight/projections/execution/projection_exec_auth_profile_0001",
        "privacy": "local_only_path",
        "privacy_class": "local_only"
      },
      "retention_state": "active",
      "privacy_class": "local_only"
    },
    "local_root_verification": {
      "verification_state": "present",
      "content_verification": "not_available_without_persisted_manifest",
      "files": 5,
      "bytes": 222
    }
  },
  "warnings": []
}
```

Projection inspect is the detailed record view. It may show the local-only root reference and scan summary. Local root contents become content-verified only when a valid persisted projection manifest is compared against the supplied local root.

## Acceptance Tests

Use the `fixture-basic-app` repository and stable labels from the artifact IO and operation transaction fixture specs.

| Fixture | Steps | Required assertions |
| --- | --- | --- |
| `json_envelope_success_shape` | Run `sun status --json` after init. | Response has `ok: true`, `data.command`, `data.repository_id`, `data.ids`, `data.view`, and `warnings: []`. |
| `json_envelope_failure_shape` | Run `sun inspect topic:missing --json`. | Response has `ok: false`, stable error code, message, details, and no `data` or `warnings`. |
| `status_repository_snapshot` | Init, create topic, start session, patch once, run `sun status --json`. | Shows base checkpoint, open topic head, active session, current generation, native errors array, and no Git working tree dependency. |
| `status_session_read_after_write` | Patch `src/auth.ts`, then run `sun status --session session_agent_a --json`. | Shows `gen_agent_a_0002`, `view_agent_a_after_patch_0001`, topic head `rev_auth_nullability_0001`, and changed artifact after hash. |
| `status_topic_without_session` | Create topic and patch, then inspect topic status without passing a session. | Shows topic metadata, head revision, revision count, and changed artifacts. |
| `status_projection_local_root_present` | Materialize the fixture projection, then run projection status with its local root. | Shows `status.projection`, lifecycle `materialized`, `verification_state: present`, local-only root path metadata, count summaries, and content verification only from persisted manifest metadata. |
| `status_projection_manifest_verified` | Materialize the fixture projection with a persisted manifest, then run projection status with its unchanged local root. | Shows manifest ref/digest, `content_verification: verified`, `dirty_local: false`, zero mismatches, and file hashes verified from manifest entries rather than scan summaries. |
| `status_projection_manifest_dirty_content` | Modify one projected file after manifest creation, then run projection status with the same local root. | Shows `content_verification: dirty`, `dirty_local: true`, one mismatched file, and no native topic/session/checkpoint mutation. |
| `status_projection_manifest_dirty_executable` | Toggle executable metadata on `scripts/build.sh` where the platform supports executable bits. | Shows `content_verification: dirty`, one metadata mismatch, and unchanged content hash for the file. |
| `status_projection_manifest_extra_missing` | Add an untracked local file and remove one manifest entry from the projection root. | Shows `content_verification: dirty`, nonzero `extra_files` and `missing_files`, bounded local-only path samples, and unchanged projection refs. |
| `status_projection_manifest_invalidated` | Corrupt the persisted local manifest envelope or make its projection identity stale. | Shows `content_verification: manifest_invalid`; status does not reuse stale content verification or report `root_mismatch`. |
| `status_projection_root_mismatch` | Change only the persisted envelope root binding for a projection that otherwise has a valid manifest. | Shows `content_verification: root_mismatch` and does not compare that root's bytes to the manifest. |
| `status_projection_no_synthetic_root_mismatch` | Supply a different directory without a persisted binding for that directory. | Shows dirty or missing content results from manifest entry comparison, not `root_mismatch`. |
| `status_projection_local_root_missing` | Run projection status with a missing local root. | Shows `verification_state: missing`, zero counts, and unchanged native projection refs. |
| `status_projection_local_root_not_directory` | Run projection status with a file path as the local root. | Shows `verification_state: not_directory`, no file content verification, and no native mutation. |
| `status_projection_store_mismatch_persists_local_quarantine` | Run projection status with `--projection-root`, `--fixture basic-app`, and `--integrity-fixture store-mismatch`. | Shows quarantined lifecycle, failed integrity, local-only `durable_record` URI, native error `execution_store_integrity_failed`, and writes `.sunlight/quarantine/projections/projection_exec_auth_profile_0001/execution_store_integrity_failed.json` under the projection root. |
| `status_projection_store_mismatch_without_root_no_write` | Run projection status with `--fixture basic-app` and `--integrity-fixture store-mismatch` but no projection root. | Shows the same local-only `durable_record` URI and failed integrity metadata, keeps `local_root_verification: null`, and writes no local quarantine file. |
| `status_projection_compat_fixture_import_surface` | Run `sun status --projection projection_compat_agent_a_0001 --fixture basic-app --json`. | Shows candidate counts, selected safe defaults, quarantine refs, local projection refs, and fixture-backed `last_import_attempt` for `op_compat_import_auth_0001`; does not claim this fixture is a pre-import/null projection state. |
| `inspect_artifact_after_patch` | Patch `src/auth.ts`, then inspect it through the same session. | Shows current after hash, path history, latest operation, topic, revision, session, and before/after refs. |
| `inspect_projection_local_root` | Inspect `projection:projection_exec_auth_profile_0001` with a local root. | Shows projection record metadata, `local_only_path` root refs, and the same local-root verification block used by projection status. |
| `inspect_projection_manifest_contract` | Inspect `projection:projection_exec_auth_profile_0001` after manifest creation. | Shows manifest identity inputs, local-only manifest ref/digest, summary counts, and content-verification status without exposing public host paths or raw local bytes. |
| `inspect_projection_store_mismatch_persists_local_quarantine` | Run projection inspect with `--projection-root`, `--fixture basic-app`, and `--integrity-fixture store-mismatch`. | Shows matching `local_store_integrity` and `local_quarantine` blocks and writes the same local-only quarantine JSON record under the projection root. |
| `inspect_projection_compat_fixture_import_surface` | Run `sun inspect projection:projection_compat_agent_a_0001 --fixture basic-app --json`. | Shows `compatibility_projection` with candidate summary/detail refs, local projection refs, `last_import_attempt`, `native_operation_ids` containing `op_compat_import_auth_0001`, and `native_revision_ids` containing `rev_auth_nullability_compat_0001`. |
| `compat_working_tree_not_source_truth_for_status_inspect` | Create unrelated main Git working tree edits, then run compatibility status/inspect for `projection_compat_agent_a_0001`. | Compatibility projection diagnostics remain fixture-backed and do not infer native status, import state, or provenance from main Git working tree changes. |
| `projection_only_edits_do_not_enter_checkpoint_or_export` | Create projection-only local edits without compatibility import, then inspect status/checkpoint/export surfaces. | Projection-only local edits remain local diagnostics and do not enter native topic revisions, checkpoints, or Git export content. |
| `projection_quarantine_cleanup_removes_selected_record` | After creating the store-mismatch local quarantine record, run `sun projection quarantine-cleanup --projection projection_exec_auth_profile_0001 --projection-root <local-root> --fixture basic-app --json`. | Returns `command: "projection.quarantine_cleanup"`, `existed: true`, the removed local URI record, `retention_state_after: "removed"`, and removes only the selected projection quarantine record/directory. |
| `projection_quarantine_cleanup_preserves_siblings_and_other_sunlight` | Create a selected projection quarantine record, a sibling projection quarantine directory, and arbitrary `.sunlight/other` content, then run cleanup for the selected projection. | Removes only the selected projection quarantine metadata and preserves the sibling projection quarantine directory plus `.sunlight/other` content. |
| `projection_quarantine_cleanup_idempotent_absent` | Run quarantine cleanup when the selected projection has no persisted local quarantine record. | Returns success with `existed: false`, empty removed record/dir lists, and `retention_state_after: "absent"`. |
| `inspect_operation_authored_context` | Inspect `operation:op_auth_trim_guard_0001`. | View block is the prior authored context `gen_agent_a_0001`/`view_base_0001`; operation includes preconditions, write set, before refs, after refs, and created revision. |
| `inspect_topic_revision_chain` | Inspect `topic:auth-nullability` after two revisions. | Revisions are ordered by revision number and each links to operation, parent revision, and changed artifacts. |
| `inspect_session_generations` | Inspect `session:session_agent_a` after patch and write. | Generation list includes session start plus each accepted mutation; current view is the latest generation. |
| `inspect_revision_selectable_boundary` | Inspect `revision:rev_auth_nullability_0001`. | Response links revision to operation, topic, tree delta, dependencies, and changed artifacts without requiring active session. |
| `inspect_missing_object_failure` | Inspect a missing operation ID. | Returns `object_not_found` with selector and requested object type. |
| `inspect_ambiguous_selector_failure` | Use a bare selector that could match a topic slug and path without type/session context. | Returns `ambiguous_selector` with candidate object types and guidance to use a typed selector. |
| `status_unknown_session_failure` | Run `sun status --session missing --json`. | Returns `session_not_found`; no native state is mutated. |
| `failed_mutation_not_inspectable` | Attempt stale patch, then inspect the expected operation ID label. | Stale patch returns `precondition_failed`; inspect returns `object_not_found` because no operation was created. |

## Implementation Boundaries

- Do not implement Phase 2 multi-topic resolver composition for this contract.
- Do not read Git working tree status to populate native status.
- Do not infer provenance from filesystem diffs or projections.
- Do not treat projection root scans as content verification until a persisted manifest exists.
- Do not expose local projection roots except as `local_only_path` / `local_only` metadata.
- Do not require executions, checkpoints, Git export, or conflict objects for these snapshots.
- Do not expose raw secret bytes or raw operation payload bytes in status/inspect; use content hashes, refs, and policy classes.
- Keep this contract compatible with future `RepoTreeMap` by preserving the `tree_identity.kind` union shape.

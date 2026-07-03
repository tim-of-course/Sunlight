# Sunlight CLI Status and Inspect Contract v0.1

| Field | Value |
| --- | --- |
| Status | Phase 1 response contract |
| Date | July 3, 2026 |
| Scope | CLI JSON envelope, command names, status snapshots, inspect snapshots, and acceptance tests |
| Sources | `docs/sunlight_consolidated_architecture_v0_3.md`, `docs/sunlight_implementation_backlog_v0_1.md`, `docs/sunlight_schema_contracts_v0_1.md`, `docs/sunlight_native_io_phase1_spec_v0_1.md`, `docs/sunlight_artifact_io_fixtures_v0_1.md`, `docs/sunlight_operation_transactions_v0_1.md` |

## Purpose

This file locks the Phase 1 JSON response contract for `sun status` and `sun inspect`. It is narrower than the native IO spec: implementers should use this as the snapshot shape for provenance and state queries after `init`, `topic create`, `session start`, `read/list/search`, `patch`, and `write`.

Phase 1 status and inspect must not rely on `git status`, filesystem projections, or inferred working tree changes. They report native Sunlight objects: repository, topic, session, session generation, resolved view, operation transaction, topic revision, and artifact records.

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
| `sun topic create ... --json` | `topic.create` |
| `sun session start ... --json` | `session.start` |
| `sun read ... --json` | `artifact.read` |
| `sun list ... --json` | `artifact.list` |
| `sun search ... --json` | `artifact.search` |
| `sun patch ... --json` | `artifact.patch` |
| `sun write ... --json` | `artifact.write` |
| `sun move ... --json` | `artifact.move` |
| `sun delete ... --json` | `artifact.delete` |
| `sun metadata set ... --json` | `artifact.metadata_set` |
| `sun status --json` | `status.repository` |
| `sun status --session <session> --json` | `status.session` |
| `sun status --topic <topic> --json` | `status.topic` |
| `sun inspect <path-or-artifact> --session <session> --json` | `inspect.artifact` |
| `sun inspect topic:<topic> --json` | `inspect.topic` |
| `sun inspect session:<session> --json` | `inspect.session` |
| `sun inspect operation:<operation> --json` | `inspect.operation` |
| `sun inspect revision:<revision> --json` | `inspect.revision` |

Selectors should be explicit when IDs can overlap. Bare path/artifact inspect is allowed only with `--session` or another explicit view selector.

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

## Acceptance Tests

Use the `fixture-basic-app` repository and stable labels from the artifact IO and operation transaction fixture specs.

| Fixture | Steps | Required assertions |
| --- | --- | --- |
| `json_envelope_success_shape` | Run `sun status --json` after init. | Response has `ok: true`, `data.command`, `data.repository_id`, `data.ids`, `data.view`, and `warnings: []`. |
| `json_envelope_failure_shape` | Run `sun inspect topic:missing --json`. | Response has `ok: false`, stable error code, message, details, and no `data` or `warnings`. |
| `status_repository_snapshot` | Init, create topic, start session, patch once, run `sun status --json`. | Shows base checkpoint, open topic head, active session, current generation, native errors array, and no Git working tree dependency. |
| `status_session_read_after_write` | Patch `src/auth.ts`, then run `sun status --session session_agent_a --json`. | Shows `gen_agent_a_0002`, `view_agent_a_after_patch_0001`, topic head `rev_auth_nullability_0001`, and changed artifact after hash. |
| `status_topic_without_session` | Create topic and patch, then inspect topic status without passing a session. | Shows topic metadata, head revision, revision count, and changed artifacts. |
| `inspect_artifact_after_patch` | Patch `src/auth.ts`, then inspect it through the same session. | Shows current after hash, path history, latest operation, topic, revision, session, and before/after refs. |
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
- Do not require executions, checkpoints, Git export, or conflict objects for these snapshots.
- Do not expose raw secret bytes or raw operation payload bytes in status/inspect; use content hashes, refs, and policy classes.
- Keep this contract compatible with future `RepoTreeMap` by preserving the `tree_identity.kind` union shape.

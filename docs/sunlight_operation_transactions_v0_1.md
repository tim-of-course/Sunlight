# Sunlight Operation Transactions v0.1

| Field | Value |
| --- | --- |
| Status | Phase 1 mutation contract |
| Date | July 3, 2026 |
| Scope | Operation transactions for `sun patch` and `sun write`, preconditions, before/after refs, topic revision boundaries, read-after-write behavior, and JSON fixtures |
| Sources | `docs/sunlight_consolidated_architecture_v0_3.md`, `docs/sunlight_implementation_backlog_v0_1.md`, `docs/sunlight_schema_contracts_v0_1.md`, `docs/sunlight_native_io_phase1_spec_v0_1.md`, `docs/sunlight_artifact_io_fixtures_v0_1.md` |

## Purpose

This document locks the next Phase 1 mutation slice: accepting patch and whole-file write requests through native artifact IO and recording them as topic-owned operation transactions. It is intentionally practical. Implementation agents should be able to create records and JSON snapshot tests from this file without reinterpreting the architecture.

This slice covers only single-session, own-topic mutation. Phase 2 composition, conflict resolution across multiple topics, execution-output promotion, move/delete, and metadata-only operations are out of scope except where fields need forward-compatible shapes.

## Required Invariants

- Every accepted mutation creates exactly one `operation_transaction` and one new `topic_revision`.
- Every operation transaction has exactly one `topic_id`, the current `session_id`, the prior `session_generation_id`, and an `authored_context_id`.
- Preconditions are evaluated before any blob, operation, revision, view, or generation record is made visible.
- Failed mutations do not create operation or revision records and do not advance the session generation.
- Accepted mutations advance only the session's write topic. Other pinned topic revisions do not move during the write response.
- The response returns the new `operation_transaction_id`, `topic_revision_id`, `session_generation_id`, `resolved_view_id`, `tree_identity`, and affected artifact refs before the command returns.

## Operation Record

The Phase 1 `operation_transaction` record uses the v1 schema contract shape below. Stable labels may be used in fixtures until canonical hashing is fully wired.

```json
{
  "schema_version": 1,
  "record_type": "operation_transaction",
  "id": "op_auth_trim_guard_0001",
  "repository_id": "repo_fixture_basic_app",
  "topic_id": "topic_auth_nullability",
  "session_id": "session_agent_a",
  "session_generation_id": "gen_agent_a_0001",
  "actor_id": "agent_a",
  "authored_context_id": "ctx_agent_a_gen_0001",
  "preconditions": {},
  "read_set": {},
  "write_set": [],
  "mutation_payload": {},
  "before_refs": {},
  "after_refs": {},
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

Required field meanings:

| Field | Requirement |
| --- | --- |
| `id` | Stable operation ID. Before canonical hashing lands, fixtures may use deterministic labels. |
| `topic_id` | The session write topic. Never inferred from path or branch state. |
| `session_generation_id` | The generation against which preconditions were evaluated, not the generation returned after success. |
| `authored_context_id` | Context record for the prior resolved view, topic frontier, path policy, operation semantics version, and actor/session hints. |
| `preconditions` | Machine-readable validation inputs. See below. |
| `read_set` | Phase 1 uses full authored context; optional file reads may be added when already available. |
| `write_set` | One entry per path/artifact changed by this transaction. Patch/write fixtures use one entry. |
| `mutation_payload` | Patch or whole-file write payload metadata and content refs. |
| `before_refs` | Exact artifact/path/content state before the mutation. |
| `after_refs` | Exact artifact/path/content state after the mutation. |
| `logical_time` | Parent topic revision and next revision number used to create the new topic revision. |
| `parents` | Parent topic revision IDs. Empty for the first operation in a topic. |

## Preconditions

Preconditions are part of operation identity. If a patch is re-authored against a newer context, it becomes a new operation rather than mutating the old one.

### Common Preconditions

```json
{
  "resolved_view_id": "view_base_0001",
  "session_generation_id": "gen_agent_a_0001",
  "write_topic_id": "topic_auth_nullability",
  "parent_topic_revision_id": null,
  "path_policy_id": "path_policy_posix_case_sensitive_v1",
  "operation_semantics_version": "file_ops_v1",
  "expected_path": "src/auth.ts",
  "expected_path_state": "active"
}
```

Validation order:

1. Session exists and is bound to the requested write topic.
2. Path policy accepts the input path and normalized path.
3. The current session generation matches `session_generation_id`.
4. The current session generation resolves to `resolved_view_id`.
5. The current write topic head matches `parent_topic_revision_id`.
6. The normalized path binding matches `expected_path` and `expected_path_state`.
7. Content and artifact-specific preconditions match.
8. The mutation payload applies or validates against the exact before bytes.

### Existing Artifact Preconditions

Use for patch and replacement writes.

```json
{
  "target": {
    "kind": "existing_artifact",
    "artifact_id": "artifact_src_auth_ts",
    "path": "src/auth.ts",
    "content_hash": "sha256:auth_base",
    "executable": false,
    "classification": "source"
  }
}
```

### New Path Preconditions

Use for whole-file creates.

```json
{
  "target": {
    "kind": "new_path",
    "path": "src/session.ts",
    "expected_hash": "new",
    "expected_absent_in_view": true
  }
}
```

`expected_hash: "new"` is valid only with `kind: "new_path"`. If an active path already exists, return `precondition_failed`.

## Patch Payload

Patch operations store the supplied patch as an immutable payload and store the computed after content as a content blob.

```json
{
  "kind": "patch",
  "format": "unified_diff",
  "patch_ref": "objects/patches/sha256/auth_trim_guard_patch",
  "patch_digest": "sha256:auth_trim_guard_patch",
  "base_content_hash": "sha256:auth_base",
  "result_content_hash": "sha256:auth_trim_guard",
  "text_encoding": "utf-8",
  "line_ending_policy": "preserve_existing",
  "hunk_count": 1,
  "byte_delta": 25
}
```

Patch semantics:

- Apply to exact `base_content_hash` bytes after preconditions pass.
- Reject binary content unless an explicit future binary patch format is requested.
- Preserve executable bit and classification unless the request includes a valid metadata payload in a later slice.
- Store failed patch attempts only in local diagnostic logs, not as operation transactions.

## Whole-File Write Payload

Whole-file writes are for new files, generated files, or intentionally broad replacements. Phase 1 should still prefer patch fixtures for small source edits.

```json
{
  "kind": "write",
  "write_mode": "create",
  "content_ref": "objects/blobs/sha256/session_new",
  "content_hash": "sha256:session_new",
  "byte_length": 72,
  "media_type": "text/typescript; charset=utf-8",
  "text_encoding": "utf-8",
  "executable": false,
  "classification": "source"
}
```

Allowed `write_mode` values:

| Mode | Before ref |
| --- | --- |
| `create` | No active path or artifact at the target path; `before_refs.content` is `null`. |
| `replace` | Active artifact exists and expected content hash matches; artifact identity is preserved. |

Phase 1 may implement `create` first, but the record shape must not block replacement writes.

## Before And After Refs

Refs are explicit because operation application and provenance should not depend on reconstructing state from path strings.

### Patch Refs

```json
{
  "before_refs": {
    "artifacts": [
      {
        "artifact_id": "artifact_src_auth_ts",
        "path": "src/auth.ts",
        "path_state": "active",
        "content_hash": "sha256:auth_base",
        "executable": false,
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
        "content_hash": "sha256:auth_trim_guard",
        "executable": false,
        "classification": "source"
      }
    ],
    "tree_identity": {
      "kind": "SingleRepoTree",
      "repository_id": "repo_fixture_basic_app",
      "tree_hash": "tree_after_auth_patch_0001"
    }
  }
}
```

### New File Write Refs

```json
{
  "before_refs": {
    "artifacts": [
      {
        "artifact_id": null,
        "path": "src/session.ts",
        "path_state": "absent",
        "content_hash": null
      }
    ],
    "tree_identity": {
      "kind": "SingleRepoTree",
      "repository_id": "repo_fixture_basic_app",
      "tree_hash": "tree_after_auth_patch_0001"
    }
  },
  "after_refs": {
    "artifacts": [
      {
        "artifact_id": "artifact_src_session_ts",
        "path": "src/session.ts",
        "path_state": "active",
        "content_hash": "sha256:session_new",
        "executable": false,
        "classification": "source"
      }
    ],
    "tree_identity": {
      "kind": "SingleRepoTree",
      "repository_id": "repo_fixture_basic_app",
      "tree_hash": "tree_after_session_write_0002"
    }
  }
}
```

## Topic Revision Boundary

The operation transaction is authored against the old session generation. The new topic revision is the selectable boundary created by applying that operation.

```json
{
  "schema_version": 1,
  "record_type": "topic_revision",
  "id": "rev_auth_nullability_0001",
  "repository_id": "repo_fixture_basic_app",
  "topic_id": "topic_auth_nullability",
  "revision_number": 1,
  "parent_revision_id": null,
  "operation_transaction_id": "op_auth_trim_guard_0001",
  "tree_delta_ref": "delta_auth_trim_guard_0001",
  "dependency_revision_ids": [],
  "privacy_class": "commit_default",
  "created_at": "2026-07-03T00:00:00Z"
}
```

After the revision is created, the session generation advances from the prior resolved view to a new resolved view that includes the new topic head.

```json
{
  "schema_version": 1,
  "record_type": "session_generation",
  "id": "gen_agent_a_0002",
  "repository_id": "repo_fixture_basic_app",
  "session_id": "session_agent_a",
  "write_topic_id": "topic_auth_nullability",
  "base_resolved_view_id": "view_base_0001",
  "resolved_view_id": "view_agent_a_after_patch_0001",
  "topic_frontier": {
    "topic_auth_nullability": "rev_auth_nullability_0001"
  },
  "generation_number": 2,
  "refresh_policy": "pinned_except_own_topic",
  "created_by": {
    "kind": "operation_transaction",
    "id": "op_auth_trim_guard_0001"
  },
  "privacy_class": "local_only",
  "created_at": "2026-07-03T00:00:00Z"
}
```

## Read-After-Write Behavior

After an accepted mutation:

| Next command | Required result |
| --- | --- |
| `sun read` same path | Returns after bytes, after hash, new `resolved_view_id`, and new `session_generation_id`. |
| `sun list` containing path | Shows changed or created path with after hash. |
| `sun search` for added text | Searches the new session generation and can find new content. |
| `sun status --session` | Shows the new generation and topic head. |
| `sun inspect <operation-id>` | Shows operation, preconditions, before/after refs, topic, session, and revision. |

If a mutation fails, all of the same commands continue to observe the prior generation.

## JSON Response Snapshots

Snapshots use the standard success and failure envelopes from the Phase 1 native IO spec.

### Patch Success

Command:

```text
sun patch src/auth.ts --session session_agent_a --expect-hash sha256:auth_base --patch-file patches/auth_trim_guard.diff --json
```

Expected snapshot:

```json
{
  "ok": true,
  "data": {
    "command": "artifact.patch",
    "repository_id": "repo_fixture_basic_app",
    "ids": {
      "session_id": "session_agent_a",
      "operation_transaction_id": "op_auth_trim_guard_0001",
      "topic_revision_id": "rev_auth_nullability_0001"
    },
    "view": {
      "resolved_view_id": "view_agent_a_after_patch_0001",
      "session_generation_id": "gen_agent_a_0002",
      "tree_identity": {
        "kind": "SingleRepoTree",
        "repository_id": "repo_fixture_basic_app",
        "tree_hash": "tree_after_auth_patch_0001"
      }
    },
    "artifacts": [
      {
        "artifact_id": "artifact_src_auth_ts",
        "path": "src/auth.ts",
        "kind": "file",
        "before_hash": "sha256:auth_base",
        "after_hash": "sha256:auth_trim_guard",
        "classification": "source",
        "executable": false
      }
    ],
    "operation": {
      "topic_id": "topic_auth_nullability",
      "mutation": "patch",
      "preconditions": {
        "resolved_view_id": "view_base_0001",
        "session_generation_id": "gen_agent_a_0001",
        "expected_path": "src/auth.ts",
        "expected_hash": "sha256:auth_base"
      },
      "before_refs": {
        "content_hash": "sha256:auth_base",
        "tree_hash": "tree_fixture_base_0001"
      },
      "after_refs": {
        "content_hash": "sha256:auth_trim_guard",
        "tree_hash": "tree_after_auth_patch_0001"
      },
      "write_set": [
        {
          "artifact_id": "artifact_src_auth_ts",
          "path": "src/auth.ts",
          "mutation": "patch"
        }
      ]
    }
  },
  "warnings": []
}
```

### Write New File Success

Command:

```text
sun write src/session.ts --session session_agent_a --expect-hash new --content-file files/session.ts --classification source --json
```

Expected snapshot:

```json
{
  "ok": true,
  "data": {
    "command": "artifact.write",
    "repository_id": "repo_fixture_basic_app",
    "ids": {
      "session_id": "session_agent_a",
      "operation_transaction_id": "op_write_session_ts_0001",
      "topic_revision_id": "rev_auth_nullability_0002"
    },
    "view": {
      "resolved_view_id": "view_agent_a_after_write_0002",
      "session_generation_id": "gen_agent_a_0003",
      "tree_identity": {
        "kind": "SingleRepoTree",
        "repository_id": "repo_fixture_basic_app",
        "tree_hash": "tree_after_session_write_0002"
      }
    },
    "artifacts": [
      {
        "artifact_id": "artifact_src_session_ts",
        "path": "src/session.ts",
        "kind": "file",
        "before_hash": null,
        "after_hash": "sha256:session_new",
        "classification": "source",
        "executable": false
      }
    ],
    "operation": {
      "topic_id": "topic_auth_nullability",
      "mutation": "write",
      "preconditions": {
        "resolved_view_id": "view_agent_a_after_patch_0001",
        "session_generation_id": "gen_agent_a_0002",
        "expected_path": "src/session.ts",
        "expected_hash": "new"
      },
      "before_refs": {
        "content_hash": null,
        "tree_hash": "tree_after_auth_patch_0001"
      },
      "after_refs": {
        "content_hash": "sha256:session_new",
        "tree_hash": "tree_after_session_write_0002"
      },
      "write_set": [
        {
          "artifact_id": "artifact_src_session_ts",
          "path": "src/session.ts",
          "mutation": "write"
        }
      ]
    }
  },
  "warnings": []
}
```

### Precondition Failure

Command:

```text
sun patch src/auth.ts --session session_agent_a --expect-hash sha256:stale --patch-file patches/auth_trim_guard.diff --json
```

Expected snapshot:

```json
{
  "ok": false,
  "error": {
    "code": "precondition_failed",
    "message": "mutation precondition failed",
    "details": {
      "failed_precondition": "expected_hash",
      "path": "src/auth.ts",
      "artifact_id": "artifact_src_auth_ts",
      "expected": "sha256:stale",
      "actual": "sha256:auth_base",
      "session_generation_id": "gen_agent_a_0001",
      "resolved_view_id": "view_base_0001",
      "operation_transaction_id": null,
      "topic_revision_id": null
    }
  }
}
```

### Patch Apply Failure

Command:

```text
sun patch src/auth.ts --session session_agent_a --expect-hash sha256:auth_base --patch-file patches/auth_bad_hunk.diff --json
```

Expected snapshot:

```json
{
  "ok": false,
  "error": {
    "code": "patch_apply_failed",
    "message": "patch did not apply to expected content",
    "details": {
      "path": "src/auth.ts",
      "artifact_id": "artifact_src_auth_ts",
      "content_hash": "sha256:auth_base",
      "failed_hunk": 1,
      "session_generation_id": "gen_agent_a_0001",
      "resolved_view_id": "view_base_0001",
      "operation_transaction_id": null,
      "topic_revision_id": null
    }
  }
}
```

## Required Negative Fixtures

| Fixture | Command shape | Expected result |
| --- | --- | --- |
| `patch_stale_hash_no_write` | Patch `src/auth.ts` with stale `--expect-hash`. | `precondition_failed`; generation unchanged; no operation/revision IDs. |
| `patch_bad_hunk_no_write` | Patch with correct hash but invalid hunk context. | `patch_apply_failed`; generation unchanged; no operation/revision IDs. |
| `patch_wrong_path_binding_no_write` | Patch by artifact ID with an expected path no longer bound to it. | `precondition_failed` on `expected_path`; no write. |
| `patch_unknown_session_no_write` | Patch with missing session ID. | `session_not_found`; no write. |
| `patch_reserved_path_no_write` | Patch `.sunlight/config.toml`. | `path_policy_violation` with `reserved_path`; no write. |
| `write_existing_with_new_precondition_no_write` | Write `src/auth.ts` with `--expect-hash new`. | `precondition_failed`; actual hash returned. |
| `write_new_reserved_path_no_write` | Write `.sunlight/records/x.json`. | `path_policy_violation`; no write. |
| `write_secret_quarantine_no_source_write` | Write secret-like content to `src/session.ts`. | Policy error or quarantine marker only; no readable source artifact in the session. |
| `write_missing_classification_no_write` | Write new file without `--classification`. | `invalid_request`; no write. |
| `failed_write_then_read_prior_generation` | Failed write followed by read/list/search. | All observe the prior generation. |

## Fixture Acceptance Checklist

- Operation record contains topic, session, prior generation, authored context, preconditions, payload, before refs, after refs, read set, and write set.
- New topic revision points to the operation and previous topic head.
- New session generation points to the new resolved view and new write-topic frontier.
- Patch success changes the existing artifact content hash and preserves artifact ID.
- New file write creates a new artifact ID and active path binding.
- Failure snapshots include unchanged generation and null operation/revision IDs.
- Read/list/search after success use the new generation.
- Read/list/search after failure use the prior generation.

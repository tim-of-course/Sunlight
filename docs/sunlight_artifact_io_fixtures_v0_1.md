# Sunlight Artifact IO Fixtures v0.1

> **Historical design record.** Secret detection, secret classification gates,
> and automatic source quarantine described here are superseded. See the
> repository README, `docs/local_mcp.md`, portable Agent Skill, and
> `docs/open_alpha_acceptance.md` for the current explicit-ignore contract.

| Field | Value |
| --- | --- |
| Status | Fixture and implementation plan for Phase 1 artifact IO |
| Date | July 3, 2026 |
| Scope | Read, list, search fixtures first; patch/write fixtures staged next |
| Sources | `docs/sunlight_consolidated_architecture_v0_3.md`, `docs/sunlight_implementation_backlog_v0_1.md`, `docs/sunlight_schema_contracts_v0_1.md`, `docs/sunlight_native_io_phase1_spec_v0_1.md` |

## Purpose

This file defines the practical fixture set for the first native artifact IO slice. It is intentionally record- and snapshot-oriented so implementation agents can build deterministic tests without re-reading the architecture.

Phase 1 starts with imported baseline content plus `read`, `list`, and `search` over a pinned authoring session. Patch and write fixtures are included as the next increment because their preconditions and read-after-write behavior shape the read/list/search data model.

## Fixture Repository

Use one tiny Git repository named `fixture-basic-app`.

```text
README.md
src/auth.ts
src/profile.ts
docs/guide.md
scripts/build.sh
```

Baseline file bytes:

```text
README.md: "# Fixture Basic App\n\nUses User.email for login.\n"
src/auth.ts: "export function login(email: string) {\n  return email.trim().toLowerCase();\n}\n"
src/profile.ts: "export const profileLabel = \"User.email\";\n"
docs/guide.md: "Search token: User.email\n"
scripts/build.sh: "#!/usr/bin/env sh\necho build\n"
```

Baseline assumptions:

- Repository ID: `repo_fixture_basic_app`.
- Base checkpoint ID: `checkpoint_base_0001`.
- Base resolved view ID: `view_base_0001`.
- Base session generation ID after `session start`: `gen_agent_a_0001`.
- Path policy ID: `path_policy_posix_case_sensitive_v1`.
- Tree identity: `SingleRepoTree { repository_id: "repo_fixture_basic_app", tree_hash: "tree_fixture_base_0001" }`.
- Fixture hashes may start as stable labels such as `sha256:auth_base`; replace them with canonical hashes when hashing helpers land.

## Core Fixture Records

These are minimal v1 records used by read/list/search snapshots. They should live in test fixtures as canonical JSON records or canonicalized JSON values.

### Artifact Records

```json
[
  {
    "schema_version": 1,
    "record_type": "artifact",
    "id": "artifact_readme_md",
    "repository_id": "repo_fixture_basic_app",
    "artifact_kind": "file",
    "path_bindings": [
      {
        "path": "README.md",
        "state": "active",
        "introduced_by_operation_id": "op_import_base_0001"
      }
    ],
    "current_content_ref": "sha256:readme_base",
    "metadata": {
      "executable": false,
      "language": "markdown"
    },
    "classification": "source",
    "created_by_operation_id": "op_import_base_0001",
    "privacy_class": "commit_default",
    "created_at": "2026-07-03T00:00:00Z"
  },
  {
    "schema_version": 1,
    "record_type": "artifact",
    "id": "artifact_src_auth_ts",
    "repository_id": "repo_fixture_basic_app",
    "artifact_kind": "file",
    "path_bindings": [
      {
        "path": "src/auth.ts",
        "state": "active",
        "introduced_by_operation_id": "op_import_base_0001"
      }
    ],
    "current_content_ref": "sha256:auth_base",
    "metadata": {
      "executable": false,
      "language": "typescript"
    },
    "classification": "source",
    "created_by_operation_id": "op_import_base_0001",
    "privacy_class": "commit_default",
    "created_at": "2026-07-03T00:00:00Z"
  },
  {
    "schema_version": 1,
    "record_type": "artifact",
    "id": "artifact_scripts_build_sh",
    "repository_id": "repo_fixture_basic_app",
    "artifact_kind": "file",
    "path_bindings": [
      {
        "path": "scripts/build.sh",
        "state": "active",
        "introduced_by_operation_id": "op_import_base_0001"
      }
    ],
    "current_content_ref": "sha256:build_base",
    "metadata": {
      "executable": true,
      "language": "shell"
    },
    "classification": "source",
    "created_by_operation_id": "op_import_base_0001",
    "privacy_class": "commit_default",
    "created_at": "2026-07-03T00:00:00Z"
  }
]
```

Add equivalent artifact records for `artifact_src_profile_ts` and `artifact_docs_guide_md` with active path bindings and source classification.

### Content Blob Records

```json
[
  {
    "schema_version": 1,
    "record_type": "content_blob",
    "id": "blob_readme_base",
    "repository_id": "repo_fixture_basic_app",
    "digest": "sha256:readme_base",
    "byte_length": 48,
    "media_type": "text/markdown; charset=utf-8",
    "classification": "source",
    "storage_ref": "objects/blobs/sha256/readme_base",
    "privacy_class": "policy_gated",
    "created_at": "2026-07-03T00:00:00Z"
  },
  {
    "schema_version": 1,
    "record_type": "content_blob",
    "id": "blob_auth_base",
    "repository_id": "repo_fixture_basic_app",
    "digest": "sha256:auth_base",
    "byte_length": 78,
    "media_type": "text/typescript; charset=utf-8",
    "classification": "source",
    "storage_ref": "objects/blobs/sha256/auth_base",
    "privacy_class": "policy_gated",
    "created_at": "2026-07-03T00:00:00Z"
  }
]
```

The byte lengths are snapshot expectations for the listed fixture bytes. If fixture bytes change, update byte lengths in the same commit.

### Content Tree Record

```json
{
  "schema_version": 1,
  "record_type": "content_tree",
  "id": "tree_fixture_base_0001",
  "repository_id": "repo_fixture_basic_app",
  "tree_hash": "tree_fixture_base_0001",
  "path_policy_id": "path_policy_posix_case_sensitive_v1",
  "entries": [
    {
      "path": "README.md",
      "artifact_id": "artifact_readme_md",
      "content_ref": "sha256:readme_base",
      "kind": "file",
      "executable": false,
      "tombstone": false
    },
    {
      "path": "docs/guide.md",
      "artifact_id": "artifact_docs_guide_md",
      "content_ref": "sha256:guide_base",
      "kind": "file",
      "executable": false,
      "tombstone": false
    },
    {
      "path": "scripts/build.sh",
      "artifact_id": "artifact_scripts_build_sh",
      "content_ref": "sha256:build_base",
      "kind": "file",
      "executable": true,
      "tombstone": false
    },
    {
      "path": "src/auth.ts",
      "artifact_id": "artifact_src_auth_ts",
      "content_ref": "sha256:auth_base",
      "kind": "file",
      "executable": false,
      "tombstone": false
    },
    {
      "path": "src/profile.ts",
      "artifact_id": "artifact_src_profile_ts",
      "content_ref": "sha256:profile_base",
      "kind": "file",
      "executable": false,
      "tombstone": false
    }
  ],
  "privacy_class": "policy_gated",
  "created_at": "2026-07-03T00:00:00Z"
}
```

Tree entries are sorted by normalized repository-relative path. The executable bit participates in tree identity; general file permissions do not.

## Command JSON Snapshots

Snapshots use stable labels until canonical hashing is wired. Keep the response envelope exact.

### `sun read`

Command:

```text
sun read src/auth.ts --session session_agent_a --json
```

Expected snapshot:

```json
{
  "ok": true,
  "data": {
    "command": "artifact.read",
    "repository_id": "repo_fixture_basic_app",
    "ids": {
      "session_id": "session_agent_a"
    },
    "view": {
      "resolved_view_id": "view_base_0001",
      "session_generation_id": "gen_agent_a_0001",
      "tree_identity": {
        "kind": "SingleRepoTree",
        "repository_id": "repo_fixture_basic_app",
        "tree_hash": "tree_fixture_base_0001"
      }
    },
    "artifacts": [
      {
        "artifact_id": "artifact_src_auth_ts",
        "path": "src/auth.ts",
        "kind": "file",
        "content_hash": "sha256:auth_base",
        "byte_length": 78,
        "classification": "source",
        "executable": false
      }
    ],
    "content": {
      "encoding": "utf-8",
      "bytes": "export function login(email: string) {\n  return email.trim().toLowerCase();\n}\n"
    }
  },
  "warnings": []
}
```

Binary or large content may return `content.ref` instead of inline `content.bytes`, but the first text fixtures should be inline for readability.

### `sun list`

Command:

```text
sun list src --session session_agent_a --json
```

Expected snapshot:

```json
{
  "ok": true,
  "data": {
    "command": "artifact.list",
    "repository_id": "repo_fixture_basic_app",
    "ids": {
      "session_id": "session_agent_a"
    },
    "view": {
      "resolved_view_id": "view_base_0001",
      "session_generation_id": "gen_agent_a_0001",
      "tree_identity": {
        "kind": "SingleRepoTree",
        "repository_id": "repo_fixture_basic_app",
        "tree_hash": "tree_fixture_base_0001"
      }
    },
    "artifacts": [
      {
        "artifact_id": "artifact_src_auth_ts",
        "path": "src/auth.ts",
        "kind": "file",
        "content_hash": "sha256:auth_base",
        "byte_length": 78,
        "classification": "source",
        "executable": false,
        "tombstone": false
      },
      {
        "artifact_id": "artifact_src_profile_ts",
        "path": "src/profile.ts",
        "kind": "file",
        "content_hash": "sha256:profile_base",
        "byte_length": 42,
        "classification": "source",
        "executable": false,
        "tombstone": false
      }
    ]
  },
  "warnings": []
}
```

Root listing uses the same entry shape and includes directory summaries only if the implementation has explicit directory artifacts. Do not invent directory IDs solely for listing.

### `sun search`

Command:

```text
sun search User.email --session session_agent_a --json
```

Expected snapshot:

```json
{
  "ok": true,
  "data": {
    "command": "artifact.search",
    "repository_id": "repo_fixture_basic_app",
    "ids": {
      "session_id": "session_agent_a"
    },
    "view": {
      "resolved_view_id": "view_base_0001",
      "session_generation_id": "gen_agent_a_0001",
      "tree_identity": {
        "kind": "SingleRepoTree",
        "repository_id": "repo_fixture_basic_app",
        "tree_hash": "tree_fixture_base_0001"
      }
    },
    "matches": [
      {
        "artifact_id": "artifact_readme_md",
        "path": "README.md",
        "content_hash": "sha256:readme_base",
        "line": 3,
        "snippet": "Uses User.email for login."
      },
      {
        "artifact_id": "artifact_docs_guide_md",
        "path": "docs/guide.md",
        "content_hash": "sha256:guide_base",
        "line": 1,
        "snippet": "Search token: User.email"
      },
      {
        "artifact_id": "artifact_src_profile_ts",
        "path": "src/profile.ts",
        "content_hash": "sha256:profile_base",
        "line": 1,
        "snippet": "export const profileLabel = \"User.email\";"
      }
    ]
  },
  "warnings": []
}
```

Search ordering is by normalized path, then line number. MVP search is literal UTF-8 text search; symbol search is later.

## Path Policy Edge Fixtures

All path policy failures use:

```json
{
  "ok": false,
  "error": {
    "code": "path_policy_violation",
    "message": "path is rejected by repository path policy",
    "details": {
      "path": "<input>",
      "policy_id": "path_policy_posix_case_sensitive_v1",
      "reason": "<reason>",
      "session_generation_id": "gen_agent_a_0001"
    }
  }
}
```

Required cases:

| Fixture | Input | Expected reason |
| --- | --- | --- |
| `path_escape_parent` | `../README.md` | `escapes_repository` |
| `path_escape_absolute` | `/tmp/README.md` | `absolute_path` |
| `path_empty_component` | `src//auth.ts` | `non_normalized_path` |
| `path_current_dir_component` | `./src/auth.ts` | `non_normalized_path` |
| `path_parent_component_inside` | `src/../README.md` | `non_normalized_path` |
| `path_backslash_separator` | `src\auth.ts` | `platform_invalid_separator` |
| `path_nul_byte` | `src/auth.ts\u0000.md` | `invalid_character` |
| `path_case_distinct_existing` | create `src/Auth.ts` when `src/auth.ts` exists | allowed under POSIX policy, rejected only under future case-folded policy |
| `path_unicode_non_normalized` | decomposed Unicode path | `unicode_normalization_required` if policy requires NFC; otherwise accepted and echoed unchanged |
| `path_symlink_traversal` | baseline symlink `link-out -> ../outside` then read `link-out/file` | `symlink_escape` |
| `path_git_internal` | `.git/config` | `reserved_path` |
| `path_sunlight_internal` | `.sunlight/config.toml` through artifact IO | `reserved_path` unless an explicit administrative API is used |

For `path_not_found`, use the standard failure code from the native IO spec, not `path_policy_violation`.

## Patch And Write Fixtures

These are next-increment fixtures. They should be drafted before implementing mutation persistence because read-after-write changes the session generation contract.

### Patch Fixture

Command:

```text
sun patch src/auth.ts --session session_agent_a --expect-hash sha256:auth_base --patch-file patches/auth_trim_guard.diff --json
```

Expected success snapshot:

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
        "expected_path": "src/auth.ts",
        "expected_hash": "sha256:auth_base"
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

Required negative fixtures:

- `patch_stale_hash_no_write`: returns `precondition_failed`, generation remains `gen_agent_a_0001`, and no operation or revision ID is present.
- `patch_bad_hunk_no_write`: returns `patch_apply_failed`, generation remains unchanged, and details identify `artifact_src_auth_ts`.
- `patch_wrong_path_binding_no_write`: returns `precondition_failed` when the artifact ID is valid but the expected path no longer binds to it.

### Write Fixture

Command:

```text
sun write src/session.ts --session session_agent_a --expect-hash new --content-file files/session.ts --classification source --json
```

Expected success snapshot:

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
        "expected_path": "src/session.ts",
        "expected_hash": "new"
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

Required negative fixtures:

- `write_existing_with_new_precondition_no_write`: `src/auth.ts` with `--expect-hash new` returns `precondition_failed`.
- `write_new_reserved_path_no_write`: `.sunlight/records/x.json` returns `path_policy_violation`.
- `write_secret_quarantine_no_write_or_policy_marker`: content matching the secret detector returns a policy error or creates only a quarantine marker; no source artifact becomes readable in the session.

## Read-After-Write Fixtures

These fixtures prove that accepted own-topic mutations advance the current session before the response returns.

| Fixture | Steps | Assertions |
| --- | --- | --- |
| `patch_then_read_same_session` | Start at `gen_agent_a_0001`, patch `src/auth.ts`, read `src/auth.ts`. | Patch returns `gen_agent_a_0002`; read uses `gen_agent_a_0002`; read bytes and hash are `sha256:auth_trim_guard`. |
| `patch_then_list_same_session` | Patch `src/auth.ts`, list `src`. | `src/auth.ts` entry has `sha256:auth_trim_guard`; unchanged entries retain base hashes. |
| `write_then_read_same_session` | Write `src/session.ts`, read it. | Read returns new artifact `artifact_src_session_ts`, `sha256:session_new`, and generation `gen_agent_a_0003`. |
| `write_then_search_same_session` | Write content containing `SessionStore`, search `SessionStore`. | Search returns `src/session.ts` at the new generation. |
| `failed_patch_then_read` | Attempt stale patch, then read `src/auth.ts`. | Failure response has no operation/revision IDs; read remains on prior generation and prior hash. |
| `other_topic_does_not_move_on_write` | Session frontier includes a pinned dependency; another topic head moves externally; agent writes own topic. | Write advances only own topic; response reports any available refresh as a warning or omits it; no silent dependency movement. |

## Minimal Implementation Sequence

1. Add fixture repository setup with the five baseline files and deterministic session/topic IDs for tests.
2. Persist or synthesize the minimal repository, artifact, content blob, content tree, topic, session, and resolved view records required by read/list/search.
3. Implement the standard JSON envelope and error envelope exactly as snapshotted.
4. Implement path normalization and path policy rejection before artifact lookup.
5. Implement `read` by resolving normalized path or artifact ID against the session generation tree and returning inline UTF-8 bytes for small text fixtures.
6. Implement `list` over the session generation tree with normalized path-prefix filtering and stable path ordering.
7. Implement literal UTF-8 `search` over session-visible text blobs with path and line ordering.
8. Add negative tests for path policy, missing paths, unknown sessions, and malformed requests.
9. Add patch/write precondition validation fixtures without broad resolver composition.
10. Implement patch/write persistence after read/list/search is stable, returning new operation, revision, resolved view, tree identity, and session generation IDs.
11. Add read-after-write tests for read, list, search, status, and inspect on the same session.
12. Tighten stable labels to canonical hashes once canonical record hashing helpers are ready.

## Boundaries

- Do not implement Phase 2 multi-topic resolver composition for these fixtures.
- Do not infer changes from the Git working tree.
- Do not use filesystem projections as the source of truth for read/list/search.
- Do not edit the development manager scratchpad for this fixture slice.
- Keep generated or secret content out of durable source fixtures unless the policy fixture explicitly expects quarantine or rejection.

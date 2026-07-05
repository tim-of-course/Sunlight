# Sunlight Native IO Phase 1 Spec v0.1

| Field | Value |
| --- | --- |
| Status | Implementation-ready Phase 1 command/API contract |
| Date | July 3, 2026 |
| Scope | `sun init`, topics, sessions, native artifact IO, status/inspect, structured responses, and fixture acceptance tests |
| Sources | `docs/sunlight_consolidated_architecture_v0_3.md`, `docs/sunlight_implementation_backlog_v0_1.md`, `docs/sunlight_schema_contracts_v0_1.md` |

## Purpose

This spec defines the Phase 1 native IO surface that implementation agents should build next. It is intentionally narrower than the architecture: implement the commands below, return structured JSON, persist the required v1 records, and prove behavior with fixture-style tests.

Phase 1 is successful when an agent can import a Git baseline, create a topic/session, read and search the session view, mutate artifacts through Sunlight commands, observe its own writes immediately, and inspect provenance without relying on direct project-directory edits or `git status`.

## Common CLI and API Rules

- Every command accepts `--json`; fixture tests should use JSON output only.
- Success responses use `{ "ok": true, "data": { ... }, "warnings": [] }`.
- Failure responses use `{ "ok": false, "error": { "code": "...", "message": "...", "details": { ... } } }`.
- Mutating commands require `session_id`, except `sun init` and `sun topic create`.
- Every mutation creates one `operation_transaction` and one new `topic_revision`.
- Every mutation response returns `operation_transaction_id`, `topic_revision_id`, `session_generation_id`, and `resolved_view_id`.
- Paths are repository-relative, normalized by the configured path policy, and rejected if they escape the repository.
- Phase 1 records exact full authored context for every operation; file-level read sets may be captured additionally.

## Response Envelope

Required fields for successful command responses:

| Field | Meaning |
| --- | --- |
| `ok` | Always `true` for success. |
| `data.command` | Stable command name, such as `artifact.patch`. |
| `data.repository_id` | Current Sunlight repository ID when applicable. |
| `data.ids` | Newly created or selected record IDs. |
| `data.view` | `resolved_view_id`, `tree_identity`, and `session_generation_id` when session-scoped. |
| `data.artifacts` | Artifact metadata affected or returned by the command. |
| `warnings` | Machine-readable warnings, default `[]`. |

Required fields for failures:

| Code | Use |
| --- | --- |
| `not_initialized` | `.sunlight` is missing or invalid for commands other than `init`. |
| `path_not_found` | Target path is absent in the session view. |
| `path_policy_violation` | Path normalization, escape, symlink, case, or platform policy rejects the path. |
| `precondition_failed` | Expected hash/path/view/revision does not match the current session generation. |
| `patch_apply_failed` | Patch hunks do not apply to the expected before content. |
| `session_not_found` | Session ID is unknown or expired under local retention. |
| `topic_not_found` | Topic selector is unknown. |
| `invalid_request` | Required flags or payload fields are missing or malformed. |

`precondition_failed` details must include the expected value, actual value, failed precondition name, unchanged `session_generation_id`, and no operation or revision IDs.

## Commands

### `sun init`

Purpose: create `.sunlight`, import current Git HEAD as the base checkpoint, and initialize policy-correct local/cache paths.

Minimum CLI:

```text
sun init --json
```

Success data includes `repository_id`, `base_checkpoint_id`, `resolved_view_id`, `tree_identity`, `path_policy_id`, `storage_schema_version`, and generated `.gitignore` policy status. Running `init` twice is idempotent when the existing repository record matches; otherwise return `invalid_request` with existing repository details.

### `sun topic create`

Purpose: create a durable write topic above the imported base checkpoint.

```text
sun topic create <slug> --display-name <name> --fixture basic-app --json
```

Success data includes `topic_id`, `slug`, `base_checkpoint_id`, `head_revision_id: null`, `status: "open"`, and owner/actor metadata. Slug collisions return `invalid_request` with the existing `topic_id`.

### `sun session start`

Purpose: bind an actor to one write topic and one pinned resolved view.

```text
sun session start --topic <topic> --view <view-selector> --actor <actor-id> --fixture basic-app --json
```

Success data includes `session_id`, `write_topic_id`, `resolved_view_id`, `session_generation_id`, `topic_frontier`, `refresh_policy: "pinned_except_own_topic"`, and capabilities for `read`, `list`, `search`, `inspect`, `patch`, `write`, `move`, `delete`, and `metadata`.

### `sun read`, `sun list`, and `sun search`

Purpose: inspect bytes and discover artifacts in the current session generation.

```text
sun read <path-or-artifact-id> --session <session> --json
sun list [path-prefix] --session <session> --json
sun search <query> --session <session> --json
```

`read` returns bytes or an encoded byte reference plus `artifact_id`, `path`, `content_hash`, `byte_length`, `classification`, `resolved_view_id`, and `session_generation_id`. `list` returns entries with path, artifact ID, kind, hash, executable bit, tombstone state, and classification. `search` returns matching paths/snippets or metadata hits scoped to the session view.

### `sun patch` and `sun write`

Purpose: mutate artifact content through topic-owned operation transactions.

```text
sun patch <path-or-artifact-id> --session <session> --expect-hash <hash> --patch-file <file> --json
sun write <path> --session <session> --expect-hash <hash-or-new> --content-file <file> --classification <class> --json
```

Required preconditions are `resolved_view_id` from the session generation, expected artifact hash or `new`, and expected path binding. `patch` applies file operation semantics v1 to the expected before bytes. `write` creates or replaces whole content and should be used for new files, generated files, or intentionally broad replacements.

Success returns before/after refs, write set, new topic revision, new session generation, and affected artifact metadata. If preconditions fail, no record is written and the session remains on the prior generation.

### `sun move`, `sun delete`, and `sun metadata set`

Purpose: record structural and metadata changes without filesystem inference.

```text
sun move <from> <to> --session <session> --fixture basic-app --expect-artifact <artifact-id> --expect-hash <hash> --json
sun delete <path-or-artifact-id> --session <session> --fixture basic-app --expect-hash <hash> --json
sun metadata set <path-or-artifact-id> --session <session> --fixture basic-app --expect-hash <hash> --classification <class> --json
```

`move` preserves `artifact_id` and adds a new path binding. `delete` tombstones the path binding and retains artifact provenance. `metadata set` records classification or other Phase 1 metadata without changing content bytes. All three commands require resolved-view and path/artifact preconditions and advance the write topic on success.

The accepted Phase 1 CLI fixture surface uses `--fixture basic-app` and `--json` for stable response envelopes on `topic.create`, `session.start`, `artifact.move`, `artifact.delete`, and `artifact.metadata_set`.

### `sun status` and `sun inspect`

Purpose: expose native state and provenance without relying on Git working tree state.

```text
sun status --json
sun status --session <session> --json
sun inspect <path-or-artifact-id|topic-id|session-id|operation-id> --json
```

`status` returns repository ID, base checkpoint, open topics, topic heads, active sessions, latest session generations, changed artifacts by topic, and unresolved native errors. `inspect` returns the selected object's identity, provenance links, path history, current hash, classification, authored context, before/after refs, and related operation/topic/session/revision IDs.

## Read-After-Write Expectations

An accepted mutation advances the session before the response returns. The next `read`, `list`, `search`, `status --session`, or `inspect` using the same session must observe the new topic revision and `session_generation_id`.

Pinned default behavior:

- The session frontier is pinned except for its own write topic.
- Accepted own-topic writes advance the session generation atomically.
- Other topics do not move during a write response.
- A failed mutation does not advance the session generation.
- Refresh and multi-topic resolver behavior are Phase 2 unless needed for a clear error.

## Fixture Acceptance Tests

Use small fixture repositories with deterministic content and JSON snapshots. Tests should assert IDs by stable fields where exact hashes are not yet finalized, then tighten to canonical IDs after hashing helpers land.

| Fixture | Steps | Required assertions |
| --- | --- | --- |
| `init_imports_git_head` | Create two-file Git repo, run `sun init --json`. | `.sunlight` exists; base checkpoint and resolved view exist; tree identity covers both files; local/cache paths are ignored. |
| `topic_session_start` | Run `topic create`, then `session start`. | Response has topic, session, resolved view, generation, pinned refresh policy, and write capabilities. |
| `read_list_search_session_view` | Read one file, list root, search known token. | Results include artifact ID, path, hash, classification, generation, and only session-view content. |
| `patch_read_after_write` | Read file hash, patch with expected hash, read again. | Operation and revision records exist; generation changed; second read returns after bytes and after hash. |
| `write_new_file_read_after_write` | Write a new source file with `--expect-hash new`, list/read it. | New artifact is visible in same session; operation has before `null`, after hash, classification, and topic ownership. |
| `precondition_failure_no_write` | Patch with stale `--expect-hash`. | Response code is `precondition_failed`; no operation/revision is created; generation is unchanged; read returns prior bytes. |
| `patch_apply_failure_no_write` | Patch with correct hash but invalid hunk context. | Response code is `patch_apply_failed`; no records are written; details include target artifact and failed hunk. |
| `move_preserves_artifact_id` | Move a file, inspect old and new paths. | New path resolves to same artifact ID; old path is tombstoned; path history is inspectable. |
| `delete_tombstones_path` | Delete a file, list/read/inspect. | Read by path returns `path_not_found`; inspect by artifact shows tombstone and delete operation provenance. |
| `metadata_records_classification` | Set classification on a file. | Content hash is unchanged; metadata operation and new revision exist; inspect shows classification source. |
| `status_and_inspect_provenance` | After patch/write/move, run status and inspect. | Status shows topic head and changed artifacts; inspect links artifact -> operation -> topic -> session -> revision. |

## Implementation Boundaries

- Do not implement Phase 2 resolver composition beyond the single-session resolved view needed for read-after-write.
- Do not infer edits from Git working tree diffs in Phase 1.
- Do not require execution projections, checkpoints, or Git export for these tests.
- Do not store raw secret bytes as durable payloads; return a policy error or quarantine marker if detection hooks classify content as secret.

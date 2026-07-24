# Sunlight Schema Contracts v0.1

> **Historical design record.** Secret detection, secret classification gates,
> and automatic source quarantine described here are superseded. See the
> repository README, `docs/local_mcp.md`, portable Agent Skill, and
> `docs/open_alpha_acceptance.md` for the current explicit-ignore contract.

| Field | Value |
| --- | --- |
| Status | Local MVP implementation contract |
| Date | July 3, 2026 |
| Scope | v1 canonical records, identity inputs, and export/privacy classes |
| Sources | `docs/sunlight_consolidated_architecture_v0_3.md`, `docs/sunlight_implementation_backlog_v0_1.md` |

## Purpose

This document is the compact schema contract for Phase 0 implementation agents. It names the v1 records the local MVP must persist, the required fields each record must carry, and the inputs that determine stable identity.

It is intentionally narrower than the architecture. If a field is listed here as required, implementation fixtures should include it. Optional future fields can be added under schema-versioned migration rules, but v1 records should not depend on Git working tree state, pretty JSON formatting, or rebuildable indexes for identity.

## Common Rules

- Canonical records use schema-versioned canonical JSON bytes for hashing. Human-readable mirrors are allowed only when strict canonicalization is preserved.
- Every record has `schema_version: 1`, `record_type`, `id`, `repository_id` or `repository_scope`, `created_at`, and `privacy_class`.
- Hashes use `sha256:<hex>` over canonical payload bytes unless a later hash policy record changes the algorithm.
- Record IDs are derived from the identity inputs listed below. Mutable pointers, wall-clock timestamps, display names, and file formatting are not identity inputs unless explicitly listed.
- `tree_identity` is `SingleRepoTree { repository_id, tree_hash }` for the local MVP. The field shape must allow future `RepoTreeMap { repositories: { repository_id: tree_hash } }`.
- Privacy/export classes are:
  - `commit_default`: safe for normal Git transport after validation.
  - `policy_gated`: may be exported only after reachability, size, secret, and privacy validation.
  - `local_only`: ignored by Git and never exported by default.
  - `secret`: quarantined or represented by a typed vault reference, not durable secret bytes.

## v1 Records

| Record | Required fields | Identity/hash inputs | Privacy/export class |
| --- | --- | --- | --- |
| `repository` | `id`, `record_type`, `schema_version`, `repository_id`, `storage_schema_version`, `config_schema_version`, `path_policy`, `projection_policy`, `git_interop_policy`, `created_at` | `record_type`, `schema_version`, `repository_id`, `path_policy`, `storage_schema_version` | `commit_default` after secret scan |
| `artifact` | `id`, `record_type`, `schema_version`, `repository_id`, `artifact_kind`, `path_bindings`, `current_content_ref`, `metadata`, `classification`, `created_by_operation_id` | `repository_id`, stable artifact seed, `artifact_kind`; path changes do not change `artifact_id` | `commit_default` for metadata; payload refs are gated |
| `content_blob` | `id`, `record_type`, `schema_version`, `repository_id`, `digest`, `byte_length`, `media_type`, `classification`, `storage_ref` | exact bytes, digest algorithm, `byte_length` | `policy_gated`; `secret` if detected |
| `content_tree` | `id`, `record_type`, `schema_version`, `repository_id`, `tree_hash`, `entries`, `path_policy_id` | sorted entries of path, artifact ID, content digest/tree hash, executable bit, symlink target, tombstone state, path policy | `policy_gated` unless manifest contains only safe metadata |
| `operation_transaction` | `id`, `record_type`, `schema_version`, `repository_id`, `topic_id`, `session_id`, `session_generation_id`, `actor_id`, `authored_context_id`, `preconditions`, `read_set`, `write_set`, `mutation_payload`, `before_refs`, `after_refs`, `classification`, `logical_time`, `parents` | `topic_id`, parent revision, authored context, preconditions, mutation payload, before/after refs, write set | metadata `commit_default`; payloads `policy_gated` |
| `topic` | `id`, `record_type`, `schema_version`, `repository_id`, `slug`, `display_name`, `owner_actor_id`, `base_checkpoint_id`, `created_at`, `visibility`, `status`, `head_revision_id` | `repository_id`, topic creation nonce or imported external identity | sanitized metadata `commit_default`; private topics `policy_gated` |
| `topic_revision` | `id`, `record_type`, `schema_version`, `repository_id`, `topic_id`, `revision_number`, `parent_revision_id`, `operation_transaction_id`, `tree_delta_ref`, `dependency_revision_ids`, `created_at` | `topic_id`, parent revision, operation transaction ID, dependency revision IDs | `commit_default` if referenced operation payloads are allowed or omitted |
| `session_generation` | `id`, `record_type`, `schema_version`, `repository_id`, `session_id`, `write_topic_id`, `base_resolved_view_id`, `resolved_view_id`, `topic_frontier`, `generation_number`, `refresh_policy`, `created_by` | `session_id`, previous generation, write topic revision, pinned frontier, resolved view ID | `local_only` by default; summaries may be `policy_gated` |
| `resolved_view` | `id`, `record_type`, `schema_version`, `repository_scope`, `base_checkpoint_ids`, `topic_frontier`, `dependency_closure`, `operation_semantics_version`, `path_policy_id`, `conflict_ids`, `staleness_ids`, `tree_identity` | repository scope, exact base checkpoints, exact topic revisions, dependency closure, operation semantics version, path policy, conflict/resolution set, tree identity | `commit_default` when no private refs |
| `conflict_staleness` | `id`, `record_type`, `schema_version`, `repository_scope`, `kind`, `resolved_view_id`, `artifact_ids`, `path_refs`, `operation_ids`, `authored_context_ids`, `policy_reason`, `candidate_refs`, `resolution_operation_id` | kind, target view, competing operations, artifact/path refs, policy reason, candidate materialization hashes | summary `commit_default`; candidate bytes `policy_gated` |
| `execution` | `id`, `record_type`, `schema_version`, `repository_scope`, `resolved_view_id`, `tree_identity`, `command`, `working_directory`, `environment_summary`, `projection_id`, `inputs`, `outputs`, `promotions`, `result`, `started_at`, `finished_at` | resolved view ID, tree identity, normalized command, working directory, environment summary digest, input refs, projection strategy | summary `policy_gated`; raw logs and sandboxes `local_only` |
| `checkpoint` | `id`, `record_type`, `schema_version`, `repository_scope`, `resolved_view_id`, `tree_identity`, `topic_frontier`, `evidence_refs`, `conflict_free`, `created_by`, `created_at`, `retention_class`, `export_refs` | resolved view ID, tree identity, topic frontier, selected evidence refs, conflict-free status | manifest `commit_default` after validation; evidence payloads gated |
| `git_export_map` | `id`, `record_type`, `schema_version`, `repository_id`, `checkpoint_id`, `tree_identity`, `git_remote`, `git_ref`, `git_commit_ids`, `export_shape`, `validation_report_id`, `exported_at` | checkpoint ID, tree identity, export shape, git ref, git commit IDs | `commit_default` |

## Identity Notes

- Artifact identity is stable across moves. Path bindings are versioned metadata with tombstones for deletes.
- Content identity is byte-exact. Text normalization, line-ending conversion, and executable-bit policy are tree concerns, not blob hash rewrites.
- Operation transaction identity includes authored context and preconditions so projected or adapted work creates a new operation instead of rewriting history.
- Resolved view identity pins exact topic revisions. Moving selectors such as `topic@head` must be normalized before identity is computed.
- Session generation identity advances after accepted writes and explicit refreshes. It is session-local and should not become a durable public coordination primitive by default.
- Git export identity maps native checkpoints to Git artifacts. Git commit messages are never the only place that native provenance is stored.

## Compact Example Record

```json
{
  "schema_version": 1,
  "record_type": "operation_transaction",
  "id": "op_sha256_2d4f0b",
  "repository_id": "repo_01JZ0LOCAL",
  "topic_id": "topic_auth_nullability",
  "session_id": "session_agent_a",
  "session_generation_id": "gen_session_agent_a_0003",
  "actor_id": "agent_a",
  "authored_context_id": "ctx_view_042",
  "preconditions": {
    "resolved_view_id": "view_042",
    "artifact_hashes": {
      "artifact_src_auth_ts": "sha256:aaa111"
    }
  },
  "read_set": {
    "mode": "full_authored_context",
    "resolved_view_id": "view_042"
  },
  "write_set": [
    {
      "artifact_id": "artifact_src_auth_ts",
      "path": "src/auth.ts",
      "mutation": "patch"
    }
  ],
  "mutation_payload": {
    "kind": "patch",
    "patch_digest": "sha256:bbb222"
  },
  "before_refs": {
    "artifact_src_auth_ts": "sha256:aaa111"
  },
  "after_refs": {
    "artifact_src_auth_ts": "sha256:ccc333"
  },
  "classification": "source",
  "privacy_class": "policy_gated",
  "logical_time": {
    "topic_revision_parent": "rev_auth_nullability_0002"
  },
  "parents": [
    "rev_auth_nullability_0002"
  ],
  "created_at": "2026-07-03T00:00:00Z"
}
```

## Implementation Fixture Expectations

- Add one fixture per record type with only the required v1 fields plus minimal nested values.
- Add canonicalization tests that prove reordered object keys and pretty formatting produce the same record ID.
- Add negative fixtures for missing `schema_version`, missing repository scope, unresolved moving selectors in `resolved_view`, operation transactions without topic ownership, and export maps without a checkpoint.
- Add policy fixtures for `commit_default`, `policy_gated`, `local_only`, and `secret` so export validation can reject unsafe reachability before Git export work starts.

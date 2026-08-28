# Sunlight Git Export Writer Contract v0.1

| Field | Value |
| --- | --- |
| Status | Docs-only implementation contract after fixture CLI |
| Date | July 4, 2026 |
| Scope | Real local Git export writer for `single_checkpoint_commit` |
| Sources | `docs/sunlight_checkpoint_git_export_v0_1.md`, `docs/sunlight_policy_validation_spec_v0_1.md`, `crates/sunlight-core/src/git_export.rs` |

## Purpose

Replace the fixture-only `sun git export` response with a local writer that creates an ordinary Git commit from a validated Sunlight checkpoint. The writer produces compatibility Git history, then records the native-to-Git mapping. Native records remain authoritative.

This contract does not add topic-per-commit export, remote push, merge orchestration, lock management across processes, or Git-based coordination.

## Inputs

The writer requires:

- Exact `checkpoint_id` resolving to one `checkpoint` record.
- Exact `tree_identity` from that checkpoint.
- Passing export validation report for the same checkpoint, target ref, export shape, and metadata policy.
- Target local Git repository root.
- Target full Git ref, normally `refs/heads/...`.
- Export shape `single_checkpoint_commit`.
- Parent policy `base_checkpoint_git_parent`.
- Metadata policy `policy_approved_manifest_only`.

The writer must reject moving checkpoint selectors, moving topic selectors, missing validation reports, validation reports for a different target, and validation reports that are stale against the checkpoint/export inputs.

## Policy Gates

Run or load export validation before writing Git objects. Hard failures stop the export before ref update.

Required gates:

- Checkpoint is conflict-free and not stale.
- Checkpoint tree, content blobs, evidence refs, resolved view, and topic frontier are reachable by exact IDs.
- Every exported project file is sourced from checkpoint content records, not the mutable working tree.
- `.sunlight` metadata is limited to policy-approved manifests.
- `secret`, `local_only`, raw execution logs, sandboxes, caches, unsafe filesystem references, and unpromoted generated outputs are denied.
- Validation report ID is persisted and later referenced by the `git_export_map`.

Warnings may be returned, but warnings cannot downgrade hard failures for secrets, local-only data, unsafe references, missing reachability, conflicted views, raw execution inclusion, or generated-output promotion gaps.

## Parent Selection

Default parent selection is `base_checkpoint_git_parent`, extended by a
durable prior-export lineage on the requested ref.

The writer must:

- If the target ref tip has a durable Sunlight export map for a different
  checkpoint on that exact ref, use the mapped commit as the parent.
- Otherwise resolve the checkpoint's imported base checkpoint chain to exactly
  one Git commit and use it as the parent.
- Fail with `export_parent_not_found` if no compatible imported base commit is recorded.
- Fail with `export_parent_ambiguous` if multiple candidate parent commits match and policy does not select one.
- Reject parent commits outside the target repository object database.
- Treat an unmapped target branch tip as a safety conflict, not as semantic
  parent authority.

If the requested target ref already exists, the writer may update it only when
the existing tip is the selected parent or an earlier export map for the same
checkpoint. Otherwise fail before updating the ref.

## Commit Shape

The MVP writes exactly one Git commit.

Required commit properties:

- Tree entries equal the checkpoint `content_tree` plus allowed `.sunlight` manifests.
- File paths are repository-relative, normalized by the checkpoint path policy, and never absolute.
- Executable bits are preserved for files marked executable by the checkpoint tree.
- File modes are limited to regular file and executable file modes unless a later policy adds symlinks or submodules.
- Author and committer identities are deterministic from configured export identity, not inferred from unrelated local Git config without an explicit fallback policy.
- Commit message includes a short title plus checkpoint ID, resolved view ID, validation report ID, and export shape.
- Essential provenance is stored in `git_export_map`, not only in the commit message.

After commit creation, persist one `git_export_map` containing checkpoint ID, tree identity, target ref, commit ID, export shape, validation report ID, timestamp, and privacy class. Update checkpoint `export_refs` only after the export map is durable.

## Local Repository Safety

The writer operates on a local Git repository but must not treat Git state as Sunlight state.

Safety requirements:

- Discover and validate the Git repository root before export.
- Refuse to run outside the configured Sunlight repository scope.
- Build Git tree objects from checkpoint content via temporary files or Git plumbing, not from the user's working tree.
- Ignore untracked, modified, and staged working-tree files for export content.
- Do not require the working tree to be clean unless a specific ref update policy needs it.
- Write to a temporary ref or unattached commit first, then atomically update the requested ref after all validation and object writes succeed.
- Do not alter unrelated refs, index entries, hooks, remotes, Git config, or user worktree files.
- If commit creation succeeds but export-map persistence fails, report `export_map_write_failed` with the commit ID and no successful export-map claim.
- If ref update fails after commit creation, report `export_ref_update_failed` with the commit ID and leave native records unchanged.

## Errors

Stable error codes:

| Code | Meaning |
| --- | --- |
| `export_policy_failed` | Export validation has hard failures. |
| `export_parent_not_found` | No imported base Git parent is available. |
| `export_parent_ambiguous` | More than one parent candidate matches. |
| `export_target_ref_invalid` | Target ref is not an allowed local Git ref. |
| `export_target_ref_conflict` | Existing target ref is not allowed by update policy. |
| `export_repository_invalid` | Git root does not match repository scope or object access failed. |
| `export_git_failed` | Git object creation failed before a commit ID was produced. |
| `export_ref_update_failed` | Commit exists but requested ref was not updated. |
| `export_map_write_failed` | Commit/ref work completed but native export map was not persisted. |

JSON errors include checkpoint ID when known, validation report ID when available, target ref, optional parent commit, optional created commit ID, and a human-actionable message.

## Verification Tests

Required tests before replacing the fixture CLI path:

| Test | Required assertion |
| --- | --- |
| `git_export_writes_real_commit` | Local Git repository receives one real commit and target ref points to it. |
| `git_export_commit_tree_matches_checkpoint` | `git ls-tree` for the commit matches checkpoint paths, bytes, and executable bits plus allowed manifests. |
| `git_export_ignores_working_tree` | Extra untracked, modified, and staged files do not appear in the exported commit. |
| `git_export_selects_base_parent` | Commit parent equals the imported base checkpoint Git commit. |
| `git_export_appends_prior_mapped_ref` | A different checkpoint exported to the same mapped ref uses the prior exported commit as parent and persists a new export map. |
| `git_export_missing_parent_fails` | Missing base Git parent returns `export_parent_not_found` and does not update the ref. |
| `git_export_ref_conflict_fails` | Conflicting existing target ref returns `export_target_ref_conflict` and leaves the ref unchanged. |
| `git_export_policy_failure_no_ref_update` | Policy hard failure writes no commit on the target ref and no export map. |
| `git_export_map_persisted_after_success` | Export map stores checkpoint ID, tree identity, ref, commit ID, export shape, and validation report ID. |
| `git_export_map_failure_reports_partial` | Simulated export-map write failure reports the created commit ID and does not mark checkpoint export refs complete. |
| `git_export_rejects_unsafe_metadata` | Secret/local-only/raw execution metadata blocks export before Git object creation. |
| `git_export_json_envelope_stable` | CLI JSON success and failure envelopes retain the existing fixture command shape with real IDs. |

Run existing fixture tests and new local Git writer tests. Also run `git diff --check` for docs/code whitespace.

## No Coordination Through Git

Git export is a compatibility artifact only.

The writer must not:

- Use Git branches, refs, lock files, reflogs, commits, or commit messages as the source of Sunlight truth.
- Infer topic order, dependency closure, evidence status, or conflict resolution from Git history.
- Coordinate native writers by pushing, pulling, rebasing, merging, or waiting on Git refs.
- Treat the target branch tip as a queue, lease, or consensus mechanism.
- Repair missing native records by reading exported commits.

All coordination, provenance, validation, and readiness decisions remain in Sunlight records. Git receives a projection after those decisions are complete.

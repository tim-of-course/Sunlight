# Sunlight Projection Strategy Spike Plan v0.1

| Field | Value |
| --- | --- |
| Status | Phase 0 spike plan |
| Date | July 3, 2026 |
| Scope | Compare local projection strategies for exact resolved-view execution sandboxes |
| Sources | `docs/sunlight_implementation_backlog_v0_1.md`, `docs/sunlight_execution_projection_v0_1.md`, `docs/sunlight_checkpoint_git_export_v0_1.md` |

## Purpose

Sunlight needs fast, isolated projections of exact resolved views so tests and tools can run without treating the mutable Git working tree as source truth.

This spike measures candidate materialization strategies on the first target filesystem and records the safety constraints needed before Phase 3 execution work depends on them. The result should pick a default strategy, a correctness fallback, and any strategies that are too risky for MVP use.

## Questions

- How long does each strategy take to materialize small, medium, and large repository fixtures?
- How much extra disk space does each strategy consume before and after a representative command?
- Can common commands run against the projection without unexpected write failures?
- Can the strategy protect immutable Sunlight store objects from accidental mutation?
- What metadata must be recorded so projection reuse is explainable and invalidation is deterministic?

## Candidate Strategies

| Strategy | What to measure | Expected role |
| --- | --- | --- |
| Full copy | Wall time, disk amplification, command compatibility | Correctness fallback for every supported filesystem |
| Reflink copy | Availability, copy speed, copy-on-write behavior, mutation isolation | Preferred fast path where filesystem support is reliable |
| Read-only hardlink | Setup speed, permission behavior, store mutation risk, tool compatibility | Possible read-mostly optimization only if store integrity is provably protected |
| Overlay or copy-up | First-run setup cost, write amplification, cleanup complexity | Candidate for writable tools after simpler strategies are understood |

## Test Matrix

Use at least three fixtures:

- `tiny`: a few source files and one test command.
- `medium`: hundreds of files with mixed source, lockfiles, and generated-looking output paths.
- `wide`: many small files to expose metadata and directory traversal costs.

For each fixture and strategy, run:

1. Materialize a projection from an immutable content tree.
2. Verify file contents, executable bits, symlink policy behavior, and tombstones if present.
3. Run a harmless read-only command, such as listing files or compiling without writes when possible.
4. Run a representative writable command, such as tests, formatting, or code generation.
5. Scan changed paths and classify source-like deltas, generated artifacts, logs, caches, and ignored output.
6. Verify immutable store objects are unchanged after command execution.
7. Remove the projection and confirm local-only cleanup behavior.

## Metrics

Record one row per fixture, strategy, and command:

- repository fixture ID and tree identity
- strategy and filesystem details
- materialization wall time
- command wall time and exit result
- apparent projection size and changed byte count
- number of files, directories, symlinks, and executable files
- changed path classifications
- integrity check result
- cleanup result
- notable compatibility failures

Raw logs and projection paths are local-only. The shareable spike report should include summaries, counts, and digests rather than machine-specific absolute paths.

## Safety Gates

A strategy is not eligible for MVP default use unless it:

- rejects conflicted or stale resolved views before materialization
- materializes from `resolved_view.tree_identity`, not the Git working tree
- prevents command writes from mutating immutable store content
- preserves path policy decisions and executable metadata
- records projection ID, resolved view ID, tree identity, strategy, cache key, and writable policy
- produces stable errors for unsupported filesystem features
- has a full-copy fallback for correctness

Read-only hardlinks require extra proof: mutation attempts must fail or affect only private copy-up data. If that cannot be guaranteed on the target filesystem, hardlinks remain out of the MVP default path.

## Deliverables

- A short benchmark report with the metric table and filesystem notes.
- A recommended MVP default projection strategy.
- A required fallback strategy and fallback trigger list.
- A list of strategies deferred from MVP, with reasons.
- Acceptance fixtures that Phase 3 can reuse for execution sandbox tests.
- Open implementation tasks for cache keys, cleanup policy, and integrity checks.

## Acceptance Criteria

The spike is complete when:

- full copy has been verified as the correctness fallback
- at least one faster strategy has been attempted and either accepted with constraints or rejected with evidence
- store-integrity checks are demonstrated after writable commands
- projection metadata required by execution records is identified
- local-only data boundaries are documented
- Phase 3 can proceed with a clear default and no dependency on mutable working-tree files

## Observed WSL/Linux Filesystem Probe

On July 5, 2026, `scripts/projection-strategy-smoke.sh` added scoped
capability probes for the default temp directory and an optional non-temp
parent such as the WSL home clone. The rows are local observations for this
host and these mounts only; they omit absolute paths and do not claim support
on other mounts or hosts.

```text
projection_fs_capability host_scope=current_wsl_linux_tempdir fs_type=tmpfs probe_root=tempdir absolute_paths=omitted
projection_fs_capability host_scope=current_wsl_linux_tempdir probe_root=tempdir strategy=copy fs_type=tmpfs accepted=accepted reason=correctness_fallback
projection_fs_capability host_scope=current_wsl_linux_tempdir probe_root=tempdir strategy=reflink fs_type=tmpfs reflink_attempt=failed writes_private=unknown accepted=deferred reason=operation_not_supported
projection_fs_capability host_scope=current_wsl_linux_tempdir probe_root=tempdir strategy=hardlink_readonly fs_type=tmpfs hardlink_attempt=ok read_only_write_blocked=yes chmod_write_mutated_store=yes mutation_isolation_risk=present accepted=deferred reason=shared_inode_owner_can_chmod_projection_and_mutate_store
projection_fs_capability host_scope=current_wsl_linux_tempdir probe_root=tempdir strategy=overlay_copyup fs_type=tmpfs overlay_attempt=failed copyup_writes_private=unknown accepted=deferred reason=permission_denied
projection_fs_capability host_scope=current_wsl_linux_non_temp_root fs_type=ext2/ext3 probe_root=non_temp absolute_paths=omitted
projection_fs_capability host_scope=current_wsl_linux_non_temp_root probe_root=non_temp strategy=copy fs_type=ext2/ext3 accepted=accepted reason=correctness_fallback
projection_fs_capability host_scope=current_wsl_linux_non_temp_root probe_root=non_temp strategy=reflink fs_type=ext2/ext3 reflink_attempt=failed writes_private=unknown accepted=deferred reason=operation_not_supported
projection_fs_capability host_scope=current_wsl_linux_non_temp_root probe_root=non_temp strategy=hardlink_readonly fs_type=ext2/ext3 hardlink_attempt=ok read_only_write_blocked=yes chmod_write_mutated_store=yes mutation_isolation_risk=present accepted=deferred reason=shared_inode_owner_can_chmod_projection_and_mutate_store
projection_fs_capability host_scope=current_wsl_linux_non_temp_root probe_root=non_temp strategy=overlay_copyup fs_type=ext2/ext3 overlay_attempt=failed copyup_writes_private=unknown accepted=deferred reason=permission_denied
```

Decision from this host probe:

- `copy` remains the accepted correctness fallback.
- `reflink` is deferred for both observed filesystems because the real reflink
  attempts failed as unsupported.
- `hardlink_readonly` is deferred because read-only file mode blocked a direct
  write but did not protect immutable store content from owner `chmod` followed
  by mutation through the linked projection path.
- `overlay_copyup` is deferred because unprivileged overlay/copy-up was not
  observable without sudo or package installation.

Any future move toward a faster-than-copy default should still keep `copy` as
the fallback and must include command compatibility plus store-integrity checks.


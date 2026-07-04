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


# Sunlight MVP Readiness Audit - 2026-07-05

## Scope

This audit compares the current implementation against the MVP phases in `docs/sunlight_consolidated_architecture_v0_3.md`, using the validation plan, smoke scripts, CLI JSON tests, and `sunlight-core` module surface as evidence. It is intentionally scoped to readiness and delegation; it does not propose broad implementation changes.

## Executive Summary

Sunlight has a strong fixture-backed MVP spine through Phase 6: repository init and policy, native artifact reads and writes, Phase 1 topic/session lifecycle, Phase 1 structural mutation commands, deterministic resolver conflicts, projection materialization and integrity, execution records and output promotion, checkpoint creation, Git export planning/execution, operator status/inspect, compatibility import, and optional external validation against a temporary Super Search clone.

The prior Phase 1 lifecycle gap is now closed for fixture CLI acceptance: `topic create`, `session start`, `move`, `delete`, and `metadata set` return stable JSON envelopes with topic/session IDs, session generation advancement, operation/revision IDs, precondition failure behavior, tombstone/path binding details, and metadata classification changes. The next narrow gap is not another native IO command; it is synchronizing CLI help and command documentation so the accepted Phase 1 commands are no longer described as parse-only or incomplete.

## Phase Readiness Matrix

| MVP phase | Current coverage | Verification evidence | Readiness note |
| --- | --- | --- | --- |
| Phase 0: schemas, hashing, path policy, operation format, tree identity, session generation, `.sunlight` policy, projection spike | Core modules cover records, identity, repository init, policy, operation transaction records, `TreeIdentity::SingleRepoTree`, projection strategy planning, manifests, root binding, and quarantine metadata. | `crates/sunlight-core/src/records.rs`, `identity.rs`, `repository.rs`, `policy.rs`, `artifacts.rs`, `projection.rs`; `scripts/projection-strategy-smoke.ps1`; `scripts/smoke-suite.ps1`; `crates/sun/tests/cli_json.rs` projection/policy/init tests. | Functionally covered for the local fixture MVP and real temp repo init. |
| Phase 1: native artifact IO vertical slice | `sun init`, `topic create`, `session start`, `read`, `list`, `search`, `patch`, `write`, `move`, `delete`, `metadata set`, status, inspect, authored context, read-after-write generation, structural mutation provenance, and precondition failures are covered for fixture CLI acceptance. | `crates/sun/tests/cli_json.rs` tests for topic/session lifecycle, read/list/search, patch/write, move/delete/metadata, stale write/move preconditions, status/inspect provenance; `crates/sunlight-core/src/artifacts.rs`; `crates/sun/src/main.rs`; `docs/sunlight_native_io_phase1_spec_v0_1.md`. Latest aggregate baseline in `docs/development_manager_scratchpad.md`: after `de5c5cb`, default `scripts/smoke-suite.ps1` passed via the Windows-native fallback with 168 CLI tests, 143 core tests, validation smoke, projection strategy smoke, and MVP smoke. | Covered for fixture CLI acceptance. Remaining polish should synchronize help/docs and may add deeper status/inspect round trips, but the lifecycle and structural mutation commands are no longer the blocking Phase 1 gap. |
| Phase 2: resolver and conflicts | Deterministic view resolution, reversed frontier stability, independent-file composition, same-artifact conflict objects, staleness, and conflict visibility through status/inspect are covered. | `crates/sunlight-core/src/resolver.rs`; `docs/sunlight_resolver_conflict_fixtures_v0_1.md`; `crates/sun/tests/cli_json.rs` resolver/conflict/staleness tests; `scripts/validation-smoke.ps1`. | Covered for same-repo fixture acceptance. |
| Phase 3: execution projections | `sun run`, projection materialization, execution records, integrity rejection before execution, failed execution records, generated output promotion, and status/inspect exposure are covered. Projection cache/integrity/quarantine lifecycle has extensive focused tests. | `crates/sunlight-core/src/execution.rs`; `projection.rs`; `docs/sunlight_execution_projection_v0_1.md`; `crates/sun/tests/cli_json.rs` run, promote-output, projection integrity, manifest, and quarantine tests; `scripts/mvp-smoke.ps1`. | Covered for fixture execution and local projection safety. |
| Phase 4: checkpoints and Git export | Conflict-free checkpoint creation, conflict/stale rejection, policy check/export explain, write planning, local Git commit/ref export, export map persistence, dirty worktree isolation, and failure envelopes are covered. | `crates/sunlight-core/src/checkpoint.rs`; `git_export.rs`; `docs/sunlight_checkpoint_git_export_v0_1.md`; `docs/sunlight_git_export_writer_v0_1.md`; `crates/sun/tests/cli_json.rs` checkpoint/git export tests; `scripts/mvp-smoke.ps1`. | Covered for the single-checkpoint local Git export MVP. |
| Phase 5: operator ergonomics | Status/inspect surfaces exist for repository, session, checkpoint, export map, Git refs, projections, compatibility imports, executions, artifacts, and operations. Policy explain provides operator-readable validation bodies. | `docs/sunlight_cli_status_inspect_v0_1.md`; `docs/sunlight_operator_status_matrix_v0_1.md`; broad `status_*` and `inspect_*` tests in `crates/sun/tests/cli_json.rs`. | Covered enough for local MVP operations; polish can continue after Phase 1 lifecycle closure. |
| Phase 6: compatibility import | Compatibility projection, diff classification, import planning, operation/revision/generation provenance, rename/delete/metadata/multi-candidate import, path policy, ignored-path/cache/secret/generated/conflicted failure gates, working-tree isolation, and projection-only checkpoint/export boundaries are covered. | `crates/sunlight-core/src/compat_import.rs`; `docs/sunlight_compatibility_import_v0_1.md`; `docs/sunlight_validation_repo_plan_v0_1.md`; `crates/sun/tests/cli_json.rs` compat tests; `scripts/external-validation-super-search.ps1`. | Covered for fixture compatibility and optional external validation boundaries. |

## Cross-Phase Verification Already Proving MVP Spine

- `scripts/smoke-suite.ps1` is the aggregate gate: format, check, tests, validation smoke, projection strategy smoke, and MVP smoke.
- Latest recorded full baseline: after `de5c5cb`, default `scripts/smoke-suite.ps1` passed via the Windows-native fallback with 168 CLI tests, 143 core tests, validation smoke, projection strategy smoke, and MVP smoke.
- `scripts/validation-smoke.ps1` covers real temp repo `sun init`, fixture artifact IO, resolver conflict, execution projection, checkpoint, policy, compatibility projection/import, and Git export write-plan envelopes.
- `scripts/mvp-smoke.ps1` runs the end-to-end path from view resolve through filesystem projection, execution, checkpoint, and real local Git export into a temporary repo.
- Latest recorded optional external validation: after `44775b8`, `scripts/external-validation-super-search.ps1` passed against Super Search, covering target `mix test`, `bun run test`, temp-clone `sun init`, fixture compat project/diff/status/inspect, happy-path and generated-failure compat import, and fixture Git export.
- `crates/sun/tests/cli_json.rs` is the main regression suite for stable JSON contracts and negative envelopes across all MVP surfaces.

## Single Next Delegation Gap

Delegate one narrow acceptance slice:

Synchronize CLI help and operator-facing command documentation for the newly accepted Phase 1 commands.

Rationale:

- The command implementation and CLI JSON fixtures now cover `topic create`, `session start`, `move`, `delete`, and `metadata set`; repeating that as an implementation slice would duplicate completed work.
- The CLI help text in `crates/sun/src/main.rs` still describes `topic` and `session` as parse-only with persistence not implemented, which conflicts with the accepted lifecycle envelopes and would mislead operators or downstream agents.
- External Super Search validation is older than the Phase 1 CLI completion, but it is optional and broader than the smallest acceptance gap; refresh it after help/docs stop advertising stale command state.
- A dedicated status/inspect round-trip for move/delete/metadata provenance would be useful follow-up coverage, but the immediate correctness issue is that the command surface now says the wrong thing about accepted commands.

Acceptance should be limited to help/docs synchronization and focused verification:

- update `sun --help` command descriptions so topic/session are described as fixture-backed Phase 1 lifecycle commands, not parse-only placeholders.
- ensure command docs mention the accepted fixture flags and stable JSON envelopes for topic/session and structural mutations.
- add or update a focused help/docs assertion if the existing test style has a nearby command-help fixture; otherwise run `sun --help` plus `git diff --check`.

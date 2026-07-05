# Sunlight MVP Readiness Audit - 2026-07-05

## Scope

This audit compares the current implementation against the MVP phases in `docs/sunlight_consolidated_architecture_v0_3.md`, using the validation plan, smoke scripts, CLI JSON tests, and `sunlight-core` module surface as evidence. It is intentionally scoped to readiness and delegation; it does not propose broad implementation changes.

## Executive Summary

Sunlight has a strong fixture-backed MVP spine through Phase 6: repository init and policy, native artifact reads and writes, Phase 1 topic/session lifecycle, Phase 1 structural mutation commands, deterministic resolver conflicts, projection materialization and integrity, execution records and output promotion, checkpoint creation, Git export planning/execution, operator status/inspect, compatibility import, and optional external validation against a temporary Super Search clone.

The prior Phase 1 lifecycle, command-surface, help/docs, and structural provenance gaps are now closed for fixture CLI acceptance: `topic create`, `session start`, `move`, `delete`, and `metadata set` return stable JSON envelopes with topic/session IDs, session generation advancement, operation/revision IDs, precondition failure behavior, tombstone/path binding details, metadata classification changes, and status/inspect provenance. The CLI help and operator docs describe those accepted fixture-backed commands, the full smoke baseline is current after `684f6ba`, and optional Super Search validation has been refreshed after the same accepted work.

Conclusion: for the current fixture/local acceptance target described by the architecture, the MVP is verifiably covered. No concrete blocking gap remains in this target. The next management action should be closure/release summarization rather than another implementation delegation slice.

## Phase Readiness Matrix

| MVP phase | Current coverage | Verification evidence | Readiness note |
| --- | --- | --- | --- |
| Phase 0: schemas, hashing, path policy, operation format, tree identity, session generation, `.sunlight` policy, projection spike | Core modules cover records, identity, repository init, policy, operation transaction records, `TreeIdentity::SingleRepoTree`, projection strategy planning, manifests, root binding, and quarantine metadata. | `crates/sunlight-core/src/records.rs`, `identity.rs`, `repository.rs`, `policy.rs`, `artifacts.rs`, `projection.rs`; `scripts/projection-strategy-smoke.ps1`; `scripts/smoke-suite.ps1`; `crates/sun/tests/cli_json.rs` projection/policy/init tests. | Functionally covered for the local fixture MVP and real temp repo init. |
| Phase 1: native artifact IO vertical slice | `sun init`, `topic create`, `session start`, `read`, `list`, `search`, `patch`, `write`, `move`, `delete`, `metadata set`, status, inspect, authored context, read-after-write generation, structural mutation provenance, and precondition failures are covered for fixture CLI acceptance. | `crates/sun/tests/cli_json.rs` tests for topic/session lifecycle, read/list/search, patch/write, move/delete/metadata, stale write/move preconditions, status/inspect provenance, structural mutation round trips, and global help text; `crates/sunlight-core/src/artifacts.rs`; `crates/sun/src/main.rs`; `docs/sunlight_native_io_phase1_spec_v0_1.md`; help/operator docs sync in `5341480`; structural inspect provenance coverage in `684f6ba`. Latest aggregate baseline in `docs/development_manager_scratchpad.md`: after `684f6ba`, default `scripts/smoke-suite.ps1` passed via the Windows-native fallback with 170 CLI tests, 143 core tests, validation smoke, projection strategy smoke, and MVP smoke. | Covered for fixture CLI acceptance. No remaining Phase 1 blocking gap for the current local target. |
| Phase 2: resolver and conflicts | Deterministic view resolution, reversed frontier stability, independent-file composition, same-artifact conflict objects, staleness, and conflict visibility through status/inspect are covered. | `crates/sunlight-core/src/resolver.rs`; `docs/sunlight_resolver_conflict_fixtures_v0_1.md`; `crates/sun/tests/cli_json.rs` resolver/conflict/staleness tests; `scripts/validation-smoke.ps1`. | Covered for same-repo fixture acceptance. |
| Phase 3: execution projections | `sun run`, projection materialization, execution records, integrity rejection before execution, failed execution records, generated output promotion, and status/inspect exposure are covered. Projection cache/integrity/quarantine lifecycle has extensive focused tests. | `crates/sunlight-core/src/execution.rs`; `projection.rs`; `docs/sunlight_execution_projection_v0_1.md`; `crates/sun/tests/cli_json.rs` run, promote-output, projection integrity, manifest, and quarantine tests; `scripts/mvp-smoke.ps1`. | Covered for fixture execution and local projection safety. |
| Phase 4: checkpoints and Git export | Conflict-free checkpoint creation, conflict/stale rejection, policy check/export explain, write planning, local Git commit/ref export, export map persistence, dirty worktree isolation, and failure envelopes are covered. | `crates/sunlight-core/src/checkpoint.rs`; `git_export.rs`; `docs/sunlight_checkpoint_git_export_v0_1.md`; `docs/sunlight_git_export_writer_v0_1.md`; `crates/sun/tests/cli_json.rs` checkpoint/git export tests; `scripts/mvp-smoke.ps1`. | Covered for the single-checkpoint local Git export MVP. |
| Phase 5: operator ergonomics | Status/inspect surfaces exist for repository, session, checkpoint, export map, Git refs, projections, compatibility imports, executions, artifacts, operations, and accepted structural mutation provenance. Policy explain provides operator-readable validation bodies. | `docs/sunlight_cli_status_inspect_v0_1.md`; `docs/sunlight_operator_status_matrix_v0_1.md`; broad `status_*` and `inspect_*` tests in `crates/sun/tests/cli_json.rs`; structural mutation inspect provenance coverage in `684f6ba`. | Covered for local MVP operations. |
| Phase 6: compatibility import | Compatibility projection, diff classification, import planning, operation/revision/generation provenance, rename/delete/metadata/multi-candidate import, path policy, ignored-path/cache/secret/generated/conflicted failure gates, working-tree isolation, and projection-only checkpoint/export boundaries are covered. | `crates/sunlight-core/src/compat_import.rs`; `docs/sunlight_compatibility_import_v0_1.md`; `docs/sunlight_validation_repo_plan_v0_1.md`; `crates/sun/tests/cli_json.rs` compat tests; `scripts/external-validation-super-search.ps1`. | Covered for fixture compatibility and optional external validation boundaries. |

## Cross-Phase Verification Already Proving MVP Spine

- `scripts/smoke-suite.ps1` is the aggregate gate: format, check, tests, validation smoke, projection strategy smoke, and MVP smoke.
- Latest recorded full baseline: after `684f6ba`, default `scripts/smoke-suite.ps1` passed via the Windows-native fallback with 170 CLI tests, 143 core tests, validation smoke, projection strategy smoke, and MVP smoke.
- `scripts/validation-smoke.ps1` covers real temp repo `sun init`, fixture artifact IO, resolver conflict, execution projection, checkpoint, policy, compatibility projection/import, and Git export write-plan envelopes.
- `scripts/mvp-smoke.ps1` runs the end-to-end path from view resolve through filesystem projection, execution, checkpoint, and real local Git export into a temporary repo.
- Latest recorded optional external validation: after `684f6ba`, `scripts/external-validation-super-search.ps1` passed against Super Search, covering target `mix test`, `bun run test`, temp-clone `sun init`, fixture compat project/diff/status/inspect, happy-path and generated-failure compat import, and fixture Git export.
- `crates/sun/tests/cli_json.rs` is the main regression suite for stable JSON contracts and negative envelopes across all MVP surfaces.

## Blocking Gap Decision

No concrete blocking gap remains for the current fixture/local acceptance target.

Rationale:

- The architecture MVP objective is to prove native artifact IO, topic-owned operations, deterministic composition, execution evidence, checkpoint/export, explicit `.sunlight` policy, operator visibility, compatibility import, and measured projection behavior in a local single-repo target.
- The latest accepted work closes the previously named residual gaps: help/docs sync landed in `5341480`, structural mutation status/inspect provenance coverage landed in `684f6ba`, default smoke passed after `684f6ba`, and optional external Super Search validation also passed after `684f6ba`.
- Remaining open architecture/product questions, such as first target agent integration, broader validation repos, projection platform priority, hosted collaboration, dashboard polish, and cross-repo timing, are post-MVP product decisions or expansion targets. They are not blockers for the current fixture/local acceptance target.

Recommended next management step: prepare a concise MVP readiness/release summary and stop creating implementation slices unless a new target beyond fixture/local acceptance is explicitly chosen.

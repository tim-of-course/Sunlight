# Sunlight MVP Readiness Audit - 2026-07-05

## Scope

This audit compares the current implementation against the MVP phases in `docs/sunlight_consolidated_architecture_v0_3.md`, using the validation plan, smoke scripts, CLI JSON tests, and `sunlight-core` module surface as evidence. It is intentionally scoped to readiness and delegation; it does not propose broad implementation changes.

## Executive Summary

Sunlight has a strong fixture-backed MVP spine through Phase 6: repository init and policy, native artifact reads and writes, deterministic resolver conflicts, projection materialization and integrity, execution records and output promotion, checkpoint creation, Git export planning/execution, operator status/inspect, compatibility import, and optional external validation against a temporary Super Search clone.

The main remaining acceptance gap is narrower than a full phase: complete the Phase 1 native authoring lifecycle CLI for `topic create`, `session start`, and structural mutation commands (`move`, `delete`, `metadata set`) so an MVP agent can start from a fresh Sunlight repo and perform the whole native authoring path through stable commands rather than fixture session IDs.

## Phase Readiness Matrix

| MVP phase | Current coverage | Verification evidence | Readiness note |
| --- | --- | --- | --- |
| Phase 0: schemas, hashing, path policy, operation format, tree identity, session generation, `.sunlight` policy, projection spike | Core modules cover records, identity, repository init, policy, operation transaction records, `TreeIdentity::SingleRepoTree`, projection strategy planning, manifests, root binding, and quarantine metadata. | `crates/sunlight-core/src/records.rs`, `identity.rs`, `repository.rs`, `policy.rs`, `artifacts.rs`, `projection.rs`; `scripts/projection-strategy-smoke.ps1`; `scripts/smoke-suite.ps1`; `crates/sun/tests/cli_json.rs` projection/policy/init tests. | Functionally covered for the local fixture MVP and real temp repo init. |
| Phase 1: native artifact IO vertical slice | `sun init`, `read`, `list`, `search`, `patch`, `write`, status, inspect, authored context, read-after-write generation, and precondition failures are covered. Topic/session record helpers exist in core. | `crates/sun/tests/cli_json.rs` tests for init, read/list/search, patch/write, stale writes, status/inspect provenance; `crates/sunlight-core/src/artifacts.rs`; `topics.rs`; `docs/sunlight_native_io_phase1_spec_v0_1.md`. | Partially covered. `topic create` and `session start` are parsed but return unimplemented errors, and structural mutation commands are not CLI-complete. |
| Phase 2: resolver and conflicts | Deterministic view resolution, reversed frontier stability, independent-file composition, same-artifact conflict objects, staleness, and conflict visibility through status/inspect are covered. | `crates/sunlight-core/src/resolver.rs`; `docs/sunlight_resolver_conflict_fixtures_v0_1.md`; `crates/sun/tests/cli_json.rs` resolver/conflict/staleness tests; `scripts/validation-smoke.ps1`. | Covered for same-repo fixture acceptance. |
| Phase 3: execution projections | `sun run`, projection materialization, execution records, integrity rejection before execution, failed execution records, generated output promotion, and status/inspect exposure are covered. Projection cache/integrity/quarantine lifecycle has extensive focused tests. | `crates/sunlight-core/src/execution.rs`; `projection.rs`; `docs/sunlight_execution_projection_v0_1.md`; `crates/sun/tests/cli_json.rs` run, promote-output, projection integrity, manifest, and quarantine tests; `scripts/mvp-smoke.ps1`. | Covered for fixture execution and local projection safety. |
| Phase 4: checkpoints and Git export | Conflict-free checkpoint creation, conflict/stale rejection, policy check/export explain, write planning, local Git commit/ref export, export map persistence, dirty worktree isolation, and failure envelopes are covered. | `crates/sunlight-core/src/checkpoint.rs`; `git_export.rs`; `docs/sunlight_checkpoint_git_export_v0_1.md`; `docs/sunlight_git_export_writer_v0_1.md`; `crates/sun/tests/cli_json.rs` checkpoint/git export tests; `scripts/mvp-smoke.ps1`. | Covered for the single-checkpoint local Git export MVP. |
| Phase 5: operator ergonomics | Status/inspect surfaces exist for repository, session, checkpoint, export map, Git refs, projections, compatibility imports, executions, artifacts, and operations. Policy explain provides operator-readable validation bodies. | `docs/sunlight_cli_status_inspect_v0_1.md`; `docs/sunlight_operator_status_matrix_v0_1.md`; broad `status_*` and `inspect_*` tests in `crates/sun/tests/cli_json.rs`. | Covered enough for local MVP operations; polish can continue after Phase 1 lifecycle closure. |
| Phase 6: compatibility import | Compatibility projection, diff classification, import planning, operation/revision/generation provenance, rename/delete/metadata/multi-candidate import, path policy, ignored-path/cache/secret/generated/conflicted failure gates, working-tree isolation, and projection-only checkpoint/export boundaries are covered. | `crates/sunlight-core/src/compat_import.rs`; `docs/sunlight_compatibility_import_v0_1.md`; `docs/sunlight_validation_repo_plan_v0_1.md`; `crates/sun/tests/cli_json.rs` compat tests; `scripts/external-validation-super-search.ps1`. | Covered for fixture compatibility and optional external validation boundaries. |

## Cross-Phase Verification Already Proving MVP Spine

- `scripts/smoke-suite.ps1` is the aggregate gate: format, check, tests, validation smoke, projection strategy smoke, and MVP smoke.
- `scripts/validation-smoke.ps1` covers real temp repo `sun init`, fixture artifact IO, resolver conflict, execution projection, checkpoint, policy, compatibility projection/import, and Git export write-plan envelopes.
- `scripts/mvp-smoke.ps1` runs the end-to-end path from view resolve through filesystem projection, execution, checkpoint, and real local Git export into a temporary repo.
- `scripts/external-validation-super-search.ps1` optionally verifies a real local Super Search baseline, temp clone init, fixture compatibility projection/import, atomic generated-output failure, and real local Git export against the disposable clone.
- `crates/sun/tests/cli_json.rs` is the main regression suite for stable JSON contracts and negative envelopes across all MVP surfaces.

## Single Next Delegation Gap

Delegate one narrow Phase 1 acceptance slice:

Implement fixture-backed CLI acceptance for native lifecycle and structural mutations:

- `sun topic create <slug> --display-name <name> --fixture basic-app --json`
- `sun session start --topic <topic> --view <view-selector> --actor <actor-id> --fixture basic-app --json`
- `sun move`, `sun delete`, and `sun metadata set` with the precondition and provenance shapes already specified in `docs/sunlight_native_io_phase1_spec_v0_1.md`.

Acceptance should be limited to stable JSON envelopes, core record reuse where available, and focused tests mirroring the existing `cli_json.rs` style:

- topic/session response includes topic ID, session ID, resolved view ID, session generation ID, pinned refresh policy, write topic, and capabilities.
- move preserves artifact identity and leaves inspectable path history/tombstone state.
- delete tombstones the path binding and leaves operation provenance inspectable.
- metadata set records classification without changing content bytes.
- status/inspect link artifact -> operation -> topic -> session -> revision for at least one structural mutation.

This gap is the best next slice because the rest of the MVP spine already has strong fixture and smoke evidence, while these commands are explicitly called out by the Phase 1 spec and are currently not command-complete.

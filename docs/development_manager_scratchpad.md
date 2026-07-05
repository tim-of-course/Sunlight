# Sunlight Development Manager Scratchpad

This is the working coordination file for project management. Keep it concise and edit it down during every heartbeat.

## Heartbeat Rule

Every heartbeat must:

1. Update this scratchpad with current project state.
2. Run `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/check-manager-scratchpad.ps1`.
3. Review WSL Codex thread progress.
4. Commit verified progress checkpoints when appropriate.
5. Start the next one or two project-wide slices if capacity is available.

The heartbeat prompt must stay generic to the whole project and point here, not to any one task.

## Management Constraints

- Delegate implementation slices to WSL2 Codex CLI using the `wsl-codex-cli` workflow.
- Keep desktop Codex as coordinator, reviewer, and tester.
- Avoid direct implementation edits from this thread except for testing, review, and management artifacts.
- If blocked, work around it or start another valuable slice unless the project is verifiably complete.
- Commit progress checkpoints as coherent increments.

## Product North Star

Sunlight is a native, event-sourced, multi-version source artifact database. Git and filesystems are projections and compatibility layers, not the coordination substrate.

## MVP Shape

- Phase 0: schemas, canonical hashing, path policy, operation format, tree identity, session generation, `.sunlight` policy, view identity, projection spike.
- Phase 1: native artifact IO vertical slice: `sun init`, topics, sessions, `read/list/search`, `patch/write/move/delete`, revisions, authored context, status.
- Phase 2: resolver and conflicts: deterministic views, same-file conflicts, non-commutative same-artifact write detection, conflict objects.
- Phase 3: execution projections: sandboxed `sun run`, execution records, output promotion.
- Phase 4: checkpoints and Git export.
- Phase 5: operator ergonomics.
- Phase 6: compatibility import.

## Current State

- Date started as development manager: 2026-07-03.
- Architecture source: `docs/sunlight_consolidated_architecture_v0_3.md`.
- Integrated repo now has a Rust workspace with `sun` CLI and `sunlight-core`.
- WSL Codex readiness: Ubuntu launches, Codex CLI responds, logged in using ChatGPT, bubblewrap available, and Rust/Cargo are available when launched with the helper PATH.
- Verification: latest full pass ran after `b2ae867` and covered the ignored-path Phase 6 gap closure. Default `scripts/smoke-suite.ps1` passed via the Windows-native fallback with 155 CLI tests, 143 core tests, validation smoke, projection strategy smoke, and MVP smoke. Optional Super Search validation passed after `2faf5dd`, covering target `mix test`, `bun run test`, temp-clone `sun init`, fixture compat project/diff/status/inspect, happy-path and generated-failure compat import, and fixture Git export.

## Active Work

- WSL base clone: `/home/timothycard/code/Sunlight-2`.
- Active slice `projection-real-fs-capability-probe`: delegated next. Probe WSL/Linux reflink, read-only hardlink, and overlay/copy-up capabilities in a scoped local harness without claiming portability beyond observed host behavior.
- Historical completed milestone range: bootstrap through policy, artifact IO, resolver, execution, checkpoints, projection, Git export, validation smoke, operator status, projection manifest/integrity/quarantine, and external Super Search validation are already integrated. Full detail remains in Git history; this scratchpad now keeps only the current management lane and recent compatibility work.
- Key historical checkpoints: initial Rust workspace and core contracts; native artifact/session/mutation CLI fixtures; resolver and conflict foundation; execution/checkpoint/Git export foundations; projection materialization/manifest/root-binding/integrity hardening; policy check/explain commands and docs; aggregate smoke and optional external Super Search validations.
- Completed slice `compat-project-diff-fixture`: integrated as `07c695a`; formatting checkpoint `b3a0f8b`.
- Completed slice `compat-status-inspect-visibility`: integrated as `6c1063a`; formatting checkpoint `d8b8f8a`.
- Completed slice `focused-smoke-wrapper-crlf-hardening`: integrated as `3ff62d1`.
- Completed slice `cli-compat-import-multiple-candidates`: integrated as `ff0caf6`.
- Completed slice `validation-plan-cli-reconcile`: integrated as `16d167e`; manager tracking checkpoint `3cce17c`.
- Completed slice `policy-check-export-cli-fixture`: integrated as `41fc082`; WSL implementation was `aff2f61` plus manager-side rustfmt review fix.
- Completed slice `policy-check-commit-cli-fixture`: integrated as `7b5b839`; WSL implementation was `14edbd2`.
- Completed slice `policy-command-smoke-coverage`: integrated as `12af17d`; WSL implementation was `d2c7071`.
- Completed verification slice `aggregate-smoke-suite-refresh`: default `scripts/smoke-suite.ps1` passed after `5aa6275`.
- Completed slice `policy-failure-operator-docs`: integrated as `297e22d`; WSL implementation was `b646648`.
- Completed slice `policy-command-docs-sweep`: integrated as `c6376d8`; WSL implementation was `3624b10`.
- Completed slice `policy-explain-cli-fixture`: integrated as `e4ec794`; WSL implementation was `6f03921` plus manager-side Windows rustfmt amendment.
- Completed slice `policy-explain-smoke-coverage`: integrated as `f4f0f1f`; WSL implementation was `cc33f29`.
- Completed verification slice `aggregate-smoke-suite-refresh-after-policy-explain`: default `scripts/smoke-suite.ps1` passed after `8f4212d`.
- Completed slice `git-ref-status-inspect-fixture`: integrated as `d862c5c`; WSL implementation was `950065c` plus manager-side Windows rustfmt amendment.
- Completed slice `git-ref-smoke-docs-coverage`: integrated as `297843e`; WSL implementation was `ae83d0f`.
- Completed verification slice `aggregate-smoke-suite-refresh-after-git-ref-smoke`: default `scripts/smoke-suite.ps1` passed after `c6a58c3`.
- Completed slice `export-selector-alias-fixture-coverage`: integrated as `2e5e552`; WSL implementation was `d1fa2c5`.
- Completed verification slice `aggregate-smoke-suite-refresh-after-export-alias`: default `scripts/smoke-suite.ps1` passed after `46c20d6`.
- Completed slice `operator-status-round-trip-coverage`: integrated as `6322652`; WSL implementation was imported manager-side after the WSL sandbox blocked committing.
- Completed verification slice `aggregate-smoke-suite-refresh-after-round-trip`: default `scripts/smoke-suite.ps1` passed after `17780c1`.
- Completed verification slice `external-validation-super-search-refresh`: optional `scripts/external-validation-super-search.ps1` passed after `e02debe` with Super Search `mix test`, `bun run test`, temp-clone `sun init`, fixture compat import, and fixture-backed Git export.
- Completed slice `repository-inspect-selector`: integrated as `175c448`; WSL implementation was imported manager-side after the WSL sandbox blocked committing.
- Completed verification slice `aggregate-smoke-suite-refresh-after-repository-inspect`: default `scripts/smoke-suite.ps1` passed after `eb6255a`.
- Completed slice `artifact-export-trace-visibility`: integrated as `6665ba7`; WSL implementation was imported manager-side after the WSL sandbox blocked committing.
- Completed verification slice `aggregate-smoke-suite-refresh-after-artifact-export-trace`: default `scripts/smoke-suite.ps1` passed after `abdac17`.
- Completed slice `view-conflict-status-inspect`: integrated as `483fac8`; WSL implementation was imported manager-side after the WSL sandbox blocked committing.
- Completed verification slice `aggregate-smoke-suite-refresh-after-view-conflict`: default `scripts/smoke-suite.ps1` passed after `ecbe476`.
- Completed slice `compat-session-status-import-visibility`: integrated as `3b9ee2d`; WSL implementation was imported manager-side after the WSL sandbox blocked committing.
- Completed verification slice `aggregate-smoke-suite-refresh-after-compat-session-visibility`: default `scripts/smoke-suite.ps1` passed after `5b1d344`.
- Completed slice `compat-artifact-import-provenance`: integrated as `24e3dbf`; WSL implementation was imported manager-side after the WSL sandbox blocked committing.
- Completed verification slice `aggregate-smoke-suite-refresh-after-compat-artifact-provenance`: default `scripts/smoke-suite.ps1` passed after `055e73f`.
- Completed slice `compat-projection-last-import-visibility`: integrated as `301477b`; WSL implementation was imported manager-side after the WSL sandbox blocked committing.
- Completed verification slice `aggregate-smoke-suite-refresh-after-compat-projection-import`: default `scripts/smoke-suite.ps1` passed after `6be9da4`.
- Completed slice `compat-import-atomic-failure-cli-coverage`: integrated as `dff513f`; WSL implementation was imported manager-side after the WSL sandbox blocked committing.
- Completed verification slice `aggregate-smoke-suite-refresh-after-compat-import-atomic-failure`: default `scripts/smoke-suite.ps1` passed after `b16a1f2`.
- Completed slice `compat-import-path-policy-cli-coverage`: integrated as `0c4894d`; WSL implementation was imported manager-side after the WSL sandbox blocked committing.
- Completed verification slice `aggregate-smoke-suite-refresh-after-compat-import-path-policy`: default `scripts/smoke-suite.ps1` passed after `fd33835`.
- Completed slice `compat-import-conflicted-delta-cli-coverage`: integrated as `6052cd8`; WSL implementation was imported manager-side after the WSL sandbox blocked committing.
- Completed verification slice `aggregate-smoke-suite-refresh-after-compat-import-conflicted-delta`: default `scripts/smoke-suite.ps1` passed after `09a56d6`.
- Completed slice `compat-import-generated-policy-cli-coverage`: integrated as `e17fa71`; WSL implementation was imported manager-side after the WSL sandbox blocked committing.
- Completed verification slice `aggregate-smoke-suite-refresh-after-compat-import-generated-policy`: default `scripts/smoke-suite.ps1` passed after `36e49b8`.
- Completed slice `compat-import-ambiguous-rename-cli-coverage`: integrated as `7d1c796`; WSL implementation was imported manager-side after the WSL sandbox blocked committing.
- Completed verification slice `aggregate-smoke-suite-refresh-after-compat-import-ambiguous-rename`: default `scripts/smoke-suite.ps1` passed after `973da56`.
- Completed slice `compat-import-delete-success-cli-coverage`: integrated as `3584d7b`; WSL implementation was imported manager-side after the WSL sandbox blocked committing.
- Completed verification slice `aggregate-smoke-suite-refresh-after-compat-import-delete-success`: default `scripts/smoke-suite.ps1` passed after `a371669`.
- Completed slice `compat-import-rename-success-cli-coverage`: integrated as `4cd2dcf`; WSL implementation was imported manager-side after the WSL sandbox blocked committing, with Windows `cargo fmt` and focused CLI verification.
- Completed verification slice `aggregate-smoke-suite-refresh-after-compat-import-rename-success`: default `scripts/smoke-suite.ps1` passed after `d8e0925`.
- Completed slice `compat-import-metadata-success-cli-coverage`: integrated as `5333445`; WSL implementation was imported manager-side after WSL `cargo-fmt` was unavailable, with Windows `cargo fmt` and focused CLI verification.
- Completed verification slice `aggregate-smoke-suite-refresh-after-compat-import-metadata-success`: default `scripts/smoke-suite.ps1` passed after `f970f10`.
- Completed slice `compat-import-rename-plus-edit-cli-coverage`: integrated as `815b49a`; WSL implementation was imported manager-side after WSL `cargo-fmt` was unavailable, with Windows `cargo fmt` and focused CLI verification.
- Completed verification slice `aggregate-smoke-suite-refresh-after-compat-import-rename-plus-edit`: default `scripts/smoke-suite.ps1` passed after `d72b79d`.
- Completed slice `compat-import-docs-acceptance-sync`: integrated as `d6162bd`; WSL docs-only implementation was imported manager-side after prompt quoting caused a nonzero helper exit, with targeted docs verification passing on Windows.
- Completed verification slice `external-validation-super-search-refresh-after-compat-import-hardening`: optional `scripts/external-validation-super-search.ps1` passed after `0bb5ae6` with Super Search `mix test`, `bun run test`, temp-clone `sun init`, fixture compat import, and fixture-backed Git export.
- Completed slice `compat-import-stale-precondition-cli-coverage`: integrated as `a4a7718`; WSL implementation was imported manager-side after WSL `cargo-fmt` was unavailable, with Windows `cargo fmt` and focused CLI verification.
- Completed verification slice `aggregate-smoke-suite-refresh-after-compat-import-stale-preconditions`: default `scripts/smoke-suite.ps1` passed after `61c9240`.
- Completed slice `compat-projection-quarantine-retention-cli-coverage`: integrated as `7e2d64e`; WSL implementation was imported manager-side after the WSL Git sandbox blocked committing, with Windows `cargo fmt --check` and focused CLI verification.
- Completed verification slice `aggregate-smoke-suite-refresh-after-compat-projection-retention`: default `scripts/smoke-suite.ps1` passed after `a6af00b`.
- Completed slice `external-validation-compat-breadth`: integrated as `2faf5dd`; WSL script implementation was imported manager-side after the WSL Git sandbox blocked committing, with Windows parse/whitespace checks and full optional Super Search validation passing.
- Completed slice `compat-working-tree-isolation-cli-coverage`: integrated as `ba43a02`; WSL test implementation was imported manager-side after the WSL Git sandbox blocked committing, with Windows `cargo fmt --check` and focused CLI verification.
- Completed verification slice `aggregate-smoke-suite-refresh-after-compat-working-tree-isolation`: default `scripts/smoke-suite.ps1` passed after `f96aedb`.
- Completed slice `compat-projection-only-checkpoint-boundary`: integrated as `7b80d72`; WSL test implementation was imported manager-side after the WSL Git sandbox blocked committing, with Windows `cargo fmt --check` and focused CLI verification.
- Completed verification slice `aggregate-smoke-suite-refresh-after-projection-only-boundary`: default `scripts/smoke-suite.ps1` passed after `ef848cd`.
- Completed slice `compat-status-docs-reconcile`: integrated as `6fb413d`; WSL docs implementation was imported manager-side after the WSL Git sandbox blocked committing, with drift search and `git diff --check` passing.
- Completed slice `phase6-acceptance-audit`: integrated as `224d567`; WSL audit found the distinct ignored-path fixture coverage gap, with focused compat import/status/inspect/checkpoint/export tests and `git diff --check` passing under `/tmp` Zig caches.
- Completed slice `compat-ignored-path-fixture-coverage`: integrated as `b2ae867`; WSL implementation was imported manager-side after the WSL Git sandbox blocked committing, with Windows `cargo fmt --check`, focused compat/status/inspect/working-tree tests, and default smoke passing.
- Completed verification slice `aggregate-smoke-suite-refresh-after-ignored-path-coverage`: default `scripts/smoke-suite.ps1` passed after `b2ae867`.
- Completed slice `projection-platform-spike-metrics`: integrated as `6342747`; WSL scripts/docs implementation was imported manager-side after the WSL Git sandbox blocked committing, with WSL shell smoke/focused projection tests and Windows projection-strategy smoke passing.

## Candidate Next Slices

- Next useful slices after the active one: turn any real-filesystem probe result into accepted/deferred strategy decisions, then continue execution projection/cache hardening.

## Decisions

- Scratchpad length budget is enforced at 20,000 characters.
- Direct implementation edits should be delegated to WSL Codex agents.
- WSL `rustfmt` is currently unavailable, so manager-side Windows `cargo fmt` remains the formatting gate after WSL imports.
- Recent WSL slice clones may stay dirty after manager-side import because the WSL Codex sandbox cannot create `.git/index.lock`; treat the Windows commits as source of truth.

## Open Questions

- Whether the next external validation step should remain an optional local harness or become a broader non-default suite is still open.
- Projection platform target still starts on WSL/Linux unless product priority changes.

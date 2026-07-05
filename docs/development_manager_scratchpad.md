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
- Verification: latest full pass ran after `de5c5cb` and covered Phase 1 structural mutation CLI acceptance. Default `scripts/smoke-suite.ps1` passed via the Windows-native fallback with 168 CLI tests, 143 core tests, validation smoke, projection strategy smoke, and MVP smoke. After `5341480`, focused help/docs verification passed with `cargo fmt --check`, `cargo test -p sun --test cli_json global_help_describes_phase1_fixture_lifecycle_commands -- --nocapture`, `cargo run -p sun -- --help`, `git diff --check`, and a stale-wording search. Optional Super Search validation passed after `44775b8`.

## Active Work

- WSL base clone: `/home/timothycard/code/Sunlight-2`.
- Active slice `phase1-structural-status-inspect-roundtrip`: ready next. Add focused status/inspect round-trip coverage for accepted Phase 1 structural mutation provenance.
- Historical completed milestone range: bootstrap through policy, artifact IO, resolver, execution, checkpoints, projection, Git export, validation smoke, operator status, projection manifest/integrity/quarantine, and external Super Search validation are already integrated. Full detail remains in Git history; this scratchpad now keeps only the current management lane and recent compatibility work.
- Key historical checkpoints: initial Rust workspace and core contracts; native artifact/session/mutation CLI fixtures; resolver and conflict foundation; execution/checkpoint/Git export foundations; projection materialization/manifest/root-binding/integrity hardening; policy check/explain commands and docs; aggregate smoke and optional external Super Search validations.
- Earlier integrated work through `6fb413d` includes policy command fixtures/docs, operator status round trips, repository/artifact/view/status visibility, external Super Search validation, compatibility import breadth and variants, projection-only checkpoint boundaries, and compatibility status docs reconcile. Full slice detail is in Git history.
- Completed slice `phase6-acceptance-audit`: integrated as `224d567`; WSL audit found the distinct ignored-path fixture coverage gap, with focused compat import/status/inspect/checkpoint/export tests and `git diff --check` passing under `/tmp` Zig caches.
- Completed slice `compat-ignored-path-fixture-coverage`: integrated as `b2ae867`; WSL implementation was imported manager-side after the WSL Git sandbox blocked committing, with Windows `cargo fmt --check`, focused compat/status/inspect/working-tree tests, and default smoke passing.
- Completed verification slice `aggregate-smoke-suite-refresh-after-ignored-path-coverage`: default `scripts/smoke-suite.ps1` passed after `b2ae867`.
- Completed slice `projection-platform-spike-metrics`: integrated as `6342747`; WSL scripts/docs implementation was imported manager-side after the WSL Git sandbox blocked committing, with WSL shell smoke/focused projection tests and Windows projection-strategy smoke passing.
- Completed slice `projection-real-fs-capability-probe`: integrated as `0a35e2f`; WSL Bash harness/docs implementation was imported manager-side after the WSL Git sandbox blocked committing. Tempdir probe observed `tmpfs`, unsupported reflink, unsafe read-only hardlink after owner chmod, unavailable unprivileged overlay/copy-up, and kept copy as accepted fallback. WSL shell smoke/projection tests, CRLF-stripped Bash syntax check, Windows projection-strategy smoke, and `git diff --check` passed.
- Completed slice `projection-non-temp-fs-capability-probe`: integrated as `80db1ef`; WSL Bash harness/docs implementation was imported manager-side after the WSL Git sandbox blocked committing. Non-temp WSL home clone probe observed `ext2/ext3`; reflink stayed unsupported, read-only hardlink stayed unsafe after owner chmod, overlay/copy-up stayed unavailable, and copy stayed the accepted fallback.
- Completed verification slice `aggregate-smoke-suite-refresh-after-non-temp-probe`: default `scripts/smoke-suite.ps1` passed after `80db1ef`.
- Completed slice `execution-projection-cache-hardening`: integrated as `80a356f`; WSL implementation was imported manager-side after the WSL Git sandbox blocked committing, with Windows `cargo fmt --check`, focused projection/execution/CLI tests, and default smoke passing.
- Completed verification slice `aggregate-smoke-suite-refresh-after-execution-cache-boundary`: default `scripts/smoke-suite.ps1` passed after `80a356f`.
- Completed slice `projection-cache-cleanup-invalidation`: integrated as `67e40d6`; WSL test implementation was imported manager-side after the WSL Git sandbox blocked committing, with Windows `cargo fmt --check`, `git diff --check`, and focused quarantine cleanup CLI verification.
- Completed verification slice `aggregate-smoke-suite-refresh-after-projection-cache-cleanup`: default `scripts/smoke-suite.ps1` passed after `67e40d6`.
- Completed slice `projection-cache-reuse-lifecycle-audit`: integrated as `8ddbb6f`; WSL test implementation was imported manager-side after the WSL Git sandbox blocked committing, with WSL core projection/execution and CLI projection filters passing, plus Windows `cargo fmt --check`, `git diff --check`, and focused quarantine cleanup CLI verification.
- Completed verification slice `external-validation-post-cache-hardening`: integrated harness sync as `44775b8`; initial target-side preflight flake passed on narrow rerun, then full optional Super Search validation passed after syncing the canonical compatibility fixture count to 13.
- Completed slice `mvp-readiness-audit`: integrated as `168783b`; WSL docs audit identified Phase 1 native lifecycle and structural mutation CLI acceptance as the next narrow gap, with `git diff --check` passing.
- Completed slice `phase1-native-lifecycle-acceptance`: integrated as `e60e1ac`; WSL implementation was imported manager-side after the WSL Git sandbox blocked committing, with Windows `cargo fmt --check`, focused topics/session CLI tests, and default smoke passing.
- Completed slice `phase1-structural-mutation-acceptance`: integrated as `de5c5cb`; WSL implementation was imported manager-side after the WSL Git sandbox blocked committing, with Windows `cargo fmt --check`, focused artifact/move/delete/metadata tests, and default smoke passing.
- Completed slice `post-phase1-readiness-refresh`: integrated as `e46e9ad`; WSL docs refresh updated MVP readiness after Phase 1 acceptance and identified stale CLI help/operator docs as the next narrow gap, with `git diff --check` and stale-phrase search passing.
- Completed slice `phase1-cli-help-docs-sync`: integrated as `5341480`; WSL help/docs implementation was imported manager-side after timeout/Git lock constraints, with Windows format, focused global help test, actual `sun --help`, `git diff --check`, and stale-wording search passing. MVP readiness now points to structural status/inspect provenance as the next narrow gap.

## Candidate Next Slices

- Next useful slices after the active one: refresh optional external validation, then run a full smoke-suite baseline if the status/inspect round trip lands cleanly.

## Decisions

- Scratchpad length budget is enforced at 20,000 characters.
- Direct implementation edits should be delegated to WSL Codex agents.
- WSL `rustfmt` is currently unavailable, so manager-side Windows `cargo fmt` remains the formatting gate after WSL imports.
- Recent WSL slice clones may stay dirty after manager-side import because the WSL Codex sandbox cannot create `.git/index.lock`; treat the Windows commits as source of truth.

## Open Questions

- Whether the next external validation step should remain an optional local harness or become a broader non-default suite is still open.
- Projection platform target still starts on WSL/Linux unless product priority changes.

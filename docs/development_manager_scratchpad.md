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
- Verification: latest integrated pass ran native `scripts/smoke-suite.ps1` after `56e0118` with `cargo fmt --check`, `cargo check`, `cargo test` with 80 Windows CLI tests and 129 core tests, validation smoke, projection strategy smoke, and MVP smoke. WSL verification covered root-mismatch status/inspect with 83 WSL CLI JSON tests and `git diff --check`. Scratchpad guard must run after this edit.

## Active Work

- WSL base clone: `/home/timothycard/code/Sunlight-2`.
- Completed slice `bootstrap-core`: integrated as `7a297c2`; follow-up lockfile/ignore checkpoint `f11e305`.
- Completed slice `implementation-backlog`: integrated as `e4a3cb7`.
- Completed slice `core-identity-hashing`: integrated as `7ba1435`; formatting checkpoint `e6307ee`.
- Completed slice `schema-contracts`: integrated as `d58857d`.
- Completed slice `canonical-records`: integrated as `2b9893e`; formatting checkpoint `22ad52e`.
- Completed slice `native-io-spec`: integrated as `4ca842d`.
- Completed slice `topic-session-skeleton`: integrated as `cef729c`; formatting checkpoint `b646faf`.
- Completed slice `policy-validation-spec`: integrated as `8f5b72d`.
- Completed slice `policy-validator-foundation`: integrated as `ba4e807`; formatting checkpoint `f6ec7ea`.
- Completed slice `artifact-io-fixtures`: integrated as `f26af68`.
- Completed slice `artifact-io-foundation`: integrated as `3f37390`; formatting checkpoint `0248474`.
- Completed slice `operation-transaction-contract`: integrated as `9bce817`.
- Completed slice `cli-envelope-spec`: integrated as `cfc0f69`.
- Completed slice `mutation-foundation`: integrated as `d3b5254`; formatting checkpoint `a9a46b2`.
- Completed slice `cli-envelope-skeleton`: integrated as `3c421fd`; formatting checkpoint `a9a46b2`.
- Completed slice `cli-artifact-fixture`: integrated as `50ba1b1`; formatting checkpoint `168f73b`.
- Completed slice `resolver-conflict-fixtures`: integrated as `ffbc014`.
- Completed slice `cli-mutation-fixture`: integrated as `6df0fa8`; formatting checkpoint `dde1d62`.
- Completed slice `resolver-foundation`: integrated as `37c837a`; formatting checkpoint `dde1d62`.
- Completed slice `cli-status-inspect-fixture`: integrated as `8edfcd8`; formatting checkpoint `aa45ff3`.
- Completed slice `checkpoint-export-contract`: integrated as `cc7e0ad`.
- Completed slice `cli-view-resolve-fixture`: integrated as `1055ace`; formatting checkpoint `7195ac5`.
- Completed slice `execution-projection-contract`: integrated as `310345b`.
- Completed slice `execution-foundation`: integrated as `7489263`; formatting checkpoint `8d57306`.
- Completed slice `compat-import-contract`: integrated as `37c9896`.
- Completed slice `cli-run-fixture`: integrated as `de33d02`; formatting checkpoint `8f0acc0`.
- Completed slice `checkpoint-foundation`: integrated as `b65a7a9`; formatting checkpoint `8f0acc0`.
- Completed slice `cli-checkpoint-fixture`: integrated as `9dd2bc3`; formatting checkpoint `9420e57`.
- Completed slice `projection-foundation`: integrated as `6788680`; formatting checkpoint `9420e57`.
- Completed slice `cli-projection-fixture`: integrated as `7e79f66`; formatting checkpoint `626ca41`.
- Completed slice `git-export-foundation`: integrated as `6bb6727`; formatting checkpoint `626ca41`.
- Completed slice `cli-git-export-fixture`: integrated as `7947b05`; formatting checkpoint `bfa5053`.
- Completed slice `projection-strategy-spike-plan`: integrated as `07302b9`.
- Completed slice `operator-status-matrix`: integrated as `4404fe2`.
- Completed slice `git-export-writer-contract`: integrated as `c1ec72b`.
- Completed slice `compat-import-foundation`: integrated as `77beaa2`; formatting checkpoint `3658140`.
- Completed slice `operator-status-cli-fixture`: integrated as `1b46f67`; formatting checkpoint `3658140`.
- Completed slice `cli-compat-import-fixture`: integrated as `647908f`; formatting checkpoint `62e28a5`.
- Completed slice `git-export-writer-foundation`: integrated as `f7b0723`; formatting checkpoint `62e28a5`.
- Completed slice `cli-git-export-writer-fixture`: integrated as `d913e39`; formatting checkpoint `1ddf6b9`.
- Completed slice `projection-materialization-foundation`: integrated as `b58f308`; formatting checkpoint `1ddf6b9`.
- Completed slice `cli-projection-materialization-fixture`: integrated as `e0b39e9`; formatting checkpoint `02911da`.
- Completed slice `compat-import-status-inspect`: integrated as `af9d5e2`; formatting checkpoint `02911da`.
- Completed slice `git-export-execution-foundation`: integrated as `988b5cf`; formatting checkpoint `22d55f9`.
- Completed slice `validation-repo-plan`: integrated as `fb13cef`.
- Completed slice `cli-git-export-execution-fixture`: integrated as `5347921`; formatting checkpoint `d553789`.
- Completed slice `validation-smoke-script`: integrated as `20a1c13`.
- Completed slice `projection-strategy-smoke-harness`: integrated as `b2c0cd6`; wrapper fix checkpoint `a9a4471`.
- Completed slice `git-export-real-writer-validation`: integrated as `7ac6236`; formatting checkpoint `3680f68`.
- Completed slice `git-export-real-writer-execution`: integrated as `7eb7c2b`; formatting checkpoint `897d45f`.
- Completed slice `projection-filesystem-materialization`: integrated as `058b4a0`; formatting checkpoint `897d45f`.
- Completed slice `cli-real-git-export-writer`: integrated as `ccc47e5`; formatting checkpoint `3eecbd8`.
- Completed slice `smoke-suite-runner`: integrated as `3112952`.
- Completed slice `git-export-index-uniqueness`: integrated as `3513e7f`.
- Completed slice `cli-projection-filesystem-copy`: integrated as `6eb1e21`; formatting checkpoint `d1ed0f9`.
- Completed slice `mvp-end-to-end-smoke`: integrated as `7bb7e69`; native PowerShell smoke fix checkpoint `1ecd029`.
- Completed slice `mvp-projection-root-smoke`: integrated as `5a5c803`.
- Completed slice `operator-local-projection-status`: integrated as `72e0cfa`; formatting checkpoint `580ee9e`.
- Completed slice `operator-projection-status-smoke`: integrated as `36c4923`.
- Completed slice `projection-status-edge-cases`: integrated as `dcd2682`.
- Completed slice `projection-inspect-edge-cases`: integrated as `0ac48a1`.
- Completed slice `projection-status-inspect-docs`: integrated as `b42df81`.
- Completed slice `projection-manifest-contract`: integrated as `2e9adc6`.
- Completed slice `projection-local-root-scan-hardening`: integrated as `3228635`.
- Completed slice `projection-manifest-foundation`: integrated as `a318f22`; formatting checkpoint `a6a99a4`.
- Completed slice `projection-scan-symlink-edge-cases`: integrated as `eddd6e8`.
- Completed slice `projection-manifest-status-fixture`: integrated as `49e9b70`; formatting checkpoint `680891e`; warning cleanup `6c8d5ce`.
- Completed slice `projection-manifest-dirty-fixture`: integrated as `b08dcf6`; formatting checkpoint `9b36b46`.
- Completed slice `projection-manifest-executable-fixture`: integrated as `919e04e`.
- Completed slice `projection-manifest-error-states`: integrated as `a97d7ec`.
- Completed slice `projection-manifest-root-binding-contract`: integrated as `1bad247`.
- Completed slice `projection-manifest-root-binding-persistence`: integrated as `bbbb3ac`; formatting checkpoint `1897fe0`; smoke harness checkpoint `6e0a842`.
- Completed slice `projection-manifest-root-mismatch-status-inspect`: integrated as `10374b2`; formatting checkpoint `56e0118`.
- Active slice `projection-manifest-local-envelope-validation`: WSL clone `/home/timothycard/code/Sunlight-2-projection-manifest-local-envelope-validation-20260704-155030`, branch `dm/projection-manifest-local-envelope-validation`, run artifacts under `C:\tmp\sunlight-manager\20260704-155030`.
- Next heartbeat should inspect active WSL progress, import verified work if finished, run full smoke-suite validation, and keep project-wide projection manifest verification moving.

## Candidate Next Slices

- Projection manifest persisted-envelope docs/status contract cleanup after validation lands.
- Validation smoke CI/usage follow-up remains optional after the smoke suite settles.

## Decisions

- Scratchpad length budget is enforced at 20,000 characters.
- Direct implementation edits should be delegated to WSL Codex agents.
- WSL `rustfmt` is currently unavailable, so manager-side Windows `cargo fmt` remains the formatting gate after WSL imports.

## Open Questions

- First external validation repo and exact test command are not yet selected.
- Projection platform target still starts on WSL/Linux unless product priority changes.

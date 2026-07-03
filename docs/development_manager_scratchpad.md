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
- Verification: latest integrated pass ran `cargo fmt --check`, `cargo test` with 37 tests, `git diff --check`, and scratchpad length guard after mutation and CLI envelope integration.

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
- Active slice `cli-artifact-fixture`: WSL clone `/home/timothycard/code/Sunlight-2-cli-artifact-fixture-20260703-172325`, branch `dm/cli-artifact-fixture`, logs under `C:\tmp\sunlight-manager\20260703-172325`.
- Active slice `resolver-conflict-fixtures`: WSL clone `/home/timothycard/code/Sunlight-2-resolver-conflict-fixtures-20260703-172325`, branch `dm/resolver-conflict-fixtures`, logs under `C:\tmp\sunlight-manager\20260703-172325`.
- Next heartbeat should inspect active slice logs, verify commits, integrate acceptable work, then launch the next one or two slices.

## Candidate First Slices

- Slice D: canonical record envelope and canonicalization helpers.
- Slice E: topic/session data model and command skeleton.
- Slice F: `.sunlight` commit/export policy validator foundation.
- Slice G: artifact store/read/list/search foundation.
- Slice H: operation transaction and patch/write foundation.
- Slice I: CLI JSON envelope, status, and inspect response contract.
- Slice J: resolver conflict fixture plan.

## Decisions

- Scratchpad length budget is enforced at 20,000 characters.
- Direct implementation edits should be delegated to WSL Codex agents.

## Open Questions

- First validation repo and exact test command are not yet selected.
- Git export shape remains undecided.
- Projection platform target likely starts on WSL/Linux unless product priority changes.

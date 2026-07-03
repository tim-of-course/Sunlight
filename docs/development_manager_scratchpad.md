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
- Verification: latest integrated pass ran `cargo fmt --check`, `cargo test` with 11 tests, `git diff --check`, and scratchpad length guard.

## Active Work

- WSL base clone: `/home/timothycard/code/Sunlight-2`.
- Completed slice `bootstrap-core`: integrated as `7a297c2`; follow-up lockfile/ignore checkpoint `f11e305`.
- Completed slice `implementation-backlog`: integrated as `e4a3cb7`.
- Completed slice `core-identity-hashing`: integrated as `7ba1435`; formatting checkpoint `e6307ee`.
- Completed slice `schema-contracts`: integrated as `d58857d`.
- Completed slice `canonical-records`: integrated as `2b9893e`; formatting checkpoint `22ad52e`.
- Completed slice `native-io-spec`: integrated as `4ca842d`.
- Next heartbeat should inspect active slice logs, verify commits, integrate acceptable work, then launch the next one or two slices.

## Candidate First Slices

- Slice D: canonical record envelope and canonicalization helpers.
- Slice E: topic/session data model and command skeleton.
- Slice F: `.sunlight` commit/export policy validation plan or first validator.
- Slice G: artifact store/read/list/search fixture design.

## Decisions

- Scratchpad length budget is enforced at 20,000 characters.
- Direct implementation edits should be delegated to WSL Codex agents.

## Open Questions

- First validation repo and exact test command are not yet selected.
- Git export shape remains undecided.
- Projection platform target likely starts on WSL/Linux unless product priority changes.

# Sunlight Development Manager Scratchpad

Purpose: keep one compact, current coordination record for Sunlight development. This file is the heartbeat handoff point and must stay under 20000 characters.

## Required Scratchpad Test

Run this after every update to this file:

```powershell
$p = "docs/sunlight_development_manager_scratchpad.md"; $n = (Get-Content -Raw -LiteralPath $p).Length; if ($n -gt 20000) { throw "Scratchpad too large: $n characters > 20000" }; "Scratchpad OK: $n characters"
```

## Management Rules

- Use WSL2 Codex CLI threads for implementation slices.
- Keep desktop Codex in the development-manager role: plan, delegate, review, test, commit, and redirect.
- Avoid direct implementation edits from the manager except for testing and review work.
- Commit coherent progress checkpoints as work lands.
- Keep moving until the v0.3 local MVP is complete; if one slice blocks, switch to another useful slice.
- Heartbeat every 55 minutes, generically for the whole project, using this scratchpad as the resume point.
- MVP means a real local tool that can manage production codebases using all features in `docs/sunlight_consolidated_architecture_v0_3.md`, not only a proof of concept.

## Product Goal

Build Sunlight as a local-first native source artifact database. Git and filesystem trees are projections or compatibility layers. The local MVP must cover native artifact IO, durable topics and sessions, deterministic safe view resolution, conflict objects, scalable projections, execution evidence, execution-output promotion, checkpoints, Git export, explicit `.sunlight` policy, operator ergonomics, and compatibility import.

## Current Baseline

- Architecture source: `docs/sunlight_consolidated_architecture_v0_3.md` v0.3.
- Manager setup started: 2026-07-06.
- Windows checkout status at setup: clean.
- WSL Codex readiness at setup: passed; Codex CLI, login, app-server, sandbox, Bun, Rust, `rg`, `jq`, and Git identity available.
- WSL working clone: `/home/timothycard/code/Sunlight-2`.
- WSL clone currently tracks the Windows checkout path as `origin`; use it for delegated work until branch sync is changed deliberately.
- Critical handoff correction: previous fixture-backed acceptance is not product completion.
- Current implementation reality: fixture prototype plus a growing repo-backed authoring path with durable multi-topic/session state, deterministic conflict reporting, persisted projection/checkpoint/export snapshots, and repo-backed execution/output promotion records; still not the full v0.3 product.
- Main product gap: complete the no-fixture flow across compatibility import, broader policy validation, operator ergonomics, and production-like acceptance.

## Workstreams

1. Repository audit and milestone backlog.
2. Phase 0 schemas, canonical hashing, path policy, object IDs, session generation semantics, and `.sunlight` commit policy.
3. Phase 1 native IO: `sun init`, topic/session lifecycle, read/list/search/inspect, patch/write/move/delete, revisions, status.
4. Phase 2 resolver and conflicts: exact views, dependency closure, same-artifact conflict detection, non-commutative write protection.
5. Phase 3 projections and execution: safe projection materialization, execution records, output classification, promotion into topic operations.
6. Phase 4 checkpoints and Git export.
7. Phase 5 operator ergonomics: rich CLI status before GUI.
8. Phase 6 compatibility import from projections.
9. Cross-cutting security, privacy, policy validation, tests, docs, and acceptance scenario.

## Active WSL Slices

| Slice | Thread | Repo/Branch | Status | Objective | Verification |
| --- | --- | --- | --- | --- | --- |
| real-repo-artifact-io | WSL Codex CLI | `/home/timothycard/code/Sunlight-2` -> Windows commit `7bbda1a` | Completed | First no-fixture repo-backed authoring bridge. | Windows `cargo fmt --check`, no-fixture CLI tests, full `cargo test`, and `git diff --check` passed. |
| repo-backed-core-storage | WSL Codex CLI | `/home/timothycard/code/Sunlight-2-core-storage-local` -> Windows commit `3b05cd7` | Completed | Move repo-backed state into `sunlight-core` and replace TSV with schema JSON. | Windows core repo-state tests, no-fixture CLI tests, full `cargo test`, `cargo fmt --check`, and `git diff --check` passed. |
| repo-backed-policy-ingest | WSL Codex CLI | `/home/timothycard/code/Sunlight-2-policy-ingest-local` -> Windows commit `f220730` | Completed | Respect Git ignore policy during repo-backed ingestion. | Windows core repo-state tests, ignore-policy no-fixture test, vertical-slice test, full `cargo test`, `cargo fmt --check`, and `git diff --check` passed. |
| repo-backed-secret-gates | WSL Codex CLI | `/home/timothycard/code/Sunlight-2-secret-gates-local` -> Windows commit `6d92afd` | Completed | Quarantine likely secrets during ingestion and block secret/local-only checkpoint/export. | Windows core repo-state tests, no-fixture tests, full `cargo test`, `cargo fmt --check`, and `git diff --check` passed. |
| repo-backed-multi-session | WSL Codex CLI | `/home/timothycard/code/Sunlight-2-multi-session-local` -> Windows commit `a005aea` | Completed | Persist multiple repo-backed topics and sessions with distinct session generations and topic heads. | Windows core repo-state tests, no-fixture CLI tests, full `cargo test`, `cargo fmt --check`, and `git diff --check` passed. |
| repo-backed-resolver | WSL Codex CLI | `/home/timothycard/code/Sunlight-2-resolver-local` -> Windows commit `16b8699` | Completed | Resolve persisted topic heads deterministically, materialize merged views, and report inspectable same-artifact conflicts. | Windows core repo-state tests, no-fixture CLI tests, full `cargo test`, `cargo fmt --check`, and `git diff --check` passed. |
| repo-backed-projection-export | WSL Codex CLI | `/home/timothycard/code/Sunlight-2-projection-export-local` -> Windows commit `8041db3` | Completed | Persist projection/checkpoint/export snapshots and export checkpoint bytes after head moves. | Windows core repo-state tests, no-fixture CLI tests, full `cargo test`, `cargo fmt --check`, and `git diff --check` passed. |
| repo-backed-execution-output | WSL Codex CLI + manager review fix | `/home/timothycard/code/Sunlight-2-execution-output-local` -> Windows commit `8f9a377` | Completed | Persist repo-backed execution records and promote execution outputs into native topic operations. | Windows no-fixture CLI tests, full `cargo test`, `cargo fmt --check`, and `git diff --check` passed. |
| repo-backed-compat-import | WSL Codex CLI | `/home/timothycard/code/Sunlight-2-compat-import-local` branch `codex/repo-backed-compat-import` | Prepared | Implement no-fixture compatibility project/diff/import from real projections into native operations. | WSL readiness and repo check passed; launch blocked by Codex usage limit until 2026-07-06 11:34 AM local time. |

## Delegation Queue

1. Add compatibility import from real projections back into repo-backed Sunlight operations.
2. Broaden no-fixture policy validation for commit/export/projection/execution records against `.sunlight` policy.
3. Add a production-like validation repository once Sunlight can run against itself.
4. Improve operator ergonomics/status for no-fixture workflows.
5. Keep fixture tests only when they directly support removing fixture dependency.

## Heartbeat Procedure

1. Read this scratchpad and the latest Git status.
2. Check active WSL Codex slices and their outputs.
3. Review, test, and commit completed coherent work.
4. Update this scratchpad by replacing stale details, not by appending indefinitely.
5. Run the required scratchpad test above.
6. Start or resume the next one or two WSL slices based on current blockers and the v0.3 workstreams.

## Open Management Decisions

- Choose the first production-like validation repository once Sunlight can run against itself.
- Decide the default Git export shape before Phase 4 implementation locks it in.
- Decide whether to retarget the WSL clone remote from the Windows checkout path to GitHub once slice branch sync requires it.

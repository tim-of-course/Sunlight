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
- Preferred WSL repo root: `/home/timothycard/code`.

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
| None yet | - | - | Pending setup | Start after scratchpad and heartbeat are in place. | - |

## Delegation Queue

1. Audit the current Rust workspace and map existing code/tests to the v0.3 feature set.
2. Implement or complete the Phase 0 object model and persistence foundations.
3. Implement or complete native CLI/API vertical slice operations.
4. Build resolver conflict tests before broad resolver expansion.
5. Add projection/execution spike with measured storage/time output.

## Heartbeat Procedure

1. Read this scratchpad and the latest Git status.
2. Check active WSL Codex slices and their outputs.
3. Review, test, and commit completed coherent work.
4. Update this scratchpad by replacing stale details, not by appending indefinitely.
5. Run the required scratchpad test above.
6. Start or resume the next one or two WSL slices based on current blockers and the v0.3 workstreams.

## Open Management Decisions

- Confirm whether the WSL development repo should be a fresh clone under `/home/timothycard/code` or whether this Windows checkout has a remote/branch setup suitable for cloning.
- Choose the first production-like validation repository once Sunlight can run against itself.
- Decide the default Git export shape before Phase 4 implementation locks it in.

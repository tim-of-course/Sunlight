# Sunlight Development Manager Scratchpad

Purpose: keep one compact, current coordination record for Sunlight development. This file is the heartbeat handoff point and must stay under 20000 characters.

## Required Scratchpad Test

Run this after every update to this file:

```powershell
$p = "docs/sunlight_development_manager_scratchpad.md"; $n = (Get-Content -Raw -LiteralPath $p).Length; if ($n -gt 20000) { throw "Scratchpad too large: $n characters > 20000" }; "Scratchpad OK: $n characters"
```

## Management Rules

- Work directly in the Windows checkout for implementation, review, test, commit, and push.
- Do not use WSL2 Codex CLI for new slices unless the user explicitly redirects work back to WSL.
- Keep a development-manager stance: plan slices, keep changes coherent, review before committing, and redirect if a path blocks.
- Commit coherent progress checkpoints as work lands.
- Keep moving until the v0.3 local MVP is complete; if one slice blocks, switch to another useful slice.
- Heartbeat every 300 minutes, generically for the whole project, using this scratchpad as the resume point.
- MVP means a real local tool that can manage production codebases using all features in `docs/sunlight_consolidated_architecture_v0_3.md`, not only a proof of concept.

## Product Goal

Build Sunlight as a local-first native source artifact database. Git and filesystem trees are projections or compatibility layers. The local MVP must cover native artifact IO, durable topics and sessions, deterministic safe view resolution, conflict objects, scalable projections, execution evidence, execution-output promotion, checkpoints, Git export, explicit `.sunlight` policy, operator ergonomics, and compatibility import.

## Current Baseline

- Architecture source: `docs/sunlight_consolidated_architecture_v0_3.md` v0.3.
- Manager setup started: 2026-07-06.
- Windows checkout status at setup: clean.
- Management lane changed on 2026-07-06: work directly in the Windows checkout at `C:\Users\TimothyCardoza\Documents\AI-Apps\Sunlight 2`.
- Older WSL clone rows below are historical checkpoints only; do not treat them as the active work lane.
- Critical handoff correction: previous fixture-backed acceptance is not product completion.
- Current implementation reality: fixture prototype plus a substantial repo-backed product path with durable authoring/resolution/projection/execution/checkpoint/export/policy/compatibility features, no-fixture self-hosting acceptance, and a repository-confined local MCP transport; still not the full v0.3 product.
- Rename-plus-edit remains unresolved because compatibility projections expose no reliable identity signal; fuzzy inference is intentionally excluded.
- Windows executions enforce process-tree, CPU, memory, process-count, and filesystem-write isolation with Job Objects plus restricted low-integrity tokens/private roots; network isolation remains explicitly unenforced. Pre-existing unrelated low-integrity host paths are outside the write boundary.
- Main product gaps: Windows execution network isolation, reusable cross-platform projection scaling/cache reuse, and a shared in-process engine/daemon boundary beyond the transitional MCP subprocess adapter.

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

## Slice Checkpoints

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
| repo-backed-compat-import | Windows Codex CLI `0.144.1` | Windows commit `7de1357` | Completed | Implement no-fixture compatibility project/diff/import from real projections into native operations. | Focused core/state/CLI tests, full `cargo test`, `cargo fmt --check`, and `git diff --check` passed; modified/new/deleted single-candidate imports reach materialization and Git export. |
| repo-backed-compat-atomic-multifile | Windows Codex CLI | Windows commit `ba4a1eb` | Completed | Import repeated safe compatibility candidates as one durable multi-effect transaction with atomic rejection. | Core/state/CLI focused tests, full `cargo test`, `cargo fmt --check`, and `git diff --check` passed; modified/new/deleted effects share one operation/revision/generation. |
| repo-backed-compat-rename | Windows Codex CLI | Windows commit `d51d0d1` | Completed | Detect conservative unambiguous renames, preserve artifact identity, and atomically reject ambiguous matches. | Exact-hash one-to-one rename only; focused core/CLI tests, full `cargo test`, formatting, and diff checks passed; ambiguous matches leave native state unchanged. |
| repo-backed-export-policy | Windows Codex CLI | Windows commit `ef4cf95` | Completed | Add config-driven no-fixture `policy check-export` and make Git export use the same validator. | Shared conservative validator covers identity/conflicts/ref/path/blob/secret/classification/provenance; focused/full tests, formatting, and diff checks passed. |
| repo-backed-policy-reports | Windows Codex CLI | Windows commit `e4e6775` | Completed | Persist deterministic validation reports and add real `policy explain`. | Canonical content-derived reports persist for pass/fail checks; tamper/missing tests, focused/full tests, formatting, and diff checks passed. |
| repo-backed-projection-config-policy | Windows Codex CLI | Windows commit `7ef05fa` | Completed | Honor validated config for managed projection roots and path semantics. | Shared config resolver confines roots and validates constructors/consumers; focused/full tests, formatting, and diff checks passed. |
| repo-backed-execution-runtime-policy | Windows Codex CLI + manager correction | Windows commit `da994a7` | Completed | Enforce timeout, output bounds, and environment policy for real executions. | Full-stream digest and descendant cleanup review passed repeatedly with normal Windows process permissions; full suite, formatting, and diff checks passed. |
| self-hosting-production-acceptance | Windows Codex CLI | Windows commit `d1e1d3f` | Completed | Run no-fixture Sunlight end to end against a temporary Git clone of its own real repository. | Structural 50+ file journey covers native authoring/conflicts/projection/compat/execution/policy/export; repeated harness and full suite passed. |
| repo-backed-operator-ergonomics | Windows Codex CLI cell `104` | Windows commit `7b6dd3a` | Completed | Make help/status/human output truthful and useful for no-fixture operators. | Primary help is repo-backed; operational summary/warnings and human inspect/status cover real lifecycle state; focused/self-hosting/full tests, formatting, and diff checks passed. |
| repo-backed-projection-scalability | Windows Codex CLI cell `123` | Windows commit `0e5f90a` | Completed | Replace hard-coded real full-copy projection paths with truthful strategy selection, safe Windows COW capability handling, and measured materialization. | Shared staged materializer uses genuine Windows block cloning when supported and explicit copy fallback; focused/self-hosting/full tests passed. Current NTFS host verified fallback/atomicity; cache reuse and non-Windows COW remain open. |
| repo-backed-session-refresh | Windows Codex CLI cells `139`, correction `146` | Windows commit `e6b2d6d` | Completed | Add durable explicit session refresh policies, exact frontiers, monotonic generations, and conflict-safe rollback. | Manual/follow/none, exact frontiers, blocked rollback, migration, and same-actor multi-session lineages verified; focused/self-hosting/full tests passed after manager correction. |
| repo-backed-atomic-recovery | Windows Codex CLI cell `155` | Windows commit `eb35899` | Completed | Add Windows-correct atomic state/record publication, interrupted-write recovery, and derivable generation-record reconciliation. | ReplaceFileW/journal recovery and failpoints verified under normal Windows permissions; focused/self-hosting/full tests passed. Individual records are atomic; command batches remain open. |
| repo-backed-command-transactions | Windows Codex CLI cells `168`, correction `176` | Windows commit `c4b9813` | Completed | Add a durable outbox transaction for canonical state plus declared derived-record batches. | Recoverable batches, narrow portable IDs, Windows writer locking, sequence CAS, ADS/tamper tests, real recovery, self-hosting, and full suite passed after manager correction. |
| repo-backed-windows-execution-containment | Windows Codex CLI cell `188` | Windows commit `40f73bc` | Completed | Enforce Windows process-tree, CPU, memory, and process-count limits with fail-closed Job Objects. | Suspended assign-before-resume launch, resource attribution, descendant cleanup, fail-closed setup, promotion, self-hosting, and full workspace passed under normal Windows permissions. |
| repo-backed-local-mcp | Windows Codex CLI cell `202` | Windows commit `e8565f0` | Completed | Add a repo-confined persistent stdio MCP server exposing the full no-fixture workflow through typed tools. | Protocol lifecycle, 26 typed tools, path/argv/content confinement, bounded contained children, malformed recovery, real Git-repo journey, self-hosting, and all 383 workspace tests passed. |
| repo-backed-windows-filesystem-isolation | Windows Codex CLI processes `31244`, correction `54744` | Windows commit `766b450` | Completed | Enforce fail-closed Windows execution filesystem-write confinement while preserving private outputs, evidence, and promotion. | Restricted low-integrity token/private labels and Job inheritance block root/descendant host writes; source labels are read-only validated, setup rollback is proven through public CLI with no records, promotion/self-hosting/release/full workspace pass after manager correction. |
| repo-backed-windows-network-isolation | Windows Codex CLI correction process `55424` | `main`, log `C:\tmp\sunlight-network-isolation-correction.jsonl` | Review correction active 2026-07-12 | Add fail-closed per-execution Windows network denial without global firewall or host ACL side effects. | Capability-less AppContainer denial is real but checkpoint held: default-only mode rejected normal Python/Git/Cargo/Bun-class toolchains, weakened acceptance with skips, lost evidence on cleanup failure, and lacked crash recovery. Correction must add truthful dual modes/override, restore tests, persist cleanup failures, and recover profiles. |

## Delegation Queue

1. Enforce fail-closed per-execution Windows network denial with truthful evidence and no broad host firewall side effects or silent fallback.
2. Add reusable projection cache/cross-platform scaling, then replace the MCP subprocess adapter with a shared engine/daemon boundary.
3. Keep fixture tests only when they directly support removing fixture dependency.



## Heartbeat Procedure

1. Read this scratchpad and the latest Git status.
2. Check the Windows checkout status and any active slice outputs.
3. Review, test, and commit completed coherent work.
4. Update this scratchpad by replacing stale details, not by appending indefinitely.
5. Run the required scratchpad test above.
6. Start or resume the next useful Windows-direct slice based on current blockers and the v0.3 workstreams.

## Open Management Decisions

- Production-like validation repository selected: Sunlight itself via isolated local clone.
- Decide the default Git export shape before Phase 4 implementation locks it in.

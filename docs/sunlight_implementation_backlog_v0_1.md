# Sunlight Implementation Backlog v0.1

| Field | Value |
| --- | --- |
| Status | Development-manager planning backlog |
| Date | July 3, 2026 |
| Source | `docs/sunlight_consolidated_architecture_v0_3.md` |
| Scope | Phase 0 through Phase 4 local MVP work packages |

## Planning Objective

Turn the architecture into launchable work that proves the local MVP: agents author through Sunlight-native artifact IO, selected topic revisions resolve into exact testable views, executions produce evidence, and accepted checkpoints export to normal Git history.

This backlog intentionally avoids Phase 5 operator polish and Phase 6 compatibility import except where earlier decisions must leave hooks for them.

## Critical Path

1. Lock schemas, canonical identity, path policy, and `.sunlight` safety rules.
2. Build a minimal Rust CLI/core around native artifact IO and topic-owned operations.
3. Add deterministic view resolution with same-artifact conflict detection.
4. Run tests from resolved-view execution projections and promote approved tool outputs.
5. Freeze resolved views as checkpoints and export them to Git.

## Phase 0: Foundations and Risk Spikes

**Outcome:** implementation contracts are precise enough that agents can build Phase 1 without rediscovering the architecture.

**Work packages**

| ID | Package | Dependencies | Acceptance tests |
| --- | --- | --- | --- |
| P0.1 | Repository bootstrap plan and Rust CLI skeleton design | None | Proposed crate/module layout, command vocabulary, and fixture strategy are documented; no implementation code required in this slice. |
| P0.2 | Canonical object schema v1 | None | Schemas cover repository ID, artifacts, blobs/trees, operation transactions, topics, revisions, sessions, views, conflicts, executions, checkpoints, and export maps; each has required identity fields and schema version. |
| P0.3 | Canonical hashing and tree identity spec | P0.2 | Same logical record hashes identically across formatting differences; `SingleRepoTree(repo_id, tree_hash)` is defined without blocking future `RepoTreeMap`. |
| P0.4 | Path and artifact identity policy | P0.2 | Policy covers case sensitivity, Unicode normalization, symlinks, executable bits, deletes, moves, and path conflicts. |
| P0.5 | Operation transaction format | P0.2, P0.4 | Patch, write, move, delete, metadata update, and execution-output promotion payloads include preconditions, before/after refs, authored context, classification, and topic ownership. |
| P0.6 | Session generation semantics | P0.2 | Write responses define returned IDs and read-after-write behavior; refresh behavior is pinned by default and conflict-aware. |
| P0.7 | `.sunlight` commit/export policy | P0.2 | Default `.gitignore` and validation rules distinguish commit-safe records, ignored local/cache data, and policy-gated payloads. |
| P0.8 | Projection strategy spike | P0.3, P0.7 | On the first target filesystem, measure full copy, reflink if available, and read-only hardlink behavior for time, disk amplification, command compatibility, and store-corruption risk. |

**Early risks**

- Schema churn can stall implementation. Keep v1 file-compatible and add explicit migration hooks.
- Projection performance may undercut the many-agent thesis. Measure in Phase 0 and retain a correctness fallback.
- `.sunlight` may accidentally capture private or large data. Commit policy must exist before Git export work starts.

**Suggested first agent-sized slices**

- Draft canonical schema tables and example records for P0.2-P0.6.
- Produce the `.sunlight` layout, `.gitignore`, and export validation policy for P0.7.
- Run the projection spike plan on a small real repo and report metrics for P0.8.

## Phase 1: Native Artifact IO Vertical Slice

**Outcome:** a coding agent can make a small source change without directly editing a project directory.

**Work packages**

| ID | Package | Dependencies | Acceptance tests |
| --- | --- | --- | --- |
| P1.1 | `sun init` import baseline | P0.2-P0.7 | Existing Git HEAD imports as a base checkpoint; `.sunlight` structure is created with policy-correct ignored/local paths. |
| P1.2 | Topic and session commands | P1.1, P0.6 | `topic create` and `session start` return stable topic, session, resolved view, and generation IDs. |
| P1.3 | Read/list/search API | P1.2 | Reads return bytes plus artifact ID/hash/path metadata; list/search operate against the session view. |
| P1.4 | Patch/write operations | P1.3, P0.5 | Patch/write require preconditions, create operation transactions, advance topic revisions, and update session generation before the response returns. |
| P1.5 | Move/delete/metadata operations | P1.4 | Moves preserve artifact identity; deletes tombstone path binding; metadata classification is recorded. |
| P1.6 | Basic status and provenance inspection | P1.4 | Status shows topics, latest revisions, sessions, authored context, and changed artifacts without relying on `git status`. |

**Early risks**

- Agents may fall back to direct file edits. Make native commands easy and keep projection editing outside the happy path.
- Whole-file writes can hide intent. Prefer patch where practical, but allow write for new/generated files.
- Read dependencies may be too broad. Record full authored context now; add file-level reads opportunistically.

**Suggested first agent-sized slices**

- Build `init`, `topic create`, and `session start` against fixtures.
- Build `read/list/search` over imported Git content.
- Build `patch/write` with precondition failures and read-your-writes tests.

## Phase 2: Resolver and Conflicts

**Outcome:** selected topic revisions compose into deterministic resolved views or structured conflict objects.

**Work packages**

| ID | Package | Dependencies | Acceptance tests |
| --- | --- | --- | --- |
| P2.1 | View specification and selector normalization | P1.2, P0.3 | Moving selectors resolve to exact base checkpoints and topic revisions; resolved view identity is stable. |
| P2.2 | Operation application engine | P1.4, P0.5 | Exact topic frontier materializes the expected tree bytes from patch/write/move/delete operations. |
| P2.3 | Dependency closure and stale-context reporting | P2.1 | Missing dependencies and stale authored contexts produce structured gaps instead of silent resolution. |
| P2.4 | Same-artifact conflict detection | P2.2 | Overlapping patches, order-sensitive same-file edits, ambiguous rename/write cases, and broad formatter writes create conflict objects. |
| P2.5 | Safe deterministic ordering | P2.4 | Tie-breakers apply only after operations are causally ordered, independent, or proven commutative. |
| P2.6 | Resolved view records | P2.1-P2.5 | Records include base, exact topic frontier, operation semantics version, path policy, conflict set, and tree identity. |

**Early risks**

- Deterministic order can mask invalid composition. Treat non-commutative same-artifact writes as conflicts unless dependency or resolution proves intent.
- Conflict objects can become vague. Store machine-readable inputs, competing operations, paths, candidate materializations, and policy reason.
- Resolver tests can grow slowly. Start with table-driven fixtures for patch conflicts, rename conflicts, and independent edits.

**Suggested first agent-sized slices**

- Implement selector normalization and resolved view records.
- Implement operation application for independent edits.
- Implement same-file conflict fixtures before broad resolver features.

## Phase 3: Execution Projections

**Outcome:** tests run against exact resolved views, and tool-produced source changes can be promoted into topic-owned operations.

**Projection manifest status/inspect baseline**

- Projection materialization persists a local manifest envelope with manifest identity and a normalized root binding. Status and inspect validate that envelope before content verification.
- Stale or malformed persisted envelopes report `content_verification: manifest_invalid`; a valid envelope whose root binding differs from the current projection `root_ref` reports `content_verification: root_mismatch` before byte comparison.
- Fixture status/inspect should continue to cover missing roots, non-directory roots, scan failures, unavailable manifest metadata, dirty content, missing/extra files, executable metadata drift, invalid envelopes, and valid root-binding mismatches.

**Work packages**

| ID | Package | Dependencies | Acceptance tests |
| --- | --- | --- | --- |
| P3.1 | Execution sandbox materialization | P2.6, P0.8 | A resolved view projects into an isolated sandbox with recorded projection strategy and protected store integrity. |
| P3.2 | `sun run` command runner | P3.1 | Runs a pinned command, captures exit status, stdout/stderr summary, timeout state, working directory, and environment summary. |
| P3.3 | Execution records and output classification | P3.2 | Execution records link to resolved view, tree identity, command, projection ID, outputs, and result. |
| P3.4 | Store integrity verification | P3.1 | Risky executions cannot mutate immutable store objects; verification failure quarantines projection/cache entries. |
| P3.5 | Execution-output promotion | P3.3, P1.4 | Approved formatter/codegen/lockfile outputs become topic-owned patch/write operations with execution provenance and policy gates. |

**Early risks**

- Test tools expect writable source trees. Use read-only source plus private output/copy-up behavior, and promote only approved source-like deltas.
- Captured logs may leak secrets. Classify and gate execution artifacts before any export path.
- Environment summaries may be incomplete. Capture enough to debug MVP runs, then refine by target stack.

**Suggested first agent-sized slices**

- Materialize a resolved view and run a harmless command such as a fixture test.
- Add execution record persistence and status display.
- Promote a formatter or generated-file delta into a topic operation.

## Phase 4: Checkpoints and Git Export

**Outcome:** an accepted resolved view freezes with evidence and exports to ordinary Git while native provenance remains authoritative.

**Work packages**

| ID | Package | Dependencies | Acceptance tests |
| --- | --- | --- | --- |
| P4.1 | Checkpoint creation | P2.6, P3.3 | Checkpoint records freeze resolved view, tree identity, provenance, conflict-free status, and selected evidence. |
| P4.2 | Export validation | P4.1, P0.7 | Export rejects unsafe `.sunlight` references, ignored cache/local paths, private payloads, secret-like data, and size-policy violations. |
| P4.3 | Git tree materialization | P4.2, P3.1 | Checkpoint tree materializes into ordinary project files without making the working tree authoritative. |
| P4.4 | Git commit/branch export | P4.3 | Exports a checkpoint as a normal Git commit or branch and records native checkpoint-to-Git mapping. |
| P4.5 | End-to-end MVP scenario | P1-P4 | Two topics resolve into one view, tests run, checkpoint is created, Git export succeeds, and provenance links artifact -> operation -> topic -> session -> view -> execution -> checkpoint -> Git ref. |

**Early risks**

- Git export can lose native meaning. Treat Git commits as lossy compatibility artifacts and keep export maps authoritative.
- Export shape may be debated. Start with one checkpoint commit; leave room for topic-per-commit or curated series later.
- Validation may block useful local workflows. Make policy failures actionable and allow explicit local-only retention.

**Suggested first agent-sized slices**

- Create checkpoint records from conflict-free resolved views.
- Implement export validation over `.sunlight` policy fixtures.
- Export one checkpoint to a Git branch and verify native mapping.

## Cross-Phase Verification Matrix

| MVP claim | First phase that must verify it | Suggested test |
| --- | --- | --- |
| Native artifact IO is the authoring path | Phase 1 | Agent-style fixture edits through `read/search/patch/write`; no direct project-directory edits. |
| Topic ownership is captured at mutation time | Phase 1 | Every operation has exactly one topic, session/generation, and authored context. |
| Composition is deterministic and safety-gated | Phase 2 | Same inputs produce same resolved view; non-commutative same-artifact edits produce conflict objects. |
| Projections are adapters, not source truth | Phase 3 | Commands run in sandboxes; source writes require explicit promotion. |
| Execution evidence is tied to exact views | Phase 3 | Execution records include resolved view ID, tree identity, command, environment summary, projection ID, outputs, and result. |
| Git interop is boring and policy-safe | Phase 4 | Checkpoint exports to a normal Git ref after `.sunlight` export validation. |

## Launch Recommendation

Start with three parallel Phase 0 slices:

1. Canonical schema and hashing contracts.
2. `.sunlight` layout, commit policy, and export validation rules.
3. Projection safety spike on the first target filesystem.

After those land, launch the Phase 1 vertical slice in thin increments: `init` plus topic/session, then read/search, then patch/write with read-your-writes. Do not launch resolver or execution work until operation records and session generations are stable enough to become test fixtures.

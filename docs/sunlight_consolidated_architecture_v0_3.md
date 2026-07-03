# Sunlight: Consolidated Architecture and Local MVP Plan
Native source database for parallel human and agent development

| Field | Value |
| --- | --- |
| Status | Architecture decision draft. Intended to replace fragmented handoff plus MVP notes with one working product and implementation plan; updated with resolver, projection-safety, Git-transport, execution-promotion, cross-repo identity, and session-freshness decisions. |
| Date | July 3, 2026 |
| Version | 0.3 review update |
| Source inputs | agent_native_version_control_design.docx; sunlight_local_repo_mvp_spec.docx; sunlight_plan_changes_handoff.md |
| Primary decision | Sunlight is a native, event-sourced, multi-version source artifact database. Git and filesystems are projections and compatibility layers, not the coordination substrate. |

> **Architecture stance** This document treats the user as the authority on the problem: many agents, many views, low ceremony, no worktree sprawl, strong provenance, and Git compatibility. It makes opinionated architecture decisions about how Sunlight should satisfy those goals.

> **v0.3 review incorporation** This revision promotes six review findings into architecture requirements: resolver order safety, projection corruption protection, explicit .sunlight commit policy, execution-output promotion, multi-repo tree identity, and session freshness after writes.

# Contents

- 1. Executive decision
- 2. Goals and product constraints
- 3. Architecture principles
- 4. System model and layers
- 5. Terminology and object model
- 6. Authoring lifecycle
- 7. Native artifact API and CLI surface
- 8. Operation model and topic revisions
- 9. View resolution, conflicts, and staleness
- 10. Projections, execution sandboxes, and zero-copy strategy
- 11. Execution evidence and live development
- 12. Git interoperability and repository layout
- 13. Storage, implementation stack, and security posture
- 14. Cross-repo intent trees
- 15. Local MVP plan and acceptance criteria
- 16. First vertical slice
- 17. Risks, mitigations, and decision record
- 18. Product decisions still needed
- 19. Final summary

# 1. Executive decision

Sunlight should not be built as a more efficient Git worktree manager. It should be built as a local-first source-control database whose native units are artifact, operation transaction, topic, resolved view, checkpoint, execution, and evidence. A checked-out directory is only one possible projection of a resolved view.

The decisive product shift is native authoring. Agents should primarily inspect and mutate source through a topic-bound artifact API: read, list, search, patch, write, move, delete, inspect, resolve, run, and checkpoint. Existing tools still need files, so Sunlight must generate filesystem projections and execution sandboxes. Those projections are adapters, not the place where source-control truth originates.

The local MVP should prove this directly. A coding agent should complete a real change without directly editing a project directory; the change should be recorded as topic-owned operations with exact authored context; selected topics should compose into deterministic, safety-gated views; tests should run through exact execution projections; and an approved checkpoint should export to ordinary Git history.

> **One-sentence thesis** Build MVCC for source code, but make the authoring surface artifact-native: isolated agents write durable topics through an API; exact topic combinations resolve into views; filesystem trees and Git commits are generated from those views when needed.

| Question | Decision |
| --- | --- |
| What stays from the original architecture | Operations, durable topics, topic revisions, views, resolved views, checkpoints, channels, executions, provenance, conflict/staleness objects, and Git interoperability. |
| What changes | The primary authoring model moves from directory-backed workspaces to native topic-bound authoring sessions. The word workspace is no longer allowed to imply a full mutable directory. |
| What the MVP must prove | Native source IO, deterministic view resolution with non-commutative write protection, first-class conflicts, safe projection materialization, execution records plus output promotion, checkpoint export to Git, explicit .sunlight commit policy, and projection scalability beyond full copies. |
| What is deferred | Hosted forge, public collaboration, CRDT shared-live editing, AST-native semantics for every language, perfect semantic dependency inference, Dropbox-like sync, and same-path multi-version filesystem as a production feature. |

# 2. Goals and product constraints

The goal is to coordinate large numbers of coding agents and humans working against the same logical codebase without forcing them through branch, worktree, commit, rebase, and directory-sprawl ceremony. The system must support speculative isolation, exact composition, reproducible execution, recovery, and provenance while still exporting boring, normal Git artifacts for adoption.

| User goal | Architectural consequence |
| --- | --- |
| Many concurrent agents across many views | Do not allocate one full checkout per agent or per candidate view. Use shared content storage, cached projections, and native API authoring. |
| Agents must see and edit their own version | Every authoring session is bound to an exact resolved view and one write topic. The agent reads and writes through Sunlight, so its world is explicit. |
| No cognitive overhead for agents | Expose a small, deterministic tool surface instead of asking agents to infer state from git status, branches, worktrees, and ad hoc diffs. |
| Continuous recoverability without noisy public history | Record operation transactions and topic revisions automatically. Publish and land deliberately through checkpoints and channels. |
| Composition and testing of arbitrary useful topic sets | Make view resolution and execution evidence first-class. Test exact resolved views, not mutable branch names. |
| Git compatibility during adoption | Import Git as base checkpoints and export accepted checkpoints as normal commits or branches. Do not make Git the native coordination model. |
| Future cross-repo work | Include repository identity in object IDs and design topics/views so a future intent can span multiple repositories. |

## 2.1 Non-goals for the local MVP

- It does not replace GitHub, hosted code review, or organization permissions.
- It does not require a custom kernel filesystem before the model is proven.
- It does not infer perfect semantic dependencies or automatically repair arbitrary conflicts.
- It does not make CRDT real-time editing the default source-control model.
- It does not require every compiler, package manager, editor, or language server to become Sunlight-native on day one.

# 3. Architecture principles

| Principle | Implication |
| --- | --- |
| Native source database first | The authoritative state is Sunlight objects. Directories are materialized views, not the source of truth. |
| Intent is durable | A topic represents one coherent intention and keeps its identity while implementation revisions evolve. |
| Authoring is explicit | Every edit happens in a session with a resolved view, one write topic, actor identity, and preconditions. |
| Composition is deterministic, not arbitrary | The same safe inputs produce the same root identity, but deterministic tie-breakers cannot hide non-commutative same-artifact writes. |
| Projection is an adapter | Filesystem trees exist for tools, execution, inspection, and Git export. They are not the baseline authoring model. |
| No full-copy assumption | Full directory copies are allowed only as an emergency fallback. The design must share unchanged content from the start. |
| Evidence is content-addressed | Tests, builds, generated outputs, and dev-server generations attach to resolved view and environment identities. Tool-produced source changes require explicit promotion. |
| History is honest | Projection onto a newer context does not rewrite the old authored context. Adaptation creates a new topic revision. |
| Security precedes sync | Continuous capture, exports, and future replication must classify secrets, generated artifacts, and private provenance before they leave a boundary. |
| Git is boring interoperability | Git remains dependable import/export, backup, review, and transport during adoption; it is not the native coordination layer. |

# 4. System model and layers

Sunlight has four conceptual layers: a native object store, an authoring and resolution layer, projection adapters, and external ecosystem bridges. The native object store must be able to represent source artifacts independently from any checked-out directory.

```text
Humans / coding agents
      |
      v
Native artifact API / CLI / MCP tools
  read | list | search | patch | write | move | delete | inspect | run
      |
      v
Authoring sessions  --->  topic-owned operation transactions
      |                         |
      v                         v
Resolved view selection <--- topics, revisions, dependencies, conflicts
      |
      v
Canonical resolved view + tree identity
      |
      +--> compatibility projection for humans/editors/legacy agents
      +--> execution sandbox for tests, builds, dev servers
      +--> export projection for Git commits, PRs, deployments
      +--> future same-path multi-version filesystem adapter
```

| Layer | Responsibility |
| --- | --- |
| Native object store | Artifacts, content blobs, tree records, operation transactions, topics, revisions, views, checkpoints, executions, conflicts, metadata, policies. |
| Authoring/session layer | Topic-bound sessions expose the artifact API, enforce write ownership, record authored context, and create operation transactions. |
| Resolver layer | Takes base checkpoints plus topic revision selections, closes dependencies, applies operations, returns a resolved view or conflict set. |
| Projection layer | Creates ordinary-file adapters for tools. Uses shared content and caching so files are materialized only when useful. |
| Interop layer | Imports Git repositories, exports checkpoints to Git history, and eventually replicates native Sunlight objects directly. |

## 4.1 The source of truth rule

The Git working tree is not authoritative for Sunlight. In the MVP, Git may store and transport policy-approved .sunlight records during adoption, but native Sunlight objects define topics, contexts, resolved views, checkpoints, and execution evidence. Git-normal project files are an export format or a compatibility projection. Because .sunlight can contain private objects and large payloads, Git transport is controlled by an explicit commit policy rather than by blindly committing the entire directory.

## 4.2 Tree identity in the core model

Sunlight should not define resolved views as if they forever produce exactly one repository tree. The single-repo MVP is the first specialization of a more general root identity type. Use SingleRepoTree(repo_id, tree_hash) for the local MVP and keep RepoTreeMap({repo_id: tree_hash}) as the compatible representation for future cross-repo topics and checkpoints.

# 5. Terminology and object model

The original design used workspace for a live projection. After the handoff discussion, that term should be split. The canonical authoring object is an authoring session. A projection is the directory-like adapter. Workspace can remain as a casual umbrella term, but specifications and APIs should avoid using it as a primary object.

| Term | Definition |
| --- | --- |
| Artifact | Stable identity for a source file, directory, binary, generated artifact, or future structured entity. Carries compatibility paths but is not reducible to a path. |
| Content object | Blob, chunk, or tree record addressed by digest. Gives checkpoints stable bytes and enables shared projection caches. |
| Operation transaction | One logical edit batch: patch, write, move, delete, metadata update, or structured transform. Owned by exactly one topic. |
| Topic | A durable intention such as auth-rework or profile-ui. User-facing term for a change thread. |
| Topic revision | Immutable selectable state of one topic after one or more operation transactions. |
| Authored context | Exact base, resolved view, active topic revisions, environment hints, and preconditions present when an operation was authored. |
| View specification | Declarative selection of base and topic selectors. May contain moving selectors such as topic@head. |
| Resolved view | Exact immutable frontier: repository scope, base checkpoint or checkpoints, exact topic revisions, dependency closure, conflict resolution set, operation semantics version, and tree identity. The tree identity is a single repo tree in the MVP and a repo-to-tree map in the cross-repo model. |
| Tree identity | Canonical materialized result of a resolved view: either SingleRepoTree(repo_id, tree_hash) or RepoTreeMap({repo_id: tree_hash}). This type is part of the core model, not only a future cross-repo add-on. |
| Authoring session | Topic-bound, view-bound context through which an agent or human reads and writes artifacts using the native API. It has a current session generation that advances after accepted writes. |
| Session generation | Monotonic session-local resolved generation returned after writes and refreshes. It guarantees read-after-write freshness for the authoring agent. |
| Projection | Ordinary-file representation generated from a resolved view for compatibility, execution, inspection, or export. |
| Execution sandbox | Projection plus environment wrapper used to run tests, builds, dev servers, language servers, or package tools against an exact view. |
| Checkpoint | Frozen resolved view with tree identity, provenance, evidence, and optional Git export references. For one repo this is one canonical tree hash; for cross-repo it is a map of repo IDs to tree hashes. |
| Channel | Governed pointer to accepted checkpoints, such as main, release, preview, or production. |
| Proposal | Review and policy process around exact topic revisions, candidate views, checks, and eventual channel advancement. |
| Conflict object | Persistent object scoped to a composition explaining incompatible operations, policies, environments, or assumptions. |
| Staleness object | Evidence that a topic revision was authored against older context and may need revalidation or adaptation. |

## 5.1 Naming decisions

| Decision | Reason |
| --- | --- |
| Use topic in UI and commands | It is short, intention-oriented, and avoids Git branch semantics. |
| Use authoring session in the API | It makes topic and context binding explicit without implying a directory. |
| Use projection for ordinary files | It prevents filesystem materialization from masquerading as source-control truth. |
| Use execution sandbox for commands | It clarifies that builds and tests may need a mutable temporary filesystem even when authoring does not. |
| Use artifact API as the formal API name | Native IO is useful explanatory language, but artifact API is clearer in a specification. |

# 6. Authoring lifecycle

The normal flow is not: create worktree, edit files, run git diff, stage, commit, rebase. The normal flow is: create topic, open authoring session, read from an exact view, submit structured edits, create topic revisions, resolve candidate views, run exact executions, checkpoint, and export or land.

| Step | Meaning |
| --- | --- |
| 1. Initialize | sun init creates .sunlight metadata, imports the current Git state as a base checkpoint, and initializes object storage and indexes. |
| 2. Create topic | sun topic create auth-rework records intent, owner, base context, visibility, and acceptance criteria. |
| 3. Start session | sun session start --topic auth-rework --view main@K100 binds the agent to one resolved view and one write topic. |
| 4. Inspect artifacts | The agent uses read, list, search, and inspect. Reads can record file-level dependencies later; MVP at least records the full authored context. |
| 5. Mutate artifacts | The agent uses patch, write, move, delete, or structured edit. Each request includes preconditions such as expected artifact version or before hash. |
| 6. Append operation transaction | Sunlight validates the edit, stores before/after references, creates an operation transaction, advances the topic to a new revision, and returns the new session generation. |
| 7. Resolve candidate view | An operator or agent selects base plus topic revisions. The resolver closes dependencies and produces a deterministic tree or conflict objects. |
| 8. Run execution | sun run creates an execution sandbox from the resolved view, runs tests/builds, captures environment summary, outputs, and result. |
| 9. Checkpoint | If acceptable, sun checkpoint create freezes the resolved view and tree identity with validation evidence. |
| 10. Export or land | sun git export materializes the checkpoint as ordinary Git history. Native history remains available under Sunlight. |

```text
sun init
sun topic create auth-rework
sun session start --topic auth-rework --view main@K100
sun read src/auth.ts --session S1
sun search "User.email" --session S1
sun patch src/auth.ts --session S1 < patch.diff
sun view resolve --base main@K100 --include auth-rework@r3,profile-ui@r2
sun run V42 -- bun test
sun checkpoint create V42 --name auth-profile-ready
sun git export K18 --branch feature/auth-profile-ready
```

## 6.1 Authoring invariants

- Every authoring session has exactly one write topic. Stray edits go to scratch or require explicit reassignment.
- Every operation transaction records exact authored context. Context can be broad at first, but it must exist.
- An accepted write advances the session to a new private resolved generation before the API call returns, so the agent sees its own latest topic revision on the next read.
- No channel changes because an edit occurred. A channel changes only through approved checkpoint landing.
- Compatibility projection editing is allowed only through an explicit import/capture path and should be visibly second-class in the MVP.

## 6.2 Session freshness after writes

A session must define read-after-write behavior precisely. Every accepted mutation returns a new operation transaction ID, topic revision ID, session generation ID, and resolved view ID. The session current generation advances atomically to include the writer's new topic revision before the write response returns. Subsequent reads through that session default to the new generation.

| Case | Required behavior |
| --- | --- |
| Own write accepted | Advance the session's write topic to the new revision and return the new session_generation_id. The agent immediately observes the new artifact state. |
| Unrelated topic movement | Do not silently advance other floating topics during the write response. Report available refreshes separately so authored context remains understandable. |
| session.refresh | Explicitly resolves a later allowed generation when dependencies or selected heads may move. It returns a new generation or conflict/staleness details. |
| Pinned default | For predictable agent reasoning, a session is pinned to its starting dependency frontier except for its own write topic unless configured otherwise. |
| Conflicting refresh | If refreshing would invalidate or conflict with local topic state, keep the last good session generation and return a conflict/staleness object. |

# 7. Native artifact API and CLI surface

The artifact API is the heart of the local MVP. The CLI is one transport over it; an MCP server, local daemon endpoint, and language SDKs should all call the same internal engine. The agent should not need to shell out to Git or reason about a hidden filesystem to create a correct change.

| API operation | MVP responsibility |
| --- | --- |
| session.start(topic, view) | Create a topic-bound authoring context. Returns a session ID, resolved view ID, session generation ID, write topic ID, and capabilities. |
| artifact.read(session, path_or_id) | Return bytes, artifact ID, version/hash, path metadata, and optional provenance. |
| artifact.list/search(session, query) | Search paths, file contents, symbols later, or metadata within the session view. |
| artifact.inspect(session, path_or_id) | Return artifact identity, path history, topic provenance, current hash, classification, and dependency hints. |
| artifact.patch(session, path_or_id, patch, preconditions) | Apply a patch against expected artifact content and store it as a structured operation transaction. |
| artifact.write(session, path_or_id, content, preconditions) | Create or replace an artifact. Prefer patch when small; use whole-content write for generated files or new files. |
| artifact.move/delete(session, target, preconditions) | Record structural changes directly rather than inferring them from filesystem rename detection. |
| session.refresh(session, policy) | Resolve the next allowed session generation when dependencies or floating selectors may move. Never hides prior authored context. |
| view.resolve(selection) | Resolve base plus topic revision selectors into an exact view with tree identity or conflict set. |
| projection.create(view, purpose) | Create a compatibility, execution, inspection, or export projection with cache policy. |
| execution.run(view, command, options) | Materialize an execution sandbox if needed, run command, capture evidence and classified outputs, and return execution ID. |
| execution.promote_output(execution, paths, topic, classification) | Convert approved execution-produced source changes into topic-owned operation transactions with execution provenance. |
| checkpoint.create(view, evidence) | Freeze a resolved view and tree identity. |
| git.export(checkpoint, policy) | Export checkpoint as ordinary Git commit(s), branch, patch, or working tree state. |

## 7.1 API design rules

- All mutations require a session ID or explicit topic ID plus authored context. Do not let write operations float without topic ownership.
- All patch/write/promote requests include preconditions: expected artifact hash, expected path binding, expected resolved view ID, or execution output reference when applicable.
- Every response should be machine-readable and stable. Agents need structured errors, not paragraphs of git-like advice.
- The CLI should be thin. Core semantics belong in a Rust library/daemon so MCP and SDK transports share behavior.
- Tool names should be boring and direct. Agents should call read, search, patch, write, move, delete, run, and checkpoint without learning database theory.

# 8. Operation model and topic revisions

The MVP should not store raw filesystem syscalls as user-visible history. It should store logical operation transactions born from API calls or explicit import/capture steps. The first operation model should be file-compatible and deterministic, with room to add symbol, schema, or AST-derived operations later.

| Field | Purpose |
| --- | --- |
| id | Content-derived or unique operation transaction ID. |
| topic_id | Owning topic. Exactly one. |
| actor_id / session_id / session_generation_id | Human, agent, tool, automation, and session generation that issued or promoted the operation. |
| authored_context_id | Resolved view and environment context in which the edit was produced. |
| preconditions | Expected artifact IDs, path bindings, hashes, revision IDs, or semantic assumptions. |
| read_set | MVP can store full context. File-level reads can be captured opportunistically through API reads and execution traces. |
| write_set | Artifacts and paths created, changed, moved, deleted, or classified. |
| mutation_payload | Patch hunks, whole-file after snapshot, move record, delete marker, metadata update, execution-output promotion, or structured transform. |
| before_refs / after_refs | Content hashes or tree references before and after the transaction. |
| classification | source, generated, cache, secret, local-only, execution-output, lockfile, migration, binary, vendored, or policy-defined. |
| promotion_source | Optional execution ID and output artifact reference when a tool-produced file is promoted into source history. |
| logical_time / parents | Causal parent links and topic revision parent. |

## 8.1 MVP mutation semantics

| Mutation | MVP rule |
| --- | --- |
| Patch | Apply unified-diff-like hunks to an expected artifact hash. If context does not match, return conflict object instead of guessing. |
| Write | Create or replace whole artifact content. Use for new files, generated files, or cases where a patch would be noisy. |
| Promote execution output | Convert a selected execution output, formatter result, codegen artifact, migration, or lockfile change into a topic-owned source operation with provenance back to the execution. |
| Move | Change artifact path binding while preserving artifact identity and path history. |
| Delete | Tombstone an artifact path binding and retain provenance. |
| Metadata update | Classification, executable bit, generated policy, language tag, retention class, or visibility class. |
| Structured transform later | Language plugins can add refactor-aware operations later, but canonical bytes and snapshots remain authoritative. |

## 8.2 Revision boundaries

Create a new topic revision at every accepted operation transaction boundary. A transaction may include multiple files when the agent submits one logical step, such as a refactor across twelve files. The UI may coalesce revisions for readability, but the selectable native model should preserve each immutable transaction boundary unless explicitly compacted under a retention policy.

## 8.3 Read dependency strategy

Version 1 should record exact full authored context for every operation. That alone supports conservative staleness: if a dependency moves, the consumer becomes unknown until re-run, inspected, or adapted. File-level read tracking can be added cheaply because native read/search calls pass through Sunlight. Symbol-level and semantic preconditions are later precision improvements, not MVP prerequisites.

# 9. View resolution, conflicts, and staleness

A resolved view is the deterministic answer to a selected base plus exact topic revisions. It is not a directory. It can be served through the artifact API, projected into files, executed, checkpointed, or exported. The resolver is therefore the most important correctness boundary in the system.

```text
view_spec candidate-auth-profile = {
  base: main@K100,
  include: [auth-rework@r8, profile-ui@r3],
  policy: dependency_closed + file_conflicts_block
}

resolved_view V42 = {
  base: main@K100,
  topic_frontier: [auth-rework@r8, profile-ui@r3, schema@r2],
  dependency_closure: complete,
  operation_semantics_version: file_ops_v1,
  path_policy: posix_case_sensitive_v1,
  conflict_set: [],
  tree_identity: SingleRepoTree(repo_id: app, tree_hash: H42)
}
```

| Resolver step | Requirement |
| --- | --- |
| Normalize selectors | Replace @head and channel names with exact checkpoints and topic revisions. |
| Close dependencies | Include required topic revisions or fail with missing dependency information. |
| Partition by artifact | Group write operations by artifact/path identity so same-artifact composition can be checked explicitly. |
| Apply causal/dependency order | Causal parents and declared topic dependencies define real order. They are correctness facts, not tie-breakers. |
| Test commutativity | Independent operations on the same artifact must prove they commute under the operation semantics. Non-commuting same-artifact writes become conflict objects unless an explicit dependency or resolution chooses the order. |
| Use deterministic tie-breakers only after safety | Topic/order tie-breakers are allowed only for canonicalizing already-commutative or independent operations. They must never silently choose between two valid but different final states. |
| Apply operation transactions | Materialize a candidate tree in memory or through CAS-backed staging. Do not write conflict markers into source by default. |
| Classify conflicts | Representational, syntactic, type/schema, semantic, policy, environment, or unknown. MVP needs same-file patch conflicts first. |
| Produce identities | Semantic identity from frontier and metadata; tree identity from materialized bytes. Single-repo is one tree hash; cross-repo is a repo-to-tree map. |
| Return evidence gaps | Report stale or unknown consumers when authored context moved and no valid execution evidence exists. |

> **Resolver safety rule** Deterministic ordering is required for reproducibility, but it is not proof of correctness. If applying independent same-artifact operations in different orders can produce different bytes or meanings, the resolver must create a conflict object unless dependency order or an explicit resolution proves the intended order.

## 9.1 Non-commutative write rule

The resolver must distinguish reproducible arbitrary order from valid composition. Two operations are safe to order by a deterministic tie-breaker only when they are causally ordered, declared as dependent, disjoint under the operation algebra, or proven commutative by a semantics-specific check. Otherwise they are representational conflicts even if both patches can be applied in either order.

| Case | Resolver behavior |
| --- | --- |
| Different artifacts | Normally independent, subject to path, rename, generated-output, and policy checks. |
| Same artifact, disjoint hunks | May be commutative for file_ops_v1 if both hunks anchor to stable before hashes and produce the same bytes regardless of order. |
| Same artifact, overlapping or order-sensitive edits | Conflict unless a dependency edge, explicit resolution operation, or reconciler topic supplies order and intent. |
| Formatters/code generators | Treat as broad writes unless the tool provides stable structured provenance. They commonly do not commute with human edits. |
| Renames plus writes | Resolve through stable artifact identity; ambiguous path identity or two incompatible target paths creates a conflict. |

## 9.2 Projection instead of destructive rebasing

When a topic authored against one context is evaluated against another, Sunlight projects the old operations into the new target context. If the projection is invalid or stale, the authoring agent creates a new topic revision that adapts the work. The old revision keeps its true authored context. Git export may synthesize rebased-looking commits, but the native model should not falsify history.

## 9.3 Conflict objects

| Aspect | Decision |
| --- | --- |
| Scope | A conflict is scoped to a target resolved view or projection attempt. A topic is not globally conflicted. |
| Inputs | Competing operations, authored contexts, artifact IDs, paths, candidate materializations, and relevant policies. |
| Output | Structured conflict object plus optional human-readable patch/merge display. |
| Resolution | A resolution is an attributed operation transaction, usually in a reconciliation topic or explicit topic revision. |
| Reuse | A prior resolution may be reused only when preconditions still hold and the system marks it as reused evidence. |

# 10. Projections, execution sandboxes, and zero-copy strategy

The product thesis requires isolated views without multiplying whole repository directories. Projection is therefore not cosmetic optimization. It is a core architecture requirement, even if the first implementation starts with a simple portable materializer.

| Projection type | Purpose |
| --- | --- |
| Compatibility projection | Ordinary files for humans, editors, legacy agents, or manual inspection. Writes require explicit import into a topic. |
| Execution sandbox | Temporary or cached projection used for test/build/dev commands. Source is usually read-only; outputs use an overlay or output directory. |
| Export projection | Materializes an approved checkpoint into Git-normal files or a temporary tree for commit creation. |
| Debug projection | Pinned ordinary files for reproducing a historical resolved view or execution. |
| Future same-path adapter | Process-specific multi-version filesystem where the same logical path resolves differently for different processes. |

## 10.1 Projection implementation tiers

| Tier | Role |
| --- | --- |
| Tier 0: Full copy fallback | Portable and useful for debugging. It must not be the default success path because it fails the many-agent thesis. |
| Tier 1a: CAS-backed reflink materialization | Preferred MVP target where available. Reflinks provide copy-on-write semantics, so tool writes should not mutate immutable store objects. |
| Tier 1b: read-only hardlink materialization | Allowed only with strict protections. Hardlinks are not copy-on-write; they must be read-only, verified, and paired with copy-up or overlay behavior before any write. |
| Tier 2: Sparse/lazy projection | Materialize files only when accessed by a tool or when a run manifest requires them. |
| Tier 3: Process-specific multi-version filesystem | High-value future adapter. Prototype early, but do not block native source model on it. |

## 10.2 MVP projection rule

The MVP should implement safe reflink/COW projection for at least one primary platform or filesystem when practical, plus Tier 0 as a correctness fallback. Hardlinks may be used only under the integrity rules below. A spike should measure materialization time, disk amplification, cache invalidation, command compatibility, and corruption resistance. If safe Tier 1 is not stable quickly, the MVP can ship a bounded fallback, but the acceptance criteria should still measure storage amplification and materialization cost.

## 10.3 Projection integrity rules

Hardlinks and reflinks are not equivalent. A reflink clone can be safe when the filesystem provides real copy-on-write semantics. A hardlink points to the same inode; an in-place write by a legacy tool can corrupt the shared content store. The projection layer must therefore treat immutable store objects as protected data, not merely as convenient files.

| Protection | Rule |
| --- | --- |
| Immutable store permissions | Content-store files are read-only and never handed out as writable paths. The engine verifies permissions when creating and garbage-collecting projections. |
| Reflink path | Use platform APIs that guarantee copy-on-write behavior or fall back. Mark the projection strategy in projection_id for auditability. |
| Hardlink path | Use only for read-only source projections or with an overlay/copy-up layer that replaces the linked file before any write can occur. |
| Write isolation | Execution sandboxes and compatibility projections use private upper/output directories for generated files, caches, and source-write attempts. |
| Post-run verification | Hash protected store objects after risky executions or maintain store integrity manifests. Any mismatch quarantines the projection and invalidates affected cache entries. |
| Fallback rule | If copy-up/read-only enforcement cannot be guaranteed on a platform, use full-copy or overlay fallback rather than unsafe hardlinks. |

## 10.4 Writes from projections

| Writer | Policy |
| --- | --- |
| Native agents | Do not write projections. They call the artifact API. |
| Execution tools | May produce outputs, caches, lockfiles, coverage, generated files, and logs. Source writes should be blocked or isolated first, then promoted explicitly when legitimate. |
| Human/legacy editing | Use a compatibility projection plus explicit sun import/capture to transform diffs into topic-owned operation transactions. |
| File watchers | Useful for compatibility import, never primary correctness. The primary path records operations before filesystem mutation. |

# 11. Execution evidence and live development

Most existing tests, compilers, language servers, package managers, and dev servers require ordinary files. Sunlight should satisfy them by creating exact execution sandboxes from resolved views. The execution object then records exactly what was run and what it observed.

| Execution field | Purpose |
| --- | --- |
| resolved_view_id | The exact view used for the run. Pinned executions must never refer only to a moving selector. |
| tree_identity | Canonical repo-to-tree map projected for the command; a single-repo execution has a one-entry map. |
| environment_summary | OS, architecture, tool versions when known, package manager state, env policy, container image, or dev shell identity. |
| command | Command and arguments, working directory, stdin policy, timeout, network policy, and cache policy. |
| projection_id | Execution sandbox identity and projection strategy. |
| inputs | Relevant config files, dependency lockfiles, external inputs, fixtures, or declared services. |
| outputs | Logs, test reports, build artifacts, coverage, generated files, lockfile deltas, migrations, and classified side effects. |
| promotions | Explicit links from execution outputs into later topic-owned operation transactions, if any. |
| result | Pass, fail, timeout, canceled, flaky, unknown, or policy-blocked. |

## 11.1 Pinned runs first

The MVP should start with pinned executions. sun run <view> resolves or accepts an exact resolved view, creates an execution sandbox, runs the command, captures outputs, and records the result. This is enough to prove view-addressed testing without solving live dev-server reloading immediately.

## 11.2 Promoting execution-produced source changes

Real development commands often modify source-like artifacts: formatters rewrite files, package managers update lockfiles, migrations create files, and code generators refresh clients. Sunlight should not treat those writes as invisible side effects or blindly capture them as source. They need a first-class promotion path from execution output to topic-owned operation transaction.

| Promotion step | Rule |
| --- | --- |
| Default pinned execution | Source tree is read-only. Caches, logs, coverage, and build outputs remain execution artifacts, not topic operations. |
| Promotion request | execution.promote_output or artifact.write_from_execution selects output paths, target topic, classification, and preconditions. |
| Diff source | Compare the execution sandbox result against the input resolved view and create patch/write/move operations with before/after hashes. |
| Provenance | The new operation transaction records execution_id, command, tool versions when known, output classification, actor/session, and authored context. |
| Policy gates | Secret scanning, generated-source classification, lockfile policy, and size limits run before promotion. |
| Result | Promotion advances the target topic to a new revision and returns the new session/view generation if performed inside an authoring session. |

## 11.3 Live dev later, but designed now

A long-running dev server can later follow a view specification. Each update resolves a new generation; gates such as parse, typecheck, or smoke tests decide whether the server advances. If the next generation is blocked, the server stays on the last good generation and Sunlight reports the pending conflict or stale dependency. The execution record must store every observed generation.

# 12. Git interoperability and repository layout

Git remains essential for adoption. The right frame is not Sunlight versus Git on day one; it is Sunlight as the authoritative coordination layer with Git as import/export, review, transport, backup, and ecosystem bridge.

| Git role | Decision |
| --- | --- |
| Import | Treat an existing Git commit as a base checkpoint and canonical tree. Native topics begin above it. |
| Metadata transport | During early adoption, only policy-approved .sunlight records should be committed through Git. Caches, raw payloads, private objects, and execution sandboxes are ignored or bundled explicitly. |
| Export | Materialize a checkpoint tree and create ordinary Git commits or branches. Record export references back in checkpoint metadata. |
| Commit shapes | Single squashed commit, topic-per-commit series, curated series, or patch stack. Native provenance remains richer than export shape. |
| Safety | Do not store essential semantics only in commit messages or fragile headers. Keep authoritative mapping in Sunlight objects. |
| Working tree | The main Git working tree is not the authoring surface for native agents. It is an export/import compatibility surface. |

## 12.1 Repository layout

The MVP should keep the repo understandable while ensuring native source state is not defined by the checked-out project directory. A practical layout is below. Exact filenames can change, but the separation of canonical objects and rebuildable indexes should remain.

| Path | Purpose |
| --- | --- |
| .sunlight/config.toml | Repository identity, schema versions, path policy, projection policy, Git interop settings, and local defaults. |
| .sunlight/objects/ | Canonical object records and content-addressed payloads. Not commit-safe as a whole; subpaths are governed by storage and privacy policy. |
| .sunlight/records/ | Small canonical or mirrored records that are intended to be reviewable and potentially commit-safe after policy validation. |
| .sunlight/local/ | Local-only state: leases, daemon state, machine identity, private defaults, and temporary journals. Ignored by Git. |
| .sunlight/cache/ | Projection inputs, materialization cache, downloaded blobs, and rebuildable acceleration data. Ignored by Git. |
| .sunlight/topics/ | Topic metadata, current revision pointers, dependency declarations, and status. |
| .sunlight/operations/ | Append-only operation transaction records, grouped or sharded for commit-friendly review. |
| .sunlight/views/ | View specifications, resolved view records, resolver inputs, and semantic IDs. |
| .sunlight/checkpoints/ | Frozen resolved views, tree hashes, evidence links, export metadata, and retention markers. |
| .sunlight/executions/ | Execution summaries may be committed after policy approval; raw logs, sandboxes, caches, and large outputs are local or separately published. |
| .sunlight/conflicts/ | Persistent conflict and staleness objects scoped to compositions. |
| .sunlight/index.sqlite | Optional rebuildable local index for fast search/query. Not authoritative; can be regenerated from canonical objects. |
| .sunlight/export-map/ | Mappings from native checkpoints/topics to Git commits, branches, PRs, or patch files. |
| projection cache root | Outside or inside .sunlight/cache depending on policy. Contains cached ordinary-file projections and execution sandboxes; ignored by Git by default. |

## 12.2 Default .sunlight commit policy

The default must be conservative: .sunlight is a repository namespace, not a promise that every child path is safe, small, or public. Early Git transport should commit only sanitized, policy-approved metadata and should ignore local caches and private payloads unless a user explicitly publishes them.

| Class | Default policy |
| --- | --- |
| Commit by default | .sunlight/config.toml after secret scan; schema files; sanitized topic metadata; resolved view/checkpoint manifests; export-map records; small conflict summaries that contain no private payload bytes. |
| Ignore by default | .sunlight/local/; .sunlight/cache/; projection roots; index.sqlite; raw execution sandboxes; raw logs; daemon sockets; temporary journals; quarantine. |
| Policy-gated | Operation payloads, content blobs, generated files, execution artifacts, raw agent provenance, private topic metadata, and large binaries. Publish only through sun publish/export-native or encrypted bundle policy. |
| Validation before Git commit/export | sun doctor/export validates .gitignore, scans for secrets, checks size budgets, rejects private object references in public manifests, and reports derived bytes from private inputs. |
| Native sharing before Git export | When teams need to share unlanded native topics, create an explicit Sunlight bundle or remote sync object with manifest, access policy, encryption decision, and object reachability audit. |

```text
# Suggested generated .gitignore fragment for the MVP
.sunlight/local/
.sunlight/cache/
.sunlight/projections/
.sunlight/tmp/
.sunlight/quarantine/
.sunlight/index.sqlite
.sunlight/executions/**/sandbox/
.sunlight/executions/**/raw-logs/

# Object payloads are not globally ignored because policy may publish some of them.
# The engine must validate exact object reachability before committing or exporting.
```

# 13. Storage, implementation stack, and security posture

## 13.1 Storage decisions

| Area | Decision |
| --- | --- |
| Canonical objects | Use deterministic serialized object records with schema versions. Canonical hashes should be over stable payload bytes, not over human-edited pretty files. |
| Human-reviewable mirrors | Where useful, write readable JSON/JSONL/TOML mirrors for early review and Git diffs. Treat them as canonical only if canonicalization is strict. |
| Content store | Blobs and trees addressed by digest; large files can be chunked later. Checkpoints always preserve canonical bytes. |
| Indexes | SQLite or similar local index is allowed and recommended for speed, but must be rebuildable. |
| Compaction | Operation transactions can be compacted or summarized under retention policy while checkpoints and landed provenance remain durable. |
| Path policy | Declare case sensitivity, Unicode normalization, symlink handling, executable bits, permissions, and platform compatibility in resolved view identity. |

## 13.2 Implementation stack

The core engine should start in Rust. This is a source-control engine, projection manager, content-addressed store, patch applicator, command runner, and Git interop tool. Starting in a low-level systems language avoids an expensive rewrite after behavior becomes compatibility-sensitive.

| Component | Decision |
| --- | --- |
| Core engine and CLI | Rust. Owns object model, hashing, materialization, patching, conflict objects, execution runner, and Git export/import. |
| Local daemon / MCP server | Rust first for MVP simplicity. Elixir can be reconsidered later for supervision, subscriptions, dashboard coordination, and distributed orchestration. |
| Metadata serialization | Schema-versioned canonical JSON/JSONL initially, with clear migration hooks. TOML is fine for user config, not necessarily for hashed canonical records. |
| Index | SQLite as rebuildable local query index. Do not make it the only durable record unless its export/migration story is explicit. |
| Git interop | Shell out to git first for reliability and speed of implementation. Consider libraries only after workflows stabilize. |
| Dashboard | Later TypeScript/React or another fast UI stack. It consumes the same daemon/API; it is not the core. |
| Agent SDKs | Thin wrappers over CLI/API/MCP after the tool vocabulary stabilizes. |

## 13.3 Security and privacy posture

Even a local-first MVP can accidentally make secrets durable by committing .sunlight metadata or exporting a checkpoint. Security cannot be postponed until cloud sync. The local engine needs classification, quarantine, and export validation from the beginning, even if enforcement is simple.

| Policy area | MVP stance |
| --- | --- |
| Capture | Classify source, generated, cache, secret, local-only, binary, and execution-output artifacts. Do not blindly capture everything. |
| Sync/transport | Git transport commits only policy-approved .sunlight records. Private payloads, raw logs, caches, and large objects are local, ignored, encrypted, or published through explicit bundles. |
| Share/publish | Export and public landing must check that private topics, secret blobs, raw agent conversations, or restricted provenance are not disclosed. |
| Secrets | Add detection hooks and quarantine before durable capture/export. Prefer typed vault references over versioning secret bytes. |
| Agent provenance | Store curated task summaries and tool provenance by default. Raw conversations should be opt-in and separately permissioned. |

# 14. Cross-repo intent trees

The unit of intent may be larger than one repository. Sunlight should not implement cross-repo topics in the first local MVP, but it should avoid object and API assumptions that make them painful later. Agent workflows often change a backend API, frontend consumer, shared schema, and contract tests as one coordinated intent.

The core model should therefore define tree_identity as a union now: either a single repository tree or a map from repository ID to tree hash. Single-repo Sunlight is the first specialization of the same model, not a different model that must be broken later.

| Future object | Meaning |
| --- | --- |
| Repo group | A named local coordination context containing multiple repositories, each with its own Git base and Sunlight store or a shared higher-level store. |
| Cross-repo topic | One durable intention whose operation transactions are partitioned by repository but reviewed and checkpointed as one unit. |
| Multi-repo view | A resolved selection across several repositories, each with exact base checkpoints and topic revisions. |
| Cross-repo execution | A command or service graph involving multiple projected repos, ports, env vars, containers, contracts, and result artifacts. |
| Cross-repo checkpoint | Frozen coordinated state containing tree identities for every participating repo plus evidence. |
| Landing group | Linked Git commits, branches, or PRs exported from one native cross-repo checkpoint. |

## 14.1 MVP design hooks

- Include repository_id or repository_scope in object IDs and operation records even for a single repo.
- Do not assume view IDs have exactly one tree forever. A single-repo view can be a special case of a multi-repo resolved view.
- Keep execution records flexible enough to describe multiple working directories, services, and environment contracts later.
- Keep Git export metadata able to map one native checkpoint to more than one Git commit or repository.

# 15. Local MVP plan and acceptance criteria

> **MVP objective** Prove that a coding agent can complete real source changes through Sunlight-native artifact IO without directly editing a project directory, and that those changes compose into exact testable views that export cleanly to Git.

| Phase | Deliverable |
| --- | --- |
| Phase 0: schema and risk spikes | Define object schemas, canonical hashing, path policy, operation transaction format, tree_identity, session generation semantics, .sunlight commit policy, and view identity. Spike reflink/read-only-hardlink projection and command compatibility on a real repo. |
| Phase 1: native IO vertical slice | sun init, topic create, session start, read/list/search, patch/write/move/delete, topic revisions, authored context, and basic status. |
| Phase 2: resolver and conflicts | Resolve base plus topic revisions into deterministic trees. Detect same-file patch conflicts, non-commutative same-artifact writes, and store conflict objects. Create resolved view records. |
| Phase 3: execution projections | Create execution sandbox from a resolved view, run commands such as bun test/npm test/cargo test, capture outputs and execution records, and promote approved source outputs into topic operations. |
| Phase 4: checkpoints and Git export | Freeze resolved views with tree hashes and evidence. Export checkpoint as ordinary Git commit/branch and record mapping. |
| Phase 5: operator ergonomics | Add a minimal dashboard or rich CLI status: topics, views, conflicts, execution matrix, checkpoint/export status. |
| Phase 6: compatibility import | Allow a human or legacy agent to edit a projection and explicitly import the diff into a topic. Keep native API authoring the primary path. |

## 15.1 Acceptance criteria

- A coding agent completes a small source change using read/search/patch/write without directly editing a project directory.
- Each edit is recorded into the correct topic without requiring the agent to stage, commit, rebase, or diff a worktree.
- Multiple agents author against different resolved views without one full repository copy per agent.
- An operator composes selected exact topic revisions into an integration view.
- The system detects at least same-file patch conflicts, including non-commutative same-artifact writes, and stores them as conflict objects.
- A test command runs through an execution sandbox and produces an execution record tied to exact resolved view and environment summary.
- A formatter/codegen/package-manager run can promote approved source outputs into a target topic with execution provenance.
- A checkpoint freezes a resolved view and can be exported as ordinary Git history.
- The .sunlight commit policy is explicit: safe manifests are reviewable, local/cache/private payloads are ignored or policy-gated, and export validation catches unsafe references.
- Projection materialization reports storage amplification and time cost so the many-agent thesis is measured, not assumed.

## 15.2 Explicit local MVP deferrals

| Deferred feature | MVP substitute |
| --- | --- |
| Same-path process-specific filesystem | Prototype early if possible, but do not require it for the first useful agent-native MVP. |
| Full AST-native operation model | Use file-level patch/write/move operations first; add language plugins later. |
| Symbol-level read dependencies | Record full authored context first; add file-level reads opportunistically; symbol tracking later. |
| Automatic semantic repair | Detect conflict/stale/unknown and ask an agent or human to create an adaptation revision. |
| Hosted forge | Keep proposals/channels in the model, but local checkpoint/export is the first implementation target. |
| Cross-repo implementation | Design hooks now; implement after single-repo native IO and projection model works. |
| Polished GUI | A dashboard helps, but the MVP must stand on CLI/API semantics. |

# 16. First vertical slice

The first slice should be small but real. It should use a repository with tests and at least two files that can be edited independently. The test is not whether Sunlight can store metadata; it is whether an agent can use Sunlight as the authoring substrate and an operator can compose and validate the result.

```text
Scenario: two agents, two topics, one composed view

1. Import Git HEAD as main@K100.
2. Create topic auth-nullability.
3. Create topic profile-ui-fix.
4. Agent A uses sun read/search/patch to change the auth model.
5. Agent B starts from a view including auth-nullability@r1 and patches UI handling.
6. Resolve main@K100 + auth-nullability@r2 + profile-ui-fix@r3.
7. Run bun test through an execution sandbox.
8. Create checkpoint K101 from the passing resolved view.
9. Export K101 as a Git branch.
10. Verify native provenance: each changed artifact links to operation -> topic -> session -> view -> execution -> checkpoint.
```

## 16.1 What this slice proves

- Agents can edit without a normal authoring directory.
- Topic ownership is captured at mutation time, not inferred from a later diff.
- A resolved view is usable both through the artifact API and through an execution projection.
- The operator can test a selected combination of topic revisions.
- Sunlight can produce normal Git output while retaining richer native history.

# 17. Risks, mitigations, and decision record

| Risk | Mitigation |
| --- | --- |
| Agents ignore native IO and edit files anyway | Provide MCP/CLI tools as the easiest path; make projection editing an explicit import path; add guardrails that warn when the Git working tree changes outside Sunlight. |
| Existing tools require files | Execution sandboxes are first-class. The system does not deny files; it prevents files from being the authoritative authoring substrate. |
| Projection scalability slips | Treat CAS-backed projection as a Phase 0/1 spike with metrics. Keep full-copy fallback but do not let it define success. |
| Hardlink projection corrupts the store | Prefer reflinks; use hardlinks only read-only with copy-up/overlay behavior and post-run hash verification. |
| Patch model is too weak | Start with deterministic file-level operations and conflict objects. Add structured transforms only where evidence shows value. |
| Git export loses provenance | Use export mapping and checkpoint metadata as authoritative. Git commits are lossy views with labeled provenance links. |
| .sunlight accidentally commits private or huge objects | Generate a conservative .gitignore, classify objects, and require policy validation before commit/export/publish. |
| Metadata grows too quickly | Use retention classes, compaction, checkpoint snapshots, cache GC, and clear output classification from the start. |
| Users do not understand new terms | Keep the core vocabulary small: topic, session, view, checkpoint, run, export. Hide database terms in advanced UI. |
| Secrets get captured into .sunlight | Add local classification, quarantine, and export validation before encouraging teams to commit policy-approved .sunlight metadata. |

## 17.1 Decision record

| ID | Decision | Rationale |
| --- | --- | --- |
| D1 | Build native source database first | This protects the project from becoming Git worktree orchestration. |
| D2 | Make artifact API the primary authoring path | It creates structured, topic-owned operations by construction. |
| D3 | Split workspace into authoring session and projection | It removes ambiguity and prevents directory materialization from becoming the model. |
| D4 | Use projections for tools, execution, inspection, and export | It preserves compatibility without giving up native semantics. |
| D5 | Do not assume full-copy workspaces | Many-agent scale is a core problem, not an optimization afterthought. |
| D6 | Start with file-level operation transactions | It is implementable and deterministic while leaving room for structured plugins. |
| D7 | Record exact authored context in MVP | Precise read dependencies can come later; reproducibility cannot. |
| D8 | Use Rust for the core engine | The core owns low-level IO, hashing, patching, projection, execution, and Git interop. |
| D9 | Use Git as interop/export/transport | This enables adoption while keeping native provenance truthful. |
| D10 | Keep cross-repo intent in the architecture | A future topic may span multiple repos; object IDs and views should not block that. |
| D11 | Treat deterministic resolver order as insufficient for correctness | Non-commutative same-artifact writes require dependency order or explicit resolution, not arbitrary topic order. |
| D12 | Do not equate hardlinks with reflinks | Hardlinks require read-only/copy-up/verification rules because in-place writes can corrupt shared immutable storage. |
| D13 | Make .sunlight Git transport policy explicit | Only safe records are commit-default; private payloads, caches, raw logs, and large objects are ignored or policy-gated. |
| D14 | Add execution-output promotion | Tool-produced source changes become topic operations through an explicit promotion path with execution provenance. |
| D15 | Define tree identity as single-tree or repo-map | The single-repo MVP should not contradict the future cross-repo model. |
| D16 | Define session generations after writes | Agents need a precise read-your-writes guarantee and IDs returned after mutations. |

# 18. Product decisions still needed

The architecture above is intentionally opinionated. The remaining decisions should come from product priorities, target users, and the first repo chosen for validation.

| Decision needed | Why it matters |
| --- | --- |
| First target agent integration | Should the first native integration be MCP, a CLI that agents call, a specific coding-agent wrapper, or all through a local daemon? |
| First validation repo | Which real project, language, package manager, and test command should define the vertical slice? |
| Projection platform target | Which OS/filesystem matters first for reflink-first and read-only-hardlink projection: macOS/APFS, Linux/Btrfs/XFS/ext4, or Windows/ReFS/NTFS? |
| Compatibility import priority | How important is it that humans or filesystem-only agents can edit projections in the first MVP? |
| Git export shape | Should the default export be one checkpoint commit, one commit per topic, or a curated series? |
| UI expectation | Is a rich dashboard necessary for early testing, or is CLI/API status enough? |
| Raw agent provenance | Should raw prompts/conversations be captured anywhere locally, or only curated summaries and tool logs? |
| Cross-repo timing | Should cross-repo intent remain pure future architecture, or should object IDs and CLI names expose repo groups from day one? |

# 19. Final summary

The idea to protect is simple: do not make agents manage Git more efficiently. Give them a source database whose native units are intention, context, composition, and evidence. Then project that model into files, Git commits, pull requests, synchronized folders, or future multi-repo checkpoints as needed.

The strongest local MVP is therefore native IO first, topics and views first, exact execution first, safe resolver semantics first, explicit .sunlight policy first, Git export first, and projection scalability measured early. Managed directories and filesystem diff capture remain valuable adapters, but they must not define the architecture.

> **North star** Sunlight wins if a human can coordinate dozens of agents across many exact views with less worktree sprawl, less branch ceremony, stronger provenance, and more trustworthy test evidence than Git-centric workflows can provide.

# Appendix A. Quick reference

| Term | Short meaning |
| --- | --- |
| Topic | Durable intention with many revisions. |
| Session | Topic-bound authoring context over one resolved view. |
| View | Selection of base and topics; resolved view pins exact revisions. |
| Projection | Directory-like adapter for tools, not source truth. |
| Execution | Command result tied to exact resolved view and environment. |
| Checkpoint | Frozen resolved view with canonical tree identity. |
| Git export | Lossy compatibility projection from checkpoint to ordinary Git history. |

# Appendix B. Minimal command vocabulary

```text
sun init
sun topic create <name>
sun session start --topic <topic> --view <view>
sun session refresh <session> --policy manual|follow|none
sun read <path> --session <session>
sun list <path> --session <session>
sun search <query> --session <session>
sun inspect <path> --session <session>
sun patch <path> --session <session> <patch>
sun write <path> --session <session> --content <file>
sun move <from> <to> --session <session>
sun delete <path> --session <session>
sun view resolve --base <ref> --include <topic@rev>...
sun project materialize <view> --purpose execution|compat|export
sun run <view> -- <command>
sun execution promote-output <execution> --to-topic <topic> --select <paths>
sun checkpoint create <view>
sun publish <topic|checkpoint> --policy <policy>
sun git export <checkpoint>
```

# Appendix C. Source context used

This consolidated document was written from three source inputs: the original Agent-Native Version Control working design, the Sunlight Local Repo MVP Specification, and the Sunlight Plan Changes Handoff. Version 0.3 also incorporates the review notes about resolver ordering, projection corruption safety, .sunlight transport policy, execution-output promotion, cross-repo tree identity, and session freshness. It intentionally replaces contradictions between those files with the native-source-database-first architecture described here.

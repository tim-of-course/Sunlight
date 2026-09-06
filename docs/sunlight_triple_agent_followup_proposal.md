# Triple-Agent Evaluation Follow-up Proposal

**Status:** Approved and implemented; runtime binding performance remains a follow-up
**Date:** 2026-09-01
**Scope:** Native authoring ergonomics, topic cleanup, artifact search, and execution latency

## Purpose

The latest blind TasGrid evaluation proved that three agents can discover
Sunlight independently, author against pinned views, and advance one canonical
checkpoint without losing one another's work. It also exposed four concrete
problems:

1. two agents initially submitted the same invalid multi-file patch shape;
2. one difficult edit was mistaken for a concurrency problem and expanded into
   eight topics, leaving obsolete completed and open topics behind;
3. one agent guessed repeatedly after `artifact_search` rejected a conventional
   `limit` argument; and
4. a warm no-op execution took 10.659 seconds even though the runtime layer and
   source projection were cache hits.

The changes below keep the established model: one pinned session per task,
immutable topic revisions, explicit canonical integration, exact execution
views, and a private writable runtime layer for every execution.

## Independent review outcome

The independent review approved the editing and search direction, required an
exact-head check for topic abandonment, and found the execution design too
broad to implement as one change. This revision adds those safeguards, bounds
long search lines, defines one `ExecutionStore` boundary for every execution
consumer, and separates runtime-layer binding from execution-record storage so
each optimization is measured on its own.

The first review suggested validating every clone-ready descendant permission
on cache reuse. A post-implementation review identified the more important
problem: writable cached descendants could be reached by a command while
filesystem isolation is unenforced. The implemented correction keeps the
complete cached target read-only and makes only the private copy-on-write clone
writable. This restores a metadata walk but does not copy dependency bytes.

## Decision 1: make patch recovery obvious without adding another patch API

Keep the existing single-file and atomic `edits` forms of `artifact_patch`.
Do not add a second combined-patch payload, infer missing compare-and-swap
hashes, or silently split an ambiguous patch.

Clarify the MCP contract and generated guidance:

- A single-file call uses top-level `path`, `expect_hash`, and `patch`.
- A multi-file call uses `edits` only.
- Each `edits[i].patch` contains hunks for exactly `edits[i].path`. It must not
  repeat a combined multi-file `*** Begin Patch` envelope.
- The schema description includes one short two-file example.

Add a preflight check in the shared command implementation so the CLI and MCP
server behave identically. Inspect only explicit `*** Update File:` markers.
Conventional `---` and `+++` labels are too ambiguous to use as scope. A batch
member may declare zero or one update path. When present, it must equal
`edits[i].path`; multiple declarations or a different path return
`patch_scope_mismatch` before any mutation.

The error details include `edit_index`, `path`, `declared_paths`, `session_id`,
`session_generation_id`, `resolved_view_id`, `current_content_hash`, and
`state_changed: false`, followed by this recovery action:

> Split the patch into one patch per `edits` entry, reread only paths whose
> hashes changed, and retry in the same session. Do not refresh the session or
> create another topic.

Strengthen `precondition_failed`, `patch_parse_failed`, and
`patch_apply_failed` the same way. Their structured details include
`recoverable_in_same_session: true`, the unchanged session facts,
`state_changed: false`, and the current hash when available. Scope, parse, and
apply failures are local patch-construction errors. A precondition failure is a
stale compare-and-swap within the same pinned session and requires rereading
only the affected path. None is evidence that another topic moved the pinned
session.

This is deliberately a guidance and error-quality change. The existing batch
transaction and compare-and-swap implementation remain unchanged.

## Decision 2: bound artifact search and accept the conventional limit

Add optional `limit` to the CLI and MCP `artifact_search` contract:

- integer range: 1 through 200;
- default: 50;
- stop collecting after `limit + 1` matches; and
- return `returned`, `limit`, and `truncated` with the matches.

Walk paths in sorted order and lines in ascending order, then stop after
`limit + 1` matches. Replace the full matching line with a match-centered
snippet of at most 512 UTF-8 bytes, without splitting a code point, and return
`snippet_truncated` for each match. The snippet may omit part of an unusually
long match. This bounds the response even when source files contain very long
lines.

The search remains a case-sensitive literal substring scan. Do not add regex,
ranking, indexing, pagination, or path filters in this change.

For every MCP tool, an unknown-argument error should return both the rejected
argument and `allowed_arguments`. No fuzzy correction system is needed.

This makes the first search call from the evaluation valid and prevents an
unbounded search response from exceeding the MCP response limit.

## Decision 3: keep pinned-session recovery simple and add topic abandonment

Do not add automatic rebasing, automatic session refresh, or replacement-topic
creation. Unrelated canonical advancement still cannot change a pinned
authoring session. A failed patch should be corrected with a new revision in
that same session.

Update the session and mutation guidance around one rule:

> Continue editing in the current session after read, hash, parse, or patch
> failures. Use `session_refresh` only when intentionally adopting newer heads
> for non-write topics already selected by the session.

Add one lifecycle operation, `topic_abandon`, for work that is intentionally no
longer a candidate:

- Inputs are the exact `topic`, an owning `session`, the expected head revision
  or `none`, and a short factual `reason`.
- The owning session supplies the actor. The command publishes one canonical
  state compare-and-swap that checks the expected head, canonical frontier, and
  existing disposition. Correctness does not depend on the MCP repository
  queue, so concurrent direct CLI calls are safe too.
- If unrelated canonical state changes before publication, reload and retry the
  same checks. If the topic head changed or entered the canonical frontier,
  return the exact stale-head or canonical-inclusion error.
- It may close an open topic or add a later disposition to a completed topic
  that is absent from the canonical frontier.
- It is rejected if the expected head is stale or any revision from the topic
  is present in the canonical checkpoint. Accepted work is never made to look
  discarded.
- It writes an immutable abandonment record containing the actor, session,
  expected head, reason, and time. Completion facts and revisions remain
  unchanged and inspectable.
- Repeating the identical request is an idempotent no-op. A different expected
  head or reason conflicts with the existing disposition.
- New sessions, mutations, and completion are rejected after abandonment.
  Existing pinned sessions remain readable but non-writable.
- Abandoned topics are excluded from pending-completed-topic warnings,
  completion guards, and recommended integration candidates.
- Repository status reports an abandoned count. Detailed topic status remains
  available by exact ID and shows both completion and abandonment without
  putting abandoned heads in the normal actionable list.

Repository-level conflict and staleness warnings should likewise count only
records reachable from the canonical checkpoint or a current open or completed
non-abandoned topic head. Preserve older failed-resolution evidence for exact
inspection, but do not present it as current repository work.

Do not add a separate superseded state or replacement graph. A reason may name
the replacement topic when useful. One abandoned state is enough to clean up
failed experiments, alternatives, and empty accidental topics.

Agent guidance should still prefer one topic per task. If an agent truly must
replace a topic, it abandons the old topic as soon as the replacement is known
to be valid.

## Decision 4: optimize execution in two independently measured changes

The measured warm no-op had this response timing:

| Phase | Time |
| --- | ---: |
| Source projection materialization | 148 ms |
| Runtime-layer cache lookup | 84 ms |
| Private runtime-layer binding | 2,742 ms |
| Command stage | 1,951 ms |
| Output scan | 274 ms |
| Final publication | 2,376 ms |
| Total | 10,659 ms |

About 3.08 seconds was outside the named phases. Code inspection also shows two
independent costs: each private binding recursively changes permissions, and
execution start and finish repeatedly load and rewrite canonical repository
state. Implement and benchmark these separately so a regression can be located
and reverted without mixing the two changes.

### Change 4A: complete the timings and protect cached content

First make the timing record honest. Persist disjoint phases that finish before
terminal publication, plus a `prepublication_total_ms`. Return terminal-record
publication, cleanup, and end-to-end time in the command response because the
record cannot contain the duration of its own write. Nested diagnostic
subphases must not be added to their parent a second time. Timings are
observational metadata and do not affect content, checkpoint, or policy
identity.

Protect the complete runtime-layer target when a cache entry is published:

- Remove write permission from cached regular files while preserving executable
  bits.
- Remove write permission from cached descendant directories without removing
  search permission.
- Never follow or change symlinks.
- Keep the cache entry root, manifest, target parents, target root, and every
  target descendant protected. Record
  `binding_layout: protected_content_private_repermission_v1` in the atomic
  manifest.

On macOS, use `clonefile`, then recursively grant write permission inside only
the private destination. Preserve the existing copy-on-write and full-copy
fallbacks on other filesystems. The permission walk is accepted because an
unenforced command must not be able to alter bytes reused by later executions.

Cache reuse still validates the protected root, manifest identity, lookup
inputs, binding layout, and protected target root. An entry lacking the new
layout marker is rejected and rebuilt. Reuse must not add a recursive
permission scan, which would recreate the cost this change removes. This
accepts the existing local-cache trust boundary: the repository owner can
deliberately alter local cache bytes today. Commands never receive the cache
path, and isolation tests must prove that changing one private binding cannot
change the cache, another execution, or the human worktree.

### Change 4B: move operational execution state behind an `ExecutionStore`

After measuring 4A, make the existing per-execution JSON record authoritative
for new execution state. Keep topics, sessions, views, checkpoints, and the
canonical checkpoint in the canonical repository record. The
`ExecutionStore` is the only boundary used for:

- execution status and exact inspection;
- checkpoint evidence validation and output promotion;
- execution projection lookup;
- repository counts, warnings, and pending-promotion calculation;
- ID reservation; and
- crash recovery.

The smallest safe publication flow is:

1. Reserve an execution ID with an exclusive reservation file and acquire a
   per-execution lock.
2. Atomically publish the projection record, including its `execution_id`.
3. Atomically publish the running execution record with the current runner PID
   and a random runner-instance token. Only this second record makes the
   execution visible.
4. Launch the command while the runner retains the per-execution lock.
5. Publish the terminal record with an atomic replacement that requires the
   same runner-instance token.

Terminal publication and crash recovery use the same per-execution lock, so
they cannot both win. Recovery ignores or removes an orphan projection that
has no running execution record only after acquiring the lock named by its
`execution_id`; this avoids mistaking the normal gap between steps 2 and 3 for
a crash. A stranded running record is recovered only after acquiring its lock
and verifying its recorded runner state. Recovery records `interrupted` with
`command_outcome: unknown`, retains or quarantines the projection for
inspection, and never makes its outputs promotable. No start, finish, or
recovery step acquires the canonical authoring queue.

Status and exact inspection read atomically published records without taking
the long-held execution lock.

The store reads standalone records first and uses legacy embedded execution and
projection arrays only when no standalone record exists for that ID. IDs are
deduplicated. New writes use only standalone records. This is read-old and
write-new behavior, not a public schema-version system, and it preserves
existing execution evidence without requiring repository reinitialization.

Ordinary counts may parse the few needed fields from each record. Do not add a
second compact-header representation unless measurement later proves it is
needed.

## Recommended implementation order

1. Ship patch scope diagnostics, bounded search, and same-session recovery
   guidance together. They share the agent-facing surface and do not change the
   history model.
2. Add topic abandonment as its own reviewed change because it extends durable
   lifecycle state.
3. Add complete timing accounting and protected cache publication, then run the
   warm benchmark before changing execution storage.
4. Introduce `ExecutionStore`, migrate every listed consumer, and rerun the
   concurrency, recovery, compatibility, and latency tests.
5. Run the blind recovery scenario only after deterministic tests pass.

Do not combine steps 3 and 4 in one implementation review. Their performance
effects and failure modes are independent.

## Acceptance criteria

### Agent ergonomics

- The invalid repeated multi-file patch shape from the evaluation returns one
  precise `patch_scope_mismatch` response and tells the caller to remain in the
  same session.
- A correct two-file batch produces one transaction and one topic revision.
- Every rejected patch leaves the publication sequence, topic head, and session
  generation unchanged.
- `artifact_search(query, limit: 50)` succeeds and reports whether results were
  truncated.
- Search ordering and truncation are deterministic. A fixture with very long
  matching lines remains below the MCP transport limit.
- Abandonment tests cover a stale expected head, canonical inclusion, exact
  idempotency, rejection of later writes, and preserved completion inspection.
- An abandoned completed topic remains inspectable and no longer appears in
  the completion guard.
- Conflicts and staleness from an abandoned or replaced attempt remain
  inspectable but disappear from current repository warnings.

### Execution correctness and performance

- Three concurrent executions with identical dependency inputs still reuse one
  content-addressed runtime layer and receive distinct writable bindings.
- Mutating one execution's dependency tree changes neither the cached layer nor
  another execution's tree.
- Failpoint tests cover a crash before projection publication, between
  projection and execution publication, and after running publication. The
  last case becomes `interrupted` with an unknown command outcome and cannot be
  promoted.
- Legacy and standalone execution records both support status, exact
  inspection, output promotion, projection lookup, and checkpoint evidence.
- After exact view resolution, execution-record start and terminal publication
  do not wait on the canonical authoring queue. Holding an execution-record
  lock does not delay an authoring mutation.
- A crashed running execution remains inspectable and recoverable, while an
  orphan projection never appears as an execution.
- On the same TasGrid view and Mac, discard one warm-up and benchmark at least
  20 warm `/usr/bin/true` runs. Median end-to-end time is at most 4 seconds and
  p95 is at most 5 seconds.
- Median private runtime-layer binding is at most 1.5 seconds.
- With at least 150 historical executions, p95 combined execution-record start
  and finish publication is below 250 ms.
- Persisted disjoint phases explain `prepublication_total_ms` within 10 ms.
  Response-only phases explain response end-to-end time within 10 ms.

### Blind evaluation evidence

After deterministic tests pass, run a focused blind-agent scenario. Give the
agent an ordinary multi-file task that naturally triggers search and one
correctable patch failure. Success means it stays in one topic and pinned
session, uses bounded search without guessing arguments, corrects the patch,
and completes the task. This is evaluation evidence rather than a deterministic
product test; record the scenario and success count separately.

## Implementation evidence

The implementation adds same-session patch recovery details, bounded artifact
search, exact topic abandonment, protected runtime-layer caches, complete
phase timing, and standalone execution and projection records with per-run
locking and crash recovery. New execution records do not rewrite canonical
repository state. Legacy embedded records remain readable by status,
inspection, promotion, projection lookup, and checkpoint evidence consumers.

The macOS workspace suite passed all 502 tests. The release build completed
without warnings. The execution-store failpoint tests cover every publication
boundary, and the MCP concurrency tests prove execution record publication no
longer waits on the canonical mutation queue.

A release-build benchmark used TasGrid view
`view_fixture_2ec98cbaa9de4ebb` and `/usr/bin/true`. The initial clone-ready
layout reached a 2.001-second median and 2.524-second p95, but that result is
superseded because cached descendants were writable. After recursively
protecting cached content, 20 warm runs had a 3.991-second median and
5.092-second p95 end-to-end time. Median private runtime-layer binding was
2.599 seconds. With more than 150 historical executions, p95 combined start and
terminal publication was 105 ms.

The protected implementation meets the median end-to-end and publication
targets. It misses the p95 end-to-end target by 92 ms and the private-binding
target. Cache integrity takes priority over those performance thresholds. The
remaining optimization must preserve read-only shared content and private
writable execution trees.

## Non-goals

- Automatic topic merging, rebasing, or refresh.
- Mutable dependency sharing between executions.
- Skipping a required runtime layer based on guessed command relevance.
- A new runtime provider or package-manager-specific fast path.
- Regex search, source indexing, or a general query language.
- Deleting immutable topic history or failed execution evidence.

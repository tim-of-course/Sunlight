# Direct Worktree Capture — Approved Addendum v0.1

**Decision status:** Approved

**Approved:** 2026-08-27

**Implementation status:** Implemented in the v0.3 local CLI and MCP workflow

This document is an approved addendum to
[`sunlight_consolidated_architecture_v0_3.md`](sunlight_consolidated_architecture_v0_3.md)
and
[`sunlight_compatibility_import_v0_1.md`](sunlight_compatibility_import_v0_1.md).
It governs direct capture from the repository worktree. It does not make the
worktree, Git, or compatibility projections native source truth.

## Goal

Allow ordinary edits made in the repository worktree by a human, editor, or
non-Sunlight agent to enter Sunlight efficiently, with full topic provenance and
without copying the worktree into a separate compatibility projection.

Direct worktree capture is an explicit compatibility path. Native artifact
operations remain the preferred authoring path for Sunlight-aware agents.

## Approved decision

Sunlight will durably track a **worktree anchor**. The anchor identifies the last
exact Sunlight view accepted as the repository root's baseline. It describes the
current repository files exactly only while the worktree is clean relative to
that anchor.

`repository_init` creates the first anchor from the ingested base checkpoint and
base resolved view. A successful, non-empty worktree capture advances it to the
resulting exact resolved view. A failed or empty capture does not advance it.

The anchor records or references:

- the base checkpoint IDs;
- the exact resolved view and its topic frontier;
- the exact tree identity and manifest digest;
- the applicable path-policy and operation-semantics versions;
- the approved `.sunignore` policy identity; and
- a monotonically increasing anchor generation.

The canonical resolved view and content store remain authoritative. The anchor
must reference them rather than maintain a second authoritative copy of their
manifest. A local per-path scan cache may be used for performance, but it is
derived, disposable, and never source truth.

Git metadata may aid diagnostics and change discovery, but Git HEAD, the index,
and `git status` do not determine the Sunlight baseline.

For v0.1, the anchor advances only during initialization, an approved
reinitialization, or a successful non-empty worktree capture. Native authoring,
view resolution, execution, checkpoint creation, and Git export do not advance
it because they do not rewrite the repository root. A future command that
explicitly materializes an exact view into the root may define its own safe
anchor-advance behavior.

## Capture behavior

Before capture, worktree differences remain external and do not affect native
source truth.

A capture command such as:

```text
sun compat capture --worktree --topic <slug> --actor <actor-id>
```

performs this workflow:

1. Read the current worktree anchor and generation.
2. Scan visible repository files and classify differences from the anchored
   view using the existing compatibility-diff rules and path policy.
3. Bind the selected candidates to the anchor generation and their exact before
   and after content and supported metadata.
4. Verify that the anchor and selected worktree facts have not changed.
5. Create a new actor-owned topic and session pinned to the anchor's exact view.
6. Record all selected changes as one multi-effect operation transaction and one
   topic revision, then complete that revision as an immutable factual handoff.
7. Resolve the anchored view plus the completed revision and advance the anchor
   to the resulting exact view. The repository files are not rewritten.

The topic, session, operation, revision, completion, resolved view, and new
anchor are published together. If native publication fails, none of those
records or the anchor advance becomes visible.

Sunlight cannot freeze an editor or other process that is changing ordinary
files. Capture therefore guarantees an atomic Sunlight publication of the exact
bytes and supported metadata it verified; it does not claim an operating-system
snapshot of a live directory. If relevant files change during capture, capture
fails without publishing native records and returns facts that direct the caller
to diff and retry. A change made after successful verification is simply a new
external difference from the new anchor.

The default completed topic preserves the captured edit as an independent,
immutable change. Completion records a factual handoff; it does not claim that
the change has passed validation or is ready for a checkpoint. Concurrent
Sunlight topics are not included in the capture unless they were already part of
the anchored view. They remain available for normal exact-view resolution,
where real overlaps become explicit conflicts.

After a successful full capture, running capture again without further visible
filesystem edits is an idempotent no-op. It creates no records and does not
advance the anchor generation.

Using an existing open topic or session is outside v0.1. It may be considered
later if a clear need justifies the added ownership and view-compatibility rules.

## Partial capture

The caller may select exact candidate IDs or request a path selection. Candidate
IDs are authoritative. A path selection expands deterministically to complete
candidate groups so that a rename or related multi-path change cannot be split
accidentally.

After partial capture:

- the anchor advances to the exact view containing the selected changes;
- unselected worktree changes remain external and visible in the next diff; and
- a later capture is based on the updated anchor and therefore follows the
  earlier capture in topic history.

Partial capture does not split one worktree snapshot into several independent
topics. A future multi-topic partitioning workflow would require a separate
design.

## Operator and agent surface

The CLI and MCP server expose equivalent operations:

- **worktree diff:** inspect exact classified candidates without mutation;
- **worktree capture:** capture all eligible candidates or an exact selection;
  and
- **repository status:** show the worktree's relationship to its anchor and the
  next relevant action.

`repository_status` reports:

- worktree state such as `clean`, `dirty`, `scan_required`, or `unavailable`;
- the anchor's checkpoint, view, tree, manifest, and generation identities;
- exact candidate counts when an exact scan is available, with freshness made
  clear otherwise; and
- a direct next action to call worktree diff or capture when appropriate.

Ordinary repository status need not expose file paths. Explicit worktree diff
returns paths, classifications, candidate IDs, selection facts, and the anchor
generation to which they apply.

The Sunlight-relative summary is the primary worktree signal. Git HEAD, index,
and working-tree transitions remain separate structured diagnostics because a
branch or index change may explain a large worktree difference, even though it
never changes the Sunlight baseline.

Status and diff discovery must remain practical on open-alpha repository sizes.
An implementation may use Git facts and a derived local scan cache to narrow
work, but capture must verify selected content against the Sunlight anchor before
publication.

## Path and change boundaries

`.git/` and `.sunlight/` are always excluded. Git-ignored untracked files and
`.sunignore`-excluded paths retain their existing boundaries. Paths already
present in the anchored view remain part of the baseline even if a later Git
ignore rule would otherwise hide them.

`.sunignore` remains human-owned policy. If it changes from the policy recorded
by the anchor, worktree capture stops before the new policy can affect candidate
discovery and directs the human through the existing reinitialization workflow.

The direct scanner must preserve the repository-ingestion path-safety rules.
Symlinks, junctions, reparse points, or path changes that could escape the
repository root are rejected rather than followed or captured.

For v0.1:

- metadata-only capture means the supported executable-file bit;
- timestamps, ACLs, extended attributes, and platform-specific metadata are not
  captured; and
- a rename is reported as a rename only when identity is exact and unambiguous.
  Rename-plus-edit and ambiguous cases remain separate changes or explicit
  ambiguity rather than guessed identity.

## Source-truth and checkpoint semantics

Uncaptured worktree edits remain outside Sunlight and cannot enter native reads,
execution, checkpoints, or Git export content. Worktree status and diff are
observations only.

Capture creates topic history, not a checkpoint. The completed captured revision
must still be combined into an exact resolved view, validated as appropriate,
and checkpointed through the normal lifecycle.

The worktree anchor is durable local adapter state. It records what native view
the worktree baseline corresponds to, but it does not replace that view or turn
filesystem contents into source truth.

## Acceptance criteria

- Initialization records generation one of the worktree anchor for the ingested
  base view and reports the unchanged worktree as clean.
- Direct modified, added, deleted, exact unambiguous renamed, and executable-bit
  changes appear as deltas relative to that anchor.
- One non-empty capture produces one new completed topic, one actor-owned
  session, one atomic multi-effect operation transaction, and one topic revision
  with the prior anchor's exact generation, view, tree, manifest, and candidate
  provenance.
- A successful full capture produces a resolved manifest identical to the
  verified visible worktree paths, bytes, executable bits, and deletions. A
  second unchanged capture is a no-op and does not advance the generation.
- Partial capture advances the anchor through only the selected changes and
  leaves every unselected change visible in the next worktree diff.
- Concurrent Sunlight topics are neither overwritten nor silently folded into
  capture; normal view resolution exposes genuine conflicts.
- Two captures racing from the same anchor generation cannot both publish. One
  may succeed; the stale attempt creates no topic, session, operation, revision,
  completion, or anchor advance.
- Relevant files changing during capture produce either one verified result or
  a no-record stale failure; mixed or silently guessed contents are never
  published.
- A changed `.sunignore` blocks capture and directs the human to the existing
  reinitialization workflow.
- Ignored and intrinsic paths remain excluded, and path escapes through links or
  reparse points are never read or captured.
- Repository status exposes the Sunlight-relative worktree state and relevant
  next action while retaining Git transitions as separate diagnostics.
- Uncaptured edits never affect native reads, execution, checkpoints, or Git
  export.

## Implementation record

Implemented on 2026-08-27. The local CLI exposes `sun compat diff --worktree`
and `sun compat capture --worktree`; MCP exposes the equivalent
`worktree_diff` and `worktree_capture` tools. `repository_status` reports the
anchor and exact current candidate summary.

Repository-backed regression coverage exercises the complete change set,
partial and repeated capture, stale anchors, ignored paths, native-source
isolation, concurrent topic independence, durable provenance, and MCP discovery
and execution.

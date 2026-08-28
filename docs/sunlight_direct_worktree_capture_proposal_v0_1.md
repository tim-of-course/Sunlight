# Direct Worktree Capture Proposal v0.1

## Goal

Allow ordinary edits made in the repository worktree by a human, editor, or
non-Sunlight agent to enter Sunlight efficiently, with full topic provenance and
without copying the worktree into a separate compatibility projection.

## Core decision

Sunlight must durably track a **worktree anchor**: the exact checkpoint and
resolved view currently represented by the repository root. The anchor is known
state, not something capture must infer or prove afterward.

`repository_init` creates the first anchor from the ingested base checkpoint and
base resolved view. Whenever Sunlight intentionally advances what the repository
root represents, including a successful external-edit capture, it advances the
anchor atomically to the resulting exact resolved view.

The anchor contains:

- base checkpoint ID;
- exact resolved view ID and tree identity;
- visible-path baseline manifest and content hashes;
- monotonically increasing anchor generation.

Git metadata may aid diagnostics and change discovery, but Git HEAD, the index,
and `git status` do not determine the Sunlight baseline.

## Capture behavior

Repository status compares the worktree directly with the anchored view. Before
capture, differences remain external and do not affect native source truth.

A capture command such as:

```text
sun compat capture --worktree --topic <slug> --actor <actor-id>
```

performs one atomic workflow:

1. Read the current worktree anchor and generation.
2. Diff visible repository files against the anchor manifest, reusing the
   existing compatibility-diff classification and path policy.
3. Create a new topic based on the anchor's checkpoint and a session pinned to
   the anchor's exact resolved view. An explicitly selected compatible open
   topic/session may be supported as an override.
4. Record the selected deltas as one multi-effect operation transaction and one
   new topic revision.
5. Resolve the anchored view plus that revision and advance the worktree anchor
   to the resulting exact view. The repository files are not rewritten.

The default new-topic path preserves the external edit as an independent change.
Other completed topic revisions can then be combined through normal view
resolution, where genuine overlaps become explicit conflicts instead of silent
merges or reversions.

After a successful capture, running capture again without further filesystem
edits is an idempotent no-op. A failed capture creates no topic, session,
operation, revision, or anchor advance.

## Operator and agent surface

`repository_status` should replace the current Boolean Git-dirty warning with a
Sunlight-relative summary containing candidate counts and the anchor's exact
checkpoint, view, tree, and generation IDs. It need not expose file paths unless
the caller explicitly requests the worktree diff.

The CLI and MCP server should expose equivalent operations:

- worktree diff: inspect classified candidate deltas without mutation;
- worktree capture: create the new topic/session/revision atomically;
- optional path or candidate selection for partial capture.

`.git/`, `.sunlight/`, Git-ignored untracked files, and `.sunignore`-excluded
paths retain their existing boundaries. A `.sunignore` policy change remains a
separate human-owned reinitialization workflow.

## Source-truth and checkpoint semantics

Uncaptured worktree edits remain outside Sunlight and cannot enter execution,
checkpoint, or export content. Capture creates topic history, not a checkpoint.
The captured topic revision must still be combined into an exact resolved view,
validated as appropriate, and checkpointed through the normal lifecycle.

## Acceptance criteria

- Initialization records a worktree anchor for the ingested base view.
- Direct modified, added, deleted, renamed, and metadata-only changes appear as
  deltas relative to that anchor.
- One capture produces one new topic, one actor-owned session, one atomic
  operation transaction, and one topic revision with exact anchor provenance.
- Concurrent Sunlight topics are neither overwritten nor silently folded into
  the capture; normal view resolution exposes conflicts.
- The post-capture resolved tree is content-identical to the captured visible
  worktree state, and a second unchanged capture is a no-op.
- Uncaptured edits never affect native reads, execution, checkpoints, or Git
  export.

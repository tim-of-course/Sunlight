---
name: sunlight
description: Use Sunlight for repository-confined source inspection, topic-owned edits, exact multi-agent coordination, validation, checkpointing, and Git export. Trigger when the user asks to use Sunlight, when a repository exposes Sunlight MCP tools, or when multiple coding agents need isolated changes without worktrees or full source copies.
---

# Sunlight

Use the repository-bound Sunlight MCP server as source truth for a
Sunlight-native task. Exercise engineering judgment; preserve exact identities,
preconditions, and handoffs rather than following a rigid script.

## Begin safely

1. Find the MCP server that exposes `repository_status` and the Sunlight
   artifact tools. Server names may be repository-specific.
2. Call `repository_status`. Initialize through `repository_init` only when the
   bound root is uninitialized. Use `repository.recommended_start` as the
   default exact checkpoint, view, and tree for new work.
3. Read `repository.worktree`. If it reports external changes, use
   `worktree_diff` to understand them. Call `worktree_capture` only when those
   edits should enter Sunlight; it returns a completed topic revision for the
   normal integration workflow.
4. If the tools are missing, stop instead of silently editing tracked source by
   another route. Read [references/setup.md](references/setup.md) and give the
   user the relevant install or doctor command.

## Author and coordinate

Read [references/workflow.md](references/workflow.md) before beginning native
authoring or integration.

- Create one bounded topic and one actor-owned session from an exact view.
- Keep that session pinned while unrelated topics resolve or conflict. The
  session remains writable until its own topic is completed.
- Inspect and mutate tracked artifacts through Sunlight. Use returned hashes as
  compare-and-swap preconditions.
- Keep authoring scoped to that exact session while other topics are open;
  their changes matter only when selected for integration.
- Complete work at the exact head revision with a factual immutable handoff.
- Use `topic_wait` only for an explicit task dependency. Use `view_resolve` with
  the recommended checkpoint and exact selected revisions for integration; the
  checkpoint frontier is carried forward automatically.
- Run validation on the exact combined view. Promote only intentional outputs,
  then create a checkpoint from matching passing evidence using the recommended
  checkpoint as the canonical compare-and-swap precondition. Retry integration
  on a concurrent canonical advance. Create a side checkpoint only for a
  user-requested isolated or alternative result.
- Export to Git only when the user requests that compatibility handoff.

Treat conflicts, staleness, policy failures, and execution results as facts to
inspect. Do not bypass them through a working tree, projection, moving head, or
another agent's session.

## Report

Immediately before the final response, call `repository_status` and read
`completion_guard`. If task-owned edits remain outside native history, inspect
them with `worktree_diff` and capture only those edits. Preserve unrelated
external edits and report them as a separate worktree condition; they do not
invalidate an existing native handoff.

Report the returned topic and checkpoint `handoff.copy_report` strings
verbatim. Do not reconstruct their IDs. State what was validated and whether
any tracked-source access occurred outside Sunlight. Do not claim implemented
work without a native topic or checkpoint handoff for the task-owned changes.

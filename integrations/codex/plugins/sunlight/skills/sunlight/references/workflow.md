# Sunlight agent workflow

Use Sunlight as the source of truth when the user requests Sunlight-native
authoring or when the repository's instructions require it. Do not silently
mix native operations with direct tracked-source edits.

## Core objects

- A **topic** owns one bounded change and its immutable revision history.
- A **session** gives one actor write access to one topic over an exact view.
- A **resolved view** selects exact topic revisions over a base checkpoint.
- An **operation** records an atomic artifact mutation and its preconditions.
- A **checkpoint** freezes a conflict-free exact view with optional passing
  execution evidence.
- A **projection** is a managed filesystem adapter. It is never source truth.
- A `source` artifact checkpoints and exports normally. A `generated` artifact
  checkpoints normally and exports only when its exact bytes have reachable
  `execution_promote_output` provenance. Relabeling an artifact as `generated`
  does not create that provenance.

Moving selectors are convenient for discovery, but durable integration,
execution, checkpointing, and export must use exact IDs.

## Source inclusion boundary

Sunlight does not scan or hide secret-like filenames or content. Git-tracked
files are visible under normal Git semantics, while Git-ignored untracked files
are excluded. Repository-root `.sunignore` patterns explicitly hide additional
tracked or untracked paths from Sunlight. `.git/` and `.sunlight/` are intrinsic
exclusions. `.sunignore` is visible but human-owned and cannot be changed through
Sunlight authoring, execution promotion, or compatibility import. Treat an
excluded path as human-owned; do not bypass `.sunignore` with direct reads or
edits. After a human changes the file, call `repository_init`; a clean state is
refreshed and authored history fails closed with preservation guidance. Secret
prevention and credential rotation happen outside Sunlight.

## Adopt existing worktree edits

`repository_status` compares ordinary repository-root files with Sunlight's
durable worktree anchor. A dirty worktree is outside native history: it cannot
affect Sunlight reads, execution, checkpoints, or export until captured.

1. Call `worktree_diff` and inspect the returned paths, classifications, and
   exact candidate IDs.
2. If the user wants those edits adopted, call `worktree_capture` with a new
   topic slug and stable actor ID. Omit selection to capture all eligible source
   candidates, or pass exact candidate IDs or paths for a partial capture.
3. Use the returned completed topic revision like any other topic: combine it
   explicitly with `view_resolve`, validate the exact combined view, and
   checkpoint only after the normal lifecycle succeeds.

A clean capture is a no-op. Do not capture merely to clear a warning, do not
claim validation from capture, and do not fold unrelated concurrent topics into
the captured revision.

## Author one change

1. Call `repository_status`. If the root is uninitialized, call
   `repository_init`, then read status again. Use
   `repository.recommended_start.resolved_view_id` for the new session unless
   the task explicitly requires an older exact state. This recommendation stays
   usable when unrelated moving heads conflict.
2. Create a narrowly named topic with `topic_create`.
3. Start a session from the intended exact view with `session_start`. Use a
   stable actor identifier and do not reuse another agent's session.
   Other visible topics are informational: continue authoring in this exact
   session. Overlapping paths become conflicts only if their revisions are
   selected together for integration. Unrelated `view_resolve` calls do not
   refresh, conflict, or close this session.
4. Discover source with `artifact_list` and `artifact_search`; inspect it with
   `artifact_read` using the session scope.
5. Mutate with `artifact_patch`, `artifact_write`, `artifact_move`,
   `artifact_delete`, or `artifact_metadata_set`. Pass the exact current
   `content_hash` as the compare-and-swap precondition. Use `new` only when a
   written path must be absent. Classify authored source as `source`; reserve
   `generated` for output promoted from a recorded execution.
6. Re-read important changes from the session view. If a focused validation is
   useful before integration, resolve the topic revision into an exact view and
   run it there.
7. Complete the topic at its exact head revision with `topic_complete`. Give a
   factual handoff: reasoning, operations, changed paths, hashes, and validation
   performed. Completion makes the topic immutable; it is not a quality claim.

Treat stale or ambiguous patch context as a request for a fresh read. Never
bypass a failed CAS by writing outside Sunlight.

## Coordinate and integrate

1. When the task has an explicit dependency, wait for its topic with
   `topic_wait`; do not infer dependencies from topic visibility or poll status
   in a loop. Consume the returned structured handoff.
2. Call `view_resolve` with the intended starting checkpoint and the exact new
   or replacement revisions being integrated. Sunlight carries that
   checkpoint's frontier forward automatically. Omitting `include` on a later
   checkpoint reproduces it exactly. Omitting `include` on the repository base
   resolves moving current heads and is discovery-only.
3. Stop on conflicts or staleness and report the structured records. Adapt
   through new topic-owned operations rather than editing a projection.
4. Inspect combined artifacts through view-scoped reads.
5. Run focused tests and the repository's required validation using
   `execution_run` on the exact combined view.
6. Inspect output classification. Pass a promotion candidate's returned
   classification (`source_like_delta` or `generated_artifact`) verbatim to
   `execution_promote_output`; these execution provenance classes are not the
   artifact classes `source` and `generated`. Ignored build/cache output should
   not be promoted. Promotion accepts one classified regular file up to 2 MiB;
   keep denied output local-only and reduce or split legitimate larger
   generated source before retrying. Promotion creates a native operation, so
   use a live topic-owned session over the validated view; when integrated
   topics are already complete, create a follow-up topic and session for the
   promotion. Resolve its returned revision into a new exact view and rerun the
   matching validation.
7. Create a checkpoint using passing evidence that matches the exact view and
   tree. Preserve its returned `handoff.exact_ids` as the integration result.
   Materialize an inspection projection only when a filesystem consumer needs
   one.
8. For a requested Git handoff, call `policy_check_export` with the exact
   checkpoint and target Git ref, then call `git_export`. `policy_check_commit`
   checks only `.sunlight/**` metadata candidates (or the managed `.gitignore`
   block when paths are omitted); it is not application-source validation and
   does not create a persisted report for `policy_explain`. Git is a
   compatibility/export surface, not native authorship. The handoff is complete
   when export returns an `export_map_id` for the checkpoint and target ref.

## Failure handling

- `precondition_failed`: re-read the artifact and reconsider the patch.
- `repository_writer_busy` or `concurrent_state_update`: the server queues,
  reloads, and retries safe native calls automatically. If one is returned,
  retry the same safe call once after a brief delay. If it recurs, report the
  exact facts as a repository-service blocker rather than coordinating writers
  or repairing native state manually.
- conflicted or stale view: inspect the referenced records and create an
  explicit adaptation. Continue unrelated work from
  `repository.recommended_start` instead of selecting the conflicted moving
  head.
- execution failure: use bounded output text and phase timings as diagnostics;
  make the correction in a topic session and resolve a new exact view.
- stale compatibility projection: create a fresh projection from the session's
  current generation, reapply the remaining filesystem change, rediff, and
  import with the generation returned by `compat_diff`.
- stale worktree anchor or changed capture candidates: call `worktree_diff`
  again, reconsider the exact current candidates, and retry deliberately.
- Git export failure: preserve the validated checkpoint and report the native
  handoff as blocked. A completed native handoff has a returned export-map ID;
  Git plumbing outside Sunlight does not substitute for that provenance.
- missing MCP server: stop the strict Sunlight workflow and give the user the
  setup or doctor command. Do not fall back to direct source access without
  explicit user approval.

## Operating principles

- Trust the agent's engineering judgment; preserve exact facts and boundaries.
- Keep topics small enough to explain and integrate.
- Use session scope for authoring and view scope for immutable inspection.
- Do not create branches, worktrees, commits, or exports unless the requested
  workflow requires them.
- Report the returned topic and checkpoint `handoff.exact_ids` verbatim, plus
  any relevant operation, projection, and export IDs.

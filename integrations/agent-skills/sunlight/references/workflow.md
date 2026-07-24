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

Moving selectors are convenient for discovery, but durable integration,
execution, checkpointing, and export must use exact IDs.

## Author one change

1. Call `repository_status`. If the root is uninitialized, call
   `repository_init`, then read status again.
2. Create a narrowly named topic with `topic_create`.
3. Start a session from the intended exact view with `session_start`. Use a
   stable actor identifier and do not reuse another agent's session.
4. Discover source with `artifact_list` and `artifact_search`; inspect it with
   `artifact_read` using the session scope.
5. Mutate with `artifact_patch`, `artifact_write`, `artifact_move`,
   `artifact_delete`, or `artifact_metadata_set`. Pass the exact current
   `content_hash` as the compare-and-swap precondition. Use `new` only when a
   written path must be absent.
6. Re-read important changes from the session view. If a focused validation is
   useful before integration, resolve the topic revision into an exact view and
   run it there.
7. Complete the topic at its exact head revision with `topic_complete`. Give a
   factual handoff: reasoning, operations, changed paths, hashes, and validation
   performed. Completion makes the topic immutable; it is not a quality claim.

Treat stale or ambiguous patch context as a request for a fresh read. Never
bypass a failed CAS by writing outside Sunlight.

## Coordinate and integrate

1. Wait for an owned dependency with `topic_wait`; do not poll status in a
   loop. Consume the returned structured handoff.
2. Call `view_resolve` with the base checkpoint and exactly one selected
   revision for every topic being integrated.
3. Stop on conflicts or staleness and report the structured records. Adapt
   through new topic-owned operations rather than editing a projection.
4. Inspect combined artifacts through view-scoped reads.
5. Run focused tests and the repository's required validation using
   `execution_run` on the exact combined view.
6. Inspect output classification. Generated or source-like outputs enter source
   truth only through explicit `execution_promote_output`; ignored build/cache
   output should not be promoted. Promotion accepts one classified regular file
   up to 2 MiB; keep denied output local-only and reduce or split legitimate
   larger generated source before retrying.
7. Create a checkpoint using passing evidence that matches the exact view and
   tree. Materialize an inspection projection only when a filesystem consumer
   needs one.
8. Run export policy checks and `git_export` only when the user requests a Git
   handoff. Git is a compatibility/export surface, not native authorship.

## Failure handling

- `precondition_failed`: re-read the artifact and reconsider the patch.
- `repository_writer_busy` or `concurrent_state_update`: safe native calls are
  retried by the server; if still returned, reload state and retry deliberately.
- conflicted or stale view: inspect the referenced records and create an
  explicit adaptation; do not choose moving heads implicitly.
- execution failure: use bounded output text and phase timings as diagnostics;
  make the correction in a topic session and resolve a new exact view.
- missing MCP server: stop the strict Sunlight workflow and give the user the
  setup or doctor command. Do not fall back to direct source access without
  explicit user approval.

## Operating principles

- Trust the agent's engineering judgment; preserve exact facts and boundaries.
- Keep topics small enough to explain and integrate.
- Use session scope for authoring and view scope for immutable inspection.
- Do not create branches, worktrees, commits, or exports unless the requested
  workflow requires them.
- Report exact topic, session, revision, operation, view, tree, execution, and
  checkpoint IDs in the final handoff when they exist.

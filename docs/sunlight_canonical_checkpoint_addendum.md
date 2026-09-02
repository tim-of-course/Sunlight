# Canonical Checkpoint Approved Addendum

| Field | Value |
| --- | --- |
| Status | Approved and implemented |
| Approved | 2026-09-01 |
| Scope | Checkpoint recommendation, concurrent integration, long-lived topics, and Git handoff |

## Purpose

Sunlight must let many agents work in one repository without making them manage
branches or worktrees. Each topic keeps its own exact starting view and remains
usable while other work is integrated. Repository-wide progress is represented
by one canonical checkpoint.

## Canonical checkpoint

Each repository durably records one canonical checkpoint. It starts at the
ingested base checkpoint and is returned by `repository.recommended_start`.

A normal checkpoint may advance the canonical checkpoint only when it preserves
every topic revision already in the canonical frontier. It may add a topic or
replace an included revision with a descendant revision from the same topic. It
may not remove or replace prior canonical work with unrelated history.

Completed topics that are not selected remain independent candidates. They do
not make the canonical checkpoint incomplete or prevent unrelated integration.

## Agent loop

1. Read `repository.recommended_start` before starting a topic.
2. Author the topic in a session pinned to that exact view.
3. Complete the topic without requiring unrelated repository progress to stop.
4. Read `repository.recommended_start` again immediately before integration.
5. Resolve the exact topic revision and its explicit dependencies onto that
   checkpoint.
6. Validate the resulting exact view.
7. Create the checkpoint with the starting recommendation as
   `expected_canonical_checkpoint`.
8. If another agent advanced first, repeat resolution and validation on the new
   recommendation.

This compare-and-swap step prevents two successful integrations from replacing
one another. It does not interrupt either topic's authoring session.

## Side checkpoints

An isolated or alternative result may be frozen with `side_checkpoint`. It is a
valid immutable checkpoint, but it never changes `repository.recommended_start`.
Side checkpoints are an explicit exception, not the normal completion path.

## Long-lived work

A topic may stay open across any number of canonical advances. Its session and
revisions remain tied to their exact view. When the topic is ready, integration
resolves its exact revision onto the current canonical checkpoint. A conflict is
reported only if the selected histories genuinely overlap.

This makes a topic the nearest Sunlight equivalent of an issue plus an isolated
change history. A resolved view is the integration candidate. Validation is the
review gate. The canonical checkpoint is the accepted repository line. Git
branches and pull requests may still be used as optional export or collaboration
surfaces, but they do not define Sunlight state.

## Compatibility

Repositories written before the canonical pointer existed infer it once from
their checkpoint history. The migration selects the last checkpoint whose
frontier preserves the previously selected frontier and skips old explicitly
partial checkpoints. The inferred pointer is persisted on the next native
mutation.

Git export remains exact: it exports the selected immutable checkpoint. Export
does not advance the canonical pointer or make Git authoritative.

## Required behavior

- Canonical advancement never removes already integrated topic history.
- A stale expected canonical checkpoint fails without creating a checkpoint.
- A side checkpoint never changes the recommended start.
- Unselected completed topics remain visible as pending integration candidates.
- Topics and pinned sessions remain usable across unrelated canonical advances.
- Status exposes the canonical checkpoint and identifies which checkpoint
  records are canonical.

# Sunlight MVP Readiness Audit - Superseded Correction

This document previously concluded that the fixture/local acceptance target was complete. That was not a valid conclusion for the actual Sunlight MVP described in `docs/sunlight_consolidated_architecture_v0_3.md`.

## Corrected Status

Sunlight currently has a useful Rust CLI prototype, but it is mostly fixture-backed:

- `sun init` creates real repository scaffolding.
- Many product commands still require `--fixture basic-app`.
- Topics, sessions, artifact reads/writes, resolver conflicts, projections, executions, checkpoints, status/inspect, compatibility import, and Git export are demonstrated largely against fake fixture state.
- Projection materialization and Git export touch real files/Git, but their source truth is still fixture data.

This is not a complete local MVP. It is a fixture prototype that must be converted into a real repo-backed product loop.

## Actual Next Milestone

Build the real vertical slice without `--fixture basic-app`:

1. `sun init` ingests an actual repo/worktree into Sunlight records and content storage.
2. A base checkpoint and resolved view are created from real files.
3. Topics and sessions persist for that repository.
4. `read/list/search` operate on real Sunlight artifact records.
5. `patch/write/move/delete/metadata` create durable operation transactions, topic revisions, and session generations.
6. `status/inspect` show those real records.
7. `project materialize` writes a real resolved view from persisted Sunlight state.
8. `checkpoint` and `git export` operate from real Sunlight records.

Do not use this superseded audit as completion evidence.

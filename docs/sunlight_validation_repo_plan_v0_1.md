# Sunlight Validation Repo Plan v0.1

| Field | Value |
| --- | --- |
| Status | Docs-only validation plan |
| Date | July 4, 2026 |
| Scope | First validation repo shape, smoke command matrix, CI expectations, and acceptance criteria |
| Sources | `docs/sunlight_native_io_phase1_spec_v0_1.md`, `docs/sunlight_execution_projection_v0_1.md`, `docs/sunlight_checkpoint_git_export_v0_1.md`, `docs/sunlight_compatibility_import_v0_1.md`, `docs/sunlight_git_export_writer_v0_1.md` |

## Selected Repo Shape

Use one small single-repository fixture as the first validation target: `basic-app`.

The fixture is a deterministic TypeScript-shaped repo with `README.md`, `docs/guide.md`, `src/auth.ts`, and `src/profile.ts`. It is intentionally not a full package install target. Its job is to prove Sunlight record, view, projection, checkpoint, compatibility import, and Git export behavior without coupling the smoke suite to npm, network, or toolchain churn.

Validation creates fresh temporary Git repositories from this fixture for every smoke flow. Each repo starts from one imported Git HEAD and then all mutations must go through `sun` commands or explicit compatibility projection import. The mutable Git working tree is never the source of Sunlight truth after `sun init`.

## Boundaries

Fixture-backed flows are allowed to use `--fixture basic-app` and stable fixture IDs such as `session_agent_a`, `view_base_0001`, and `checkpoint_auth_profile_ready_0001`. These tests validate command envelopes, identities, policy gates, and provenance links.

Real-repo flows are limited to repository bootstrap and export safety planning:

- `sun init --json` must create `.sunlight` metadata in a real temporary Git repo.
- Git export planning must inspect local Git parent/ref state but must not build export content from the working tree.
- No test may require network access, package installation, remote Git operations, or direct edits to project source as an authoring path.

## Command Matrix

| Flow | Command | Boundary | Required smoke assertion |
| --- | --- | --- | --- |
| Init | `sun init --json` | Real temp Git repo | Returns `repository.init`, creates `.sunlight/config.toml`, and is idempotent for the same repo. |
| Read | `sun read src/auth.ts --session session_agent_a --fixture basic-app --json` | Fixture | Returns `artifact.read`, artifact ID, path, content hash, bytes, resolved view, and generation. |
| Read missing | `sun read src/missing.ts --session session_agent_a --fixture basic-app --json` | Fixture | Fails with `path_not_found` and does not advance generation. |
| List | `sun list src --session session_agent_a --fixture basic-app --json` | Fixture | Returns `artifact.list` ordered by path and scoped to the prefix. |
| Search | `sun search User.email --session session_agent_a --fixture basic-app --json` | Fixture | Returns deterministic matches with artifact IDs, paths, hashes, lines, and snippets. |
| Write patch | `sun patch src/auth.ts --session session_agent_a --fixture basic-app --expect-hash sha256:auth_base --patch-file <patch> --json` | Fixture | Returns `artifact.patch`, operation, topic revision, after hash, new generation, and after view. |
| Write file | `sun write src/new.ts --session session_agent_a --fixture basic-app --expect-hash new --content-file <file> --classification source --json` | Fixture | Returns `artifact.write`, new artifact, operation, topic revision, and read-after-write generation. |
| Write stale | `sun patch src/auth.ts --session session_agent_a --fixture basic-app --expect-hash sha256:stale --patch-file <patch> --json` | Fixture | Fails with `precondition_failed`; no operation or revision is created. |
| Resolve | `sun view resolve --fixture basic-app --json` | Fixture | Returns exact resolved view, `SingleRepoTree`, topic frontier, deterministic order, and no conflicts for compatible revisions. |
| Resolve conflict | `sun view resolve --fixture basic-app --scenario overlapping-auth --json` | Fixture | Returns structured same-artifact conflict or staleness record without checkpoint eligibility. |
| Run | `sun run --view view_auth_profile_ready_0001 --fixture basic-app --json -- cargo test` | Fixture | Returns `execution.run`, projection ID, command argv, output summaries, result, view, and tree identity. |
| Projection | `sun projection create --view view_auth_profile_ready_0001 --purpose execution --fixture basic-app --json` | Fixture | Returns projection ID, purpose, strategy, root ref or handle, tree identity, and local-only policy. |
| Checkpoint | `sun checkpoint create --view view_auth_profile_ready_0001 --fixture basic-app --json` | Fixture | Returns `checkpoint.create`, checkpoint ID, conflict-free status, tree identity, selected evidence, and export readiness. |
| Compat import | `sun compat import --projection projection_compat_agent_a_0001 --session session_agent_a --select compat_delta_src_auth_ts_0001 --fixture basic-app --json` | Fixture | Returns one `compat_import` operation, one topic revision, selected delta refs, and generation advance. |
| Git export planning | `sun git export checkpoint_auth_profile_ready_0001 --branch refs/heads/sunlight/auth-profile-ready --fixture basic-app --plan-only --json` | Fixture plus local Git state | Returns export shape `single_checkpoint_commit`, validation report, selected parent, ref update plan, and no working-tree content dependency. |

If an implementation uses `project materialize` as the CLI spelling for projection creation, the same assertions apply to `sun project materialize --view <resolved-view-id> --purpose <purpose> --fixture basic-app --json`. The validation repo should keep one canonical spelling per release and cover aliases only while both are intentionally supported.

## Smoke Flow Order

Run smoke flows in this order so failures point to the earliest broken contract:

1. Create a fresh temp Git repo from `basic-app` and run `sun init --json`.
2. Read, list, and search the imported base fixture view.
3. Patch one existing artifact and write one new artifact; verify read-after-write IDs.
4. Resolve compatible topic revisions into `view_auth_profile_ready_0001`.
5. Resolve the overlapping-auth scenario and verify a structured conflict prevents checkpoint creation.
6. Create an execution projection and run the fixture command.
7. Create a checkpoint from the conflict-free resolved view and passing execution evidence.
8. Create a compatibility projection, import one selected delta, and verify normal operation provenance.
9. Plan Git export for `single_checkpoint_commit`; later writer tests may create the real commit after policy gates are implemented.

## Expected CI Commands

Required for every validation-plan change:

```text
cargo test --workspace
cargo test -p sun --test cli_json
git diff --check
```

Optional when Git export writer work is active:

```text
cargo test -p sunlight-core git_export
cargo test -p sun git_export
```

CI must run in offline mode where practical and must not depend on global Git config except for explicitly configured temporary author/committer identity inside the test repo.

## Acceptance Criteria

- The validation repo shape is `basic-app`, a tiny single-repo fixture, before any larger real-repo validation target is introduced.
- Every smoke command emits the stable JSON success or failure envelope.
- Fixture IDs are deterministic enough for snapshot or string-shape assertions, and exact canonical hashes can be tightened as hashing helpers mature.
- Native write flows create topic-owned operations and revisions; failed writes leave generation and provenance unchanged.
- Resolver smoke tests distinguish compatible composition from same-artifact conflicts.
- Projection and execution flows prove projections are adapters and local-only runtime state is not source truth.
- Checkpoint creation accepts only conflict-free exact resolved views with matching evidence.
- Compatibility import captures projection deltas into one normal operation transaction and one topic revision.
- Git export planning selects `single_checkpoint_commit`, validates parent/ref policy, and never reads export source bytes from the working tree.
- `cargo test --workspace`, `cargo test -p sun --test cli_json`, and `git diff --check` pass before the plan is accepted.

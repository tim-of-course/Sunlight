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

## First External Validation Target

The first real local repository target is Super Search at `C:\Users\TimothyCardoza\Documents\AI-Apps\Super Search`, available from WSL as `/mnt/c/Users/TimothyCardoza/Documents/AI-Apps/Super Search`. This target is useful because it is an existing mixed Elixir and JavaScript project with a passing local baseline, which gives `sun init` coverage against a real application tree without making that tree part of Sunlight's source of truth.

Baseline commands for the original target repo:

```text
mix test
bun run test
```

Observed manager preflight on July 4, 2026:

- `mix test` passed 1 doctest plus 167 tests.
- `bun run test` passed 8 Vitest tests.

External validation boundaries:

- This target is optional local validation only, not default smoke coverage and not CI coverage.
- Do not edit the Super Search target repo.
- Do not run package install commands or any network work.
- Require a clean target Git status before validation so baseline results stay stable.
- Sunlight validation must create a temporary clone or copy of Super Search before running `sun init`.
- Compatibility import coverage remains deterministic and fixture-backed. It may be run after temp clone init to assert the stable command envelope, operation/revision IDs, selected delta payloads, provenance, and policy failures; it does not claim general real-filesystem diff/import coverage.
- Local Git export execution may target the temporary validation clone with a dedicated `refs/heads/sunlight/*` branch and fixture checkpoint content.
- The temporary validation clone may receive `.sunlight` metadata, local export commits, export maps, and validation refs, and must be removed after the run.

## Boundaries

Fixture-backed flows are allowed to use `--fixture basic-app` and stable fixture IDs such as `session_agent_a`, `view_base_0001`, and `checkpoint_auth_profile_ready_0001`. These tests validate command envelopes, identities, policy gates, and provenance links.

Real-repo flows are limited to repository bootstrap and local export execution against a disposable clone:

- `sun init --json` must create `.sunlight` metadata in a real temporary Git repo.
- Git export planning must inspect local Git parent/ref state but must not build export content from the working tree.
- Local Git export execution may create commits, export maps, and refs only inside the temporary clone, using fixture checkpoint content and `--execute-local --repo <temp-clone>`.
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
| Projection materialization | `sun project materialize --view view_auth_profile_ready_0001 --purpose execution --fixture basic-app --json` | Fixture | Returns `projection.materialize`, projection ID, purpose, selected strategy, root ref or handle, tree identity, and local-only policy. |
| Checkpoint | `sun checkpoint create --view view_auth_profile_ready_0001 --fixture basic-app --json` | Fixture | Returns `checkpoint.create`, checkpoint ID, conflict-free status, tree identity, selected evidence, and export readiness. |
| Policy explain | `sun policy explain validation_export_auth_profile_ready_0001 --json` | Fixture | Returns `policy.explain`, the requested validation report ID in the envelope and IDs block, the validation report body, and no failures for the ready export report. |
| Compat project | `sun compat project --session session_agent_a --fixture basic-app --json` | Fixture | Returns `compat.project`, compatibility projection ID, baseline resolved view, baseline manifest digest, tree identity, strategy, root ref, and retention state. |
| Compat diff | `sun compat diff --projection projection_compat_agent_a_0001 --fixture basic-app --json` | Fixture | Returns `compat.diff`, projection ID, baseline view/tree, candidate counts, quarantine refs, selected safe default `compat_delta_src_auth_ts_0001`, and candidate detail refs for source, rename, metadata, generated, cache, secret, conflicted, and policy-blocked fixture deltas. |
| Compat import | `sun compat import --projection projection_compat_agent_a_0001 --candidate compat_delta_src_auth_ts_0001 --fixture basic-app --json` | Fixture | Returns `compat.import`, one operation transaction, one topic revision, selected candidate refs, selected delta payloads, and generation advance. Additional fixture candidates cover multi-candidate import, delete tombstone, unambiguous rename, metadata-only stable hash, rename-plus-edit nested operations, and atomic policy failures. |
| Git export write planning | `sun git export --checkpoint checkpoint_auth_profile_ready_0001 --branch refs/heads/sunlight/auth-profile-ready --fixture basic-app --write-plan --json` | Fixture plus local Git state | Returns `git.export.write_plan`, export shape `single_checkpoint_commit`, validation report, selected parent, ref update plan, planned commit ID, and no working-tree content dependency. |
| Git status lookup | `sun status --git refs/heads/sunlight/auth-profile-ready --fixture basic-app --json` | Fixture | Returns `status.git`, Git ref, export map ID, checkpoint ID, validation report ID, and commit ID mapping for the exported fixture ref. |
| Git inspect lookup | `sun inspect git:git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa --fixture basic-app --json` | Fixture | Returns `inspect.git`, Git ref, export map ID, checkpoint ID, validation report body, and `git_export_map` record body for the exported fixture commit. |

## Smoke Flow Order

Run smoke flows in this order so failures point to the earliest broken contract:

1. Create a fresh temp Git repo from `basic-app` and run `sun init --json`.
2. Read, list, and search the imported base fixture view.
3. Patch one existing artifact and write one new artifact; verify read-after-write IDs.
4. Resolve compatible topic revisions into `view_auth_profile_ready_0001`.
5. Resolve the overlapping-auth scenario and verify a structured conflict prevents checkpoint creation.
6. Materialize an execution projection and run the fixture command.
7. Create a checkpoint from the conflict-free resolved view and passing execution evidence.
8. Create a compatibility projection, diff it, import one selected candidate delta, and verify normal operation provenance.
9. Write-plan Git export for `single_checkpoint_commit`; writer tests may create the real commit after policy gates are implemented.
10. Look up the fixture Git export by ref with `sun status --git` and by commit with `sun inspect git:`.

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
- Compatibility import captures fixture projection deltas into one normal operation transaction and one topic revision, including selected delta payloads for modified source, multi-candidate, delete, rename, metadata, and rename-plus-edit cases, plus atomic failure envelopes for generated, ambiguous, conflicted, policy-blocked, secret, cache, missing-candidate, and no-candidate cases.
- Git export planning selects `single_checkpoint_commit`, validates parent/ref policy, and never reads export source bytes from the working tree.
- `cargo test --workspace`, `cargo test -p sun --test cli_json`, and `git diff --check` pass before the plan is accepted.

# Internal alpha follow-up, September 7, 2026

## Scope

macOS validation of the installed Sunlight executable in the disposable TasGrid
repository. Fresh Banana Split root threads discover repository instructions and
tools themselves. Prompts specify product tasks and validation, without teaching
Sunlight operation. Each root owns authoring, review, integration, and checkpointing;
no child agents are requested. This run does not revalidate Windows.

Sunlight source development used ordinary filesystem/Git tools in the Sunlight
repository. TasGrid native source authoring and review use its bound Sunlight MCP.

## Fixes

- `184391e`: stop MCP dispatch on client disconnect before joining the canceled
  worker. All 18 MCP integration tests and 15 MCP unit tests passed before the
  first fresh-thread run. The installed release also exited promptly on EOF.
- `e3a7a75`: checkpoint creation, handoffs, and checkpoint status no longer claim
  export readiness for private topics. Responses explicitly state that complete
  export validation is still required. The existing private-topic regression
  failed before the fix and passed afterward; all 18 checkpoint tests passed.

## Initial run: validation passed, export blocked, reporting defect found

Workflow: `wf_e225f267-8866-495f-b97a-59eece8f11a2`.
Root: `agt_81372be7-a2d7-4a28-bcc3-e6e3d96e98f5`.
Thread: `01a07a51-c2aa-7761-899d-8641fc41b8a9`.
Configured model: `gpt-6-astra`, reasoning `medium`; no children.
Mechanically completed with outcome `partial`; zero managed-tool rejections,
revisions, acceptances, or advice resolutions.

Task: add `normalizeAlphaLabel(value: unknown): string`, focused regression tests,
run the existing installation smoke and production build, and export locally to
`refs/heads/test/alpha-cycle-one-20260907`.

- Focused Bun tests: 4 passed, 19 assertions (`exec_native_0029`).
- Installation smoke: 31 assertions passed (`exec_native_0030`).
- Production build passed (`exec_native_0031`).
- Combined view: `view_fixture_517524fbc5aa1988`.
- Canonical checkpoint: `checkpoint_1bf95da4f6af82dd_a495a97f7d13`.

Five private topics from another test were already in the combined frontier.
Export policy correctly rejected their disclosure. No Git commit, ref update, or
export map was produced. The misleading `export_ready: true` response was fixed.
There is no exposed topic visibility update operation; introducing declassification
is a product decision, not part of this reporting fix. Follow-up tests use native
handoffs and do not claim successful Git export for this private frontier.

Other observations: Vite chunk-size warning, macOS isolation reported unenforced,
execution setup overhead, and inspection queued behind the build. An optional
shell process diagnostic was sandbox-denied. Four pre-existing external worktree
changes were preserved with unchanged hashes. Eight historical failed executions
were unchanged; all three new executions passed.

Local detailed evidence, including verbatim native handoffs:
`/Users/tim/.local/state/sunlight/evaluations/20260907-alpha-cycle/first-run.json`
and `/private/tmp/alpha-cycle-one-20260907-report.txt`.

The 13 old parent-PID-1 servers observed before this cycle were not terminated:
another test may own them, and their original cause is not established. They are
not evidence that a new test leaked a process.

## Move run: validated, but not counted as an unqualified clean result

Workflow: `wf_d45141d0-5f1b-4078-8944-ce2ec71240a1`.
Root: `agt_3da79645-72dc-43dd-8570-f3c40552022f`.
Configured model: `gpt-6-astra`, reasoning `medium`; no children.
Mechanically completed, root outcome `partial`.

Moved the helper to `src/lib/alpha-label-normalization.ts`, preserving its content
hash and removing the old path. Updated its sole consumer. Focused tests passed
(4 tests, 19 expectations), smoke passed (31 assertions), and production build
passed. Executions: `exec_native_0032`, `exec_native_0033`, `exec_native_0034`.
Checkpoint: `checkpoint_cec46802f5f8a6f9_d46d4f179b23`.

The host declined capture of unrelated installation files and supplied an ownership
clarification. The agent still reported partial because those files remained
external. Unsupported list arguments and a sandboxed process diagnostic recovered;
no new Sunlight implementation defect was demonstrated. There were zero Banana
managed-tool rejections, revisions, acceptances, or advice resolutions; those
counters do not include Sunlight argument errors or host approval declines.

An earlier startup attempt failed before a thread ran during a concurrent Banana
plugin replacement (`wf_88050504-a35c-4161-bd1c-346ec54d1b56`,
`agt_45d09e0a-1eb8-4902-991e-2f51adc96c51`). A retry started normally. No shared
runtime was killed or reconfigured by this test.

Installed executable SHA-256 after the reporting fix:
`33d548c9654ba1e4f67ed7129616ee8c717df626a13c8291a3f38bd820ba7d47`.
The installed CLI independently returned `export_ready: false` for the private
checkpoint from the initial run.

 Detailed local evidence and verbatim handoffs:
`/Users/tim/.local/state/sunlight/evaluations/20260907-alpha-cycle/move-run.json`.

## Batch run: blocked; stop threshold reached

Workflow: `wf_bbaa4acb-6c92-404b-b407-d8079f54f92c`.
Root: `agt_6ff4222d-691d-4348-a4cb-a5acdb835f7c`.
Thread: `01a07a68-3d1b-70a2-b936-09e926e59016`.
Configured model: `gpt-6-astra`, reasoning `medium`; no children.
Mechanically completed, root outcome `blocked`. Zero Banana managed-tool
rejections, revisions, acceptances, or advice resolutions.

Task: add batch normalization beside the moved helper, discard empty results,
deduplicate in first-occurrence order, and test mixed input, duplicates, ordering,
and input immutability. External installation changes were explicitly excluded.

Three native mutations failed with `conflicted_view`: an atomic full-content patch,
smaller patch hunks, and a helper-only write. All used the current source hash and
the same pinned session over `view_fixture_062d236462115ef3`. The reported candidate
view `view_fixture_94052b42db919e9d` could not be inspected (`object_not_found`), and
the failure response contained no conflict IDs. The batch topic persisted at
revision zero with no head. The canonical checkpoint remained
`checkpoint_cec46802f5f8a6f9_d46d4f179b23`. No batch implementation or new tests
were persisted. Passing baseline tests and smoke are not validation of that work.

Two issues are deferred for product judgment:

1. **Sequential editing after a move is blocked.** The repro is a normal move,
   checkpoint, new topic/session, then edit the moved artifact. Repair may affect
   artifact identity, historical operation replay, dependency ordering, and
   conflict classification. Inspection located the mutation rejection in
   `real_accept_mutation` and its dependency on `resolve_session_view` /
   `resolve_real_repo_view`; the exact resolver cause is not established. Do not
   bypass the conflict, drop canonical topics, or reset native history. Recommend
   prioritizing a bounded resolver investigation and regression reproducer,
   preserving old checkpoint identities and genuine conflict detection.
2. **Private topics have no supported transition back to exportable work.** Once
   they enter the canonical frontier, subsequent combined exports inherit the
   restriction. The rejection itself is intentional. Whether to add explicit
   declassification, separate source export from private provenance, or retain a
   permanently native-only line is a product decision. Recommend keeping the
   privacy gate and deciding the lifecycle before adding an override.

One additional confirmed reporting defect remains: `artifact_list` reports zero
byte lengths for nonempty files. It loads metadata without blob bytes, then
`real_artifact_view` uses `entry.bytes.len()`. Recommend obtaining lengths from
stored metadata/blob metadata without hydrating the whole repository. This was
recorded when the two-risk stop threshold was reached, not silently fixed with a
potentially expensive full-content load.

Detailed evidence and proposed-but-unpersisted source:
`/Users/tim/.local/state/sunlight/evaluations/20260907-alpha-cycle/batch-run.json`.

## Final outcome

The requested two consecutive successful, issue-free tests were **not achieved**.
The loop stopped at the user's alternative threshold of two changes considered
too consequential to make without product judgment. Two fixes were committed,
pushed, and installed. No further Sunlight source changes were made after that
threshold; this report records the remaining defects and policy decision.

Three fresh root agents actually ran, all Astra medium, with zero child agents.
One additional root startup failed before a thread ran. Host model family: GPT-6;
exact host variant and reasoning level unavailable. Final process comparison
found no new parent-PID-1 Sunlight servers; all 13 pre-existing ones remained.
This evidence supports the narrow fixed behaviors, not an unrestricted clean bill
of health for internal alpha.

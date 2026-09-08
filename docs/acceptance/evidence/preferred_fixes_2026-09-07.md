# Preferred fixes and fresh-thread retest, September 7, 2026

Outcome: approved fixes implemented, built, installed, and pushed. Two consecutive
fresh-thread retests after the final correction completed without new Sunlight
defects. Final installed source is `a4132e4`.

## Initial fixes

Source commit: `e551f85` (pushed to `origin/main`).
Installed executable: `/Users/tim/.local/bin/sun`.
SHA-256: `ad7acff110551c6c070c9e0d17469702a44216efb7ea53f39d7bee5bf4bb37d8`.
Previous executable retained under
`/Users/tim/.local/state/sunlight/install-backups/20260907-preferred-fixes/`.

1. Sequential edits after moves: dependency-chain checks now require remaining
   prerequisites to be consumed before selecting the next revision. Moves retain
   their content hash, so matching the predecessor hash alone was insufficient.
   The regression recreates native creation, move plus another-file edit, a new
   dependent topic, and competing edits. The dependent edit succeeds, the old path
   stays absent, artifact identity and the old checkpoint's content are preserved,
   and competing edits still conflict. No checkpoint reset or history rewrite.
   Rejected mutation responses also include candidate conflict records directly
   and state that the candidate view was not persisted.
2. Explicit declassification: CLI `sun topic declassify` and MCP
   `topic_declassify` release an exact completed private topic. The actor, reason,
   revision, and release time are persisted with a separate immutable decision.
   Exact retries are no-ops; open topics cannot inherit a release for future work.
   Source operations and checkpoint identities are unchanged. Normal export
   validation still applies, including restrictions on other private topics.
3. Correct artifact lengths: list responses use blob filesystem metadata instead
   of the empty byte buffers returned by metadata-only loading. No full-content
   hydration or persistent-state migration is needed for file lengths. The
   installed executable independently reported the original moved helper's
   correct 170-byte length.

Source development in the Sunlight repository used ordinary filesystem and Git
tools. TasGrid source authoring, review, execution, and handoffs used its bound
Sunlight MCP. Host handoff verification also inspected native records and Git refs.
This is macOS validation; Windows was not rerun in this cycle.

## Validation and harness corrections

The expanded move regression reproduced `conflicted_view` before the dependency
ordering fix and passed afterward. The first smaller repro did not fail and was
replaced with the full native-history sequence. Declassification regression checks
release, idempotency, persistence, policy acceptance, export planning, and unchanged
source operations/checkpoints. Both view and session listings have length checks.

`cargo test --workspace` passed on the stable final checkout, including 257 CLI
integration tests, 18 MCP integration tests, self-hosting acceptance, and the core
tests. Formatting and diff checks passed. Release build passed.

An initial self-hosting run was invalidated when the host committed during the
test: its final working-tree-preservation assertion correctly detected the change
from dirty to clean. The complete suite was rerun with the checkout unchanged and
passed. This was a test orchestration mistake, not a Sunlight defect.

The first fresh root was blocked before source work because the updated Banana
runtime's default MCP allowlist was empty:
`wf_114c96b8-29d7-4dc7-a0cd-5eb17a937ce6`, root
`agt_a148b7e4-14a1-4d85-8769-af01d9d9e8b1`.
The root discovered the skill and ran doctor, which verified installation; it
correctly refused to bypass missing native tools. After confirming no live Banana
workflows, the host allowed only `sunlight_33af511dbf74815b` in the installed Banana
configuration and gracefully restarted the idle runtime. One startup call during
restart returned connection refused and created no workflow. No other MCP names
were added. The config backup and restart log are in the local evidence directory.

## First substantive run

Workflow: `wf_109874e2-249b-4918-8807-bc2951111733`.
Root: `agt_cd49f507-0511-479c-bb94-613c2649cf26`.
Thread: `01a07ef6-34d9-75b3-9e8f-f37479c48094`.
Mechanically completed; root outcome `success`. Astra medium, configured default,
no children. Zero managed-tool rejections, no-disposition turns, revisions,
acceptances, or advice resolutions. No Sunlight tool errors in the inspected run.

Added `normalizeAlphaLabels` beside the previously moved helper, with mixed-input,
deduplication, ordering, and readonly-input coverage. The thread discovered tools
and documentation without operating instructions. The task explicitly authorized
release of completed private disposable-test topics for the requested local export.

- Focused tests: 8 passed, 25 expectations.
- Existing installation smoke: 31 assertions passed.
- Production build: passed (185.954 seconds command, 200.605 seconds end-to-end).
- Five completed private topics were explicitly released at their exact heads.
- Canonical checkpoint: `checkpoint_61748e82f8236529_cd43f393daf6`.
- View: `view_fixture_08fe2ff3898ae965`.
- Local Git ref: `refs/heads/test/preferred-fixes-one-20260907`.
- Commit: `3cdadf2ac173589129759c32424433c8a9e56b56`.

The host independently verified the ref commit against the native export map and
the canonical checkpoint. Existing source revisions, the empty failed topic, and
four external worktree edits were preserved. Only known observations remained:
Vite's chunk-size warning, unenforced macOS isolation, execution overhead, and
eight historical failed executions. This run added three passing executions.

Full local transcript, result, and verbatim native handoffs:
`/Users/tim/.local/state/sunlight/evaluations/20260907-preferred-fixes/first-clean-run.json`.

## Logs

- Full suite: `/tmp/sunlight-preferred-workspace-stable.log`.
- Invalidated initial suite: `/tmp/sunlight-preferred-workspace.log`.
- Move regression before correction: `/tmp/sunlight-move-after.log`.
- Move regression after correction: `/tmp/sunlight-move-fixed.log`.
- Final declassification regression: `/tmp/sunlight-declassification-final.log`.
- Release build: `/tmp/sunlight-preferred-release.log`.
- Durable local evidence directory:
  `/Users/tim/.local/state/sunlight/evaluations/20260907-preferred-fixes/`.

## Additional issues found and corrected

The second substantive run completed its move, helper implementation, 12 focused
Bun tests (45 expectations), smoke checks (31 assertions), production build,
checkpoint, and local export. Workflow `wf_9cb737fd-525b-4ee0-8c05-179472ca03ae`,
root `agt_fb5b31e9-4878-4dc0-af98-404d4e04491d`; local ref
`refs/heads/test/preferred-fixes-two-20260907` points to
`5985c0a8af3d29c4c74d3c3e5b253b7919b21f73`.

Its caller hit a TypeError because the advertised MCP output contract said
`artifact` while real responses returned `artifacts`. Commit `408a0f3` corrected
read and mutation contracts, described text content, and added assertions comparing
advertised schemas with live responses. Focused MCP tests passed (16 unit and 14
integration tests). This was a real contract defect; the clean-run count reset.

The next thread, `wf_a7dd4d31-8e27-4550-b659-cc940a6ec46a`, root
`agt_761cae2c-1ce2-4186-9199-eb99b9355437`, found another false conflict while
extending `tests/alpha-label.test.ts`. Neither rejected mutation persisted. The
thread recovered with a separate test file, passed 13 tests/48 expectations and
31 smoke assertions, and exported locally, but this did not count as a clean run.

Commit `a4132e4` fixes native conflict checking against complete artifact history:

- The fixture projection contains one effect per topic head and can miss secondary
  files in batch edits. Valid native frontiers now recompute same-artifact
  conflicts using every operation effect. Frontiers with dependency or frontier
  errors retain all existing diagnostics.
- All intermediate revisions are retained for dependency-chain verification,
  including repeated edits in the same topic. Topic ancestry establishes their
  order. Matching final results across topics retain the existing convergence rule.
- The expanded CLI regression covers a move, repeated edits, a batch's secondary
  file, a later dependent edit, and competing edits to both primary and secondary
  files. Sequential changes succeed and competing edits still conflict.

The regression failed before correction. During development, two test-input
mistakes were corrected (an old byte-length assertion and an invalid topic slug).
The existing live-MCP overlapping-edit test also caught a diagnostic regression in
an intermediate implementation: missing-dependency views lost their conflict IDs.
That was fixed before installation. The final complete `cargo test --workspace`
passed, including 257 CLI tests, all 18 MCP integration tests, self-hosting,
open-alpha handoff, and core tests. Formatting and diff checks passed.

Final installed release: `/Users/tim/.local/bin/sun`, source `a4132e4`.
SHA-256: `fdf1cef259b43801c9b8478ae5be64ce24c8eaee5b3de9ec0f5dcb349eac5e9f`.
Previous executable retained under
`/Users/tim/.local/state/sunlight/install-backups/20260907-complete-history/`.
All three source commits were pushed to `origin/main`.

## Final clean retest one

Workflow `wf_674c7b82-f628-4321-9f97-533231cb6eb8`, root
`agt_7d1b83d1-1f6c-44a6-a897-907773b1e892`, mechanically completed with outcome
`success`. The fresh thread consolidated the workaround regression into the exact
existing file that previously failed, then deleted the redundant separate file.
Both native mutations succeeded. Readback, byte lengths, deleted-path absence,
external changes, earlier canonical work, and release decisions were verified.

- Focused Bun tests: 13 passed, 48 expectations (`exec_native_0045`).
- Installation smoke: 31 assertions passed (`exec_native_0046`).
- Checkpoint: `checkpoint_664a2ecf2eb83143_996ea8b4f80a`.
- View: `view_fixture_7a564b316d717ae1`.
- Ref: `refs/heads/test/history-retest-one-20260907`.
- Commit: `f0d3884f24a6832c15e973910281da695e0cd01c`.

No new native command or test failures. The thread observed that listing prefix
`tests/alpha-label` does not match filename fragments; host inspection confirmed
expected exact-path/directory-subtree semantics. Listing `tests` worked. Broad
discovery output was truncated and recovered through narrower reads. These were
caller discovery adjustments, not new Sunlight defects. Known macOS isolation and
historical-status warnings persisted. No production build was needed for this
test-only change; the two earlier production builds passed.

## Additional logs

- Contract checks: `/tmp/sunlight-artifact-contract-tests.log`.
- Reproduced conflict: `/tmp/sunlight-multifile-before.log`.
- Final full suite: `/tmp/sunlight-complete-history-workspace.log`.
- Final release: `/tmp/sunlight-complete-history-release.log`.
- Full thread payloads and verbatim handoffs are retained in the local evidence
  directory named above.

## Final clean retest two

Workflow `wf_aa0d4b45-de2b-43cd-b908-c964503a836e`, root
`agt_8841971d-dbec-4da4-ada2-7c7654b5874d`, mechanically completed with outcome
`success`. The thread documented all three helpers in `docs/alpha-label-api.md`,
read the document back, and verified its 3,440-byte length and hash. It extracted
and executed the exact Bun examples from the persisted Markdown, then ran the
focused tests and installation smoke in `exec_native_0047`.

- Examples: passed.
- Focused Bun tests: 13 passed, 48 expectations.
- Installation smoke: 31 assertions passed.
- Checkpoint: `checkpoint_4ab8f7f833b9f1d2_24464cab674a`.
- View: `view_fixture_3f04106c9dd11b11`.
- Ref: `refs/heads/test/history-retest-two-20260907`.
- Commit: `b9d469ee32dc69e3f3bf49a08d0ed4a0c7ad2f82`.

No task errors, failed validation, or export-policy failures. Broad discovery
output required narrower reads again; known runtime and historical warnings
remained unchanged. The host reviewed the document and inspected the returned
validation/transcript evidence. Both final local refs were independently checked
against their durable native export maps; all earlier exports in this continuation
were checked as well. No TasGrid refs were pushed remotely.

## Final preservation and scope

The installed executable matches the release build byte-for-byte. Five explicit
release decisions remain persisted. The old empty failed normalize topic still
has no head. All four host-owned external changes retain their original hashes.
No task-owned work is pending outside the canonical native handoff.

The final process snapshot found no new Sunlight servers reparented to PID 1.
The 13 old CPU-heavy PID-1 servers from the baseline remain untouched because
other tests may own them and their origin was not established. This result does
not claim that the machine has no old process problem. New managed-thread servers
remain attached to their Codex parent while those durable threads are retained.

The two final runs satisfy the requested clean-test stopping condition. This
cycle validates the installed macOS setup; it does not repeat Windows validation.
Known macOS execution isolation limitations remain explicitly reported. Historical
failed executions/policy records and old unexported checkpoints were preserved.
The TasGrid production build's known chunk-size warning remains. None of those
observations was treated as a new defect or silently cleared.

## Agent and evidence recap

Six fresh Banana roots participated in this continuation, all configured and
observed as `gpt-6-astra`, medium reasoning, with no children. All six workflows
mechanically completed: one root reported a setup block, five reported task
success. Of the successful tasks, two exposed defects and were excluded from the
final clean pair. All terminal diagnostic counts were zero for managed-tool
rejections, no-disposition turns, revisions, acceptances, and advice resolutions.
These runtime counters do not count the native errors described above or host
script mistakes. The parent reviewed results and verified export evidence before
accepting the final pair. The host is a GPT-6-based Codex agent; its exact model
variant and reasoning setting were not exposed.

Durable evidence files in
`/Users/tim/.local/state/sunlight/evaluations/20260907-preferred-fixes/`:

- `setup-blocked-run.json`
- `first-clean-run.json`
- `second-edit-run.json`
- `contract-retest-found-conflict.json`
- `history-retest-one.json`
- `history-retest-two.json`
- `handoffs.md` (verbatim native `copy_report` strings)
- `external-before-final.json` and `processes-final.json`
- Validation/build logs listed above, copied from `/tmp` for retention.

The Banana MCP allowlist correction was applied to the installed runtime config;
a future Banana reinstall may replace that config. No unrelated plugin source or
MCP permissions were changed.

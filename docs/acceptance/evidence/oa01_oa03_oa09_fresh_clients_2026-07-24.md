# OA-01 / OA-03 / OA-09 fresh-client acceptance evidence

Date: 2026-07-24 (America/Chicago)  
Build SHA-256: `d69c6cb6ddd6e75f76491ab040a6fb3ec723249831635a610157e58cb1de10b8`  
Source: commit `7a7ed0dbaa0ac41d06248ea0bde2c40284faf098` plus the tested acceptance worktree  
Host: Windows NT 10.0.26200, Windows/NTFS supported scope  
Harness/model: five fresh Codex desktop tasks, `gpt-5.6-terra`, high effort  
Target: disposable CheckChecker commit `796c191e8c2e129da75b048678a9d938c2bdc2da`, 34 tracked files, 266,745 tracked bytes, no remote

Result: **OA-01 partial (Codex pass, Cursor pending); OA-03 pass; OA-09 pass**.

The target was reset to its original disposable baseline before the campaign.
Each run started with fresh native Sunlight state while retaining only the
current portable Agent Skill and repository-bound Codex MCP configuration.
The application working tree stayed byte-identical to Git because all accepted
changes remained in Sunlight. The final Git status contains only untracked
`.agents/`, `.codex/`, and `.sunlight/`; no branch, worktree, commit, export,
remote, or push was created.

## OA-01: three consecutive fresh Codex runs

All three tasks independently discovered the repository-scoped `sunlight` MCP,
called status, recognized the uninitialized repository, initialized it, created
their own topic/session, inspected and authored through Sunlight, resolved exact
revisions, ran the project build, and created a checkpoint. None received
Sunlight object IDs, MCP tool names, or a lifecycle sequence.

### Run 1 — idempotent pending attachment uploads

Task: `019f946e-fab0-7d03-b170-04a87776aac1`  
Duration: 358,432 ms

Exact prompt:

> Fresh-client open-alpha acceptance run 1 of 3.
>
> In this disposable CheckChecker repository, use Sunlight to implement this real engineering task:
>
> Make attachment upload queuing idempotent while an attachment is pending: queueing the same logical attachment more than once must not create duplicate pending uploads, while a failed upload must still be retryable. Keep the change lean and compatible with the existing offline-first behavior.
>
> Do not create Git branches, worktrees, commits, remotes, exports, or pushes. Do not use direct tracked-source access or mutation outside Sunlight; if Sunlight is unavailable, stop and report the exact setup/recovery action instead of falling back. Complete the task to engineering quality, validate it, and report what changed, the evidence Sunlight produced, and whether any tracked source was accessed outside Sunlight.

The agent created `topic_idempotent_pending_attachment_queue` and, after its
own final review found an empty-error edge case, a second independent topic
`topic_recognize_empty_attachment_failure`. It integrated
`rev_idempotent_pending_attachment_queue_0001` and
`rev_recognize_empty_attachment_failure_0001` into
`view_fixture_6b4bbea0e360cbd5`. `bun run build` passed as
`exec_native_0002`; final checkpoint
`checkpoint_3fa0b98c72c6eb15_3921c7500df2` froze tree
`tree_3fa0b98c72c6eb15dcef81ef3306688a5460256981688502dce5c4f361b5ae12`.
The agent reported no tracked-source access outside Sunlight.

### Run 2 — caller-invoked stale offline-run cleanup

Task: `019f9475-711a-7571-86cb-cfcd5c208040`  
Duration: 161,766 ms

Exact prompt:

> Fresh-client open-alpha acceptance run 2 of 3.
>
> In this disposable CheckChecker repository, use Sunlight to implement this real engineering task:
>
> Prevent stale checklist runs from lingering forever in offline storage. Add a lean cleanup operation that removes locally cached runs older than a caller-supplied cutoff while preserving newer runs and their attachments. Keep the API explicit and avoid introducing background timers.
>
> The repository already contains unrelated local working-tree and client-configuration changes; preserve them exactly. Do not create Git branches, worktrees, commits, remotes, exports, or pushes. Do not use direct tracked-source access or mutation outside Sunlight; if Sunlight is unavailable, stop and report the exact setup/recovery action instead of falling back. Complete the task to engineering quality, validate it, and report what changed, the evidence Sunlight produced, and whether any tracked source was accessed outside Sunlight.

Before this run, a tracked `.gitignore` comment and an unrelated `.codex`
configuration comment were added as sentinels. Both were present byte-for-byte
after the run. The agent completed `topic_cleanup_stale_cached_runs` at
`rev_cleanup_stale_cached_runs_0001`, built
`view_fixture_1da44e53f4307fc7` successfully as `exec_native_0001`, and created
`checkpoint_6cee8c2ae8e4b7d4_7a476b3bb2ee`. It reported no application-source
access outside Sunlight; its only direct read was the installed untracked Agent
Skill required for discovery.

### Run 3 — collision-resistant local record IDs

Task: `019f9478-dfec-7a31-b73d-532ed5bce677`  
Duration: 148,921 ms

Exact prompt:

> Fresh-client open-alpha acceptance run 3 of 3.
>
> In this disposable CheckChecker repository, use Sunlight to implement this real engineering task:
>
> Make locally generated record IDs robust across simultaneous browser tabs. Preserve the existing readable type prefix, prefer a platform-provided cryptographic UUID when available, and retain a safe fallback for environments without that API. Keep the change small and compatible with existing callers.
>
> Do not create Git branches, worktrees, commits, remotes, exports, or pushes. Do not use direct tracked-source access or mutation outside Sunlight; if Sunlight is unavailable, stop and report the exact setup/recovery action instead of falling back. Complete the task to engineering quality, validate it, and report what changed, the evidence Sunlight produced, and whether any tracked source was accessed outside Sunlight.

The agent completed `topic_robust_local_record_ids` at
`rev_robust_local_record_ids_0001`, built
`view_fixture_5077d6737ae49296` successfully as `exec_native_0001`, and created
`checkpoint_175359053f9f223f_7b293cb36424` over tree
`tree_175359053f9f223ff129b593579e92c41e588ed6a4f92ce55c5cc6f78147e520`.
It reported no application-source access outside Sunlight.

Each run first tried the stronger `network: disabled` execution policy. The
Windows AppContainer could not expose the user-local Bun installation. Each
agent used the structured next action and repository policy facts to retry the
same exact view with the permitted `network: not_enforced` mode; all builds
then passed. This is a documented containment fact, not a silent downgrade.

OA-01's three-consecutive-Codex criterion therefore passes. OA-01 remains
**partial** solely because no actual Cursor agent run has been executed.

## OA-03: fresh delegated supervisor

Task: `019f947b-cdee-7c51-9a7a-9e9c7f43d47d`  
Duration: 714,225 ms

Exact prompt:

> Fresh-supervisor open-alpha acceptance run.
>
> Use Sunlight to improve CheckChecker’s offline-sync reliability as one integrated engineering objective. The result should cover attachment-upload deduplication with retry support, caller-invoked stale-run cleanup, and a small diagnostic summary of pending versus failed sync work. Delegate the work naturally across three or four workers using the agent coordination available to you.
>
> As part of this real concurrency run, demonstrate and recover from these conditions without being given a Sunlight workflow: two workers overlap on one source file; one author encounters a stale compare-and-swap; one worker depends on another worker’s durable handoff; and one worker is stopped before completion and explicitly excluded or replaced. Integrate only exact completed work, run focused validation plus the final project build on the combined result, and produce the normal handoff evidence.
>
> Do not create Git branches, worktrees, commits, remotes, exports, or pushes. No agent may read or mutate tracked application source outside Sunlight; if a required capability is unavailable, stop and report it rather than bypassing Sunlight. Report the worker outcomes, conflicts/staleness/recovery, exact integration evidence, validation, and whether any tracked source was accessed outside Sunlight.

The supervisor naturally chose Banana Split and launched three workers. Sunlight
recorded distinct topics/sessions, completed upload and cleanup revisions, one
partial diagnostics topic, a real same-file resolver conflict on
`src/lib/syncQueue.ts`, and an intentional stale CAS on
`src/lib/pocketbase.ts`. The upload worker re-read and completed
`rev_attachment_upload_dedupe_retry_0002`. The supervisor consumed durable
upload/cleanup handoffs through `topic_wait`, stopped the stalled orchestration
workflow, explicitly excluded the incomplete diagnostics topic, and created a
replacement topic. Focused TypeScript validation then rejected the first
cleanup result; the supervisor excluded it and authored a validated replacement.

The final exact view selected only:

- `rev_attachment_upload_dedupe_retry_0002`;
- `rev_offline_sync_diagnostics_replacement_0001`; and
- `rev_offline_sync_stale_cleanup_replacement_0001`.

`view_fixture_c20c053e8e015fc8` resolved with zero final conflicts or
staleness. Focused TypeScript validation `exec_native_0003` and full
`bun run build` `exec_native_0004` passed. Checkpoint
`checkpoint_e5fc8ef209aefc6b_c7b47848ba7b` froze tree
`tree_e5fc8ef209aefc6b59a97ffccd377084c61aa404e50f43dbfd0494c9c505a0a7`.
No tracked application source was accessed outside Sunlight.

OA-03 passes. Banana Split setup, approval-label, capacity, and cancellation
issues are recorded separately in `docs/acceptance/banana_split_feedback.md`
and are not attributed to Sunlight.

## OA-09: unfamiliar tester, public documentation only

Task: `019f9487-903a-7f43-9426-c10eca55bea8`  
Duration: 251,241 ms

Exact prompt:

> Unfamiliar-tester open-alpha acceptance run.
>
> Act as a user who has not participated in Sunlight development. Use only the built Sunlight artifact and the public documentation/configuration present in this disposable CheckChecker repository.
>
> Verify or repair the local Sunlight agent installation using the documented commands, then use Sunlight to implement this engineering task: add a reusable sync-queue summary that reports pending and failed work separately, and use it in the offline checklist notice so failed uploads are visibly distinguished from ordinary pending work.
>
> During the work, deliberately trigger one safe stale-content/precondition error and recover from the facts and next action the product provides. Validate the exact final result and obtain the documented handoff result. Do not use private prompts, internal maintainer knowledge, Git branches, worktrees, commits, remotes, exports, or pushes. Do not read or mutate tracked application source outside Sunlight; if public documentation is insufficient, stop and report the specific missing or contradictory information.
>
> Report the installation/doctor outcome, error and recovery, exact evidence, validation, handoff result, documentation gaps, and whether any tracked source was accessed outside Sunlight.

The fresh tester found that `sun` was not on `PATH`, obtained the absolute
release path from public `.codex/config.toml`, and ran the documented doctor
command. Doctor reported a current skill and a repository ready for
initialization. The tester initialized, created `topic_sync_queue_summary` and
`session_open_alpha_tester`, then deliberately submitted a harmless zero hash.
The structured `precondition_failed` response supplied the actual hash and
fresh-read/retry action. The tester followed it and created `op_native_0001`
and `op_native_0002` without duplicate or partial state.

It completed `rev_sync_queue_summary_0002`, resolved
`view_fixture_effd3a0a6b1ebac4`, passed `bun run build` as
`exec_native_0001`, and created
`checkpoint_27ad15e6d03a201d_3534df3303ba` over tree
`tree_27ad15e6d03a201dff6086c48dac6e8971043c783cc2d25ee859d316df227242`.
The immutable topic completion was the documented handoff. No private IDs or
maintainer help were supplied. The tester reported no blocking documentation
gap and no tracked-source access outside Sunlight. The only caveat—`sun` not on
`PATH`—is already covered by the public absolute-path instructions and managed
client configuration. OA-09 passes.

## Final classification and remaining action

- OA-01: **partial** — three consecutive fresh Codex runs pass; actual Cursor
  run remains required.
- OA-03: **pass**.
- OA-09: **pass**.
- Source/worktree safety: **pass** — no Git handoff or remote action occurred.

The only user-operated open-alpha gate remaining from this campaign is one
fresh Cursor agent run using the installed Cursor adapter.

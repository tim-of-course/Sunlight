# Banana Split feedback from Sunlight acceptance testing

Date: 2026-07-24 (America/Chicago)  
Context: OA-03 fresh-supervisor run in the disposable CheckChecker repository  
Codex task: `019f947b-cdee-7c51-9a7a-9e9c7f43d47d`  
Result: Sunlight workflow completed; Banana Split workflow was cancelled after
an orchestration stall

## What worked

- The fresh supervisor discovered Banana Split without being told to use it.
- After setup, Banana Split launched a root supervisor plus three parallel
  workers with distinct upload, cleanup, and diagnostics responsibilities.
- Those workers exercised real Sunlight concurrency. Sunlight retained their
  completed topics, partial topic, stale compare-and-swap failure, and resolver
  conflict even after the Banana workflow was cancelled.
- Cancellation did not corrupt or erase Sunlight state. The Codex supervisor
  resumed from durable Sunlight facts, excluded incomplete work, integrated
  exact revisions, validated the combined view, and created a checkpoint.

## Friction and blockers

1. **No usable zero-configuration path.** The first workflow start failed
   because the target repository had no `banana-split.json`. The fresh
   supervisor had to infer and write a configuration file before any worker
   could start.
2. **Host policy vocabulary mismatch.** The host exposed approval policy
   `managed`, but Banana Split rejected that value. The supervisor guessed the
   compatible label `on-request` and restarted. Policy labels should be
   normalized by the integration or reported with an exact supported mapping.
3. **Capacity-unaware descendant scheduling.** With
   `max_active_turns: 4`, the root plus three workers filled all capacity, while
   workers scheduled additional descendants. The root then waited on work that
   could not start because no slot could become available.
4. **Supervisor progress stalled behind optional descendants.** Multiple
   bounded polls showed useful Sunlight work had already completed, but the
   Banana root did not advance to integration. It required two external
   instructions to stop waiting on optional descendants.
5. **Cancellation was required to finish.** After roughly ten minutes, the
   Codex supervisor cancelled the Banana workflow and completed the bounded
   integration directly through Sunlight. The overall Codex task took 714
   seconds; a meaningful portion was orchestration wait rather than Sunlight
   mutation, resolution, or validation latency.
6. **Mechanical and model outcomes were hard to distinguish while active.**
   Polling exposed cursors and activity, but did not make the capacity deadlock,
   runnable worker count, or root wait dependency obvious enough for an
   unfamiliar supervisor to diagnose early.

## Recommended Banana Split changes

- Provide a safe default configuration or an explicit documented
  zero-configuration mode for ordinary local repositories.
- Accept the host's trusted approval-policy facts directly, or return a
  machine-readable mapping from host labels to Banana labels.
- Make scheduling capacity-aware: reserve a slot for the root, bound child
  fan-out, and do not queue descendants whose parent cannot yield its slot.
- Prevent the root supervisor from blocking on optional scheduled descendants
  when required leaf work is already complete.
- Surface a compact wait graph: active agents, queued agents, occupied slots,
  which result each agent is waiting for, and whether that dependency can run.
- Emit an explicit `capacity_deadlock` or `no_runnable_agents` event with a
  recommended action instead of relying on repeated semantic polls.
- Make cancellation results clearly distinguish completed descendant output,
  incomplete output, and durable external state that remains usable.
- Add a doctor/init command that validates configuration and policy mapping
  before starting an expensive workflow.

## Sunlight test-policy decision

Banana Split is now excluded from future Sunlight acceptance runs by default.
Use direct fresh Codex tasks or an explicitly fixed, bounded worker set when the
goal is to evaluate Sunlight itself. Use Banana Split only in a separately
named integration test whose evidence and verdict distinguish:

- Sunlight product behavior;
- coding-harness behavior; and
- Banana Split orchestration behavior.

This decision is about test isolation, not a claim that Banana Split is
generally unusable. The run proved that it can launch productive workers, but
its current setup and scheduling behavior materially confounded the Sunlight
efficiency assessment.

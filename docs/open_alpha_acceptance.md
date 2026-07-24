# Sunlight Open Alpha Acceptance

Status: Acceptance in progress — OA-02 through OA-09 pass; OA-01 awaits Cursor
Created: 2026-07-20  
Scope: Local, single-repository Sunlight workflows for coding agents

## Purpose

This document defines the minimum evidence required to describe Sunlight as an
open alpha. An open alpha may have documented limitations and rough edges, but
an unfamiliar user must be able to install it, complete real work, recover from
ordinary failures, and obtain an exact Git-compatible result without risking
silent source corruption or repository escape.

Passing unit tests or deterministic fixtures alone is insufficient. Required
acceptance runs must use real repositories, fresh agent contexts, behavioral
tests, and persisted Sunlight evidence.

## Release decision

Sunlight is open-alpha ready only when:

- every required gate below passes in its declared release scope;
- there are no known critical or high-severity data-loss, state-corruption,
  repository-escape, or silent-integration defects;
- failures observed during acceptance are either fixed and retested or clearly
  classified as intentional negative evidence;
- supported platforms and containment limitations are stated prominently;
- a user can follow repository documentation without maintainer intervention;
- the release evidence bundle is retained with the tested build identity.

A known limitation may remain when it is non-destructive, accurately reported,
has an actionable recovery path, and does not invalidate a normal majority-case
workflow.

## Current baseline

Eight gates now have final-source evidence in the tested Windows/NTFS scope:
common-path/error recovery, MCP termination recovery, exact Git handoff,
repository/execution safety, and realistic four-author scale. The final release
executable is SHA-256
`d69c6cb6ddd6e75f76491ab040a6fb3ec723249831635a610157e58cb1de10b8`.

The previously observed deleted-tracked-path, execution-policy reporting,
dependency ancestry, invalid-session recovery, projection reuse, and writer
contention defects are fixed and covered by the final regression. Three
consecutive fresh Codex runs, a fresh delegated supervisor, and a fresh
unfamiliar-tester run now pass. Automated installation/documentation checks
also pass, but they do not replace an actual Cursor run, which OA-01 still
requires.

Current evidence bundle:

- [OA-01/OA-09 automated evidence](acceptance/evidence/oa01_oa09_automated_2026-07-23.md)
- [OA-02/OA-06 evidence](acceptance/evidence/oa02_oa06_2026-07-23.md)
- [OA-04/OA-05 evidence](acceptance/evidence/oa04_oa05_2026-07-23.md)
- [OA-07 final machine-readable evidence](acceptance/evidence/oa07_2026-07-23.json)
- [OA-07 attempt and remediation history](acceptance/evidence/oa07_attempts_2026-07-23.md)
- [OA-08 Windows-only platform evidence](acceptance/evidence/oa08_windows_2026-07-24.md)
- [Fresh Codex, supervisor, and unfamiliar-tester evidence](acceptance/evidence/oa01_oa03_oa09_fresh_clients_2026-07-24.md)
- [Banana Split integration feedback](acceptance/banana_split_feedback.md)

Future Sunlight acceptance runs use direct fresh coding-client tasks or a fixed,
bounded set of workers by default. Banana Split is excluded unless the run is
specifically testing the Sunlight/Banana Split integration, so orchestration
failures are not misclassified as Sunlight product failures.

## Result vocabulary

- **Pass**: The acceptance criteria and required evidence are satisfied.
- **Partial**: Useful evidence exists, but at least one required case is absent.
- **Fail**: A required behavior is incorrect, unsafe, or needs undocumented
  maintainer intervention.
- **Blocked**: The test cannot run because a prerequisite capability or
  supported environment is unavailable. Blocked is not a passing result.
- **Not applicable**: Allowed only when the published release scope explicitly
  excludes the case.

## Required gates

### OA-01: Clean installation and agent discovery

Run from untouched repositories and fresh client state. Test at least one new
repository and one existing repository with unrelated local configuration and
working-tree changes.

Required cases:

1. Install and diagnose the portable Agent Skill with `sun agent install` and
   `sun agent doctor`.
2. Install the Codex adapter, restart Codex when instructed, and start a fresh
   task in the target repository.
3. Install the Cursor adapter, reload Cursor when instructed, and start a fresh
   agent task in the target repository.
4. Verify that unrelated client configuration is preserved.
5. Give the agent a natural engineering task that says only to use Sunlight.
   Do not provide Sunlight object IDs, MCP tool names, or a lifecycle sequence.

Pass criteria:

- Three consecutive fresh Codex runs discover and use Sunlight without manual
  configuration edits or workflow coaching.
- At least one fresh Cursor run does the same.
- Doctor output accurately states whether initialization or client restart is
  still required.
- Missing tools produce a concise setup/recovery answer instead of silent
  direct-source fallback.
- No configuration lines need to be pasted into a shell.

### OA-02: Common-path correctness and error recovery

Exercise the normal lifecycle using a real repository: initialize, inspect,
author, complete, resolve, execute, checkpoint, and inspect status.

Required negative cases:

- a Git-tracked path deleted from the working tree during initialization;
- a stale content hash during mutation;
- ambiguous or stale patch context;
- an invalid, completed, or wrong-owner session;
- an incompatible or unsupported execution policy;
- a conflicted or stale resolved view;
- an execution failure with bounded output;
- a repeated initialization and repeated read-only inspection.

Pass criteria:

- The common path completes without manual `.sunlight` edits.
- Safe retries do not duplicate operations or corrupt sequence state.
- Every expected failure is structured, preserves state, and gives the agent
  enough facts to select the correct next action.
- Completed revisions and checkpoints remain immutable.
- A dependent-topic workflow can be integrated without copying already
  completed changes into replacement topics merely to express ancestry.

### OA-03: Delegated concurrent authoring and integration

Give one fresh supervisor a realistic engineering objective broad enough to
benefit from three or four workers. Permit normal harness-native delegation,
but do not prescribe Sunlight IDs or a tool-by-tool workflow.

The run must naturally or deliberately cover:

- at least two independent source changes;
- two writers touching the same file;
- one stale compare-and-swap precondition;
- one dependency communicated through `topic_wait` or its public equivalent;
- one worker stopped before completion;
- integration of exact completed revisions rather than moving heads;
- focused tests and a final repository build on the combined exact view.

Pass criteria:

- Every concurrent writer owns a distinct topic and session.
- Independent work integrates without a full repository checkout per author.
- Overlap becomes an inspectable conflict or an explicitly adapted result; it
  is never silently overwritten.
- A stopped worker does not poison unrelated topics or repository state.
- The supervisor completes or clearly excludes abandoned work using durable
  facts rather than polling guesses.
- No application source is accessed or mutated outside Sunlight.

### OA-04: MCP termination and crash recovery

Repeat a real authoring workflow while terminating the MCP server at each of
these boundaries:

1. During or immediately after an artifact mutation.
2. While an execution is running.
3. After topic completion and before integration.
4. While a repository writer lock exists.
5. Immediately before checkpoint creation.

Pass criteria:

- Restart does not lose an acknowledged operation or expose a partial one.
- Locks are recovered automatically or by a documented bounded command.
- Completed topic revisions remain readable and immutable.
- Running or interrupted executions receive an accurate durable state.
- The agent can resume from repository facts without reconstructing native
  state or deleting `.sunlight`.

### OA-05: Exact validation and Git compatibility handoff

Use a sacrificial repository with no production remote. Produce a checkpoint
from passing tests, then exercise the complete Git export path.

Required cases:

- focused behavioral tests, including failing-before and passing-after evidence;
- full repository build or equivalent required validation;
- checkpoint evidence tied to the same exact view and tree;
- export into a new local branch;
- dirty-working-tree protection;
- existing branch-name collision;
- repeated export or retry;
- ordinary Git diff and build of the exported result.

Pass criteria:

- The exported Git tree is content-identical to the checkpoint tree.
- Generated, ignored, quarantined, and `.sunlight`-internal files do not leak
  into the commit.
- Export never changes an existing branch or dirty working tree silently.
- A normal Git consumer can inspect and build the result without Sunlight.
- No push is required for acceptance.

### OA-06: Repository boundary, secrets, and execution safety

Exercise hostile or malformed inputs against an isolated test repository.

Required cases:

- `..` traversal and absolute paths outside the repository;
- symlinks or Windows junctions that point outside the repository;
- mutations targeting `.git` or protected Sunlight state;
- secret ingestion, quarantine, false positives, and explicit recovery;
- execution attempts to write outside the private projection;
- malformed MCP arguments, oversized patches, and unexpected output volume;
- promotion attempts for ignored, secret, oversized, and source-like outputs;
- command, environment, network, CPU, memory, and filesystem policy reporting.

Pass criteria:

- No source, metadata, credential, or execution write escapes its declared
  boundary.
- Policy dimensions distinguish enforced, best-effort, and not-enforced states.
- Secret material does not appear in normal status, logs, checkpoints, or
  exports.
- Denials are fail-closed and include safe recovery guidance.
- No known critical or high-severity safety defect remains.

### OA-07: Realistic repository scale and storage behavior

Run a representative medium repository with thousands of tracked files and
four concurrent authors. Retain the repository's real test/build commands.

Record:

- p50 and p95 latency for status, list, search, read, mutation, and resolution;
- writer-lock contention and retry counts;
- projection creation and cache-reuse time;
- execution queue and command duration separately;
- logical repository bytes, physically materialized bytes, and storage
  amplification;
- `.sunlight` growth across repeated revisions, executions, and checkpoints;
- end-to-end task duration and agent action count compared with a conventional
  working-tree run of similar scope.

Pass criteria:

- No correctness failure, unbounded queue, or full checkout per author occurs.
- Repeated exact views reuse safe cached projections.
- Reported projection strategy and physical-cost measurements are truthful.
- Latency and storage remain practical for interactive agent work. Numeric
  release thresholds must be recorded before the final run and may not be
  relaxed after results are observed without an explicit release decision.

### OA-08: Supported platform contract

Choose the published open-alpha platform scope before final acceptance.

For every supported platform, run installation, initialization, authoring,
execution, checkpointing, restart recovery, and Git export. Record every
containment dimension that is weaker than the Windows implementation.

Pass criteria:

- The published support statement matches tested behavior.
- A Windows-only alpha is labeled Windows-only prominently.
- macOS or Linux is not described as supported based only on compilation or
  unit tests.
- Best-effort containment is never described as enforced isolation.

### OA-09: Documentation and unaided recovery

Give the built artifact and repository documentation to a tester who has not
participated in Sunlight development.

Ask the tester to install Sunlight, complete one coding change, recover from one
injected error, validate it, and obtain the documented handoff result.

Pass criteria:

- The tester succeeds without private prompts, internal object IDs, or direct
  help from a maintainer.
- The README, portable skill, MCP schemas, CLI help, and doctor output agree on
  command names and lifecycle facts.
- Every public tool is discoverable and has its normal use, preconditions,
  important failure modes, and next action documented.
- Restart requirements and local machine-specific configuration are explicit.

## Evidence required for every acceptance run

Retain a short Markdown or JSON record containing:

- date, Sunlight version or build identity, and source commit;
- operating system, filesystem, coding harness, model, and relevant effort;
- target repository identity, size, initial Git status, and remote safety state;
- the exact unscaffolded user prompt;
- topics, sessions, operations, exact revisions, views, trees, executions,
  projections, checkpoints, policy reports, and exports created;
- changed paths and content hashes;
- failing and passing test output with exit results;
- conflicts, staleness, retries, termination points, and recovery actions;
- latency, projection, cache, and storage measurements when applicable;
- whether any tracked source was accessed outside Sunlight;
- final Git status and an explicit statement about commits, remotes, and pushes;
- pass, partial, fail, or blocked result for each applicable gate.

Intentional failing tests, conflict fixtures, rejected safety operations, and
interrupted executions are valid evidence only when the system records them
accurately and the final accepted view is separately validated.

## Open-alpha decision checklist

- [ ] OA-01 clean installation and discovery passes.
- [x] OA-02 common-path correctness and recovery passes.
- [x] OA-03 delegated concurrent integration passes.
- [x] OA-04 termination and crash recovery passes.
- [x] OA-05 exact validation and Git handoff passes.
- [x] OA-06 repository and execution safety passes.
- [x] OA-07 realistic scale and storage behavior passes.
- [x] OA-08 supported platform contract passes for the declared scope.
- [x] OA-09 documentation and unaided recovery passes.
- [x] All observed high-severity findings are fixed and retested.
- [x] Remaining limitations are non-destructive and documented.
- [x] The release evidence bundle identifies the exact tested build.
- [ ] The final release decision and approver are recorded below.

## Decision record

- Decision: Pending OA-01 actual Cursor run and final approver
- Date: 2026-07-24
- Build: `d69c6cb6ddd6e75f76491ab040a6fb3ec723249831635a610157e58cb1de10b8`
- Approver: Pending
- Supported platforms: Windows only for open alpha; Windows/NTFS is the tested scope
- Evidence bundle: `docs/acceptance/evidence/`
- Known limitations: Actual Cursor discovery remains unproven. `sun` may require
  an absolute path when it is not on `PATH`; generated client configuration and
  doctor output expose that path. User-local Bun is not AppContainer-readable
  on this host, so exact-view builds used the explicitly reported and permitted
  `network: not_enforced` mode. macOS and Linux are explicitly unsupported in
  the open alpha.

## Post-alpha tests

These are valuable but are not required for the initial open alpha unless the
published scope claims them:

- very large monorepositories and more than four concurrent authors;
- cross-repository topics and integration;
- hosted forge and automatic pull-request workflows;
- polished graphical management interfaces;
- fuzzy rename-plus-edit inference;
- symbol- or AST-native operations;
- performance optimization beyond the recorded interactive threshold.

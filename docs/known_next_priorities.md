# Known next priorities

This list records the clearest next steps from the current Sunlight evaluations.

## 1. Reduce runtime dependency preparation time

Preparing a private `node_modules` tree remains the largest execution cost. In
the latest TasGrid run, Bun reported 206 ms for 104 tests, while the full
Sunlight execution took 14.61 seconds. Runtime dependency preparation accounted
for 9.03 seconds. Preserve private execution isolation while avoiding a full
per-execution dependency-tree preparation.

## 2. Validate project binding and bounded writer waits under load

Repository-specific MCP identities now prevent project-local servers from
sharing one generic name, and ordinary canonical-state contention waits inside
one bounded command budget. Repeat blind multi-project and shared-repository
tests to confirm clients select the correct binding and no routine status or
authoring call exposes writer contention.

## 3. Keep agent guidance easy to follow

Agents still make occasional incorrect tool-argument and patch-format attempts
before recovering from schema errors. Continue simplifying tool descriptions,
examples, and recovery messages where real evaluations show repeated friction.

## 4. Make long-lived repository status less noisy

Historical failed validation attempts and duplicate checkpoints accumulate in
status output. Keep the durable evidence, but make current actionable state
clear without making agents reason through old expected failures or equivalent
checkpoint records.

## 5. Continue blind shared-repository evaluations

Keep testing multiple unrelated agents in one realistic repository without
telling them about each other or how to use Sunlight. Track natural discovery,
conflicts, retries, total task time, execution overhead, final checkpoint
coverage, and preservation of unrelated worktree edits.

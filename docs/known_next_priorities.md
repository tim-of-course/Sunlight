# Known next priorities

This list records the clearest next steps from the current Sunlight evaluations.

## 1. Reduce protected runtime binding time

Runtime layers now reuse one protected dependency snapshot, and execution
records no longer rewrite canonical repository state. The protected TasGrid
path now spends a 2.599-second median making each private dependency clone
writable. Optimize that phase without making shared cache content writable or
package-manager-specific.

## 2. Validate project binding and bounded writer waits under load

Repository-specific MCP identities now prevent project-local servers from
sharing one generic name, and ordinary canonical-state contention waits inside
one bounded command budget. Repeat blind multi-project and shared-repository
tests to confirm clients select the correct binding and no routine status or
authoring call exposes writer contention.

## 3. Keep agent guidance easy to follow

Patch failures now explain how to recover in the same pinned session, and
artifact search accepts a bounded `limit`. Confirm in blind evaluations that
agents correct ordinary mistakes without creating replacement topics. Simplify
guidance further only where evaluations show repeated friction.

## 4. Make long-lived repository status less noisy

Abandoned topic attempts and their stale conflicts no longer appear as current
work. Historical failed validation attempts and equivalent checkpoints can
still accumulate. Keep the evidence inspectable while making the current
actionable state obvious.

## 5. Continue blind shared-repository evaluations

Keep testing multiple unrelated agents in one realistic repository without
telling them about each other or how to use Sunlight. Track natural discovery,
conflicts, retries, total task time, execution overhead, final checkpoint
coverage, and preservation of unrelated worktree edits.

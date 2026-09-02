# Runtime Layers Approved Addendum

**Decision status:** Approved

**Approved:** 2026-09-01

**Implementation status:** Implemented on 2026-09-01

This document is an approved addendum to
[`sunlight_execution_projection_v0_1.md`](sunlight_execution_projection_v0_1.md).
It replaces that document's runtime-dependency discovery, preparation, cache,
materialization, and execution-record behavior. It also replaces priority 1 in
[`known_next_priorities.md`](known_next_priorities.md). When those documents
conflict with this addendum inside that scope, this addendum controls. Their
unaffected requirements remain in force.

The runtime implementation and public MCP schemas implement this addendum.

## Purpose

Sunlight should let several agents run builds and tests against independent
exact views of one repository without asking the user to create worktrees,
install dependencies for each topic, or coordinate the agents.

Runtime dependencies such as `node_modules` are often large, mostly unchanged,
and excluded from source truth. Sunlight currently scans and privately
materializes them for every execution. That preserves isolation, but it makes a
short test command take several seconds longer and repeats the work for topics
with identical dependency inputs.

The approved design introduces package-neutral **runtime layers**. Sunlight
prepares one immutable local layer for a compatible set of dependency inputs,
shares that exact layer across every matching topic and view, and gives each
execution a cheap private writable binding. A topic never owns a permanent copy
of the layer.

The priorities, in order, are:

1. Ordinary agents can use Sunlight without understanding worktrees or runtime
   layer mechanics, including when many agents work in one repository.
2. Sunlight's core does not depend on one language, package manager, directory
   name, or repository layout.
3. Repeated execution avoids redundant tree scans and copies.

## Product decisions

- The runtime-layer design may replace the current runtime-dependency records,
  cache layout, and implementation directly. There is no compatibility or
  migration requirement for testing-phase repositories or disposable caches.
- This feature does not introduce a versioned provider API, document series,
  schema rollout, or staged release. Provider implementations are internal to
  Sunlight.
- Existing concepts that have already proved useful remain in force: exact
  resolved views, private writable executions, immutable provenance, topic
  independence, explicit output promotion, and truthful isolation reporting.
- Package downloads selected by the repository's lockfile are allowed when the
  layer-building caller's effective network policy allows them. Sunlight does
  not silently weaken a requested network policy. A layer already in the cache
  remains usable by an offline execution.
- Runtime layers are local execution inputs. They never become source truth,
  topic content, checkpoint content, Git export content, or promotable source
  output.

## Terms

**Runtime layer**

An immutable, content-addressed local directory tree used at one or more paths
inside an execution projection. A layer may contain packages, generated SDKs,
compiler support files, or another large derived runtime tree.

**Provider**

An internal component that recognizes a runtime dependency system, declares the
exact source and host facts that determine it, identifies its target paths, and
can prepare it when no reusable layer exists. The core runtime-layer system does
not interpret package manifests or lockfiles.

**Layer lookup key**

A canonical digest of the provider's bounded reuse contract: its declared
resolved-view inputs, relevant local host and tool facts, and target bindings.
Equal keys mean one already-built layer is compatible for reuse on this local
host. The key does not claim that an arbitrary rebuild must produce identical
bytes. The content ID records the exact bytes that were built. A resolved view
ID is not part of the key, so different topics and views with identical
dependency inputs share the same layer.

**Layer content ID**

The digest of the completed layer manifest, including every path, file content,
executable bit, and supported link. The execution record stores the content ID
that was actually bound.

**Layer set**

One provider result containing one or more target paths that must be prepared
and published together.

**Private binding**

An execution-owned writable copy-on-write fork or full-copy fallback of a layer
member at its target path. Private bindings are created per execution, not per
topic. Changes made by a command are discarded with that execution and never
modify the cached layer, another execution, or the human worktree.

## User experience

The existing `execution_run` CLI and MCP inputs remain the normal entry point.
Agents do not need a separate dependency command or provider selection.

For each execution, Sunlight performs this flow:

1. Resolve the exact view and materialize its source projection as it does now.
2. Ask each built-in provider for one of three outcomes: `not_applicable`,
   `required`, or `recognized_unsupported`.
3. Validate all targets from every required plan before cache lookup or
   preparation.
4. Compute lookup keys only from the exact resolved-view source facts and the
   provider's bounded local reuse contract.
5. Reuse every matching cached layer without scanning the human worktree's
   runtime dependency directories or resolving a preparation tool.
6. For a cache miss, prepare the layer from the exact resolved view into an
   empty provider target. Existing worktree dependency directories are not read.
7. Publish a verified immutable layer once. Concurrent executions waiting for
   the same key reuse that result or independently take over after an
   attempt-local failure.
8. Bind each layer into the execution projection through a private writable
   materialization.
9. Run the requested command and record the exact layer identities used.
10. Ignore runtime-layer mutations during source-output discovery. Continue to
   expose nonignored files created outside registered runtime-layer targets as
   ordinary output candidates.

A cache hit must continue to work if the worktree dependency directory was
changed, deleted, or replaced after the layer was created. A topic that changes
any declared dependency input receives a different key and therefore cannot
silently reuse the old layer.

The execution response reports whether each layer was reused or prepared.
Normal cache construction overlap appears as a bounded wait, not a
writer-conflict error. The target command does not start if a required layer
cannot be prepared or bound.

Provider outcomes have exact meanings:

- `not_applicable` means the provider does not own this working directory. It
  cannot block or change the execution.
- `required` contains a complete plan. Sunlight must bind that layer before the
  target command starts.
- `recognized_unsupported` means the repository explicitly selects that
  provider but uses a mode the implementation cannot handle safely. Sunlight
  returns the provider's stable reason before the target command starts.

Providers do not guess command relevance. Once a provider recognizes a complete
workspace for the requested working directory, its layer is required for every
command in that workspace. This eager rule may prepare dependencies before a
source-only command on the first cold run, but it avoids unreliable inspection
of shell scripts and nested commands. Later runs reuse the layer.

## Architecture

### Core runtime-layer service

The core owns behavior that is independent of package tooling:

- provider orchestration;
- canonical lookup-key construction;
- immutable layer manifests and content IDs;
- local content storage and lookup-key entry discovery;
- per-key cross-process single-flight coordination;
- atomic staging, verification, publication, quarantine, and cancellation;
- target-path, metadata, and symlink safety;
- private binding strategy selection for the managed projection filesystem;
- execution provenance and response timing; and
- exclusion of layer targets from source-output discovery.

The core does not contain names such as `node_modules`, `package.json`, Bun,
npm, pnpm, or Yarn. Those belong to a provider.

The internal provider contract returns a `RuntimeLayerPlan` with:

- `provider_id` and a provider semantics digest used only to invalidate cache
  entries when the provider's reuse meaning changes;
- repository-relative workspace root and target paths;
- exact resolved-view input refs, each with path, artifact identity, and content
  hash;
- provider-declared local OS, architecture, configuration, and controlled
  environment facts that bound reuse;
- preparation command and working directory when preparation is supported;
- the expected output target set; and
- the reason for a required or unsupported outcome.

This is an internal Rust boundary, not a stable external plugin interface. A new
provider should be addable without changing cache, execution, or provenance
semantics. An external provider system is a separate future decision.

### Lookup identity

The core hashes one canonical record containing:

- provider ID and provider semantics digest;
- ordered target paths;
- ordered resolved-view input paths, artifact IDs, and content hashes;
- provider-selected repository-declared package-manager identity when present;
- operating system and architecture;
- provider-selected configuration and patch inputs; and
- the digest of controlled environment values available to preparation.

The key excludes unrelated source files, topic IDs, revision IDs, resolved view
IDs, timestamps, worktree state, preparation network policy, and command
arguments. Changing source code alone therefore does not invalidate
dependencies. Preparation network policy is an attempt capability, not a
property of reusable content.

The lookup key is deliberately a bounded reuse policy, not a proof that every
lifecycle script is a pure function of the key. Runtime layers are local to one
repository and host. The preparation record captures the actual builder tool,
environment, and policy facts, while the content ID identifies the exact result.
The first successfully published object for a lookup key is reused until it is
removed or quarantined.

The first implementation does not deduplicate objects across different lookup
keys. This keeps publication and corruption recovery inside one per-key lock.
Execution provenance always stores both the lookup key and content ID.

### Local storage

Runtime layers live under a local-only directory beside the managed execution
roots so the preferred binding strategy can share filesystem blocks:

```text
<managed-projection-root>/.runtime-layers/
  entries/<lookup-key>/
    manifest.json
    targets/<member-id>/
  staging/<attempt-id>/
  locks/<lookup-key>.lock
  quarantine/<attempt-id>/
```

One entry directory contains both the active manifest and every target member
for one lookup key. The manifest is the authority for the content ID. There is
no separately published key mapping. Timestamps, ACLs, extended attributes, and
platform-specific metadata are normalized or omitted; paths, bytes, executable
bits, and supported relative links are preserved.

Published entries are made read-only and are never exposed directly to a
writable command. This protects them from normal execution writes. It is not a
claim that files owned by the same operating-system user are adversarially
immutable. If Sunlight detects manual corruption, it quarantines the entry and
rebuilds or returns a stable error. Warm execution does not perform a complete
content reread because that would restore the current latency problem.

Runtime-layer caches are disposable. The cutover ignores the previous runtime
dependency cache and does not migrate it. Cache garbage collection and a public
cache-management interface are outside this addendum. If storage is exhausted,
Sunlight returns `runtime_layer_storage_full` and identifies the local
`.runtime-layers` directory that may be removed after active executions finish.

### Cross-process coordination

The lock scope is one lookup key. Different layer keys can build concurrently,
and runtime-layer work never holds the canonical repository publication lock.

For one key:

1. The first process becomes the builder.
2. Other processes wait with their own cancellation signal, preparation
   deadline, and build capabilities.
3. The builder rechecks the key facts, creates a private build projection,
   prepares the layer, and then materializes a separate cache-owned staging
   entry. Every regular file is copied or safely copy-on-write cloned into a new
   file identity. The publisher never renames the provider-created target into
   the cache and never preserves an external hardlink.
4. The builder computes and verifies the staged manifest, makes the complete
   staged entry read-only, then atomically renames it to
   `entries/<lookup-key>` with create-if-absent behavior. Manifest and targets
   therefore become visible together, already sealed.
5. Waiters validate the published marker and manifest identity, then bind the
   same object.
6. A canceled builder or attempt-local failure publishes nothing and releases
   the lease. Network-capable waiters recheck the cache and may elect a new
   builder. A disabled-network caller never becomes a cold builder; it may wait
   for an already active capable builder and reuse that result.
7. A permanent planning failure, target violation, or invalid repository input
   is returned independently by every caller and is not retried under the lock.

Each caller may become builder at most once per execution. A caller that already
failed its own build attempt returns that failure instead of creating a retry
loop. No process runs the target command while holding the layer lock. A lock
left by a dead process is recoverable through the existing bounded-owner
evidence pattern rather than an unbounded wait.

Recovery under the key lock treats any final entry as one unit:

- a missing entry is a cache miss;
- a complete entry with a valid manifest is a cache hit, including after a
  builder crashed immediately after the atomic rename;
- a final entry without a complete valid manifest is quarantined before a new
  build begins; and
- quarantine moves the entire entry, so a manifest can never point to a target
  that was moved separately.

### Provider preparation on a cold miss

Preparation uses a source projection materialized from the exact resolved view,
never source bytes from the mutable worktree. Sunlight establishes the same
effective execution policy before launching the provider command. Provider
preparation must never receive weaker filesystem, process, or network policy
than the requested execution. Platforms that currently report a policy as
`not_enforced` continue to report that limitation for preparation and the target
command rather than claiming containment they do not provide.

Preparation uses a canonical private environment. Sunlight creates empty home,
configuration, cache, and temporary directories inside the builder projection
and points `HOME`, `USERPROFILE`, `HOMEPATH`, `XDG_CONFIG_HOME`, `APPDATA`,
`LOCALAPPDATA`, `TEMP`, and `TMP` to them. The lookup record represents these as
stable tokens such as `PRIVATE_HOME` and `PRIVATE_TEMP`, never as
execution-specific absolute paths. It hashes the remaining effective allowlist
values after replacing the private builder-root prefix with `PRIVATE_ROOT`.
This produces the same key for two executions whose only difference is their
private projection ID.

The provider explicitly unsets Bun configuration override variables and does
not inherit the human account's global Bun or npm configuration. Repository
configuration files named by the provider remain available from the exact view.
The construction record stores the same normalized environment plus the actual
private policy used, without serializing secrets or machine-specific private
paths.

The provider command runs in frozen or immutable lockfile mode and may download
the packages selected by the repository's lockfile when network access is
effective. Its package download cache is private to the builder projection and
is discarded after publication. Sunlight does not expose or write the human
account's Bun cache.

An already-published layer is usable with a disabled network request. On a cold
miss, a disabled-network caller may wait for an already active network-capable
builder. If no such builder publishes within the caller's preparation deadline,
the caller returns `runtime_layer_network_unavailable` without launching Bun.
Sunlight never overrides the requested policy.

Preparation scripts are allowed because some trusted packages require them.
They run with the same effective containment and bounded process handling as the
target execution. The execution record identifies the provider command,
executable digests, requested policies, and effective policies. Sunlight does
not claim that a package manager can prevent arbitrary network use by lifecycle
scripts when network access is allowed.

Preparation starts with empty provider targets. Sunlight never reads an existing
worktree dependency directory when building or reusing a layer. After
preparation, Sunlight rejects source-path changes in the builder projection,
captures only the provider's declared target paths, verifies those paths, and
publishes them as one layer set. A successful layer remains reusable even if
the original target command later fails.

Runtime-layer acquisition has its own deadline equal to the configured command
timeout. It begins when provider discovery finishes and covers lock waiting,
preparation, publication, and private binding. The target command then
receives the full existing command timeout, beginning immediately before the
target process is launched. This preserves the current meaning of the execution
timeout while bounding cold preparation. Both stages share the caller's
cancellation signal and report separate elapsed and remaining time.

### Private binding

Every execution receives its own writable binding. Topics and views retain only
references to shared immutable objects and never receive persistent directory
copies.

The materializer reuses the existing projection cache's cloning, permission,
path, staging, and stale-recovery helpers. It selects a strategy from measured
capabilities of the managed projection filesystem:

- a filesystem-supported recursive copy-on-write clone is preferred;
- a safe per-file copy-on-write clone may be used when recursive cloning is not
  available;
- full copy is the required correctness fallback; and
- writable hardlinks are forbidden.

The layer store and execution roots are under the same configured managed root.
The implementation preserves executable bits and relative links. A stored link
may target another path in the completed execution projection, but validation
rejects an absolute target or any target that escapes the projection.

A command may freely mutate its private binding. Sunlight records the immutable
starting layer identity and does not perform a full post-command dependency
fingerprint. Runtime-layer mutations are local execution side effects and are
discarded. This avoids another large traversal while preserving the exact input
provenance that Sunlight controls.

## Initial built-in provider

The first provider supports the repository shape already exercised by TasGrid:
a single-root Bun project with one root `package.json`, one text `bun.lock`,
optional root `bunfig.toml`, Bun's `node_modules` linker, and one root
`node_modules` target. The legacy binary `bun.lockb` format is unsupported in
the first provider. This is the first use of the generic contract, not a special
case in the core.

Provider behavior follows Bun's official
[`bun install`](https://bun.sh/docs/pm/cli/install),
[`bunfig.toml`](https://bun.sh/docs/runtime/bunfig), and
[lifecycle script](https://bun.sh/docs/pm/lifecycle) documentation. The
supported-mode checks are requirements, not best-effort warnings.

Provider root selection is deterministic. Starting at the requested working
directory, walk ancestors inside the resolved view and select the nearest
directory containing `package.json` and `bun.lock`. A
`packageManager` field naming Bun confirms the selection. A field naming another
manager conflicts with the Bun lockfile and returns
`runtime_layer_provider_ambiguous`. A root that contains only `bun.lockb` and
explicitly selects Bun is `recognized_unsupported`; otherwise it is not
applicable.

The initial provider returns `recognized_unsupported` when `package.json`
declares workspaces, the lockfile contains a workspace key other than the root
`""` entry, or the selected root uses a non-hoisted linker, nested managed
targets, project `preinstall`, `install`, `postinstall`, `preprepare`, `prepare`,
or `postprepare` scripts, `file:`, `link:`, or workspace-local dependencies, or
another preparation mode not represented above. npm, pnpm, Yarn, Yarn
Plug'n'Play, and multi-root workspace support are future providers or provider
extensions. Their absence does not change the package-neutral core contract.

For a supported Bun root, the provider declares:

- the root `package.json`, selected Bun lockfile, optional root `bunfig.toml`,
  optional root `.npmrc`, and every lockfile-referenced patch file;
- the repository-declared Bun identity when present;
- OS, architecture, and the canonical controlled preparation-environment
  record;
- root `node_modules` as the only target; and
- `bun install --frozen-lockfile --linker hoisted --concurrent-scripts 1
  --cache-dir <private-builder-cache>` at the selected root as the preparation
  command.

The provider parses both the manifest and lockfile before returning `required`.
It rejects every unsupported lifecycle hook and local dependency protocol even
when it appears only in the lockfile. The synthetic private home prevents Bun
from loading `$HOME/.bunfig.toml`, user `.npmrc`, or other account-global
configuration. The repository root files listed above are the only Bun and npm
configuration inputs allowed in this provider.

The initial provider does not support `.npmrc` or `bunfig.toml` settings that
select additional filesystem inputs or output locations, including certificate,
cache, prefix, or global-directory paths. It rejects those settings whether the
path is relative or absolute. A later provider extension may support one by
adding every referenced repository file to the exact lookup inputs. Environment
substitutions and paths outside the resolved workspace also remain unsupported.

The lookup key uses the repository-declared Bun identity, not the content of a
machine-local executable. On a cache miss, the actual `bun` executable must be
available through the controlled environment. Preparation provenance records
its resolved path digest and reported version. A cache hit does not require Bun
to remain available unless the target command itself invokes it.

When `packageManager` declares `bun@<version>`, a cold miss requires the reported
Bun version to match exactly before preparation. Without a declared version,
the available Bun executable is accepted and its exact digest and reported
version are construction provenance rather than lookup-key inputs.

Installing Bun is outside this addendum. A cold miss without Bun returns
`runtime_layer_provider_tool_missing`. A `package.json` without a Bun lockfile
is `not_applicable` unless it explicitly declares Bun, in which case it is
`recognized_unsupported` with reason `missing_lockfile`.

Before any cache lookup or build, the core validates every provider target. A
target must be repository-relative, Git-ignored even when absent, accessible
to Sunlight because `.sunignore` does not exclude it, and disjoint from `.git`,
`.sunlight`, all native view entries, every other target in the layer set, and
every target returned by another provider. Equal, ancestor, and descendant
target overlaps are rejected with `runtime_layer_target_conflict`. This rule
applies equally to preparation and binding, so a layer can never mask
resolved-view source.

Git-ignore eligibility is evaluated from the exact resolved view's repository
`.gitignore` files, not the mutable worktree, Git index, `.git/info/exclude`, or
global Git configuration. `.sunignore` remains the human-owned repository policy
and is evaluated through its existing recorded policy identity.

On Windows, provider preparation uses the same executable-compatibility check
as the target runner. A Bun executable that cannot run under the requested
AppContainer or filesystem policy returns
`runtime_layer_provider_tool_incompatible`; it is not reported as a missing
tool or a network failure.

Repositories outside this supported shape continue execution without runtime
layers when the provider returns `not_applicable`. The requested command remains
responsible for any runtime files it creates.

## Persisted and public data

New execution records replace `runtime_dependency_paths`,
`preexisting_runtime_dependency_paths`, `runtime_dependency_bindings`, and
`runtime_dependency_strategy` with `runtime_layers`.

Each execution stores one entry per layer set. `lookup_inputs` preserves the
exact declared source facts behind the lookup key without introducing a second
digest of the same record. `construction` is copied from the immutable layer
manifest and always describes how the cached object was originally built. The
execution's existing policy fields continue to describe the current target
command.

```json
{
  "layer_set_id": "runtime_layer_set_...",
  "provider_id": "bun_single_root",
  "provider_semantics_digest": "sha256:...",
  "lookup_key": "sha256:...",
  "lookup_inputs": [
    {
      "path": "package.json",
      "artifact_id": "artifact_...",
      "content_hash": "sha256:..."
    },
    {
      "path": "bun.lock",
      "artifact_id": "artifact_...",
      "content_hash": "sha256:..."
    }
  ],
  "content_id": "sha256:...",
  "acquisition": "cache_reuse",
  "construction": {
    "origin": "provider_preparation",
    "command_argv": [
      "bun",
      "install",
      "--frozen-lockfile",
      "--linker",
      "hoisted",
      "--concurrent-scripts",
      "1",
      "--cache-dir",
      "PRIVATE_PACKAGE_CACHE"
    ],
    "working_directory": ".",
    "tool": {
      "name": "bun",
      "executable_digest": "sha256:...",
      "reported_version": "..."
    },
    "environment": {
      "HOME": "PRIVATE_HOME",
      "XDG_CONFIG_HOME": "PRIVATE_CONFIG",
      "TEMP": "PRIVATE_TEMP"
    },
    "environment_digest": "sha256:canonical-environment",
    "network_policy_requested": "not_enforced",
    "network_policy_effective": "not_enforced",
    "filesystem_write_policy_effective": "not_enforced"
  },
  "targets": [
    {
      "path": "node_modules",
      "materialization_strategy": "recursive_cow"
    }
  ]
}
```

`acquisition` is `provider_preparation` for the caller that publishes the layer
and `cache_reuse` for later callers. Multiple target bindings from one atomic
provider result share one layer-set identity, lookup key, content ID, and
construction record. `layer_set_id` is the digest of provider ID, lookup key,
and content ID, so it remains identical on cache reuse.

Construction argv uses stable path tokens such as `PRIVATE_PACKAGE_CACHE`.
Sunlight substitutes the private local path only when launching the builder and
does not persist or expose that machine-specific path.

The `execution_run` response adds response-only timing for:

- provider discovery and key calculation;
- cache lookup;
- wait for an in-progress build;
- provider preparation;
- private binding; and
- the target command.

These timings are diagnostic observations and are not durable identity fields.
Status and inspect show durable layer identities, construction, acquisition,
target, strategy, and policy facts. They do not expose machine-specific cache
paths by default.

No public schema version, cache migration, or dual-write period is required.
Newly written records use only `runtime_layers`, and old local runtime cache
entries are ignored. The loader continues to accept existing historical
execution records whose optional legacy runtime-dependency fields are already
supported. It presents those records as `legacy_unrecorded` runtime provenance
and never rewrites them. This read-old/write-new tolerance preserves authored
repository history without constraining the new model.

## Failure and recovery

| Condition | Required behavior |
| --- | --- |
| Same key is already building | Wait within the caller's preparation deadline, report wait time, then reuse the published layer or take over after an attempt-local failure. |
| Builder is canceled or crashes before final rename | Publish no final entry; release or recover the lease so an uncanceled waiter can become builder. |
| Builder crashes after final rename | The next lock owner adopts the complete valid entry or quarantines the whole invalid entry before rebuilding. |
| Disabled-network caller sees a cold miss | Wait for an already active capable builder when present; otherwise return `runtime_layer_network_unavailable` without launching Bun. |
| Provider inputs are ambiguous | Do not guess a manager or lockfile; return `runtime_layer_provider_ambiguous` before the target command starts. |
| Provider recognizes an unsupported mode | Return `runtime_layer_provider_unsupported` with the exact reason before the target command starts. |
| Targets overlap source, reserved paths, or each other | Return `runtime_layer_target_conflict` before cache lookup or preparation. |
| Required tool is missing | Return `runtime_layer_provider_tool_missing` with the provider and executable name. |
| Required Bun version does not match | Return `runtime_layer_provider_tool_version_mismatch` with declared and reported versions. |
| Bun cannot run under the requested containment | Return `runtime_layer_provider_tool_incompatible` with the effective policy and command-started false. |
| Frozen preparation rejects repository inputs | Return `runtime_layer_preparation_failed` with bounded command output and no cache publication. |
| Network is required but unavailable by policy | Preserve the policy and return `runtime_layer_network_unavailable`. |
| Preparation changes a source path | Reject the result with `runtime_layer_preparation_modified_source`; do not promote or publish the change. |
| Layer entry or link is unsafe | Reject and quarantine staging with `runtime_layer_invalid_content`. |
| Published entry is detectably corrupt | Under the same key lock, quarantine the whole entry and allow one rebuild by a caller that has not already built during this execution. |
| Preferred binding strategy is unsupported | Fall back to full copy and record the selected strategy. |
| Private binding fails | Remove the unpublished execution projection and return `runtime_layer_binding_failed`; do not run the target command. |
| Layer storage is full | Publish nothing and return `runtime_layer_storage_full` with the local cache-removal location. |

Failures before the target command create no running execution record. The
error response includes `command_started: false`, the provider, target, lookup
key when available, requested and effective policies, and one next action.

## Implementation sequence

1. Introduce the package-neutral provider outcome, plan, manifest, cache entry,
   execution binding, and response timing types without changing current
   execution writes.
2. Implement local storage, content hashing, one-directory atomic publication,
   orphan reconciliation, quarantine, and cancellable per-key leader election
   independently of the canonical repository writer.
3. Reuse the projection materializer for private binding, copy-on-write fast
   paths, full-copy fallback, metadata normalization, and whole-projection link
   validation.
4. Implement target ownership validation and provider orchestration, then add
   the narrow `bun_single_root` provider outside core code.
5. Implement frozen Bun preparation from empty targets and verify that cold and
   warm paths never scan the worktree dependency tree, while warm hits do not
   resolve the Bun executable.
6. Establish execution policy before provider command preparation, preserve a
   separate preparation deadline, and verify builder source paths remain
   unchanged.
7. Wire registered-target exclusion, execution responses, status, inspect, MCP,
   and read-only loading of historical execution records to understand the new
   shape. Exercise the complete path behind the internal boundary.
8. Only after every executable reader and output scanner is ready, cut new
   execution writes over to `runtime_layers`. Then update operator documentation
   and agent guidance.
9. Remove the superseded per-execution dependency scan, previous binding writes,
   and old cache use. Run controlled concurrency and performance validation
   before calling the addendum implemented.

## Acceptance criteria

- Two topics whose declared dependency inputs are identical receive the same
  lookup key and reuse the one locally published content ID even when their
  resolved view IDs and unrelated source files differ.
- A topic that changes the Bun lockfile, root manifest, Bun configuration,
  referenced patch, repository-declared Bun identity, controlled environment
  digest, OS, or architecture cannot reuse the prior lookup key.
- Two Windows executions with different private projection paths receive the
  same lookup key when their canonical preparation environments are otherwise
  equal.
- Project install or prepare hooks, local dependency protocols, external Bun
  configuration, non-root lockfile workspaces, and non-hoisted linkers are
  rejected as unsupported before a build begins.
- Three concurrent executions for one cold key publish one layer set. The other
  executions wait and reuse it without duplicate builds or canonical-state
  writer conflicts.
- Canceling the elected builder does not cancel an unrelated waiter. A waiter
  may take over and publish the layer within its own preparation deadline.
- A disabled-network caller never becomes the cold builder. It may reuse an
  active network-capable builder's result, while its own inability to build does
  not affect that builder or another waiter.
- Three executions using one cached layer have distinct writable target paths.
  Mutating one target changes neither another target, the cache object, nor the
  human worktree.
- A warm execution reuses its layer when the corresponding worktree directory
  is missing or has different contents, and performs no traversal of that
  worktree directory.
- Cold preparation does not read or trust an existing worktree dependency tree.
  The worktree tree may be missing, incomplete, or modified without changing the
  prepared result or its cache publication.
- A supported single-root Bun project with no worktree `node_modules` can
  prepare a layer automatically and run its requested command without a
  separate Sunlight instruction.
- A supported Bun root requires its layer for every command in that workspace.
  An unrelated repository is `not_applicable`, while a selected unsupported Bun
  workspace fails before layer preparation with a precise reason.
- A provider target that overlaps source, `.git`, `.sunlight`, another target,
  or a `.sunignore` exclusion is rejected before cache lookup and never masks an
  exact-view path.
- Git-ignore eligibility follows the exact resolved-view `.gitignore` content,
  so an unrelated worktree edit cannot enable or disable a layer target.
- Disabled network policy is never weakened for preparation. An offline cache
  may satisfy the build; otherwise the failure identifies the required policy
  or missing package without starting the target command.
- A failed or canceled preparation publishes no final entry and cannot poison a
  later retry. A crash immediately after final rename leaves one complete entry
  that the next caller adopts without rebuilding.
- A provider-created hardlink to a writable package cache is broken during
  publication. Mutating the external inode cannot change the published entry.
- Runtime-layer paths and their mutations never appear as source promotion
  candidates. A nonignored command-created file outside those paths remains
  visible and promotable.
- Relative links that remain inside the completed projection work. Absolute
  links and projection escapes are rejected.
- An unsupported copy-on-write strategy uses the full-copy fallback and
  preserves the content ID and private-write isolation.
- Execution, status, and inspect identify every starting layer by provider,
  provider semantics, lookup key, content ID, construction provenance, current
  acquisition, target, materialization strategy, and effective policy without
  exposing local cache paths.
- Layer acquisition is bounded independently from the target command. A cold
  build cannot consume the target command's full timeout.
- Historical execution records with legacy runtime-dependency fields remain
  readable, while new records write only `runtime_layers`.
- A declared Bun version mismatch and an executable that is incompatible with
  the effective Windows containment return their distinct stable errors before
  preparation starts.
- No runtime-layer object or diagnostic path enters a topic, checkpoint, Git
  export, or commit-default manifest.

## Validation plan

Use focused unit tests for canonical key construction, canonical environment
normalization, manifest/content hashing, path and link validation, provider
outcomes, Bun root selection and supported mode detection, target ownership,
one-directory publication, and stale-lock recovery. These protect the generic
boundaries without duplicating end-to-end tests.

Use repository-backed integration tests for:

- reuse across distinct views with identical inputs;
- invalidation by each declared input class;
- one-build behavior under three concurrent processes;
- canceled-builder takeover and mixed offline/online waiters;
- private mutation isolation;
- warm reuse after worktree dependency removal or replacement;
- cold preparation with a missing, incomplete, and concurrently changing
  worktree dependency tree;
- actual Bun provider discovery fixtures, including nested-root selection,
  conflicting lockfiles, explicit-manager conflict, missing lockfile, and
  rejected workspace modes, lifecycle hooks, local dependency protocols,
  root `.npmrc` invalidation, and account-global configuration exclusion;
- stable lookup keys across different private Windows runtime roots;
- source, reserved-path, cross-provider, and ancestor/descendant target
  collisions;
- crash after final entry rename, adoption of a complete orphaned entry,
  quarantine of an incomplete final entry, and whole-entry corruption recovery
  with exactly one rebuild;
- normalization of a provider-created hardlink to a writable external file;
- declared Bun version mismatch and Windows containment incompatibility;
- successful and failed provider preparation through a deterministic fake
  provider, without depending on a public registry;
- legacy execution-record loading and new-record serialization;
- separate acquisition and target-command deadlines;
- output exclusion at registered targets and normal promotion elsewhere; and
- full-copy fallback when copy-on-write is unavailable.

When Bun is available, an opt-in local smoke fixture runs a frozen install from
local fixture packages and proves the real command boundary without relying on
a public registry. Controlled tests, not a live network install, decide merge
readiness.

Before removing the old implementation, record a release-build baseline on the
same Mac, filesystem, TasGrid checkout, resolved view, and harmless test command.
Compare at least five warm runs before and ten warm runs after, with one cold
run reported separately. The median warm runtime-layer preparation and binding
time must improve by at least 2x from the old dependency-preparation median. The
comparison must separate provider discovery, cache wait, cache build, binding,
and command time. The requested command and its result must remain equivalent.
The new layer's declared inputs and content ID, rather than equality with the
legacy worktree snapshot, establish dependency provenance.

After the controlled acceptance gates pass, run three blind Luna-high tasks
against independent, possibly overlapping TasGrid topics. They receive ordinary
task goals and no runtime-layer instructions. This is product evaluation
evidence, not a deterministic merge gate. Record natural `execution_run` use,
worktree or dependency coordination attempts, routine writer-contention errors,
topic independence, and observed layer timing.

## Implementation evidence

The initial implementation uses the generic runtime-layer store and the
`bun_single_root` provider described here. Focused tests cover cold preparation,
warm reuse without Bun in `PATH`, worktree dependency replacement, private
mutation isolation, disabled-network cold misses, persisted status reporting,
and two-process single-flight publication. The full workspace suite passed on
macOS after the cutover.

A release-build TasGrid smoke test used the same resolved view and harmless
`bun --version` target command for one cold run and ten sequential warm runs.
The cold provider preparation took 29.741 seconds and was reported separately.
Every warm run reported `cache_reuse`; median provider discovery, cache lookup,
wait, and private binding together took 2.243 seconds, including a 2.186-second
median private binding. The recorded pre-cutover dependency preparation was
about 9.03 seconds, so the runtime-layer portion improved by about 4x. Median
full warm execution time was 6.367 seconds. Full execution includes resolved
view projection, command startup, output scanning, and durable publication, so
those costs remain separate from runtime-layer timing.

## Explicit non-goals

- A public provider SDK, dynamic plugin loader, or provider marketplace.
- Reading, copying, or publishing an observed worktree dependency directory as
  a runtime-layer input.
- npm, pnpm, Yarn, Plug'n'Play, and Bun workspace support in the first provider.
- Automatic installation of package managers, language runtimes, compilers, or
  operating-system packages.
- Container or virtual-machine images for the entire execution environment.
- Adversarial protection against the repository owner's direct modification of
  local cache storage.
- Capturing runtime-layer mutations as source or carrying them between
  executions.
- Cache garbage collection, remote cache sharing, or cross-repository cache
  sharing.
- Content-object deduplication across different lookup keys.
- Guaranteed bit-for-bit package-manager output across different host
  toolchains when the provider has not declared those toolchain facts.

## Remaining assumptions

- The repository's lockfile and package-manager configuration are the intended
  authority for package selection.
- The selected package-manager and runtime executables already exist in the
  controlled execution environment.
- The managed projection root has enough local storage for one immutable layer
  plus concurrent private changes or the full-copy fallback.
- Runtime layers are trusted local derived data. Content identity and private
  binding protect normal use; they are not a security boundary against the
  operating-system account that owns the repository.

There are no unresolved product or architecture decisions blocking the
implemented architecture. Broader providers and the blind multi-agent product
evaluation remain follow-up work.

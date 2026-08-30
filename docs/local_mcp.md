# Local MCP stdio server

Sunlight can expose one repository's native v0.3 workflow to local coding
agents over MCP JSON-RPC 2.0 stdio:

```powershell
cargo build -p sun
target\debug\sun.exe init --repo C:\src\my-repo
target\debug\sun.exe mcp serve --repo C:\src\my-repo
```

The server negotiates MCP `2025-11-25` and the stable `2025-06-18`,
`2025-03-26`, and `2024-11-05` revisions. It advertises the `tools`
capability with a stable list (`listChanged: false`). Stdio messages are
newline-delimited UTF-8 JSON. Legacy `Content-Length` framed input is accepted;
output always uses the current newline transport. Stdout contains protocol
messages only.

An uninitialized existing directory may also be bound so the
`repository_init` tool can perform the one-time ingest. All other repository
tools require initialized native state.

## Harness-neutral agent setup

Install the portable Sunlight Agent Skill and the selected client's local MCP
entry from the target repository:

```powershell
sun agent install --client generic
sun agent install --client codex
sun agent install --client cursor
```

Use one client value per installation. `generic` installs only
`.agents/skills/sunlight`; its doctor does not verify a client transport or live
tools. Configure a generic client separately to launch `sun mcp serve --repo
<repository-directory>`. Codex also manages `.codex/config.toml`, and Cursor
also manages `.cursor/mcp.json`. Existing unrelated client configuration is
preserved. Verify an adapter-managed result with:

```powershell
sun agent doctor --client codex
```

Restart or reload the client after its MCP configuration changes. Setup is
complete when `repository_status` and the artifact tools are visible for the
intended repository. The generated
configuration contains local absolute paths and should be treated as
machine-local unless the team intentionally replaces them with a portable
installation convention.

## Manual Codex configuration

Add a server entry to `%USERPROFILE%\.codex\config.toml`, using absolute paths:

```toml
[mcp_servers.sunlight_my_repo]
command = 'C:\src\sunlight\target\debug\sun.exe'
args = ['mcp', 'serve', '--repo', 'C:\src\my-repo']
startup_timeout_sec = 15
tool_timeout_sec = 900
```

For project-local configuration, place the same entry in
`<repository>/.codex/config.toml`. `sun agent install --client codex` uses this
safer repository-scoped form.

Cursor uses an equivalent project-local `.cursor/mcp.json` entry:

```json
{
  "mcpServers": {
    "sunlight": {
      "command": "C:\\src\\sunlight\\target\\release\\sun.exe",
      "args": ["mcp", "serve", "--repo", "C:\\src\\my-repo"]
    }
  }
}
```

For Claude Desktop, the equivalent entry in its JSON configuration is:

```json
{
  "mcpServers": {
    "sunlight-my-repo": {
      "command": "C:\\src\\sunlight\\target\\debug\\sun.exe",
      "args": ["mcp", "serve", "--repo", "C:\\src\\my-repo"]
    }
  }
}
```

Restart the client after changing its configuration. Configure a separate
server entry for each repository; a running server cannot switch roots.

## Tools

The server exposes these typed tools:

- `repository_init`, `repository_status`
- `topic_create`, `topic_complete`, `topic_wait`, `session_start`, `session_refresh`
- `artifact_read`, `artifact_list`, `artifact_search`
- `artifact_patch`, `artifact_write`, `artifact_move`, `artifact_delete`,
  `artifact_metadata_set`
- `view_resolve`, `project_materialize`
- `compat_project`, `compat_diff`, `compat_import`
- `worktree_diff`, `worktree_capture`
- `execution_run`, `execution_promote_output`
- `checkpoint_create`
- `policy_check_export`, `policy_check_commit`, `policy_explain`
- `git_export`, `inspect`

Repository status returns `repository.recommended_start`, the newest usable
checkpoint, view, and tree for new work. `session_start` accepts that exact view
and remains pinned to it while unrelated topics resolve or conflict.
For durable integration, pass the recommended checkpoint to `view_resolve.base`
and exact new or replacement topic revisions to `view_resolve.include`.
Sunlight includes the checkpoint's existing frontier automatically. Omitting
`include` on a later checkpoint reproduces that checkpoint; omitting it on the
repository base is discovery-only and resolves moving current heads.
`checkpoint_create` returns `handoff.exact_ids` with the exact checkpoint, view,
tree, and execution IDs to report or pass to the next agent.
`compat_diff` re-echoes the projection's `session_generation_id`, and MCP
requires that value for the immediately following `compat_import`.
Repository status also compares ordinary repository-root files with Sunlight's
durable worktree anchor. `worktree_diff` inspects those external changes without
mutation. `worktree_capture` adopts all eligible candidates, or an explicit
candidate/path selection, as one operation in a new completed topic and advances
the anchor without rewriting the files. A clean capture creates no records.
For `execution_promote_output`, pass the candidate classification returned by
`execution_run` verbatim (`source_like_delta` or `generated_artifact`); those
execution provenance classes are distinct from artifact classifications.
Artifact mutations expose `source` and `generated`: both checkpoint, but a
generated artifact exports only with reachable execution-output promotion
provenance. Relabeling does not create provenance.
`policy_check_commit` validates `.sunlight/**` metadata candidates, or the
managed `.gitignore` block when paths are omitted. It returns an inline report,
not a persisted `validation_report_id`. Source-artifact safety uses
`policy_check_export` with both an exact checkpoint and target ref; its persisted
report can be passed to `policy_explain`.
When a target ref points to a prior Sunlight export on that same ref, a later
checkpoint export appends to the mapped commit and writes a new export map.
Unrecognized ref tips still fail closed. A Git handoff is complete when export
returns the new `export_map_id` for that checkpoint and ref.

Every successful or native command-error result includes both MCP text content
and `structuredContent` containing the existing `sun --json` envelope. Native
errors such as `precondition_failed`, `repository_writer_busy`, and
`concurrent_state_update` are returned as tool errors without translation. Every
structured error contains `code`, `message`, inspectable `details`, and a
`next_action`. Treat `next_action` as the safe normal recovery rule, then use
the returned exact IDs, hashes, conflicts, or staleness facts from `details`
before acting; never retry a mutation blindly.
Successful tool results also include `data.transport.queue_ms`, `worker_ms`, and
`automatic_concurrency_retries`. `queue_ms` includes both this server's local
call queue and any cross-process repository-mutation queue; `worker_ms` excludes
that wait. Automatic retries count repository CAS/writer retries, not queue
polls. These are observational facts for diagnosing interactive latency and
writer contention; they do not change operation semantics. Failed tool calls
put the same transport object under `error.details.transport`.

Source inclusion follows the [repository README](../README.md) and the portable
skill's [workflow reference](../integrations/agent-skills/sunlight/references/workflow.md).
`repository_init` and `repository_status` expose the effective policy and return
exact recovery steps when human-owned `.sunignore` changes require re-ingest or
preservation.

Tool-specific output schemas describe the main returned IDs and payloads while
allowing forward-compatible additional fields. The initialization response also
describes the core authoring and coordination lifecycle, so a generic MCP client
does not depend on a Codex- or Cursor-specific prompt.

## Confinement and lifecycle

The canonical repository root is fixed at server startup and is the cwd for
every delegated command. Tool schemas accept repository-relative artifact paths
and native object IDs, never caller-selected content files, projection roots,
export repositories, or a generic CLI argv. Patch and whole-file content arrive
as JSON strings and are written with create-new semantics to a process-exclusive
directory below `.sunlight/local/mcp`; each file is removed after its call and
the directory is removed at shutdown.

`execution_run` accepts a bare, non-shell program plus a JSON string array of
arguments and an optional repository-relative cwd. It rejects shell programs,
absolute executable paths, and shell command strings. Runtime filesystem and
network limits remain those reported by Sunlight's existing execution policy;
on Windows, the delegated `sun` process tree is additionally placed in a
kill-on-close Job Object.

Each server serializes its own active calls. Short state-mutating calls,
including `worktree_capture`, from
independent servers additionally wait on the crash-released OS lock
`.sunlight/local/mcp-mutation-queue.lock` for at most ten seconds. The file is a
stable lock identity and is never deleted. Read-only calls, `topic_wait`, and
long external work such as execution or projection materialization do not hold
that repository queue for their whole duration; their short state publications
still use the repository CAS/writer transaction boundary. This prevents a slow
command from blocking unrelated reads or authoring while retaining atomic
publication. Delegated calls use the same built `sun` executable and exact JSON
contracts. There is no recursive `mcp serve` path.

On Windows, automatic inspection projections hardlink from a fully verified,
immutable cache and remain read-only. This avoids a second physical source-tree
copy. Writable compatibility and execution projections continue to use safe
copy-on-write where available and otherwise isolated private copies.

Requests are limited to 4 MiB, content fields to 2 MiB, execution-output source
promotion to a 2 MiB regular file, subprocess stdout to 8 MiB, and stderr to
64 KiB. Ignored, log, cache, and oversized outputs remain local-only;
the denial reports classification and size facts. Ordinary calls time out after
two minutes and runs after fifteen minutes. Malformed messages produce a
JSON-RPC error without stopping the server where recovery is possible.
Cancellation, stdin EOF, and server shutdown terminate the active contained
process tree, discard queued calls, and remove staged files.

This slice has no network listener, background service manager, subscriptions,
or dashboard. The client owns the stdio server process lifetime.

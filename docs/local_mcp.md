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
`.agents/skills/sunlight`; Codex also manages `.codex/config.toml`, and Cursor
also manages `.cursor/mcp.json`. Existing unrelated client configuration is
preserved. Verify the result with:

```powershell
sun agent doctor --client codex
```

Restart or reload the client after its MCP configuration changes. The generated
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
- `execution_run`, `execution_promote_output`
- `checkpoint_create`
- `policy_check_export`, `policy_check_commit`, `policy_explain`
- `git_export`, `inspect`

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

Sunlight does not detect or hide secret-like content. Repository ingest follows
normal Git semantics: tracked files are included, Git-ignored untracked files
are excluded, and repository-root `.sunignore` patterns explicitly exclude
additional tracked or untracked paths. `.git/` and `.sunlight/` are intrinsic
exclusions. Secret prevention belongs to repository hygiene, permissions,
deployment tooling, and dedicated scanners outside Sunlight. Status reports
`automatic_secret_detection: false` and the effective ignore-policy contract.
`.sunignore` itself remains visible and is human-owned worktree policy. Sunlight
mutation and compatibility-import paths cannot change it. After a human changes
it, call `repository_init`: clean state is re-ingested, while authored history
fails closed with instructions that preserve the existing state.

An old native state containing legacy automatic-quarantine entries can be
migrated by calling `repository_init` again when it has no authored Sunlight
history. If history exists, initialization fails with preservation instructions
instead of silently rewriting prior views.

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

Each server serializes its own active calls. Short state-mutating calls from
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

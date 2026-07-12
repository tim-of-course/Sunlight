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

## Codex configuration

Add a server entry to `%USERPROFILE%\.codex\config.toml`, using absolute paths:

```toml
[mcp_servers.sunlight_my_repo]
command = 'C:\src\sunlight\target\debug\sun.exe'
args = ['mcp', 'serve', '--repo', 'C:\src\my-repo']
startup_timeout_sec = 15
tool_timeout_sec = 900
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
- `topic_create`, `session_start`, `session_refresh`
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
`concurrent_state_update` are returned as tool errors without translation.

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

Calls are serialized. This preserves publication transactions and writer CAS,
and prevents reads from racing an in-process publication. Delegated calls use
the same built `sun` executable and exact JSON contracts; this is a transitional
transport boundary until the production engine is split into a shared library.
There is no recursive `mcp serve` path.

Requests are limited to 4 MiB, content fields to 2 MiB, subprocess stdout to
8 MiB, and stderr to 64 KiB. Ordinary calls time out after two minutes and runs
after fifteen minutes. Malformed messages produce a JSON-RPC error without
stopping the server where recovery is possible. Cancellation, stdin EOF, and
server shutdown terminate the active contained process tree, discard queued
calls, and remove staged files.

This slice has no network listener, background service manager, subscriptions,
or dashboard. The client owns the stdio server process lifetime.

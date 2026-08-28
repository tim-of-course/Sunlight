# Install and troubleshoot Sunlight agent access

The `sun` executable owns setup. Every MCP server is bound to one canonical
repository root for its entire lifetime.

## Install

From the target repository:

```text
sun agent install --client generic
sun agent doctor --client generic
```

`generic` installs and checks the portable skill only. It does not create or
verify a client transport. Configure the generic client separately to launch:

```text
sun mcp serve --repo <repository-directory>
```

Or select a supported adapter:

```text
sun agent install --client codex
sun agent install --client cursor
```

All clients receive the same portable skill at
`.agents/skills/sunlight/SKILL.md`. Codex additionally receives a managed entry
in `.codex/config.toml`. Cursor additionally receives a `sunlight` stdio server
entry in `.cursor/mcp.json`. Existing unrelated configuration is preserved.

The generated client configuration contains local absolute paths. Treat it as
machine-local unless your team intentionally replaces those paths with a
portable installation convention. The portable skill itself is safe to share.

Restart the client after the first MCP installation or after changing the
server executable. Setup is complete when that client exposes
`repository_status` and the artifact tools for the intended repository.

## Codex plugin

The repository also contains a thin Codex plugin marketplace under
`integrations/codex`. The plugin supplies skill discovery only; repository MCP
binding still comes from `sun agent install --client codex`.

## Cursor

Cursor reads project MCP servers from `.cursor/mcp.json` and discovers the
portable Agent Skill from `.agents/skills/sunlight`. Restart or reload Cursor
after installation, then confirm the `sunlight` server is enabled in its MCP
tools UI.

## Doctor results

`sun agent doctor` always checks that:

- the portable skill and references match the running Sunlight build;
- the repository is initialized, or is ready for the MCP agent to initialize.

For Codex and Cursor it also checks that the managed repository-bound MCP entry
matches the running executable and repository. Generic doctor reports
`mcp_binding_verified: false`: it did not inspect client configuration, start a
server, or verify live tool visibility.

If doctor reports stale files, rerun install with `--force`. This replaces only
Sunlight-managed files or entries and preserves unrelated client configuration.

If the tools remain unavailable after a successful doctor result, restart the
client and inspect its MCP diagnostics. Do not paste TOML or JSON configuration
lines into a shell prompt; they belong in the generated client file.

If a human adds, removes, or edits repository-root `.sunignore` after
initialization, normal tools fail with `sunignore_policy_changed` before exposing
persisted views. Call `repository_init` to adopt it. Clean state refreshes
automatically; authored state returns `sunignore_policy_change_blocked` and
remains byte-for-byte unchanged until its preservation steps are followed.

# Sunlight

Sunlight is a local-first source artifact system for coding agents. Agents author
topic-owned operations against exact repository views, compose exact revisions,
run validation, and freeze checkpoints without creating a Git worktree or full
checkout for every authoring agent.

The open alpha supports **Windows only**. The current product scope is a local,
single-repository workflow on Windows/NTFS. macOS and Linux builds are not
supported alpha targets; their compilation or best-effort behavior is not a
claim of enforced isolation or product support.

## Build

```powershell
cargo build --release --workspace
```

The executable is `target\release\sun.exe` on Windows and
`target/release/sun` elsewhere.

## Set up a repository for coding agents

If `sun` is not on `PATH`, substitute the absolute path to the built executable
in the commands below.

Run the setup command from the target repository, or supply `--repo`:

```powershell
sun agent install --client generic --repo C:\src\my-repo
sun agent doctor --client generic --repo C:\src\my-repo
```

`generic` installs the portable Agent Skills version under
`.agents/skills/sunlight`. Use a client adapter to install that same skill plus
repository-bound MCP configuration:

```powershell
sun agent install --client codex --repo C:\src\my-repo
sun agent install --client cursor --repo C:\src\my-repo
```

Restart the coding client after adding MCP configuration. The bound server can
initialize an existing uninitialized repository through `repository_init`; it
cannot switch to a different repository root.

Use `--force` only to replace an older Sunlight-managed skill or MCP entry.
Existing unrelated client configuration is preserved.

## Agent workflow

The harness-neutral skill source is
[`integrations/agent-skills/sunlight/SKILL.md`](integrations/agent-skills/sunlight/SKILL.md).
Its detailed references cover:

- [native authoring and coordination](integrations/agent-skills/sunlight/references/workflow.md)
- [installation and troubleshooting](integrations/agent-skills/sunlight/references/setup.md)

The MCP server is independently self-describing. A connected client receives
the normal lifecycle in the initialization response and precise contracts from
the tool schemas.

## Client adapters

- Portable Agent Skill: `integrations/agent-skills/sunlight`
- Codex marketplace and thin plugin: `integrations/codex`
- Cursor: installed project-locally by `sun agent install --client cursor`

Client adapters contain discovery and configuration only. Durable Sunlight
concepts and operating practices remain in the portable skill.

To install the optional Codex discovery adapter from this checkout:

```powershell
codex plugin marketplace add C:\src\sunlight\integrations\codex
codex plugin add sunlight@sunlight-local
```

Then run `sun agent install --client codex` in each target repository to create
that repository's confined MCP binding, and start a new Codex task.

## Verify the implementation

```powershell
cargo fmt --all -- --check
cargo test --workspace
scripts\validation-smoke.ps1
scripts\mvp-smoke.ps1
```

See [the local MCP reference](docs/local_mcp.md) and the
[v0.3 readiness audit](docs/sunlight_mvp_readiness_audit_20260716.md) for the
implemented lifecycle and current evidence. The
[open-alpha acceptance gate](docs/open_alpha_acceptance.md) defines the
remaining release tests and decision criteria.

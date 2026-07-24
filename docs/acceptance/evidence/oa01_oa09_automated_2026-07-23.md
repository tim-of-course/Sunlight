# OA-01 / OA-09 automated acceptance evidence

Date: 2026-07-23 (America/Chicago)  
Result: **OA-01 partial; OA-09 partial**  
Sunlight: `sun 0.1.0`, source commit `7a7ed0dbaa0ac41d06248ea0bde2c40284faf098` plus the tested uncommitted worktree  
Final release build: SHA-256 `d69c6cb6ddd6e75f76491ab040a6fb3ec723249831635a610157e58cb1de10b8`  
Host: Windows 10.0.26200, NTFS, Rust/Cargo 1.95.0  
Harness: local Cargo tests and the release CLI; no network, remote, commit, or push.

This record intentionally separates automated setup/document-contract proof from the fresh-client and unfamiliar-tester evidence required to pass OA-01 and OA-09. It is not a substitute for those acceptance runs.

## OA-01 automated cases

`cargo test -p sun agent_setup::tests -- --nocapture` passed **5/5**:

- generic installation writes the portable `.agents/skills/sunlight` package and is idempotent;
- Codex installation synchronizes the repository-local adapter with the portable skill;
- Codex installation updates only its managed MCP block;
- Cursor installation preserves unrelated MCP servers;
- Windows extended path prefixes are not leaked into generated client configuration.

`cargo test -p sun --test cli_json agent_install_and_doctor_create_a_discoverable_cursor_setup -- --nocapture` passed **1/1**. The test uses a new disposable repository, runs the public `sun agent install --client cursor` and `sun agent doctor --client cursor` commands, verifies the exact portable skill files and `.cursor/mcp.json` binding, confirms the server arguments are `mcp serve --repo <canonical-root>`, verifies the healthy doctor contract, then changes the executable identity and confirms doctor returns structured `agent_setup_incomplete` rather than a false healthy result.

The unit coverage also proves that unrelated Codex TOML and Cursor JSON configuration survives managed installation. The installed setup reference tells the user when a client restart/reload is required and explicitly warns that TOML/JSON lines are configuration, not shell commands.

Still required for OA-01 pass:

- three consecutive fresh Codex client runs from untouched client contexts, each given only a natural engineering task that says to use Sunlight;
- one actual fresh Cursor run after reload with the same no-coaching constraint;
- retained prompts and object/test evidence from those four real client runs.

## OA-09 automated cases

`cargo test -p sun --test cli_json global_and_primary_help_describe_repo_backed_operator_workflow -- --nocapture` passed **1/1**. Direct release-binary inspection also produced successful help for:

```text
sun agent install --help
sun agent doctor --help
sun mcp serve --help
```

The help contract names the same `generic|codex|cursor` adapters, repository binding, portable skill, managed MCP configuration, configuration-preservation behavior, and stdio server command used by the README and installed setup reference.

`cargo test -p sun mcp::tests::local_mcp_documentation_names_every_advertised_tool -- --nocapture` passed **1/1**. Every advertised public MCP tool appears in `docs/local_mcp.md`. Additional MCP unit assertions require every tool schema to expose its typed input, result identities where applicable, transport timing, and structured errors with `code`, `message`, `details`, and a concrete `next_action`. The portable skill and workflow reference describe the normal lifecycle, exact-ID/CAS preconditions, topic/session ownership, `topic_wait`, resolution, validation, checkpointing, Git handoff, and the required stop-and-diagnose behavior when tools are absent.

Still required for OA-09 pass:

- a fresh tester context that did not participate in Sunlight development;
- installation from only the built artifact and public repository documentation;
- one real coding change, one injected recoverable error, exact validation, and the documented handoff result without maintainer help;
- retained exact prompt, client/model identity, Sunlight objects, test output, recovery action, and final safety state.

## Safety and classification

All automated fixtures were disposable and local. The tests did not configure a remote, create a source-repository commit, or push. No production application repository was edited. OA-01 and OA-09 remain **partial** because their defining evidence is behavioral discovery and recovery by genuinely fresh clients/testers, not merely generated-file or schema correctness.

Final-source revalidation: the complete unsandboxed `cargo test -p sun` run passed unit **29/29**, CLI **231/231**, engine **2/2**, MCP **7/7**, OA-05 handoff **1/1**, and self-hosting **1/1**. The setup, doctor, help, schema, and documentation-contract cases above are included in those totals.

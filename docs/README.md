# Sunlight documentation authority

Current operational behavior is defined by the runtime implementation and its
public MCP schemas. These documents explain that behavior:

- the repository [README](../README.md);
- [local MCP reference](local_mcp.md);
- the harness-neutral [Sunlight Agent Skill](../integrations/agent-skills/sunlight/SKILL.md).

[Open-alpha acceptance](open_alpha_acceptance.md) and its retained evidence are
release-test records. They demonstrate tested behavior but are not operational
instructions or an independent contract.

Files whose names end in versioned planning labels such as `_v0_1` or
`_v0_3`, including the architecture DOCX, are historical design records. They
remain useful provenance but are not normative when they conflict with current
runtime behavior. In particular, all automatic secret detection,
secret classification gates, and automatic source quarantine proposals are
superseded. Sunlight treats eligible repository content as source; normal Git
ignore semantics and human-owned repository-root `.sunignore` define source
exclusion.

# OA-10 source completeness and explicit exclusion

Date: 2026-07-24 (America/Chicago)  
Result: **Pass — Windows open-alpha acceptance restored**  
Scope: Windows NT 10.0.26200.0, local single-repository workflows  
Source base: `1f8d9871c35c644b14d95cce68d6d0d190fc00e4` plus the reviewed uncommitted remediation  
Release executable SHA-256: `2dfa361b3e8cc23718dad13e431013cdcb0091f8ebf3f7198bb37b977caa7a32`

## Decision and contract

The earlier Windows open-alpha approval was suspended when automatic secret
detection quarantined legitimate authentication, setup, test, and environment
template source. Timothy Cardoza established the replacement contract:

- Sunlight does not detect, quarantine, hide, or block content because it
  resembles a secret. Secret handling happens outside Sunlight.
- Normal Git semantics include tracked files even when `.gitignore` matches;
  ignored untracked files are excluded.
- Repository-root `.sunignore` is the only additional human-authored policy
  that excludes otherwise eligible paths, including tracked paths.
- `.sunignore` remains visible and auditable. `.git/` and `.sunlight/` are
  intrinsic exclusions.

## Implemented remediation

- Removed content and filename secret detection from repository ingestion,
  compatibility import, checkpoint validation, export, MCP schemas, and normal
  source operations.
- Made Git discovery authoritative and fail-closed for Git repositories while
  retaining a non-Git fallback.
- Added repository-root `.sunignore` matching, visibility, drift detection,
  human ownership, and native/compatibility/execution mutation guards.
- Added clean-state reinitialization and byte-preserving refusal for authored
  state when `.sunignore` or legacy automatic-quarantine state requires
  migration.
- Removed compatibility filename heuristics and made Git-ignore evaluation
  index-aware, so tracked ignore matches remain source.
- Made execution ignore classification index-aware and fail-closed for every
  Git probe error path.
- Made Git export filter stale hidden checkpoint content while preserving
  hidden tracked bytes from the selected parent.
- Updated README, MCP documentation, portable Agent Skill, Codex adapter, and
  historical-document authority notices to describe one harness-agnostic
  contract.

## Independent review

A new read-only Codex task used `gpt-5.6-sol` at high reasoning effort:
`019f94fa-ed7f-71f0-a9f0-fa883fad3d38`.

The first pass identified seven P1 findings: silent Git fallback, missing
ignored `.sunignore`, post-init policy drift, mutable projection policy during
compatibility import, filename heuristics, execution-promotion bypass, and
hidden-parent export overwrite. After remediation, the reviewer found two
follow-on Git-semantics issues: index-blind ignore checks and fail-open
execution probe errors. Both were fixed and covered by adversarial tests.

Final reviewer verdict: no product blocker remains; all seven original findings
and both follow-on issues are closed; Windows open-alpha acceptance may be
restored.

## Adversarial evidence

Coverage includes:

- tracked and untracked secret-like names/content treated as ordinary source;
- tracked `.gitignore` matches kept visible and ignored untracked peers
  excluded;
- malformed Git metadata and corrupted Git index failures handled fail-closed;
- ignored root `.sunignore` kept visible while its patterns apply;
- clean policy refresh and authored-state byte-preserving refusal;
- direct, compatibility, and execution `.sunignore` bypass attempts rejected;
- ordinary `dist` source accepted when Git does not ignore it;
- tracked-but-deleted ignore match recreated through compatibility import;
- forced-tracked ignore match modified by execution and kept promotable;
- ignored execution output excluded from promotion;
- hidden tracked parent bytes preserved during export even when stale hidden
  content is present in the checkpoint;
- clean legacy quarantine migration and authored legacy refusal without state
  changes.

## Validation

- `cargo fmt --all -- --check`: pass
- `cargo check --workspace`: pass
- `cargo test --workspace -- --test-threads=1`: **472 passed, 0 failed**
  - `sun` library: 29
  - CLI integration: 238
  - engine integration: 2
  - MCP integration: 7
  - exact handoff integration: 1
  - self-hosting acceptance: 1
  - `sunlight-core`: 194
- `cargo build --release --workspace`: pass
- `git diff --check`: pass

The Windows ACL-dependent execution tests were run with the permissions needed
to create Sunlight's low-integrity/AppContainer boundaries. The focused tracked
ignore-match, ignored-untracked-output, and corrupted-index fail-closed tests
all passed.

## Repository safety and provenance

No commit, remote update, export, or push was performed. The release hash above
identifies the locally built executable from the reviewed working tree. Two
unrelated user-owned feature-report artifacts were present and excluded from
this remediation and its review:

- `docs/sunlight_open_alpha_feature_report.docx`
- `scripts/build_open_alpha_feature_report.py`

Timothy Cardoza previously approved the Windows-only open alpha and directed
that acceptance be marked complete after this remediation and independent
review. OA-10 passes, and the approval is reinstated for the documented
Windows-only scope.

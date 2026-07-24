# Sunlight Policy Validation Spec v0.1

> **Historical design record.** Secret detection, secret classification gates,
> and automatic source quarantine described here are superseded. See the
> repository README, `docs/local_mcp.md`, portable Agent Skill, and
> `docs/open_alpha_acceptance.md` for the current explicit-ignore contract.

| Field | Value |
| --- | --- |
| Status | Implementation-ready Phase 0/4 validation contract |
| Date | July 3, 2026 |
| Scope | `.sunlight` commit/export policy classes, generated ignore rules, validation checks, unsafe reference detection, and fixture acceptance tests |
| Sources | `docs/sunlight_consolidated_architecture_v0_3.md`, `docs/sunlight_implementation_backlog_v0_1.md`, `docs/sunlight_schema_contracts_v0_1.md`, `docs/sunlight_native_io_phase1_spec_v0_1.md` |

## Purpose

This spec defines the first `.sunlight` policy validator. It is intentionally practical: implement the policy classes, generate the default ignore fragment, walk candidate Git commits/exports, reject unsafe records, and prove the behavior with small fixtures.

The validator is used in two places:

- Before committing `.sunlight` records through normal Git transport.
- Before exporting a checkpoint to ordinary Git history.

The validator must produce structured JSON that a CLI, manager, or agent can act on without reading prose logs.

## Policy Classes

Every persisted Sunlight record or payload has exactly one effective policy class. Record fields may point to payloads with a stricter class; the stricter referenced class controls export reachability.

| Class | Meaning | Default action |
| --- | --- | --- |
| `commit_default` | Sanitized metadata intended for review and Git transport after validation. | May be committed/exported when all checks pass. |
| `policy_gated` | Payloads or metadata that may be safe in some contexts but require explicit checks. | Block until reachability, size, secret, and privacy checks pass. |
| `local_only` | Machine-local or rebuildable state. | Never commit or export by default. |
| `secret` | Secret bytes or secret-derived payloads. | Never commit/export raw bytes; allow only typed external vault references. |

Effective class resolution:

1. Start with the record type default from `sunlight_schema_contracts_v0_1.md`.
2. Apply explicit record `privacy_class` if present.
3. Apply path policy overrides from `.sunlight/config.toml`.
4. Promote to the strictest class found in referenced payloads, evidence refs, execution outputs, or provenance refs.

## Generated Ignore Expectations

`sun init` must create or update a generated `.gitignore` fragment that keeps raw local state out of Git while leaving room for explicitly policy-approved records.

Required ignored paths:

```text
.sunlight/local/
.sunlight/cache/
.sunlight/projections/
.sunlight/tmp/
.sunlight/quarantine/
.sunlight/index.sqlite
.sunlight/executions/**/sandbox/
.sunlight/executions/**/raw-logs/
```

Required generated comments:

```text
# Sunlight local/cache state. Do not commit directly.
# Object payloads are policy-validated before commit or export.
```

Implementation rules:

- Preserve unrelated user `.gitignore` content.
- Replace only the managed Sunlight block between stable begin/end markers.
- Treat missing required ignores as a policy error, not as a reason to silently proceed.
- Do not globally ignore `.sunlight/objects/`; exact object reachability is validator-owned.

Suggested markers:

```text
# BEGIN SUNLIGHT MANAGED IGNORE
# END SUNLIGHT MANAGED IGNORE
```

## Commit Candidate Set

For Git transport validation, the candidate set is every staged or requested path under `.sunlight` plus any generated ignore file being changed.

Allowed by default after validation:

- `.sunlight/config.toml`
- `.sunlight/records/**`
- `.sunlight/topics/**` sanitized metadata
- `.sunlight/views/**` resolved view and view-spec manifests
- `.sunlight/checkpoints/**` checkpoint manifests and retention metadata
- `.sunlight/conflicts/**` summaries without candidate bytes
- `.sunlight/export-map/**`

Blocked by default:

- `.sunlight/local/**`
- `.sunlight/cache/**`
- `.sunlight/projections/**`
- `.sunlight/tmp/**`
- `.sunlight/quarantine/**`
- `.sunlight/index.sqlite`
- `.sunlight/executions/**/sandbox/**`
- `.sunlight/executions/**/raw-logs/**`
- daemon sockets, lock files, temporary journals, and machine identity files

Policy-gated:

- `.sunlight/objects/**`
- `.sunlight/operations/**` payload-bearing operation transactions
- `.sunlight/executions/**` summaries, reports, output manifests, and selected evidence
- generated files, lockfiles, binaries, vendored payloads, and raw agent provenance
- private topic metadata or any record whose effective privacy class is not public

## Export Candidate Set

For checkpoint export, the candidate set includes:

- The checkpoint record and resolved view being exported.
- The exact topic frontier and dependency closure.
- Operation transactions reachable from that frontier.
- Content blobs and trees needed to materialize the checkpoint tree.
- Selected evidence refs included in the checkpoint.
- The export-map record that will map native checkpoint IDs to Git refs.

The export validator must reject moving selectors. All checkpoint, topic revision, resolved view, content, evidence, and export-map refs must be exact immutable IDs before validation starts.

## Validation Checks

Run these checks for both Git commit validation and checkpoint export unless a row says otherwise.

| Check | Required behavior |
| --- | --- |
| `ignore_policy` | Verify the generated ignore block exists and includes all required local/cache exclusions. |
| `path_scope` | Reject paths outside the repository or outside the candidate export tree. Reject path traversal, absolute paths, and platform-invalid names. |
| `schema` | Parse every candidate record; require known `record_type`, supported `schema_version`, repository scope, and `privacy_class`. |
| `policy_class` | Compute effective policy class and block `local_only` or `secret` raw payloads. |
| `reachability` | Starting from the commit/export roots, walk referenced objects and ensure every referenced ID is present, allowed, or explicitly omitted by policy. |
| `unsafe_reference` | Reject public manifests that point to blocked local paths, raw logs, sandboxes, quarantine, machine identity, private topics, or secret payloads. |
| `size_budget` | Reject files or reachable payload sets over configured limits unless an explicit bundle/publish policy is selected. |
| `secret_scan` | Scan candidate text bytes, metadata fields, paths, and small binary headers with the configured detector set. |
| `derived_secret` | Reject records marked as derived from secret/private inputs unless they are represented only by a safe summary or vault reference. |
| `execution_raw_exclusion` | Ensure raw execution logs, sandboxes, caches, coverage directories, and unpromoted tool outputs are not included. |
| `generated_policy` | Require generated outputs and lockfiles to be classified and either promoted into operations or excluded from export. |
| `export_tree` | For checkpoint export only, verify materialized Git files come from the checkpoint tree, not from the mutable Git working tree. |
| `report_integrity` | Emit a validation report ID and stable JSON report; export-map records must reference the report ID. |

Fixture export validation exposes a narrow generated-output gate for `basic-app`:
exporting `checkpoint_auth_profile_ready_0001` to
`refs/heads/sunlight/unpromoted-generated-output` synthesizes a generated
source output at `src/generated/auth.generated.ts` with no promotion
provenance. The validator must fail before any Git commit, ref update, or
export-map write with check `generated_policy` and code
`generated_output_requires_promotion`. This fixture is only a visible policy
failure path; it is not a persistent execution store, provenance scanner, or
filesystem diff inference mechanism.

Validator-layer failure responses use the standard envelope:

```json
{
  "ok": false,
  "error": {
    "code": "policy_validation_failed",
    "message": "candidate contains blocked Sunlight paths",
    "details": {
      "failures": [
        {
          "check": "execution_raw_exclusion",
          "path": ".sunlight/executions/exec_1/raw-logs/stdout.log",
          "reason": "raw execution logs are local_only"
        }
      ]
    }
  }
}
```

## Operator Failure Guidance

Policy validation failures are hard gates, not warnings. A failed
`sun policy check-commit --json` surfaces the CLI error code
`commit_policy_failed` and must stop Git transport until the native records or
candidate path set are fixed. A failed
`sun policy check-export --checkpoint <checkpoint-id> --fixture basic-app --json`
surfaces `export_policy_failed`; `sun git export` uses the same CLI-facing
error code when export validation blocks the operation. The lower-level
validator report may still identify the abstract validator-layer failure as
`policy_validation_failed`. Operators must not repair the failure by editing
generated Git commits, export-map output, or projected Git files directly.

For any failure, inspect the JSON fields before changing records:

- `error.code` identifies the CLI blocking class, normally
  `commit_policy_failed` for commit checks or `export_policy_failed` for export
  checks. Nested validator report context may use `policy_validation_failed`.
- `error.details.failures[].check` names the failed gate, such as
  `ignore_policy`, `unsafe_reference`, `execution_raw_exclusion`,
  `generated_policy`, `reachability`, or `size_budget`.
- `error.details.failures[].path` identifies a blocked candidate path when the
  problem is path based.
- `error.details.failures[].record_id`, `checkpoint_id`, `resolved_view_id`,
  `evidence_ref`, `export_target`, or `git_ref` identify the native object or
  export target context when the problem is record based.
- `error.details.failures[].reason` is the operator-facing explanation; it is
  advisory text, while `check`, IDs, and paths are the stable repair handles.

For `policy.check-commit` failures, first inspect the staged/requested
`.sunlight` paths and the generated managed ignore block. The common repair is
to rerun `sun init` or the relevant native command so the
`# BEGIN SUNLIGHT MANAGED IGNORE` block again excludes `.sunlight/local/`,
`.sunlight/cache/`, `.sunlight/projections/`, `.sunlight/quarantine/`, raw
execution logs, and sandboxes. If a staged record points at those roots, a
projection path, a cache path, a quarantine path, a local filesystem URI, or a
raw execution path, fix the native record or rerun promotion/import so the
record references an allowed summary, promoted operation output, exact object
ID, or typed external reference instead.

Safe operator actions for commit validation failures are limited to removing
blocked local/cache/quarantine/projection paths from the candidate set,
restoring the managed ignore block, promoting generated outputs into
topic-owned operation transactions, importing source bytes through native IO,
or rewriting the affected native record through Sunlight commands. Do not mark
raw logs, sandboxes, caches, local leases, machine identity, or quarantine
records as `commit_default` just to pass validation.

For `policy.check-export` failures, inspect the checkpoint, resolved view,
selected evidence, export-map draft, and target Git ref named in the report.
Generated source, lockfiles, migrations, and formatter output selected into the
exported project tree must have promotion provenance. Local-only evidence must
stay represented as bounded summaries, digests, omission reasons, or approved
external references. Moving refs such as `topic@head`, `main`, `latest`, or an
unpinned Git target context must be replaced by exact checkpoint, topic
revision, content, evidence, export-map, or allowed full Git ref identities
before export validation runs again.

Safe operator actions for export validation failures are to rerun checkpoint
planning from an exact conflict-free resolved view, rerun promotion for
generated outputs, import missing content into native records, select a
different evidence policy, choose a valid non-moving export target, or rerun
`sun policy check-export --checkpoint <checkpoint-id> --fixture basic-app --json`
after the native inputs are corrected. Do not edit the Git export output
directly; exported Git history is a projection of the validated checkpoint and
native records remain authoritative.

## Unsafe Reference Detection

The validator must inspect references inside records, not just candidate filenames.

Reject references to:

- Any path under required ignored roots.
- `file://`, absolute filesystem paths, home-directory paths, or temporary paths in public manifests.
- Raw execution log IDs, sandbox IDs, projection cache IDs, daemon state IDs, local lease IDs, machine identity IDs, or quarantine IDs.
- Secret payload IDs or content blobs whose classification is `secret`.
- Private topics, private actor metadata, or raw agent provenance when export policy is public.
- Moving selectors such as `topic@head`, `main`, `latest`, or unpinned view specs in checkpoint/export records.

Allow references when:

- The target is an exact immutable ID and has an allowed effective policy class.
- The record carries only a redacted summary with a digest, byte count, classification, and omission reason.
- The target is a typed external reference, such as a vault URI, with no raw secret bytes.

## Size And Secret Gates

Default size limits should be conservative and configurable in `.sunlight/config.toml`.

Suggested MVP defaults:

| Limit | Default |
| --- | --- |
| Single `commit_default` record | 256 KiB |
| Single policy-gated text payload for Git transport | 1 MiB |
| Total `.sunlight` metadata added in one Git commit | 5 MiB |
| Single Git-exported project file | 10 MiB unless already present in base checkpoint |
| Binary payload without explicit allow policy | Block |
| Raw execution log | Always local-only |

Secret scanning MVP requirements:

- Scan UTF-8 text, JSON/TOML field values, paths, and the first/last 64 KiB of unknown binary files.
- Include detector hooks for common token names, private keys, environment files, cloud credentials, and high-entropy strings.
- Treat detector matches as `secret` until explicitly overridden by a local policy decision.
- Store override decisions as local-only by default; exporting overrides requires a separate policy-gated review record.

## Raw Execution, Log, And Cache Exclusions

Executions produce useful evidence, but raw output is not commit-safe by default.

Commit/export-safe execution metadata may include:

- Execution ID, resolved view ID, tree identity, command digest or sanitized command, result, timestamps, tool versions, output manifest digest, and selected promoted output refs.

Local-only execution data includes:

- Raw stdout/stderr logs, sandbox directories, coverage HTML, package-manager caches, downloaded dependencies, temporary service state, environment dumps, and unpromoted generated files.

If a checkpoint selects execution evidence, the record must reference the safe execution summary or a redacted report. It must not reference raw local paths.

## CLI Surface

Minimum validator commands:

```text
sun policy check-commit --json
sun policy check-commit --paths <path>... --json
sun policy check-export --checkpoint <checkpoint-id> --fixture basic-app --json
sun policy explain <validation-report-id> --json
```

Suggested success data:

```json
{
  "command": "policy.check-commit",
  "repository_id": "repo_01JZ0LOCAL",
  "validation_report_id": "validation_sha256_1234",
  "candidate": {
    "kind": "sunlight_commit"
  },
  "summary": {
    "records_checked": 8,
    "payloads_checked": 1,
    "warnings": 0,
    "blocked": 0
  }
}
```

```json
{
  "command": "policy.check-export",
  "repository_id": "repo_01JZ0LOCAL",
  "validation_report_id": "validation_sha256_abcd",
  "candidate": {
    "kind": "checkpoint_export",
    "checkpoint_id": "checkpoint_101"
  },
  "summary": {
    "records_checked": 42,
    "payloads_checked": 7,
    "warnings": 0,
    "blocked": 0
  }
}
```

## Fixture Acceptance Tests

Use small fixture repositories and JSON snapshots. Keep fixtures readable and deterministic; do not use real secrets.

| Fixture | Steps | Required assertions |
| --- | --- | --- |
| `generated_ignore_block` | Run `sun init`, then `sun policy check-commit --json`. | Managed ignore block exists, required paths are ignored, unrelated `.gitignore` lines remain. |
| `commit_default_metadata_passes` | Stage config, topic metadata, resolved view, checkpoint manifest, and export-map records. | Validation succeeds and report lists only `commit_default` records. |
| `local_cache_rejected` | Stage `.sunlight/cache/blob.tmp` or `.sunlight/local/lease.json`. | Validation fails with `policy_class` or `ignore_policy`; path is reported. |
| `raw_execution_log_rejected` | Stage `.sunlight/executions/exec_1/raw-logs/stdout.log`. | Validation fails with `execution_raw_exclusion`. |
| `sandbox_reference_rejected` | Commit a checkpoint manifest that references `.sunlight/executions/exec_1/sandbox/`. | Validation fails with `unsafe_reference` even if the sandbox path itself is not staged. |
| `policy_gated_payload_requires_allow` | Stage an operation transaction with embedded patch payload over the default gated limit. | Validation fails with `size_budget` and effective class `policy_gated`. |
| `secret_blob_quarantined` | Candidate record references a synthetic private-key-shaped payload. | Validation fails with `secret_scan`; raw bytes are classified `secret` and not exportable. |
| `vault_reference_allowed` | Candidate record references `vault://team/app/token` with digest and no secret bytes. | Validation succeeds or remains policy-gated according to config; no raw secret bytes are present. |
| `moving_selector_rejected_for_export` | Checkpoint/export record uses `topic@head` or `main` instead of exact IDs. | Export validation fails with `unsafe_reference`. |
| `export_reachability_missing_object` | Checkpoint references a content blob absent from candidate store. | Export validation fails with `reachability`. |
| `export_uses_checkpoint_tree` | Working tree has an extra untracked file not in checkpoint; run export validation. | Validation succeeds only if export tree excludes the working-tree-only file. |
| `generated_output_requires_promotion` | Execution produced a generated source file but no promotion operation exists. | Export validation fails with `generated_policy` when the file is selected for project export. |
| `large_binary_blocked` | Candidate export includes a new binary above default limit. | Validation fails with `size_budget` unless explicit allow policy is present. |
| `report_referenced_by_export_map` | Successful export validation writes report and export-map draft. | Export-map references `validation_report_id`, checkpoint ID, tree identity, and target Git ref. |

## Implementation Boundaries

- Do not build native sync, encrypted bundles, or hosted policy workflows in this slice.
- Do not write implementation code from this spec slice.
- Do not treat Git's staged state as authoritative for Sunlight semantics; it is only the candidate set for transport validation.
- Do not allow validation warnings to downgrade hard failures for `secret`, `local_only`, unsafe references, missing reachability, or raw execution/log/cache inclusion.

# Audit fixes and TasGrid evaluation, 2026-09-06

## Implementation

- Git initialization records `imported_base_git_commit_id` in canonical native
  state. Export uses that recorded commit or an existing mapped export lineage,
  never the current Git HEAD as a substitute for the original base.
- Failed Git ref updates return their actual failure, created commit, and
  publication facts without recording a successful native handoff.
- A checkpoint's existing handoff is checked before Git writes. Repeating the
  same handoff is idempotent; a second destination is rejected without creating
  a commit or branch. An externally moved handoff branch is not overwritten.
- Git ignore evaluation writes stdin and drains stdout/stderr concurrently.
  Its deadline and cancellation cover the input feed as well as process exit.

Old native repositories without a recorded Git base remain readable. First
export from such a repository fails instead of guessing. The disposable TasGrid
repository was reinitialized for this evaluation; its prior `.sunlight` was
preserved at `/Volumes/OS/Development/Sunlight Test repos/TasGrid-sunlight-before-audit-20260906-073114`.
Existing source edits were retained in the new ingested snapshot.

## Automated verification

- `cargo fmt --check` and `git diff --check`: passed.
- `cargo test --workspace`: 507 tests passed.
- Validation, projection-strategy, and MVP smoke scripts: passed.
- Release build: passed.

New regression coverage verifies recorded ancestry after HEAD advancement,
preservation of newer human work when merging the export, missing-base failure,
locked-ref failure and retry, idempotent existing handoffs, rejection of a
second destination before any Git object/ref write, externally moved branch
preservation, and classification of 3,000 ignored files without a pipe deadlock.
An existing handoff test now expects the earlier `export_map_conflict` when a
previously exported checkpoint is requested on a second branch.

## Fresh-thread evaluation setup

Three independent Banana Split root threads started in the same TasGrid
workspace, using the configured `gpt-5.6-sol` / high reasoning preset. They did
not inherit the audit conversation. No Sunlight workflow instructions were
included in task prompts. Only the diagnostic task named `execution_run`.

The installed project skill and project-bound MCP configuration were left in
place. The MCP server executable was rebuilt from the fixes. All three threads
discovered the installed skill, read its workflow reference, and used
`repository_status` and `worktree_diff` before authoring. They left the unrelated
external marker outside their task changes.

Controlled conditions:

- `.audit-output/` was added to the disposable repository's Git ignore rules.
- Recorded initialization Git base: `2b633ddec715c46ed9e0028808ad551607f3e899`.
- A separate Git commit added `audit-external-marker.txt` after initialization.
- The upload task's destination ref was held with a lock file to exercise the
  failed-ref-update path.
- Host approvals were relayed for the user-authorized local test work. These
  approvals contain no instructions about how to operate Sunlight.

## Tasks and thread identities

| Task | Workflow | Root agent |
| --- | --- | --- |
| Handle invalid upload byte counts | `wf_4284f165-e692-4276-acfe-01c463eec2be` | `agt_f0e44f13-be76-4874-a01b-bb156ef5f120` |
| Normalize catalog scope whitespace and case | `wf_52aef01b-5345-4660-875f-d91c81f7cc4e` | `agt_21d61ed1-bc93-4528-9cba-76ab0100e066` |
| Generate and classify 3,000 ignored files | `wf_3539cc8f-7037-40d9-b82b-eb5babeff368` | `agt_50f925ac-1ca5-40f7-957b-08979c7d6946` |
| Repeat an existing handoff, then request a second destination | `wf_7d1ea2d9-7024-4d7c-b894-6bca689f0722` | `agt_6adf1d45-5658-4e71-b1b4-091e34380264` |

The fourth task received only the exact existing checkpoint, the two requested
branch names, and the requirement to preserve source and checkpoint content.
It received no Sunlight operating instructions.

## Observed results

### Diagnostic output

`exec_native_0002` passed with exactly 3,000 outputs, all `ignored`, zero
source-like deltas, and zero promotion candidates. Independent filesystem
inspection confirmed 3,000 files, minimum filename length 105, and 13,890 total
bytes in the retained execution projection. The command took 297 ms and output
classification took 816 ms. End-to-end execution was 38,629 ms, including
32,227 ms cold runtime preparation and 4,137 ms private binding.

The first offline attempt returned `runtime_layer_network_unavailable` before
command start. The agent used the returned recovery guidance to prepare the
layer on retry, without host coaching. Other offline executions waited for and
reused the same layer.

Diagnostic checkpoint: `checkpoint_408d5efc5b8baf23_6dec7f1d7bea`.

### Catalog feature and concurrent integration

The catalog agent modified the canonical and shipped copies through one atomic
batch patch. Focused tests passed (10 tests, 45 expectations), and the combined
view's TypeScript, worker typecheck, and Vite production build passed.

The agent encountered canonical advancement, resolved onto the new checkpoint,
and reran validation without replacement topics or host workflow guidance.
The final canonical checkpoint contains all three task topics:
`checkpoint_f8d85943817f28f8_74c4302e6379`.

Successful local export:

- Ref: `refs/heads/audit/catalog-scope-20260906`
- Commit: `9b95d0bfa867d670f98d4e0f3fb37dad4f1a40cb`
- Map: `export_map_checkpoint_f8d85943817f28f8_74c4302e6379`

Independent Git inspection confirmed that the parent is the recorded
initialization base, source includes the requested normalization, and the
external marker remains outside the export and intact in the human worktree.

### Held-ref failure

The upload agent's first export returned `export_ref_update_failed` with
`ref_updated: false` and `export_map_written: false`. The created commit was
`072d9242c03796d7aaca630da1e54cf45fa75e79`, with the recorded initialization base
as its parent. No successful map or branch was created by that failed attempt.
The agent reported the handoff as incomplete, preserved its checkpoint, and
inspected lock ownership. The host then removed its own test lock and supplied
only that changed environmental fact so the original handoff could be retried.

Upload checkpoint: `checkpoint_b776c406d80f597f_61981c01c777`.

The retry succeeded with the same created commit and checkpoint. The final ref
is `refs/heads/audit/upload-bytes-20260906`, and the durable map is
`export_map_checkpoint_b776c406d80f597f_61981c01c777`. Independent inspection
confirmed that both successful refs match their native export maps, both have
the recorded initialization commit as parent, and the upload export contains
the requested finite-number check. Its five focused tests and production build
passed. Exporting this older checkpoint did not move the canonical checkpoint
back from the combined three-topic checkpoint.

### Repeated and conflicting handoffs

The fourth fresh thread repeated the catalog handoff successfully, receiving
the same commit and export-map ID. The request to export that exact checkpoint
to `refs/heads/audit/catalog-second-20260906` returned `export_map_conflict`.
Independent Git inspection confirmed the original branch remained unchanged
and the second branch did not exist. No replacement checkpoint was created.

### TasGrid baseline failures

Full `bun run verify` did not pass. Three test failures reproduced on the
unchanged base: one Intl locale-format expectation and two Windows-style
import-path failures on this macOS host. The agents reported these separately
from the passing focused tests and production builds. This evaluation does not
claim a clean TasGrid full-suite result.

## Final assessment and workflow recap

The four audited defects are fixed and covered by automated regression tests.
Fresh-thread evaluation also exercised each failure condition in the running
MCP server. The installed skill and recovery documentation were sufficient for
these tasks: agents discovered the workflow, recovered from an uncached offline
runtime and concurrent checkpoint advancement, and accurately reported an
incomplete Git handoff. No additional documentation blocker was observed in
this sample. Cold runtime preparation remains a noticeable first-run cost.

All four workflows in the identity table reached `completed` with accepted
`success` results and no pending attention. The conflicting export was an
expected refusal within a successful evaluation task. There were four fresh
root agents and zero child agents. Every root used `gpt-5.6-sol` with high
reasoning; no model-routing changes were observed. The host reviewed results
and independently checked Git refs, ancestry, native maps, retained diagnostic
files, and preservation of the external marker.

Native thread IDs, in table order:

- Upload: `01a076b4-66ca-7093-91df-3b370ad59fab`
- Catalog: `01a076b4-693f-74a3-98ed-9e4c2d817f0e`
- Diagnostic: `01a076b4-6bb7-7571-be15-95fbb5d7c01d`
- Repeat export: `01a076c0-e86a-7b42-b82f-a95a9babe0ee`

## Scope

Execution and integration verification ran on macOS/APFS. This is a development
host, not evidence of Windows isolation enforcement. No remote deployment or
Git push was requested or performed.

# Projection Strategy Smoke

The projection strategy smoke verifies the current `sun project materialize`
JSON contract for the `basic-app` fixture. It is deterministic and local-only:
it builds the local `sun` CLI with Cargo, uses fixture data, writes only
temporary command output, and does not require network access or real filesystem
reflink support.

Run on Linux or WSL:

```sh
scripts/projection-strategy-smoke.sh
```

Run on Windows PowerShell:

```powershell
scripts/projection-strategy-smoke.ps1
```

By default the PowerShell wrapper uses WSL when `wsl.exe` is available and the
paired Bash script is LF-clean. Set `SUNLIGHT_SMOKE_USE_WSL=0` to run the native
PowerShell implementation; if the paired Bash script has CRLF line endings, the
wrapper warns and uses the native lane.

The smoke covers:

- exact resolved-view materialization from `resolved_content_tree`
- full-copy correctness fallback and cache-key selection
- portable fixture copy metrics for the local WSL/Linux lane: selected strategy,
  elapsed materialization time, file/directory/byte counts, executable-file
  count, manifest summary counts, and deferred real-filesystem strategies
- explicit reflink strategy selection through fixture capabilities
- local-only projection metadata and local root references with
  `local_only_path` privacy
- current scan-only local root verification, with persisted projection manifest
  content verification reserved for the status/inspect contract
- fallback from an ineligible preferred strategy to copy
- stable JSON failure for an unsupported required strategy with
  `--no-copy-fallback`

The reflink case is a planner/CLI contract check. It intentionally relies on
the fixture capability model instead of probing the host filesystem.

## Current WSL/Linux Metric Slice

On July 5, 2026, the WSL/Linux smoke passed locally and emitted this shareable
fixture metric row:

```text
projection_metric fixture=basic-app view=view_base_0001 purpose=execution selected_strategy=copy elapsed_ms=5 files_written=5 directories_created=4 bytes_written=222 executable_files=1 manifest_entries=5 manifest_files=5 manifest_directories=3 manifest_bytes=222 deferred_strategies=reflink_real_fs,hardlink_readonly_real_fs,overlay_copyup_real_fs scope=fixture_copy_materialization_only
```

Interpretation:

- This is a fixture-only copy materialization measurement, not a benchmark of a
  real repository.
- `elapsed_ms` is useful as a smoke signal only; it is expected to vary by host.
- The composed auth/profile view is still covered by planner JSON assertions,
  while the filesystem copy metric uses `view_base_0001` because the current
  fixture content tree materializer is bound to the base fixture tree.
- Real WSL/Linux reflink, read-only hardlink, and overlay/copy-up probes remain
  deferred. They need host filesystem capability checks, mutation isolation
  checks, command compatibility checks, and store-integrity verification before
  P0.8 can claim a platform strategy result.

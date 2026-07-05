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
- observed WSL/Linux temporary filesystem capability rows for reflink,
  read-only hardlink mutation isolation, and overlay/copy-up availability,
  without committing absolute probe paths
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

## Current WSL/Linux Metric And Capability Slice

On July 5, 2026, the WSL/Linux smoke passed locally and emitted these
shareable fixture metric and host capability rows:

```text
projection_metric fixture=basic-app view=view_base_0001 purpose=execution selected_strategy=copy elapsed_ms=5 files_written=5 directories_created=4 bytes_written=222 executable_files=1 manifest_entries=5 manifest_files=5 manifest_directories=3 manifest_bytes=222 deferred_strategies=reflink_real_fs,hardlink_readonly_real_fs,overlay_copyup_real_fs scope=fixture_copy_materialization_only
projection_fs_capability host_scope=current_wsl_linux_tempdir fs_type=tmpfs probe_root=tempdir absolute_paths=omitted
projection_fs_capability strategy=reflink fs_type=tmpfs reflink_attempt=failed writes_private=unknown accepted=deferred reason=operation_not_supported
projection_fs_capability strategy=hardlink_readonly fs_type=tmpfs hardlink_attempt=ok read_only_write_blocked=yes chmod_write_mutated_store=yes mutation_isolation_risk=present accepted=deferred reason=shared_inode_owner_can_chmod_projection_and_mutate_store
projection_fs_capability strategy=overlay_copyup fs_type=tmpfs overlay_attempt=failed copyup_writes_private=unknown accepted=deferred reason=permission_denied
```

Interpretation:

- This is a fixture-only copy materialization measurement, not a benchmark of a
  real repository.
- `elapsed_ms` is useful as a smoke signal only; it is expected to vary by host.
- Capability rows are scoped to the current WSL/Linux temp directory filesystem
  type observed by the harness. They do not claim portability to other WSL
  installs, mounts, Linux filesystems, or Windows-native paths.
- The composed auth/profile view is still covered by planner JSON assertions,
  while the filesystem copy metric uses `view_base_0001` because the current
  fixture content tree materializer is bound to the base fixture tree.
- Reflink remains deferred on this temp filesystem because
  `cp --reflink=always` reported unsupported operation.
- Read-only hardlink remains deferred even though hardlink creation worked:
  the initial write was blocked by file mode, but the projection owner could
  `chmod` the linked path and mutate the shared store inode.
- Overlay/copy-up remains deferred because neither kernel overlay mounting nor
  the optional `fuse-overlayfs` lane produced an unprivileged copy-up result in
  this harness.
- Full copy remains the only accepted correctness fallback from this smoke
  slice. Any future fast path needs a non-temp host probe, command compatibility
  checks, and store-integrity verification before it can become a default.

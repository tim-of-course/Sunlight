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

The smoke covers:

- exact resolved-view materialization from `resolved_content_tree`
- full-copy correctness fallback and cache-key selection
- explicit reflink strategy selection through fixture capabilities
- local-only projection metadata and local root references with
  `local_only_path` privacy
- fallback from an ineligible preferred strategy to copy
- stable JSON failure for an unsupported required strategy with
  `--no-copy-fallback`

The reflink case is a planner/CLI contract check. It intentionally relies on
the fixture capability model instead of probing the host filesystem.

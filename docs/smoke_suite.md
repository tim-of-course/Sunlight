# Smoke Suite

The smoke suite is a local validation entry point for the current workspace. It
does not assume network access or CI services.

Run on Linux or WSL:

```sh
scripts/smoke-suite.sh
```

Run on Windows PowerShell:

```powershell
scripts/smoke-suite.ps1
```

By default the PowerShell wrapper uses WSL when `wsl.exe` is available, matching
the focused smoke wrappers. Set `SUNLIGHT_SMOKE_USE_WSL=0` to run the suite in
native PowerShell mode.

The suite runs, in order:

- `cargo fmt --check`
- `cargo check`
- `cargo test`
- `scripts/validation-smoke`
- `scripts/projection-strategy-smoke`
- `scripts/mvp-smoke`

`scripts/mvp-smoke` covers the end-to-end fixture path through real local Git
export with `--execute-local`, including projection status and inspect local
root verification. See [mvp_smoke.md](mvp_smoke.md) for details.

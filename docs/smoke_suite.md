# Smoke Suite

The smoke suite is a local validation entry point for the current workspace. It
does not assume network access or CI services.

Use the aggregate suite before handing off a branch, after changes that touch
shared CLI or core behavior, and as the provider-neutral CI handoff command for
this repository. The repo does not currently carry provider-specific workflow
files, so CI systems should wire their own runner to these commands instead of
assuming a GitHub Actions, Azure Pipelines, or other hosted-CI convention.

Run on Linux or WSL:

```sh
scripts/smoke-suite.sh
```

Run on Windows PowerShell:

```powershell
scripts/smoke-suite.ps1
```

By default the PowerShell wrapper uses WSL when `wsl.exe` is available, matching
the focused smoke wrappers, but it falls back to native PowerShell when the
suite's Bash scripts have CRLF line endings. Set `SUNLIGHT_SMOKE_USE_WSL=0` to
run the suite in native PowerShell mode, or set it to a non-zero value to
require the WSL lane.

Platform lanes:

- Linux/WSL: run `scripts/smoke-suite.sh` from the repository root with a local
  Rust toolchain, `cargo`, and `git` available on `PATH`.
- Windows native: run `scripts/smoke-suite.ps1` in PowerShell with Rust and
  `git` available on the Windows `PATH`. If WSL is installed, the wrapper
  delegates to the Linux/WSL lane unless `SUNLIGHT_SMOKE_USE_WSL=0` is set or
  the suite's Bash scripts have CRLF line endings.
- Current WSL manager caveat: when WSL `rustfmt` is unavailable, use the
  Windows-native lane as the formatting gate because the aggregate suite starts
  with `cargo fmt --check`.

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

Use an individual smoke script when iterating on the covered behavior and the
Cargo gates have already passed. For validation-plan changes, the aggregate
suite is the canonical handoff because it includes the validation smoke plus the
format, check, test, projection strategy, and MVP coverage.

Run the production-like self-hosting acceptance journey by itself with:

```powershell
cargo test -p sun --test self_hosting
```

This test uses the built `sun` binary and an isolated local Git clone of this
repository; it does not require network access or fixture data.

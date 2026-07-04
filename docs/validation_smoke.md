# Validation Smoke Script

The validation smoke script exercises the `basic-app` fixture from
`docs/sunlight_validation_repo_plan_v0_1.md` without network access or package
installation. It builds the existing `sun` CLI with Cargo, creates only
temporary files for command inputs, and checks the current JSON command
envelopes used by `crates/sun/tests/cli_json.rs`.

Run on Linux or WSL:

```sh
scripts/validation-smoke.sh
```

Run on Windows PowerShell:

```powershell
scripts/validation-smoke.ps1
```

By default the PowerShell wrapper uses WSL when `wsl.exe` is available so the
same Bash script runs on Windows/WSL. Set `SUNLIGHT_SMOKE_USE_WSL=0` to run the
native PowerShell implementation instead.

The smoke covers init idempotency, fixture read/list/search, fixture patch/write
preconditions, compatible and conflicted view resolution, projection
materialization with the current `sun project materialize` spelling, fixture
`sun run -- cargo test`, checkpoint creation, compatibility import, and Git
export write planning with `--write-plan`.

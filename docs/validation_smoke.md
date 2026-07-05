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

For a broader local validation pass, run `scripts/smoke-suite.sh` or
`scripts/smoke-suite.ps1`. The suite adds Cargo format, check, and test gates
before the focused smoke scripts.

Optional local real-repo coverage for Super Search is available through
`scripts/external-validation-super-search.ps1`. It runs the target repo's
existing `mix test` and `bun run test` baselines, builds `sun`, clones the
target into a temporary directory, verifies `sun init --json` against that temp
clone, exercises fixture compatibility import, and executes fixture-backed local
Git export against the temp clone under `refs/heads/sunlight/*`. This script is
intentionally outside the default smoke suite and CI; do not add it to
`scripts/smoke-suite.ps1` unless the external validation policy changes.

Use this focused script while iterating on the validation fixture contract. Use
the aggregate smoke suite for branch handoff or CI wiring so the validation
smoke runs with the workspace format, check, test, projection strategy, and MVP
coverage. The CI handoff is provider-neutral; this repository does not
currently define a `.github` workflow or another provider-specific CI file.

By default the PowerShell wrapper uses WSL when `wsl.exe` is available so the
same Bash script runs on Windows/WSL. Set `SUNLIGHT_SMOKE_USE_WSL=0` to run the
native PowerShell implementation instead. If the paired Bash script has CRLF
line endings, the wrapper warns and uses the native lane.

The Linux/WSL lane expects a local Rust toolchain, `cargo`, and `git` on
`PATH`. The Windows-native lane expects those tools on the Windows `PATH`; when
WSL `rustfmt` is unavailable, run the aggregate smoke suite in native
PowerShell as the formatting gate because it starts with `cargo fmt --check`.

The smoke covers init idempotency, commit policy validation of the generated
managed `.sunlight/.gitignore` block, fixture read/list/search, fixture
patch/write preconditions, compatible and conflicted view resolution,
projection materialization with the current `sun project materialize` spelling,
fixture `sun run -- cargo test`, checkpoint creation, export policy validation,
policy validation explanation, compatibility project creation, compatibility
diff, compatibility import, and Git export write planning with `--write-plan`.

For focused projection strategy coverage, run
`scripts/projection-strategy-smoke.sh`. It verifies copy fallback, explicit
strategy selection JSON, local-only materialization metadata and root refs, and
unsupported required strategy errors without depending on real reflink support.

# MVP End-to-End Smoke

The MVP smoke is a deterministic local check for the current fixture CLI path.
It requires only the local Rust toolchain and `git`; it does not call network
services.

Run on Linux, macOS, or WSL:

```sh
scripts/mvp-smoke.sh
```

Run in native Windows PowerShell:

```powershell
scripts/mvp-smoke.ps1
```

The smoke builds `sun`, then exercises the `basic-app` fixture end to end:

- resolves the compatible auth/profile view
- materializes a base execution projection into an empty temporary projection
  root and verifies the projected fixture files
- checks projection status and inspect JSON against the materialized local root,
  including local-only path metadata and `verification_state: present`
- plans the compatible execution projection
- records the fixture `cargo test` execution
- creates the export-ready checkpoint
- initializes a temporary local Git repository
- runs `sun git export --execute-local --repo <temp-repo>`
- verifies the exported ref points to the created commit
- verifies the commit parent and exported tree contain the expected fixture files

Expected exported fixture files:

- `src/auth.rs`
- `src/profile.rs`
- `bin/run-auth-check`
- `.sunlight/export-manifest.json`

Expected projected fixture files:

- `README.md`
- `docs/guide.md`
- `scripts/build.sh`
- `src/auth.ts`
- `src/profile.ts`

On platforms with executable mode support, the smoke also verifies
`scripts/build.sh` is executable and `src/auth.ts` is not.

Projection status and inspect smoke coverage verifies the local root exists and
reports file, byte, executable, sample-path, and persisted-manifest verification
summaries. When comparable persisted manifest metadata is unavailable, the JSON
reports `content_verification: not_available_without_persisted_manifest`; valid
persisted envelopes enable verified, dirty, invalid, and root-mismatch outcomes.
The persisted manifest contract and acceptance coverage are defined in
[sunlight_cli_status_inspect_v0_1.md](sunlight_cli_status_inspect_v0_1.md).

The aggregate smoke suite also runs the validation and projection strategy
smokes, which cover artifact validation, compatibility import, write-plan
validation, projection strategy fallback, and failure cases.

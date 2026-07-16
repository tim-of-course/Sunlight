# Sunlight Local v0.3 MVP Readiness Audit - 2026-07-16

## Conclusion

Sunlight is certified as a local, single-repository v0.3 MVP against the acceptance criteria in `docs/sunlight_consolidated_architecture_v0_3.md`. This conclusion is based on repo-backed behavior, not fixture-only demonstrations.

The product loop is complete through the shared CLI/MCP engine: initialize a real repository, author topic-owned operations through native artifact IO, resolve exact views and conflicts, materialize or reuse safe projections, execute a command with persisted evidence, promote approved outputs, freeze a checkpoint, validate policy, and export ordinary Git history.

## Acceptance evidence

| v0.3 criterion | Repo-backed evidence | Result |
| --- | --- | --- |
| Native read/search/patch/write without direct project-directory editing | `no_fixture_real_repo_artifact_io_vertical_slice`; `self_hosting_real_repository_acceptance` | Pass |
| Edits belong to the correct topic without Git staging/rebase/diff | Durable topic/session/operation assertions in the same vertical slice and self-hosting journey | Pass |
| Multiple agents author against different resolved views without a repository copy per authoring agent | Native topic/session state requires no authoring checkout; `no_fixture_topics_and_sessions_are_distinct_repo_backed_records`; verified semantic projection-cache reuse is covered by `no_fixture_repeated_exact_view_reuses_one_durable_projection_cache_entry` | Pass |
| Exact topic revisions compose into an integration view | `no_fixture_repo_backed_resolver_merges_independent_topics_and_reports_conflicts` | Pass |
| Same-file and non-commutative conflicts become inspectable objects | The repo-backed resolver test plus self-hosting conflict/status/inspect assertions | Pass |
| A test command runs against an exact view with environment evidence | `no_fixture_execution_runtime_policy_is_enforced_and_reported`; self-hosting execution; persisted environment-summary CLI, MCP, core round-trip, and tamper tests | Pass |
| Approved formatter/codegen-style outputs promote with execution provenance | `no_fixture_execution_output_promotion_repo_backed_slice` | Pass |
| Checkpoint freezes a view and exports ordinary Git history | `no_fixture_checkpoint_persists_and_exports_validated_execution_evidence`; `no_fixture_checkpoint_export_uses_persisted_snapshot_after_head_moves`; MVP smoke | Pass |
| `.sunlight` commit/export policy is explicit and fail closed | Commit-policy tests, shared persisted export validation, secret/local-only rejection, and self-hosting policy checks | Pass |
| Projection cost and storage amplification are measured | Forced-copy/automatic-strategy tests and projection-strategy smoke report elapsed time, bytes, files, directories, logical amplification, cache status, and truthful strategy | Pass |

The “no full repository copy per agent” criterion applies to native authoring, which is state/API based and does not create a checkout per author. Tool projections are created only when required. The architecture explicitly permits a bounded correctness copy fallback when native COW is unavailable, provided cost and amplification are reported; Sunlight does this and never labels the fallback zero-copy.

## Gap closure in this audit

- Execution records now persist bounded, content-addressed environment summaries: OS, platform hint, architecture, Sunlight build, runner version, bounded executable digests, a digest of allowlisted environment values, and the requested environment/network/filesystem policies. Raw environment values are not serialized. Legacy records remain readable and modified summaries fail digest validation.
- Projection-cache staging cleanup now removes only strictly named, older-than-grace, dead-process staging directories. Scanning and deletion are bounded per materialization; fresh, live, malformed, non-directory, and reparse-point entries are preserved.
- The end-to-end smoke suite exposed a repeat-`sun init` sequence-CAS failure. Initialization now validates and reuses existing native state, handles concurrent initialization safely, and has an integration regression test proving the second init leaves native-state bytes unchanged.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --workspace --tests`
- `cargo test --workspace`: 418 tests passed
- `scripts/validation-smoke.ps1`: passed
- `scripts/projection-strategy-smoke.ps1`: passed
- `scripts/mvp-smoke.ps1`: passed
- `cargo build --release --workspace`: passed
- Self-hosting acceptance against an isolated clone of Sunlight: passed
- Scratchpad size check and `git diff --check`: passed

Strict `cargo clippy --workspace --all-targets -- -D warnings` is not a clean repository baseline on the current Rust toolchain. It reports roughly 200 existing warnings, primarily large public error enums and legacy test assertions. That unrelated refactor is recorded as post-MVP lint debt rather than folded into this bounded completion slice.

## Explicit post-MVP limitations

- ReFS block cloning is used only when capability checks prove it. NTFS and unsupported platforms use the measured full-copy correctness fallback. Physical allocation is reported as null when it cannot be measured truthfully.
- Windows has the strongest process, resource, filesystem-write, and explicit network-denial enforcement. Non-Windows containment remains the documented bounded/best-effort implementation.
- Compatibility import preserves exact unambiguous renames. Rename-plus-edit without a reliable identity signal remains conservative; fuzzy matching is intentionally deferred.
- Same-path process-specific filesystems, AST-native operations, symbol-level dependency tracking, hosted forge behavior, cross-repository implementation, and a polished GUI remain the architecture's explicit MVP deferrals.

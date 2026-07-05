#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_bin="${CARGO:-cargo}"

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/sun-validation-smoke.XXXXXX")"
cleanup() {
    rm -rf "$tmp_dir"
}
trap cleanup EXIT

sun_bin="$repo_root/target/debug/sun"
if [[ "${OS:-}" == "Windows_NT" || ! -x "$sun_bin" && -x "$repo_root/target/debug/sun.exe" ]]; then
    sun_bin="$repo_root/target/debug/sun.exe"
fi

step() {
    printf '==> %s\n' "$*"
}

fail() {
    printf 'validation smoke failed: %s\n' "$*" >&2
    exit 1
}

assert_contains() {
    local haystack="$1"
    local needle="$2"
    local label="$3"

    if [[ "$haystack" != *"$needle"* ]]; then
        printf 'missing expected output for %s:\n%s\n\nstdout was:\n%s\n' "$label" "$needle" "$haystack" >&2
        exit 1
    fi
}

run_ok() {
    local label="$1"
    shift

    local stdout
    local stderr
    local status
    stdout="$tmp_dir/${label//[^A-Za-z0-9_.-]/_}.stdout"
    stderr="$tmp_dir/${label//[^A-Za-z0-9_.-]/_}.stderr"

    set +e
    "$@" >"$stdout" 2>"$stderr"
    status=$?
    set -e

    if [[ $status -ne 0 ]]; then
        printf 'command failed for %s with status %s\ncommand:' "$label" "$status" >&2
        printf ' %q' "$@" >&2
        printf '\nstdout:\n%s\nstderr:\n%s\n' "$(cat "$stdout")" "$(cat "$stderr")" >&2
        exit 1
    fi

    cat "$stdout"
}

run_fail() {
    local label="$1"
    shift

    local stdout
    local stderr
    local status
    stdout="$tmp_dir/${label//[^A-Za-z0-9_.-]/_}.stdout"
    stderr="$tmp_dir/${label//[^A-Za-z0-9_.-]/_}.stderr"

    set +e
    "$@" >"$stdout" 2>"$stderr"
    status=$?
    set -e

    if [[ $status -eq 0 ]]; then
        printf 'command unexpectedly succeeded for %s\ncommand:' "$label" >&2
        printf ' %q' "$@" >&2
        printf '\nstdout:\n%s\nstderr:\n%s\n' "$(cat "$stdout")" "$(cat "$stderr")" >&2
        exit 1
    fi

    cat "$stdout"
}

json_string_field() {
    local json="$1"
    local field="$2"
    printf '%s' "$json" | sed -n "s/.*\"$field\":\"\\([^\"]*\\)\".*/\\1/p" | head -n 1
}

sun() {
    "$sun_bin" "$@"
}

step "Building sun CLI"
(cd "$repo_root" && "$cargo_bin" build -p sun --quiet)
[[ -x "$sun_bin" ]] || fail "built CLI not found at $sun_bin"

init_repo="$tmp_dir/init-repo"
mkdir -p "$init_repo"

step "Init real temporary repository"
out="$(run_ok init sun init --json --repo "$init_repo")"
assert_contains "$out" '"command":"repository.init"' "init command"
assert_contains "$out" '"ok":true' "init ok"
[[ -f "$init_repo/.sunlight/config.toml" ]] || fail "sun init did not create .sunlight/config.toml"

out="$(run_ok init-idempotent sun init --json --repo "$init_repo")"
assert_contains "$out" '"command":"repository.init"' "init idempotent command"
assert_contains "$out" '"ok":true' "init idempotent ok"

out="$(cd "$init_repo" && run_ok policy-check-commit sun policy check-commit --json)"
assert_contains "$out" '"command":"policy.check-commit"' "policy check commit command"
assert_contains "$out" '"ok":true' "policy check commit ok"
assert_contains "$out" '"managed_ignore_blocks_checked":1' "policy check commit managed ignore blocks"
assert_contains "$out" '"candidate_paths_checked":0' "policy check commit candidate paths"
assert_contains "$out" '"blocked":0' "policy check commit blocked"

step "Read/list/search fixture artifacts"
out="$(run_ok read sun read src/auth.ts --session session_agent_a --fixture basic-app --json)"
assert_contains "$out" '"command":"artifact.read"' "read command"
assert_contains "$out" '"artifact_id":"artifact_src_auth_ts"' "read artifact"
assert_contains "$out" '"content_hash":"sha256:auth_base"' "read hash"

out="$(run_fail read-missing sun read src/missing.ts --session session_agent_a --fixture basic-app --json)"
assert_contains "$out" '"code":"path_not_found"' "read missing"
assert_contains "$out" '"session_generation_id":"gen_agent_a_0001"' "read missing generation"

out="$(run_ok list sun list src --session session_agent_a --fixture basic-app --json)"
assert_contains "$out" '"command":"artifact.list"' "list command"
assert_contains "$out" '"path":"src/auth.ts"' "list auth"
assert_contains "$out" '"path":"src/profile.ts"' "list profile"

out="$(run_ok search sun search User.email --session session_agent_a --fixture basic-app --json)"
assert_contains "$out" '"command":"artifact.search"' "search command"
assert_contains "$out" '"path":"README.md"' "search readme"
assert_contains "$out" '"path":"docs/guide.md"' "search guide"
assert_contains "$out" '"path":"src/profile.ts"' "search profile"

step "Exercise fixture writes and preconditions"
patch_file="$tmp_dir/auth.patch"
cat >"$patch_file" <<'PATCH'
--- a/src/auth.ts
+++ b/src/auth.ts
@@ -1,3 +1,4 @@
 export function login(email: string) {
-  return email.trim().toLowerCase();
+  const normalized = email.trim().toLowerCase();
+  return normalized;
 }
PATCH

out="$(run_ok patch sun patch src/auth.ts --session session_agent_a --fixture basic-app --expect-hash sha256:auth_base --patch-file "$patch_file" --json)"
assert_contains "$out" '"command":"artifact.patch"' "patch command"
assert_contains "$out" '"operation_transaction_id":"op_auth_trim_guard_0001"' "patch operation"
assert_contains "$out" '"after_hash":"sha256:auth_trim_guard"' "patch hash"

content_file="$tmp_dir/session.ts"
printf 'export const sessionLabel = "SessionStore";\n' >"$content_file"
out="$(run_ok write sun write src/session.ts --session session_agent_a --fixture basic-app --expect-hash new --content-file "$content_file" --classification source --json)"
assert_contains "$out" '"command":"artifact.write"' "write command"
assert_contains "$out" '"artifact_id":"artifact_src_session_ts"' "write artifact"
assert_contains "$out" '"after_hash":"sha256:session_new"' "write hash"

out="$(run_fail patch-stale sun patch src/auth.ts --session session_agent_a --fixture basic-app --expect-hash sha256:stale_auth --patch-file "$patch_file" --json)"
assert_contains "$out" '"code":"precondition_failed"' "stale patch"
assert_contains "$out" '"session_generation_id":"gen_agent_a_0001"' "stale patch generation"

step "Resolve compatible and conflicted views"
include_ready='topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001'
out="$(run_ok resolve-ready sun view resolve --fixture basic-app --include "$include_ready" --json)"
assert_contains "$out" '"command":"view.resolve"' "resolve ready command"
assert_contains "$out" '"conflict_ids":[]' "resolve ready conflicts"
assert_contains "$out" '"staleness_ids":[]' "resolve ready staleness"
view_id="$(json_string_field "$out" resolved_view_id)"
[[ -n "$view_id" ]] || fail "could not extract resolved_view_id"

out="$(run_ok resolve-conflict sun view resolve --fixture basic-app --include topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_auth_overlap_0001 --json)"
assert_contains "$out" '"view":null' "resolve conflict no view"
assert_contains "$out" '"conflict_ids":["conflict_src_auth_ts_0001"]' "resolve conflict id"
assert_contains "$out" '"kind":"same_artifact_conflict"' "resolve conflict kind"

step "Materialize, run, and checkpoint ready view"
out="$(run_ok project-execution sun project materialize --view "$view_id" --purpose execution --fixture basic-app --json)"
assert_contains "$out" '"command":"projection.materialize"' "execution projection command"
assert_contains "$out" '"projection_id":"projection_exec_auth_profile_0001"' "execution projection id"
assert_contains "$out" '"selected_strategy":"copy"' "execution projection strategy"

out="$(run_ok run sun run --view "$view_id" --fixture basic-app --json -- cargo test)"
assert_contains "$out" '"command":"execution.run"' "run command"
assert_contains "$out" '"execution_id":"exec_auth_profile_tests_0001"' "run execution"
assert_contains "$out" '"status":"pass"' "run result"

out="$(run_ok checkpoint sun checkpoint create --view "$view_id" --fixture basic-app --json)"
assert_contains "$out" '"command":"checkpoint.create"' "checkpoint command"
assert_contains "$out" '"checkpoint_id":"checkpoint_auth_profile_ready_0001"' "checkpoint id"
assert_contains "$out" '"export_ready":true' "checkpoint export ready"

out="$(run_ok policy-check-export sun policy check-export --checkpoint checkpoint_auth_profile_ready_0001 --fixture basic-app --json)"
assert_contains "$out" '"command":"policy.check-export"' "policy check export command"
assert_contains "$out" '"validation_report_id":"validation_export_auth_profile_ready_0001"' "policy check export validation report"
assert_contains "$out" '"failures":[]' "policy check export failures"

out="$(run_ok policy-explain sun policy explain validation_export_auth_profile_ready_0001 --json)"
assert_contains "$out" '"command":"policy.explain"' "policy explain command"
assert_contains "$out" '"validation_report_id":"validation_export_auth_profile_ready_0001"' "policy explain validation report id"
assert_contains "$out" '"ids":{"validation_report_id":"validation_export_auth_profile_ready_0001"}' "policy explain ids validation report id"
assert_contains "$out" '"validation_report":{"id":"validation_export_auth_profile_ready_0001"' "policy explain validation report"
assert_contains "$out" '"failures":[]' "policy explain failures"

step "Compatibility project, diff, import, and Git export write plan"
out="$(run_ok compat-project sun compat project --session session_agent_a --fixture basic-app --json)"
assert_contains "$out" '"command":"compat.project"' "compat project command"
assert_contains "$out" '"projection_id":"projection_compat_agent_a_0001"' "compat projection id"
assert_contains "$out" '"baseline_manifest_digest":"sha256:compat_baseline"' "compat project baseline manifest digest"

out="$(run_ok compat-diff sun compat diff --projection projection_compat_agent_a_0001 --fixture basic-app --json)"
assert_contains "$out" '"command":"compat.diff"' "compat diff command"
assert_contains "$out" '"selected_candidate_delta_ids":["compat_delta_src_auth_ts_0001"]' "compat diff selected safe default"
assert_contains "$out" '"quarantine_refs":["quarantine://compat/projection_compat_agent_a_0001/env"]' "compat diff quarantine refs"

out="$(run_ok compat-import sun compat import --projection projection_compat_agent_a_0001 --candidate compat_delta_src_auth_ts_0001 --fixture basic-app --json)"
assert_contains "$out" '"command":"compat.import"' "compat import command"
assert_contains "$out" '"operation_transaction_id":"op_compat_import_auth_0001"' "compat import operation"
assert_contains "$out" '"topic_revision_id":"rev_auth_nullability_compat_0001"' "compat import revision"

out="$(run_ok git-export-write-plan sun git export --checkpoint checkpoint_auth_profile_ready_0001 --branch refs/heads/sunlight/auth-profile-ready --fixture basic-app --write-plan --json)"
assert_contains "$out" '"command":"git.export.write_plan"' "git export write plan command"
assert_contains "$out" '"validation_report_id":"validation_export_auth_profile_ready_0001"' "git export validation report"
assert_contains "$out" '"planned_commit_id":"git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"' "git export planned commit"

step "Look up fixture Git export by ref and commit"
out="$(run_ok status-git-ref sun status --git refs/heads/sunlight/auth-profile-ready --fixture basic-app --json)"
assert_contains "$out" '"command":"status.git"' "status git command"
assert_contains "$out" '"git_ref":"refs/heads/sunlight/auth-profile-ready"' "status git ref"
assert_contains "$out" '"export_map_id":"export_map_checkpoint_auth_profile_ready_0001"' "status git export map"
assert_contains "$out" '"checkpoint_id":"checkpoint_auth_profile_ready_0001"' "status git checkpoint"
assert_contains "$out" '"validation_report_id":"validation_export_auth_profile_ready_0001"' "status git validation report"
assert_contains "$out" '"git_commit_ids":["git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"]' "status git commit ids"

out="$(run_ok inspect-git-commit sun inspect git:git_sha1_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa --fixture basic-app --json)"
assert_contains "$out" '"command":"inspect.git"' "inspect git command"
assert_contains "$out" '"git_ref":"refs/heads/sunlight/auth-profile-ready"' "inspect git ref"
assert_contains "$out" '"export_map_id":"export_map_checkpoint_auth_profile_ready_0001"' "inspect git export map"
assert_contains "$out" '"checkpoint_id":"checkpoint_auth_profile_ready_0001"' "inspect git checkpoint"
assert_contains "$out" '"validation_report":{"id":"validation_export_auth_profile_ready_0001"' "inspect git validation report"
assert_contains "$out" '"export_map":{"record_type":"git_export_map","id":"export_map_checkpoint_auth_profile_ready_0001"' "inspect git export map record type"

step "Validation smoke passed"

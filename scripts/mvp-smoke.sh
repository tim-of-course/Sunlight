#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_bin="${CARGO:-cargo}"

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/sun-mvp-smoke.XXXXXX")"
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
    printf 'mvp smoke failed: %s\n' "$*" >&2
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

    local stdout="$tmp_dir/${label//[^A-Za-z0-9_.-]/_}.stdout"
    local stderr="$tmp_dir/${label//[^A-Za-z0-9_.-]/_}.stderr"
    local status

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

json_string_field() {
    local json="$1"
    local field="$2"
    printf '%s' "$json" | sed -n "s/.*\"$field\":\(null\|\"\\([^\"]*\\)\"\).*/\\2/p" | head -n 1
}

git_capture() {
    git -C "$export_repo" "$@"
}

sun() {
    "$sun_bin" "$@"
}

command -v git >/dev/null 2>&1 || fail "git is required"

export ZIG_LOCAL_CACHE_DIR="${ZIG_LOCAL_CACHE_DIR:-$tmp_dir/zig-local-cache}"
export ZIG_GLOBAL_CACHE_DIR="${ZIG_GLOBAL_CACHE_DIR:-$tmp_dir/zig-global-cache}"
mkdir -p "$ZIG_LOCAL_CACHE_DIR" "$ZIG_GLOBAL_CACHE_DIR"

step "Building sun CLI"
(cd "$repo_root" && "$cargo_bin" build -p sun --quiet)
[[ -x "$sun_bin" ]] || fail "built CLI not found at $sun_bin"

step "Resolving basic-app view"
include_ready='topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001'
out="$(run_ok resolve-ready sun view resolve --fixture basic-app --include "$include_ready" --json)"
assert_contains "$out" '"command":"view.resolve"' "resolve command"
assert_contains "$out" '"conflict_ids":[]' "resolve conflicts"
assert_contains "$out" '"staleness_ids":[]' "resolve staleness"
view_id="$(json_string_field "$out" resolved_view_id)"
[[ -n "$view_id" ]] || fail "could not extract resolved_view_id"

step "Materializing projection and running fixture command"
out="$(run_ok project-execution sun project materialize --view "$view_id" --purpose execution --fixture basic-app --json)"
assert_contains "$out" '"command":"projection.materialize"' "projection command"
assert_contains "$out" '"projection_id":"projection_exec_auth_profile_0001"' "projection id"
assert_contains "$out" '"selected_strategy":"copy"' "projection strategy"

out="$(run_ok run-tests sun run --view "$view_id" --fixture basic-app --json -- cargo test)"
assert_contains "$out" '"command":"execution.run"' "execution command"
assert_contains "$out" '"status":"pass"' "execution status"
assert_contains "$out" '"execution_id":"exec_auth_profile_tests_0001"' "execution id"

step "Creating export-ready checkpoint"
out="$(run_ok checkpoint sun checkpoint create --view "$view_id" --fixture basic-app --json)"
assert_contains "$out" '"command":"checkpoint.create"' "checkpoint command"
assert_contains "$out" '"checkpoint_id":"checkpoint_auth_profile_ready_0001"' "checkpoint id"
assert_contains "$out" '"export_ready":true' "checkpoint export ready"
checkpoint_id="$(json_string_field "$out" checkpoint_id)"
[[ -n "$checkpoint_id" ]] || fail "could not extract checkpoint_id"

step "Preparing temporary Git repository"
export_repo="$tmp_dir/export-repo"
mkdir -p "$export_repo"
git -C "$export_repo" init --quiet
git -C "$export_repo" config user.name "Sunlight Smoke"
git -C "$export_repo" config user.email "sunlight-smoke@example.invalid"
printf 'base\n' >"$export_repo/README.md"
git -C "$export_repo" add README.md
git -C "$export_repo" commit --quiet -m "Base"
base_commit="$(git_capture rev-parse --verify HEAD^{commit})"

step "Executing local Git export"
target_ref='refs/heads/sunlight/mvp-smoke'
out="$(run_ok git-export-local sun git export --checkpoint "$checkpoint_id" --branch "$target_ref" --fixture basic-app --execute-local --repo "$export_repo" --json)"
assert_contains "$out" '"command":"git.export.execute"' "git export command"
assert_contains "$out" '"lifecycle_state":"exported"' "git export lifecycle"
assert_contains "$out" '"commit_created":true' "git export commit"
assert_contains "$out" '"ref_updated":true' "git export ref"
assert_contains "$out" '"export_map_written":true' "git export map"
created_commit="$(json_string_field "$out" created_commit_id)"
[[ -n "$created_commit" ]] || fail "could not extract created_commit_id"

step "Verifying exported Git ref and tree"
ref_commit="$(git_capture rev-parse --verify "$target_ref^{commit}")"
[[ "$ref_commit" == "$created_commit" ]] || fail "target ref points to $ref_commit, expected $created_commit"
git_capture cat-file -e "$created_commit^{commit}"
parent_commit="$(git_capture rev-parse --verify "$created_commit^")"
[[ "$parent_commit" == "$base_commit" ]] || fail "export parent is $parent_commit, expected $base_commit"

tree_paths="$(git_capture ls-tree -r --name-only "$created_commit")"
for path in src/auth.rs src/profile.rs bin/run-auth-check .sunlight/export-manifest.json; do
    if [[ "$tree_paths" != *"$path"* ]]; then
        fail "exported commit tree missing $path"
    fi
done

run_mode="$(git_capture ls-tree "$created_commit" bin/run-auth-check | awk '{print $1}')"
[[ "$run_mode" == "100755" ]] || fail "bin/run-auth-check mode is $run_mode, expected 100755"

auth_content="$(git_capture show "$created_commit:src/auth.rs")"
manifest_content="$(git_capture show "$created_commit:.sunlight/export-manifest.json")"
[[ "$auth_content" == "pub fn auth() {}" ]] || fail "unexpected src/auth.rs content"
assert_contains "$manifest_content" '"policy":"approved_manifest_only"' "export manifest"

step "MVP smoke passed"

#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_bin="${CARGO:-cargo}"

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/sun-projection-strategy-smoke.XXXXXX")"
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
    printf 'projection strategy smoke failed: %s\n' "$*" >&2
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

assert_not_contains() {
    local haystack="$1"
    local needle="$2"
    local label="$3"

    if [[ "$haystack" == *"$needle"* ]]; then
        printf 'unexpected output for %s:\n%s\n\nstdout was:\n%s\n' "$label" "$needle" "$haystack" >&2
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

run_fail() {
    local label="$1"
    shift

    local stdout="$tmp_dir/${label//[^A-Za-z0-9_.-]/_}.stdout"
    local stderr="$tmp_dir/${label//[^A-Za-z0-9_.-]/_}.stderr"
    local status

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

json_number_field() {
    local json="$1"
    local field="$2"
    printf '%s' "$json" | sed -n "s/.*\"$field\":\\([0-9][0-9]*\\).*/\\1/p" | head -n 1
}

sun() {
    "$sun_bin" "$@"
}

export ZIG_LOCAL_CACHE_DIR="${ZIG_LOCAL_CACHE_DIR:-$tmp_dir/zig-local-cache}"
export ZIG_GLOBAL_CACHE_DIR="${ZIG_GLOBAL_CACHE_DIR:-$tmp_dir/zig-global-cache}"
mkdir -p "$ZIG_LOCAL_CACHE_DIR" "$ZIG_GLOBAL_CACHE_DIR"

step "Building sun CLI"
(cd "$repo_root" && "$cargo_bin" build -p sun --quiet)
[[ -x "$sun_bin" ]] || fail "built CLI not found at $sun_bin"

step "Resolving deterministic basic-app view"
include_ready='topic_auth_nullability:rev_auth_nullability_0001,topic_profile_ui:rev_profile_ui_0001'
out="$(run_ok resolve-ready sun view resolve --fixture basic-app --include "$include_ready" --json)"
assert_contains "$out" '"command":"view.resolve"' "resolve command"
assert_contains "$out" '"conflict_ids":[]' "resolve conflicts"
assert_contains "$out" '"staleness_ids":[]' "resolve staleness"
assert_contains "$out" '"tree_identity":{"kind":"SingleRepoTree","repository_id":"repo_fixture_basic_app","tree_hash":"tree_fixture_' "resolve tree identity"
view_id="$(json_string_field "$out" resolved_view_id)"
[[ -n "$view_id" ]] || fail "could not extract resolved_view_id"

step "Verifying copy fallback and local-only metadata"
out="$(run_ok default-copy sun project materialize --view "$view_id" --purpose execution --fixture basic-app --json)"
assert_contains "$out" '"command":"projection.materialize"' "copy command"
assert_contains "$out" '"selected_strategy":"copy"' "copy selected strategy"
assert_contains "$out" '"strategy":"copy"' "copy strategy"
assert_contains "$out" '"source":"resolved_content_tree"' "copy source"
assert_contains "$out" '"created_from_content_tree":"tree_fixture_' "copy content tree"
assert_contains "$out" '"local_materialization":{"privacy_class":"local_only","projection_id":"projection_exec_auth_profile_0001"' "copy local metadata"
assert_contains "$out" '"root_ref":{"value":"local://.sunlight/projections/execution/projection_exec_auth_profile_0001","privacy":"local_only_path","privacy_class":"local_only"}' "copy local root"
assert_contains "$out" ':execution:copy:read_only_source_private_outputs"' "copy cache key"
assert_contains "$out" '"store_integrity_policy":"verify_before_reuse"' "copy integrity policy"
assert_not_contains "$out" "$tmp_dir" "local-only metadata excludes smoke temp path"

step "Collecting portable fixture copy metrics"
metrics_view_id="view_base_0001"
projection_root="$tmp_dir/projection-root"
start_ns="$(date +%s%N)"
out="$(run_ok copy-metrics sun project materialize --view "$metrics_view_id" --purpose execution --fixture basic-app --projection-root "$projection_root" --json)"
end_ns="$(date +%s%N)"
elapsed_ms="$(((end_ns - start_ns) / 1000000))"
files_written="$(json_number_field "$out" files_written)"
directories_created="$(json_number_field "$out" directories_created)"
bytes_written="$(json_number_field "$out" bytes_written)"
executable_files="$(json_number_field "$out" executable_files)"
manifest_entries="$(printf '%s' "$out" | grep -o '"content_hash":' | wc -l | tr -d '[:space:]')"
manifest_directories="$(printf '%s' "$out" | sed -n 's/.*"summary":{"directories":\([0-9][0-9]*\),"files":[0-9][0-9]*,"bytes":[0-9][0-9]*.*/\1/p' | head -n 1)"
manifest_files="$(printf '%s' "$out" | sed -n 's/.*"summary":{"directories":[0-9][0-9]*,"files":\([0-9][0-9]*\),"bytes":[0-9][0-9]*.*/\1/p' | head -n 1)"
manifest_bytes="$(printf '%s' "$out" | sed -n 's/.*"summary":{"directories":[0-9][0-9]*,"files":[0-9][0-9]*,"bytes":\([0-9][0-9]*\).*/\1/p' | head -n 1)"
assert_contains "$out" '"selected_strategy":"copy"' "copy metrics selected strategy"
assert_contains "$out" '"local_projection_manifest":{' "copy metrics manifest"
assert_contains "$out" '"cleanup":{"projection_root":{' "copy metrics cleanup"
[[ -n "$files_written" && -n "$directories_created" && -n "$bytes_written" && -n "$executable_files" ]] || fail "could not extract filesystem materialization counts"
[[ -n "$manifest_entries" && -n "$manifest_directories" && -n "$manifest_files" && -n "$manifest_bytes" ]] || fail "could not extract manifest counts"
printf 'projection_metric fixture=basic-app view=%s purpose=execution selected_strategy=copy elapsed_ms=%s files_written=%s directories_created=%s bytes_written=%s executable_files=%s manifest_entries=%s manifest_files=%s manifest_directories=%s manifest_bytes=%s deferred_strategies=reflink_real_fs,hardlink_readonly_real_fs,overlay_copyup_real_fs scope=fixture_copy_materialization_only\n' \
    "$metrics_view_id" "$elapsed_ms" "$files_written" "$directories_created" "$bytes_written" "$executable_files" "$manifest_entries" "$manifest_files" "$manifest_directories" "$manifest_bytes"

step "Verifying explicit reflink strategy selection JSON"
out="$(run_ok reflink sun project materialize --view "$view_id" --purpose execution --strategy reflink --fixture basic-app --json)"
assert_contains "$out" '"selected_strategy":"reflink"' "reflink selected strategy"
assert_contains "$out" '"strategy":"reflink"' "reflink strategy"
assert_contains "$out" ':execution:reflink:read_only_source_private_outputs"' "reflink cache key"
assert_contains "$out" '"local_materialization":{"privacy_class":"local_only","projection_id":"projection_exec_auth_profile_0001"' "reflink local metadata"
assert_contains "$out" '"source":"resolved_content_tree"' "reflink source"

step "Verifying ineligible preferred strategy falls back to copy"
out="$(run_ok hardlink-copy-fallback sun project materialize --view "$view_id" --purpose execution --strategy hardlink_readonly --fixture basic-app --json)"
assert_contains "$out" '"selected_strategy":"copy"' "hardlink fallback selected strategy"
assert_contains "$out" '"strategy":"copy"' "hardlink fallback strategy"
assert_contains "$out" ':execution:copy:read_only_source_private_outputs"' "hardlink fallback cache key"
assert_contains "$out" '"writable_policy":"read_only_source_private_outputs"' "hardlink fallback writable policy"

step "Verifying unsupported required strategy failure"
out="$(run_fail hardlink-required sun project materialize --view "$view_id" --purpose execution --strategy hardlink_readonly --no-copy-fallback --fixture basic-app --json)"
assert_contains "$out" '"ok":false' "required failure envelope"
assert_contains "$out" '"code":"projection_materialization_hardlink_readonly_requires_read_only_policy"' "required failure code"
assert_contains "$out" '"message":"read-only hardlink materialization requires a read-only projection policy"' "required failure message"
assert_contains "$out" "\"resolved_view_id\":\"$view_id\"" "required failure view id"
assert_contains "$out" '"strategy":"hardlink_readonly"' "required failure strategy"
assert_contains "$out" '"projection_id":null' "required failure no projection"

step "Projection strategy smoke passed"

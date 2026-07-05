#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_bin="${CARGO:-cargo}"

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/sun-projection-strategy-smoke.XXXXXX")"
non_temp_probe_dir=""
cleanup() {
    rm -rf "$tmp_dir"
    if [[ -n "$non_temp_probe_dir" ]]; then
        rm -rf "$non_temp_probe_dir"
    fi
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

hash_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        cksum "$1" | awk '{print $1 ":" $2}'
    fi
}

classify_probe_error() {
    local error_text="$1"
    local lower
    lower="$(printf '%s' "$error_text" | tr '[:upper:]' '[:lower:]')"

    case "$lower" in
        *"operation not supported"* | *"not supported"*)
            printf 'operation_not_supported'
            ;;
        *"invalid argument"*)
            printf 'invalid_argument'
            ;;
        *"permission denied"* | *"must be superuser"* | *"not permitted"*)
            printf 'permission_denied'
            ;;
        *"command not found"* | *"not found"*)
            printf 'command_not_found'
            ;;
        "")
            printf 'none'
            ;;
        *)
            printf 'other'
            ;;
    esac
}

probe_reflink_capability() {
    local probe_root="$1"
    local fs_type="$2"
    local host_scope="$3"
    local probe_label="$4"
    local src="$probe_root/reflink-source"
    local dst="$probe_root/reflink-dest"
    local stderr_file="$probe_root/reflink.stderr"
    local before_hash
    local after_hash
    local private_writes
    local reason

    printf 'sunlight-projection-reflink-source\n' >"$src"
    before_hash="$(hash_file "$src")"

    if cp --reflink=always "$src" "$dst" 2>"$stderr_file"; then
        printf 'sunlight-projection-reflink-dest-mutation\n' >"$dst"
        after_hash="$(hash_file "$src")"
        if [[ "$before_hash" == "$after_hash" ]]; then
            private_writes="yes"
            reason="reflink_copy_succeeded_and_dest_write_left_source_hash_unchanged"
        else
            private_writes="no"
            reason="dest_write_changed_source_hash"
        fi
        printf 'projection_fs_capability host_scope=%s probe_root=%s strategy=reflink fs_type=%s reflink_attempt=ok writes_private=%s accepted=%s reason=%s\n' \
            "$host_scope" "$probe_label" "$fs_type" "$private_writes" "$([[ "$private_writes" == "yes" ]] && printf accepted || printf deferred)" "$reason"
    else
        reason="$(classify_probe_error "$(cat "$stderr_file")")"
        printf 'projection_fs_capability host_scope=%s probe_root=%s strategy=reflink fs_type=%s reflink_attempt=failed writes_private=unknown accepted=deferred reason=%s\n' \
            "$host_scope" "$probe_label" "$fs_type" "$reason"
    fi
}

probe_hardlink_readonly_capability() {
    local probe_root="$1"
    local fs_type="$2"
    local host_scope="$3"
    local probe_label="$4"
    local store="$probe_root/hardlink-store"
    local link="$probe_root/hardlink-projection"
    local stderr_file="$probe_root/hardlink.stderr"
    local before_hash
    local after_hash
    local read_only_write_blocked="unknown"
    local chmod_write_mutated_store="unknown"
    local mutation_risk="unknown"
    local reason
    local status

    printf 'sunlight-projection-hardlink-store\n' >"$store"
    chmod 0444 "$store"
    before_hash="$(hash_file "$store")"

    if ln "$store" "$link" 2>"$stderr_file"; then
        set +e
        sh -c 'printf "%s\n" "hardlink-write-without-chmod" >"$1"' sh "$link" 2>"$stderr_file"
        status=$?
        set -e
        if [[ $status -ne 0 ]]; then
            read_only_write_blocked="yes"
        else
            read_only_write_blocked="no"
        fi

        chmod u+w "$link"
        printf 'hardlink-write-after-chmod\n' >"$link"
        after_hash="$(hash_file "$store")"
        chmod u+w "$store"

        if [[ "$before_hash" == "$after_hash" ]]; then
            chmod_write_mutated_store="no"
            mutation_risk="absent_in_probe"
            reason="read_only_hardlink_did_not_mutate_store_after_chmod_probe"
        else
            chmod_write_mutated_store="yes"
            mutation_risk="present"
            reason="shared_inode_owner_can_chmod_projection_and_mutate_store"
        fi

        printf 'projection_fs_capability host_scope=%s probe_root=%s strategy=hardlink_readonly fs_type=%s hardlink_attempt=ok read_only_write_blocked=%s chmod_write_mutated_store=%s mutation_isolation_risk=%s accepted=deferred reason=%s\n' \
            "$host_scope" "$probe_label" "$fs_type" "$read_only_write_blocked" "$chmod_write_mutated_store" "$mutation_risk" "$reason"
    else
        chmod u+w "$store"
        reason="$(classify_probe_error "$(cat "$stderr_file")")"
        printf 'projection_fs_capability host_scope=%s probe_root=%s strategy=hardlink_readonly fs_type=%s hardlink_attempt=failed read_only_write_blocked=unknown chmod_write_mutated_store=unknown mutation_isolation_risk=unknown accepted=deferred reason=%s\n' \
            "$host_scope" "$probe_label" "$fs_type" "$reason"
    fi
}

probe_overlay_copyup_capability() {
    local probe_root="$1"
    local fs_type="$2"
    local host_scope="$3"
    local probe_label="$4"
    local lower="$probe_root/overlay-lower"
    local upper="$probe_root/overlay-upper"
    local work="$probe_root/overlay-work"
    local merged="$probe_root/overlay-merged"
    local stderr_file="$probe_root/overlay.stderr"
    local lower_before
    local lower_after
    local private_writes="unknown"
    local reason

    mkdir -p "$lower" "$upper" "$work" "$merged"
    printf 'sunlight-projection-overlay-lower\n' >"$lower/file.txt"
    lower_before="$(hash_file "$lower/file.txt")"

    if mount -t overlay overlay -o "lowerdir=$lower,upperdir=$upper,workdir=$work" "$merged" 2>"$stderr_file"; then
        printf 'sunlight-projection-overlay-merged-mutation\n' >"$merged/file.txt"
        lower_after="$(hash_file "$lower/file.txt")"
        if [[ "$lower_before" == "$lower_after" && -f "$upper/file.txt" ]]; then
            private_writes="yes"
            reason="overlay_mount_succeeded_and_write_copied_up"
        else
            private_writes="no"
            reason="overlay_write_did_not_preserve_lower_file"
        fi
        umount "$merged" 2>/dev/null || true
        printf 'projection_fs_capability host_scope=%s probe_root=%s strategy=overlay_copyup fs_type=%s overlay_attempt=ok copyup_writes_private=%s accepted=%s reason=%s\n' \
            "$host_scope" "$probe_label" "$fs_type" "$private_writes" "$([[ "$private_writes" == "yes" ]] && printf accepted || printf deferred)" "$reason"
    elif command -v fuse-overlayfs >/dev/null 2>&1 \
        && fuse-overlayfs -o "lowerdir=$lower,upperdir=$upper,workdir=$work" "$merged" 2>"$stderr_file"; then
        printf 'sunlight-projection-overlay-merged-mutation\n' >"$merged/file.txt"
        lower_after="$(hash_file "$lower/file.txt")"
        if [[ "$lower_before" == "$lower_after" && -f "$upper/file.txt" ]]; then
            private_writes="yes"
            reason="fuse_overlayfs_mount_succeeded_and_write_copied_up"
        else
            private_writes="no"
            reason="fuse_overlayfs_write_did_not_preserve_lower_file"
        fi
        fusermount -u "$merged" 2>/dev/null || umount "$merged" 2>/dev/null || true
        printf 'projection_fs_capability host_scope=%s probe_root=%s strategy=overlay_copyup fs_type=%s overlay_attempt=ok copyup_writes_private=%s accepted=%s reason=%s\n' \
            "$host_scope" "$probe_label" "$fs_type" "$private_writes" "$([[ "$private_writes" == "yes" ]] && printf accepted || printf deferred)" "$reason"
    else
        reason="$(classify_probe_error "$(cat "$stderr_file")")"
        printf 'projection_fs_capability host_scope=%s probe_root=%s strategy=overlay_copyup fs_type=%s overlay_attempt=failed copyup_writes_private=unknown accepted=deferred reason=%s\n' \
            "$host_scope" "$probe_label" "$fs_type" "$reason"
    fi
}

probe_filesystem_capabilities() {
    local probe_root="$1"
    local host_scope="$2"
    local probe_label="$3"
    local fs_type

    mkdir -p "$probe_root"
    fs_type="$(stat -f -c %T "$probe_root" 2>/dev/null || printf 'unknown')"
    printf 'projection_fs_capability host_scope=%s fs_type=%s probe_root=%s absolute_paths=omitted\n' "$host_scope" "$fs_type" "$probe_label"
    printf 'projection_fs_capability host_scope=%s probe_root=%s strategy=copy fs_type=%s accepted=accepted reason=correctness_fallback\n' "$host_scope" "$probe_label" "$fs_type"
    probe_reflink_capability "$probe_root" "$fs_type" "$host_scope" "$probe_label"
    probe_hardlink_readonly_capability "$probe_root" "$fs_type" "$host_scope" "$probe_label"
    probe_overlay_copyup_capability "$probe_root" "$fs_type" "$host_scope" "$probe_label"
}

probe_real_filesystem_capabilities() {
    probe_filesystem_capabilities "$tmp_dir/real-fs-probe" current_wsl_linux_tempdir tempdir

    if [[ -n "${SUNLIGHT_PROJECTION_SMOKE_NON_TEMP_ROOT:-}" ]]; then
        mkdir -p "$SUNLIGHT_PROJECTION_SMOKE_NON_TEMP_ROOT"
        non_temp_probe_dir="$(mktemp -d "$SUNLIGHT_PROJECTION_SMOKE_NON_TEMP_ROOT/sun-projection-non-temp-fs-probe.XXXXXX")"
        probe_filesystem_capabilities "$non_temp_probe_dir" current_wsl_linux_non_temp_root non_temp
    fi
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

step "Probing observed WSL/Linux filesystem capabilities"
probe_real_filesystem_capabilities

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

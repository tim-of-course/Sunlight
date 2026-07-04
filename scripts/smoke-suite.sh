#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cargo_bin="${CARGO:-cargo}"

tmp_dir="$(mktemp -d "${TMPDIR:-/tmp}/sun-smoke-suite.XXXXXX")"
cleanup() {
    rm -rf "$tmp_dir"
}
trap cleanup EXIT

export ZIG_LOCAL_CACHE_DIR="${ZIG_LOCAL_CACHE_DIR:-$tmp_dir/zig-local-cache}"
export ZIG_GLOBAL_CACHE_DIR="${ZIG_GLOBAL_CACHE_DIR:-$tmp_dir/zig-global-cache}"
mkdir -p "$ZIG_LOCAL_CACHE_DIR" "$ZIG_GLOBAL_CACHE_DIR"

step() {
    printf '==> %s\n' "$*"
}

run_in_repo() {
    step "$*"
    (cd "$repo_root" && "$@")
}

run_script() {
    local script="$1"
    step "$script"
    "$repo_root/$script"
}

run_in_repo "$cargo_bin" fmt --check
run_in_repo "$cargo_bin" check
run_in_repo "$cargo_bin" test
run_script scripts/validation-smoke.sh
run_script scripts/projection-strategy-smoke.sh

step "Smoke suite passed"

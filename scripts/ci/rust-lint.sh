#!/usr/bin/env bash
# Run Rust lint checks with the same command set used in CI.

set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"
cd "$repo_root"

fmt_cmd=(cargo fmt --all -- --check)
clippy_cmd=(cargo clippy --locked --all-targets -- -D clippy::correctness)

echo "==> rust-lint: ${fmt_cmd[*]}"
"${fmt_cmd[@]}"

echo "==> rust-lint: ${clippy_cmd[*]}"
"${clippy_cmd[@]}"

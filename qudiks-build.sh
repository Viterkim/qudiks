#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
profile=release
print_path=""

usage() {
  cat <<'USAGE'
usage: ./qudiks-build.sh [--debug|--release] [--print-binary-path]

Builds Qudiks and the code mode helper from this checkout.
Release is the default. --print-binary-path shows the output path without building.
USAGE
}

while (( $# )); do
  case $1 in
    --debug) profile=debug; shift ;;
    --release) profile=release; shift ;;
    --print-binary-path) print_path=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 1 ;;
  esac
done

source_dir=$repo_root/codex-rs
target_dir=${CARGO_TARGET_DIR:-$source_dir/target}
[[ $target_dir == /* ]] || target_dir=$source_dir/$target_dir
if [[ -n ${CARGO_BUILD_TARGET:-} ]]; then
  target_profile=$target_dir/$CARGO_BUILD_TARGET/$profile
else
  target_profile=$target_dir/$profile
fi

built=$target_profile/codex
if [[ -n $print_path ]]; then
  printf '%s\n' "$built"
  exit 0
fi

command -v cargo >/dev/null || { echo "cargo not found; install rustup first" >&2; exit 1; }
[[ -f $source_dir/Cargo.toml ]] || {
  echo "The source build needs the full checkout. Run git sparse-checkout disable, or use ./qudiks-install.sh --download." >&2
  exit 1
}

cargo_args=(build --locked -p codex-cli --bin codex -p codex-code-mode-host --bin codex-code-mode-host)
[[ $profile != release ]] || cargo_args+=(--release)
cd "$source_dir"
CARGO_TARGET_DIR="$target_dir" bash "$repo_root/scripts/with-qudiks-v8.sh" cargo "${cargo_args[@]}"
printf '%s\n' "$built"

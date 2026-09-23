#!/usr/bin/env bash
# Reset the fork and install its latest published Linux binary without Cargo.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
bin_dir=${QUDIKS_BIN_DIR:-$HOME/.cargo/bin}
remote=${QUDIKS_REMOTE:-origin}
branch=fork-off

usage() {
  cat <<'USAGE'
usage: ./install-qudiks-bin.sh [--bin-dir DIR]

Fetches and resets the checkout to origin/fork-off, discarding local changes.
Then downloads and installs the latest Linux x86_64 binary.
USAGE
}

while (( $# )); do
  case $1 in
    --bin-dir) bin_dir=$2; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 1 ;;
  esac
done

command -v git >/dev/null || { echo "git is required" >&2; exit 1; }
cd "$repo_root"
[[ $(git rev-parse --show-toplevel) == "$repo_root" ]] || {
  echo "Run this from a Qudiks checkout." >&2; exit 1;
}
echo "==> resetting to $remote/$branch (local changes will be discarded)"
git fetch --prune "$remote" "$branch"
git reset --hard FETCH_HEAD
exec "$repo_root/setup-qudiks-bin.sh" --bin-dir "$bin_dir"

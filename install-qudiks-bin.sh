#!/usr/bin/env bash
# Update the fork and install its latest published Linux binary without Cargo.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
bin_dir=${QUDIKS_BIN_DIR:-$HOME/.cargo/bin}
remote=${QUDIKS_REMOTE:-origin}
branch=fork-off

usage() {
  cat <<'USAGE'
usage: ./install-qudiks-bin.sh [--bin-dir DIR]

Fetches and resets the checkout to origin/fork-off, stashing tracked and
untracked changes first. Local-only commits are saved on a backup branch.
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
echo "==> fetching $remote/$branch"
git fetch --prune "$remote" "$branch"
before=$(git rev-parse HEAD)
local_only=$(git rev-list --left-only --cherry-pick --count "$before...FETCH_HEAD")
if (( local_only > 0 )); then
  backup_branch="qudiks-backup-$(date +%Y%m%d-%H%M%S)-$$"
  git branch "$backup_branch" "$before"
  echo "==> saved local commits on $backup_branch"
fi
if [[ -n $(git status --porcelain) ]]; then
  git stash push --include-untracked -m "install-qudiks-bin.sh before reset to $remote/$branch"
  if [[ -n $(git status --porcelain) ]]; then
    echo "Could not stash all local changes; refusing to reset." >&2
    exit 1
  fi
  echo "==> saved local changes in git stash (restore with: git stash pop)"
fi
echo "==> resetting to $remote/$branch"
git reset --hard FETCH_HEAD
exec "$repo_root/setup-qudiks-bin.sh" --bin-dir "$bin_dir"

#!/usr/bin/env bash
# Pull the latest qudiks (even after a force-push) and reinstall it.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
bin_dir=${QUDIKS_BIN_DIR:-$HOME/.cargo/bin}
remote=${QUDIKS_REMOTE:-origin}
branch=""
discard_local=""

usage() {
  cat <<'USAGE'
usage: ./install-qudiks.sh [options]

  --branch NAME      branch to track (default: current, else the remote's HEAD)
  --discard-local    throw away local commits and uncommitted changes
  --bin-dir DIR      where to install (default: ~/.cargo/bin)

Rebases rewrite history, so this resets to the remote instead of merging.
It refuses to run with local changes unless you pass --discard-local.
USAGE
}

while (( $# )); do
  case $1 in
    --branch) branch=$2; shift 2 ;;
    --discard-local) discard_local=1; shift ;;
    --bin-dir) bin_dir=$2; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 1 ;;
  esac
done

cd "$repo_root"
command -v git >/dev/null || { echo "git not found" >&2; exit 1; }
command -v cargo >/dev/null || { echo "cargo not found; install rustup first" >&2; exit 1; }

if [[ -z $branch ]]; then
  branch=$(git symbolic-ref --quiet --short HEAD 2>/dev/null || true)
  if [[ -z $branch ]]; then
    branch=$(git remote show "$remote" | sed -n 's/.*HEAD branch: //p')
  fi
fi
[[ -n $branch ]] || { echo "could not work out which branch to track" >&2; exit 1; }

if [[ -z $discard_local ]] && [[ -n $(git status --porcelain) ]]; then
  echo "You have local changes:" >&2
  git status --short >&2
  echo >&2
  echo "Commit or stash them, or re-run with --discard-local to throw them away." >&2
  exit 1
fi

echo "==> fetching $remote/$branch"
git fetch --prune "$remote" "$branch"

before=$(git rev-parse HEAD)
# Reset rather than pull: rebases rewrite history, so a merge would conflict.
git reset --hard "$remote/$branch"
after=$(git rev-parse HEAD)

if [[ $before == "$after" ]]; then
  echo "==> already at $(git log --oneline -1)"
else
  echo "==> updated $(git rev-parse --short "$before") -> $(git log --oneline -1)"
fi

echo "==> building (cached, so usually quick)"
(cd codex-rs && cargo build --release --locked -p codex-cli --bin codex)

mkdir -p "$bin_dir"
install -m 755 codex-rs/target/release/codex "$bin_dir/qudiks"
echo "==> installed $bin_dir/qudiks"

# Refresh the model catalog too, but only if already signed in.
if "$bin_dir/qudiks" login github-copilot status >/dev/null 2>&1; then
  "$bin_dir/qudiks" login github-copilot setup >/dev/null 2>&1 \
    && echo "==> refreshed the model catalog" \
    || echo "==> could not refresh the model catalog (ignored)"
fi

echo "Done. Run: qudiks"

#!/usr/bin/env bash
# Pull the latest qudiks (even after a force-push) and reinstall it.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
bin_dir=${QUDIKS_BIN_DIR:-$HOME/.cargo/bin}
remote=${QUDIKS_REMOTE:-origin}
branch=fork-off
keep_local=""
debug=""
skip_update=""

usage() {
  cat <<'USAGE'
usage: ./install-qudiks.sh [options]

  --branch NAME      branch to track (default: fork-off)
  --keep-local       do not reset when local changes or commits exist
  --discard-local    reset to the remote (default)
  --skip-update      build this checkout without fetching or resetting
  --bin-dir DIR      where to install (default: ~/.cargo/bin)
  --debug            build the faster, larger debug binary instead

By default this resets to the remote branch, discarding local changes and commits.
USAGE
}

while (( $# )); do
  case $1 in
    --branch) branch=$2; shift 2 ;;
    --keep-local) keep_local=1; shift ;;
    --discard-local) keep_local=""; shift ;;
    --skip-update) skip_update=1; shift ;;
    --bin-dir) bin_dir=$2; shift 2 ;;
    --debug) debug=1; shift ;;
    --release) shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 1 ;;
  esac
done

cd "$repo_root"
command -v git >/dev/null || { echo "git not found" >&2; exit 1; }
command -v cargo >/dev/null || { echo "cargo not found; install rustup first" >&2; exit 1; }

if [[ -z $skip_update ]]; then
  echo "==> fetching $remote/$branch"
  git fetch --prune "$remote" "$branch"

  before=$(git rev-parse HEAD)
  local_only=$(git rev-list --left-only --cherry-pick --count "$before...FETCH_HEAD")
  dirty=$(git status --porcelain)
  if [[ -n $keep_local ]] && { [[ -n $dirty ]] || (( local_only > 0 )); }; then
    echo "==> local work found, installing the current tree without resetting it"
  else
    git reset --hard FETCH_HEAD
    after=$(git rev-parse HEAD)
    if [[ $before == "$after" ]]; then
      echo "==> already at $(git log --oneline -1)"
    else
      echo "==> updated $(git rev-parse --short "$before") -> $(git log --oneline -1)"
    fi
    update_args=(--skip-update --bin-dir "$bin_dir")
    [[ -n $debug ]] && update_args+=(--debug)
    exec "$repo_root/install-qudiks.sh" "${update_args[@]}"
  fi
fi

setup_args=(--skip-login --bin-dir "$bin_dir")
[[ -n $debug ]] && setup_args+=(--debug)
if ! "$bin_dir/qudiks-bin" login github-copilot status >/dev/null 2>&1; then
  setup_args=(--bin-dir "$bin_dir")
  [[ -n $debug ]] && setup_args+=(--debug)
fi
./setup-qudiks.sh "${setup_args[@]}"

# Refresh the model catalog too, but only if already signed in.
if "$bin_dir/qudiks" login github-copilot status >/dev/null 2>&1; then
  "$bin_dir/qudiks" login github-copilot setup >/dev/null 2>&1 \
    && echo "==> refreshed the model catalog" \
    || echo "==> could not refresh the model catalog (ignored)"
fi

echo "Done. Run: qudiks"

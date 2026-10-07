#!/usr/bin/env bash
# Re-authenticate to GitHub Copilot from scratch.
set -euo pipefail

bin=${QUDIKS_BIN:-$HOME/.cargo/bin/qudiks}
model=""
keep=""

usage() {
  cat <<'USAGE'
usage: ./login-qudiks.sh [--model MODEL] [--keep-model]

Clears stored credentials and runs the GitHub device flow again, then rewrites
the config and model catalog. Use when auth is stuck or your Copilot seat moved.
USAGE
}

while (( $# )); do
  case $1 in
    --model) model=$2; shift 2 ;;
    --keep-model) keep=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 1 ;;
  esac
done

[[ -x $bin ]] || { echo "qudiks not found at $bin; run ./setup-qudiks.sh first" >&2; exit 1; }

config=${QUDIKS_HOME:-$HOME/.qudiks}/config.toml
if [[ -z $model && -n $keep && -f $config ]]; then
  model=$(sed -n 's/^model = "\(.*\)"$/\1/p' "$config" | head -1)
  [[ -n $model ]] && echo "==> keeping model $model"
fi

echo "==> clearing stored credentials"
"$bin" login github-copilot logout || true

echo "==> signing in again"
args=(login github-copilot)
[[ -n $model ]] && args+=(--model "$model")
"$bin" "${args[@]}"

"$bin" login github-copilot status || true

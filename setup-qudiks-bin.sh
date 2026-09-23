#!/usr/bin/env bash
# Download and install the latest published Linux binary without Cargo.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
bin_dir=${QUDIKS_BIN_DIR:-$HOME/.cargo/bin}
setup_args=()

usage() {
  cat <<'USAGE'
usage: ./setup-qudiks-bin.sh [--bin-dir DIR] [--model MODEL] [--no-yolo]

Downloads the latest Linux x86_64 binary and installs Qudiks without building.
USAGE
}

while (( $# )); do
  case $1 in
    --bin-dir) bin_dir=$2; shift 2 ;;
    --model) setup_args+=(--model "$2"); shift 2 ;;
    --no-yolo) setup_args+=(--no-yolo); shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 1 ;;
  esac
done

[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || {
  echo "Only Linux x86_64 binaries are published; use ./setup-qudiks.sh to build from source." >&2
  exit 1
}
for command in curl tar sha256sum; do
  command -v "$command" >/dev/null || { echo "$command is required" >&2; exit 1; }
done

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
asset=qudiks-linux-x86_64.tar.gz
base=https://github.com/Viterkim/qudiks/releases/latest/download
echo "==> downloading latest Linux binary"
curl -fsSL --retry 3 -o "$tmp/$asset" "$base/$asset" || {
  echo "No Qudiks binary release is available yet. Use ./setup-qudiks.sh to build from source." >&2
  exit 1
}
curl -fsSL --retry 3 -o "$tmp/$asset.sha256" "$base/$asset.sha256"
(cd "$tmp" && sha256sum -c "$asset.sha256")
tar -xzf "$tmp/$asset" -C "$tmp" qudiks-bin
"$tmp/qudiks-bin" --version

signed_in=""
if [[ -x $bin_dir/qudiks-bin ]] && "$bin_dir/qudiks-bin" login github-copilot status >/dev/null 2>&1; then
  signed_in=1
  setup_args+=(--skip-login)
fi
"$repo_root/setup-qudiks.sh" --binary-path "$tmp/qudiks-bin" --bin-dir "$bin_dir" "${setup_args[@]}"

if [[ -n $signed_in ]]; then
  "$bin_dir/qudiks-bin" login github-copilot setup >/dev/null \
    && echo "==> refreshed the model catalog" \
    || echo "==> could not refresh the model catalog (ignored)"
fi

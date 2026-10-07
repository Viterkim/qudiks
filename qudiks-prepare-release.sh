#!/usr/bin/env bash
# Package a Linux release build for manual upload to GitHub.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
asset=qudiks-linux-x86_64.tar.gz
build=1
install_local=1
bin_dir=${QUDIKS_BIN_DIR:-$HOME/.cargo/bin}

usage() {
  cat <<'USAGE'
usage: ./qudiks-prepare-release.sh [--skip-build] [--skip-install] [--bin-dir DIR]

Builds the release binary, packages it into dist/, and installs it locally.
Pass --skip-build to package the existing release binary instead.
Pass --skip-install to leave the local installation unchanged.
The install directory defaults to QUDIKS_BIN_DIR or ~/.cargo/bin.
Existing credentials and launcher trust mode are kept; no login flow is started.
Upload the archive to a GitHub release.
USAGE
}

while (( $# )); do
  case $1 in
    --build) build=1; shift ;;
    --skip-build) build=""; shift ;;
    --skip-install) install_local=""; shift ;;
    --bin-dir) bin_dir=$2; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 1 ;;
  esac
done

[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || {
  echo "Only Linux x86_64 binaries are supported by this release script." >&2
  exit 1
}

cd "$repo_root"
command -v strip >/dev/null || { echo "strip is required to package the release binary" >&2; exit 1; }
if [[ -n $build ]]; then
  command -v cargo >/dev/null || { echo "cargo is required to build the release binary" >&2; exit 1; }
  echo "==> building release binary"
  (cd "$repo_root/codex-rs" && cargo build --locked -p codex-cli --bin codex --release)
fi
[[ -x codex-rs/target/release/codex ]] || {
  echo "Missing release binary; run ./qudiks-prepare-release.sh without --skip-build first." >&2
  exit 1
}

mkdir -p dist
install -m 755 codex-rs/target/release/codex dist/qudiks-bin
strip --strip-debug --strip-unneeded dist/qudiks-bin
tar -czf "dist/$asset" -C dist qudiks-bin -C "$repo_root" setup-qudiks.sh
if [[ -n $install_local ]]; then
  setup_args=(--binary-path "$repo_root/dist/qudiks-bin" --skip-login --bin-dir "$bin_dir")
  if [[ -f $bin_dir/qudiks ]] && ! grep -Fqx 'auto_trust=1' "$bin_dir/qudiks"; then
    setup_args+=(--no-yolo)
  fi
  bash "$repo_root/setup-qudiks.sh" "${setup_args[@]}"
fi
echo "Upload this file to your GitHub release:"
echo "  $repo_root/dist/$asset"

#!/usr/bin/env bash
# Package a Linux release build for manual upload to GitHub.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
asset=qudiks-linux-x86_64.tar.gz
build=""

usage() {
  cat <<'USAGE'
usage: ./qudiks-prepare-release.sh [--build]

Packages the existing release build into dist/. Pass --build to rebuild first.
Upload the archive to a GitHub release.
USAGE
}

while (( $# )); do
  case $1 in
    --build) build=1; shift ;;
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
  cargo build --locked --release -p codex-cli --bin codex --manifest-path codex-rs/Cargo.toml
fi
[[ -x codex-rs/target/release/codex ]] || {
  echo "Missing release binary; run ./qudiks-prepare-release.sh --build first." >&2
  exit 1
}

mkdir -p dist
install -m 755 codex-rs/target/release/codex dist/qudiks-bin
strip --strip-debug --strip-unneeded dist/qudiks-bin
tar -czf "dist/$asset" -C dist qudiks-bin
echo "Upload this file to your GitHub release:"
echo "  $repo_root/dist/$asset"

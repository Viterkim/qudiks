#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
asset=qudiks-linux-x86_64.tar.gz
build=1
install_local=""
bin_dir=${QUDIKS_BIN_DIR:-$HOME/.cargo/bin}

usage() {
  cat <<'USAGE'
usage: ./qudiks-release.sh [--skip-build] [--install] [--bin-dir DIR]

Builds from the current checkout and packages the release archive in dist/.
Pass --skip-build to package the existing release binary instead.
Pass --install to also install it in QUDIKS_BIN_DIR or ~/.cargo/bin.
USAGE
}

while (( $# )); do
  case $1 in
    --build) build=1; shift ;;
    --skip-build) build=""; shift ;;
    --install) install_local=1; shift ;;
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

for command in strip sha256sum; do
  command -v "$command" >/dev/null || { echo "$command is required to package the release" >&2; exit 1; }
done
if [[ -n $build ]]; then
  echo "==> building release binary"
  "$repo_root/qudiks-build.sh" --release
fi
built=$("$repo_root/qudiks-build.sh" --release --print-binary-path)
host_built=$(dirname "$built")/codex-code-mode-host
[[ -x $built && -x $host_built ]] || {
  echo "Missing release binary or code mode helper. Run ./qudiks-release.sh without --skip-build." >&2
  exit 1
}

dist=$repo_root/dist
mkdir -p "$dist"
stage=$(mktemp -d "$dist/.qudiks-release.XXXXXX")
trap 'rm -rf -- "$stage"' EXIT
install -m 755 "$built" "$stage/qudiks-bin"
install -m 755 "$host_built" "$stage/codex-code-mode-host"
strip --strip-debug --strip-unneeded "$stage/qudiks-bin" "$stage/codex-code-mode-host"
"$stage/qudiks-bin" --version

# Older Qudiks binaries look for this name when they update.
install -m 755 "$repo_root/qudiks-install.sh" "$stage/setup-qudiks.sh"
tar -czf "$stage/$asset" -C "$stage" qudiks-bin codex-code-mode-host setup-qudiks.sh \
  -C "$repo_root" qudiks-install.sh
(cd "$stage" && sha256sum "$asset" > "$asset.sha256")
mv -f -- "$stage/qudiks-bin" "$stage/codex-code-mode-host" "$stage/$asset" "$stage/$asset.sha256" "$dist/"

if [[ -n $install_local ]]; then
  "$repo_root/qudiks-install.sh" --binary-path "$dist/qudiks-bin" --skip-login --bin-dir "$bin_dir"
fi
printf '%s\n' "$dist/$asset"

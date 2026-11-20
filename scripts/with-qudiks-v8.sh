#!/usr/bin/env bash
set -euo pipefail

(( $# > 0 )) || { echo "usage: bash scripts/with-qudiks-v8.sh COMMAND [ARGS...]" >&2; exit 1; }

if [[ -n ${V8_FROM_SOURCE:-}${RUSTY_V8_ARCHIVE:-}${RUSTY_V8_SRC_BINDING_PATH:-}${RUSTY_V8_SRC_BINDING_URL:-}${RUSTY_V8_MIRROR:-}${RUSTY_V8_MIRROR_TAG:-} ]]; then
  exec "$@"
fi

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)
version=$(awk '$0 == "name = \"v8\"" { found = 1; next } found && /^version = / { gsub(/"/, "", $3); print $3; exit }' "$repo_root/codex-rs/Cargo.lock")
target=${CARGO_BUILD_TARGET:-$(cd "$repo_root/codex-rs" && rustc -vV | sed -n 's/^host: //p')}
checksums=$repo_root/third_party/v8/rusty_v8_${version//./_}.sha256
cache_dir=${XDG_CACHE_HOME:-$HOME/.cache}/qudiks/rusty-v8/$version/$target
profile=ptrcomp_sandbox_release
if [[ $target == *-pc-windows-msvc ]]; then
  archive=rusty_v8_${profile}_${target}.lib.gz
else
  archive=librusty_v8_${profile}_${target}.a.gz
fi
binding=src_binding_${profile}_${target}.rs

[[ -s $checksums ]] || { echo "Missing trusted V8 checksums: $checksums" >&2; exit 1; }
for asset in "$archive" "$binding"; do
  awk -v name="$asset" '$2 == name { found = 1 } END { exit !found }' "$checksums" || {
    echo "No verified V8 sandbox build for $target. Configure a matching archive and binding, or set V8_FROM_SOURCE=1." >&2
    exit 1
  }
done

if command -v sha256sum >/dev/null 2>&1; then
  checksum_command=(sha256sum --check --status)
else
  checksum_command=(shasum -a 256 --check --status)
fi
mkdir -p "$cache_dir"
download=""
trap '[[ -z $download ]] || rm -- "$download"' EXIT
for asset in "$archive" "$binding"; do
  expected=$(awk -v name="$asset" '$2 == name { print $1 }' "$checksums")
  if [[ -f $cache_dir/$asset ]] && printf '%s  %s\n' "$expected" "$cache_dir/$asset" | "${checksum_command[@]}"; then
    continue
  fi

  download=$(mktemp "$cache_dir/.download.XXXXXX")
  echo "==> downloading verified V8 sandbox asset: $asset" >&2
  curl -fsSL --retry 3 "https://github.com/openai/codex/releases/download/rusty-v8-v$version/$asset" -o "$download"
  printf '%s  %s\n' "$expected" "$download" | "${checksum_command[@]}"
  mv -f "$download" "$cache_dir/$asset"
  download=""
done

export RUSTY_V8_ARCHIVE=$cache_dir/$archive
export RUSTY_V8_SRC_BINDING_PATH=$cache_dir/$binding
trap - EXIT
exec "$@"

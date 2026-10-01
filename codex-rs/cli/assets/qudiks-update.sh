#!/usr/bin/env bash
set -euo pipefail

bin_dir=$1
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || {
  echo "Only Linux x86_64 binary updates are supported." >&2
  exit 1
}
for command in curl tar bash; do
  command -v "$command" >/dev/null || { echo "$command is required" >&2; exit 1; }
done
[[ -d $bin_dir && -w $bin_dir ]] || {
  echo "Install directory is not writable: $bin_dir" >&2
  exit 1
}

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
asset=qudiks-linux-x86_64.tar.gz
base=https://github.com/Viterkim/qudiks/releases/latest/download
echo "==> downloading latest Qudiks binary and launcher"
curl -fsSL --retry 3 -o "$tmp/$asset" "$base/$asset"
tar -xzf "$tmp/$asset" -C "$tmp" qudiks-bin setup-qudiks.sh || {
  echo "This release does not include the launcher installer. Ask for a new release or use ./install-qudiks-bin.sh from a checkout." >&2
  exit 1
}
[[ -x $tmp/qudiks-bin && -f $tmp/setup-qudiks.sh ]] || {
  echo "Release is missing the binary or launcher installer." >&2
  exit 1
}
"$tmp/qudiks-bin" --version

setup_args=(--skip-login --no-yolo)
if [[ -f $bin_dir/qudiks ]] && grep -Fqx 'auto_trust=1' "$bin_dir/qudiks"; then
  setup_args=(--skip-login --yolo)
fi
bash "$tmp/setup-qudiks.sh" --binary-path "$tmp/qudiks-bin" --bin-dir "$bin_dir" "${setup_args[@]}"

#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
bin_dir=${QUDIKS_BIN_DIR:-$HOME/.cargo/bin}
model=""
yolo=1
skip_build=""
skip_login=""
profile=release
binary_path=""
download=""
download_dir=""
stage=""

usage() {
  cat <<'USAGE'
usage: ./qudiks-install.sh [options]

  --model MODEL         model to use (default: GPT-6.2 Sol xhigh if available)
  --no-yolo             keep approval prompts and the sandbox
  --yolo                default. no approval prompts, no sandbox
  --bin-dir DIR         where to install (default: ~/.cargo/bin)
  --debug               build the faster, larger debug binary instead
  --release             build the optimized binary (default)
  --download            install the latest published Linux release
  --binary-path FILE    install a binary and its adjacent code mode helper
  --skip-build          use an existing binary for the selected build profile
  --skip-login          skip sign-in when no credentials are saved
USAGE
}

while (( $# )); do
  case $1 in
    --model) model=$2; shift 2 ;;
    --yolo) yolo=1; shift ;;
    --no-yolo) yolo=""; shift ;;
    --bin-dir) bin_dir=$2; shift 2 ;;
    --debug) profile=debug; shift ;;
    --release) profile=release; shift ;;
    --download) download=1; shift ;;
    --binary-path) binary_path=$2; shift 2 ;;
    --skip-build) skip_build=1; shift ;;
    --skip-login) skip_login=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 1 ;;
  esac
done

cleanup() {
  [[ -z $stage ]] || rm -rf -- "$stage"
  [[ -z $download_dir ]] || rm -rf -- "$download_dir"
}
trap cleanup EXIT

if [[ -n $download ]]; then
  [[ -z $binary_path && -z $skip_build ]] || {
    echo "--download cannot be combined with --binary-path or --skip-build" >&2
    exit 1
  }
  [[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || {
    echo "Only Linux x86_64 binaries are published. Run ./qudiks-install.sh to build from source." >&2
    exit 1
  }
  for command in curl tar; do
    command -v "$command" >/dev/null || { echo "$command is required" >&2; exit 1; }
  done

  download_dir=$(mktemp -d)
  asset=qudiks-linux-x86_64.tar.gz
  echo "==> downloading the latest Qudiks release"
  curl -fsSL --retry 3 -o "$download_dir/$asset" "https://github.com/Viterkim/qudiks/releases/latest/download/$asset"
  tar -xzf "$download_dir/$asset" -C "$download_dir" qudiks-bin codex-code-mode-host
  binary_path=$download_dir/qudiks-bin
fi

wrapper=$bin_dir/qudiks
real_bin=$bin_dir/qudiks-bin

if [[ -n $yolo ]]; then
  echo "Yolo mode is on: no approval prompts, no sandbox. Pass --no-yolo to keep them."
  echo
fi

if [[ -n $binary_path ]]; then
  [[ -z $skip_build ]] || { echo "--binary-path cannot be combined with --skip-build" >&2; exit 1; }
  built=$binary_path
else
  build_args=("--$profile")
  if [[ -z $skip_build ]]; then
    "$repo_root/qudiks-build.sh" "${build_args[@]}"
  fi
  built=$("$repo_root/qudiks-build.sh" "${build_args[@]}" --print-binary-path)
fi

[[ -x $built ]] || { echo "missing executable $built" >&2; exit 1; }
host_built=$(dirname "$built")/codex-code-mode-host
[[ -x $host_built ]] || {
  echo "Missing code-mode helper: $host_built. Rebuild both binaries or download a release that includes it." >&2
  exit 1
}

echo "==> installing to $bin_dir"
mkdir -p "$bin_dir"
bin_dir=$(cd "$bin_dir" && pwd -P)
wrapper=$bin_dir/qudiks
real_bin=$bin_dir/qudiks-bin
stage=$(mktemp -d "$bin_dir/.qudiks-install.XXXXXX")
staged_bin=$stage/qudiks-bin
staged_host=$stage/codex-code-mode-host
staged_wrapper=$stage/qudiks
if [[ -z $binary_path && $profile == release ]]; then
  install -s -m 755 "$built" "$staged_bin"
  install -s -m 755 "$host_built" "$staged_host"
else
  install -m 755 "$built" "$staged_bin"
  install -m 755 "$host_built" "$staged_host"
fi
mv -f "$staged_host" "$bin_dir/codex-code-mode-host"
mv -f "$staged_bin" "$real_bin"

signed_in=""
if "$real_bin" login github-copilot status >/dev/null 2>&1; then
  signed_in=1
fi
if [[ -z $skip_login && -z $signed_in ]]; then
  echo "==> signing in to GitHub Copilot"
  login_args=(login github-copilot)
  [[ -n $model ]] && login_args+=(--model "$model")
  "$real_bin" "${login_args[@]}"
elif [[ -n $model ]]; then
  "$real_bin" login github-copilot --model "$model" setup
elif [[ -n $signed_in ]]; then
  "$real_bin" login github-copilot setup \
    || echo "Could not refresh the model catalog; run 'qudiks login github-copilot setup' before starting a new session." >&2
fi

if [[ -n $yolo ]]; then
  qudiks_home=${QUDIKS_HOME:-${CODEX_HOME:-$HOME/.qudiks}}
  config=$qudiks_home/config.toml
  echo "==> enabling yolo mode in $config"
  mkdir -p "$qudiks_home"
  touch "$config"
  root_config=$(sed '/^[[:space:]]*\[/,$d' "$config")
  root_settings=""
  if ! grep -q '^approval_policy[[:space:]]*=' <<< "$root_config"; then
    root_settings+='approval_policy = "never"'$'\n'
  fi
  if ! grep -q '^sandbox_mode[[:space:]]*=' <<< "$root_config"; then
    root_settings+='sandbox_mode = "danger-full-access"'$'\n'
  fi
  if [[ -n $root_settings ]]; then
    updated_config=$(mktemp)
    printf '%s' "$root_settings" > "$updated_config"
    cat "$config" >> "$updated_config"
    cat "$updated_config" > "$config"
    rm "$updated_config"
  fi
fi

echo "==> writing auto-trusting launcher to $wrapper"
{
  cat <<'HEAD'
#!/usr/bin/env bash
# Trusts the active working tree before handing off to the real binary.
set -euo pipefail

HEAD
  printf "real_qudiks=\${QUDIKS_BIN:-%q}\n" "$real_bin"
  if [[ -n $yolo ]]; then
    echo 'auto_trust=1'
  else
    echo 'auto_trust=""'
  fi
  cat <<'TAIL'

defaults=(-c 'tui.alternate_screen="auto"' -c 'tui.fullscreen_transcript=true')

if [[ -z $auto_trust ]]; then
  exec "$real_qudiks" "${defaults[@]}" "$@"
fi

defaults+=(-c 'approval_policy="never"' -c 'sandbox_mode="danger-full-access"')

workdir=$PWD
args=("$@")
for ((index = 0; index < ${#args[@]}; index++)); do
  case "${args[index]}" in
    --) break ;;
    -C|--cd)
      if (( index + 1 < ${#args[@]} )); then
        workdir=${args[index + 1]}
        ((index += 1))
      fi
      ;;
    -C?*)
      workdir=${args[index]#-C}
      ;;
    --cd=*)
      workdir=${args[index]#--cd=}
      ;;
  esac
done

workdir_abs=$(cd "$workdir" 2>/dev/null && pwd -P) || workdir_abs=$PWD

seen_paths=":"
trust_entries=""

add_trust_entry() {
  local path=$1 escaped
  case "$seen_paths" in
    *:"$path":*) return ;;
  esac
  seen_paths+="$path:"
  escaped=${path//\\/\\\\}
  escaped=${escaped//\"/\\\"}
  [[ -z $trust_entries ]] || trust_entries+=", "
  trust_entries+="\"$escaped\" = { trust_level = \"trusted\" }"
}

add_trust_entry "$workdir_abs"
if git_root=$(git -C "$workdir_abs" rev-parse --show-toplevel 2>/dev/null); then
  if git_root_abs=$(cd "$git_root" 2>/dev/null && pwd -P); then
    add_trust_entry "$git_root_abs"
  fi
fi

exec "$real_qudiks" "${defaults[@]}" -c "projects = { $trust_entries }" "$@"
TAIL
} > "$staged_wrapper"
chmod +x "$staged_wrapper"
mv -f "$staged_wrapper" "$wrapper"

cat <<DONE

Done. qudiks is at $wrapper

  qudiks                  start a session
  qudiks login github-copilot models   list models your account can use

DONE

case ":$PATH:" in
  *":$bin_dir:"*) ;;
  *)
    cat <<DONE
Add this directory to PATH using your shell's normal configuration:

  $bin_dir

Then open a new shell and run: qudiks

DONE
    ;;
esac

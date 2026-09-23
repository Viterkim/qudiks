#!/usr/bin/env bash
# Build qudiks, sign in to GitHub Copilot, and generate a wrapper.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
bin_dir=${QUDIKS_BIN_DIR:-$HOME/.cargo/bin}
model=""
yolo=1
skip_build=""
skip_login=""
profile=release
binary_path=""

usage() {
  cat <<'USAGE'
usage: ./setup-qudiks.sh [options]

  --model MODEL         model to use (default: GPT-6 Sol medium if available)
  --no-yolo             keep approval prompts and the sandbox
  --yolo                default. no approval prompts, no sandbox
  --bin-dir DIR         where to install (default: ~/.cargo/bin)
  --debug               build the faster, larger debug binary instead
  --release             build the optimized binary (default)
  --binary-path FILE    install a downloaded binary; skip Cargo entirely
  --skip-build          use an existing binary for the selected build profile
  --skip-login          keep existing credentials
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
    --binary-path) binary_path=$2; shift 2 ;;
    --skip-build) skip_build=1; shift ;;
    --skip-login) skip_login=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 1 ;;
  esac
done

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
  if [[ -z $skip_build ]]; then
    command -v cargo >/dev/null || { echo "cargo not found; install rustup first" >&2; exit 1; }
    echo "==> building $profile binary (first build may take a while)"
    cargo_args=()
    [[ $profile == release ]] && cargo_args+=(--release)
    # --locked is required: without it cargo resolves rama-error to a version that
    # no longer has OpaqueError and rama-core fails to compile.
    (cd "$repo_root/codex-rs" && cargo build --locked -p codex-cli --bin codex "${cargo_args[@]}")
  fi
  built=$repo_root/codex-rs/target/$profile/codex
fi

[[ -x $built ]] || { echo "missing executable $built" >&2; exit 1; }

echo "==> installing to $bin_dir"
mkdir -p "$bin_dir"
if [[ -z $binary_path && $profile == release ]]; then
  install -s -m 755 "$built" "$real_bin"
else
  install -m 755 "$built" "$real_bin"
fi

if [[ -z $skip_login ]]; then
  echo "==> signing in to GitHub Copilot"
  login_args=(login github-copilot)
  [[ -n $model ]] && login_args+=(--model "$model")
  "$real_bin" "${login_args[@]}"
elif [[ -n $model ]]; then
  "$real_bin" login github-copilot --model "$model" setup
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

real_qudiks=${QUDIKS_BIN:-BIN_DIR_PLACEHOLDER/qudiks-bin}
HEAD
  if [[ -n $yolo ]]; then
    echo 'auto_trust=1'
  else
    echo 'auto_trust=""'
  fi
  cat <<'TAIL'

if [[ -z $auto_trust ]]; then
  exec "$real_qudiks" "$@"
fi

workdir=$PWD
args=("$@")
for ((index = 0; index < ${#args[@]}; index++)); do
  case "${args[index]}" in
    -C|--cd)
      if (( index + 1 < ${#args[@]} )); then
        workdir=${args[index + 1]}
      fi
      ;;
    --cd=*)
      workdir=${args[index]#--cd=}
      ;;
  esac
done

workdir_abs=$(cd "$workdir" 2>/dev/null && pwd -P) || workdir_abs=$PWD

seen_paths=":"
qudiks_home=${QUDIKS_HOME:-${CODEX_HOME:-$HOME/.qudiks}}
config=$qudiks_home/config.toml
mkdir -p "$qudiks_home"
touch "$config"

add_trust_entry() {
  local path=$1 escaped section
  case "$seen_paths" in
    *:"$path":*) return ;;
  esac
  seen_paths+="$path:"
  escaped=${path//\\/\\\\}
  escaped=${escaped//\"/\\\"}
  section="[projects.\"$escaped\"]"
  if ! grep -Fqx -- "$section" "$config"; then
    printf '\n%s\ntrust_level = "trusted"\n' "$section" >> "$config"
  fi
}

add_trust_entry "$workdir_abs"
if git_root=$(git -C "$workdir_abs" rev-parse --show-toplevel 2>/dev/null); then
  if git_root_abs=$(cd "$git_root" 2>/dev/null && pwd -P); then
    add_trust_entry "$git_root_abs"
  fi
fi

exec "$real_qudiks" "$@"
TAIL
} | sed "s|BIN_DIR_PLACEHOLDER|$bin_dir|" > "$wrapper"
chmod +x "$wrapper"

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

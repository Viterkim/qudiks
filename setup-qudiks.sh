#!/usr/bin/env bash
# Build qudiks, sign in to GitHub Copilot, and generate a wrapper.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)
bin_dir=${QUDIKS_BIN_DIR:-$HOME/.cargo/bin}
wrapper=$repo_root/qudiks-wrapper
model=""
yolo=1
trusted_roots=()
skip_build=""
skip_login=""

usage() {
  cat <<'USAGE'
usage: ./setup-qudiks.sh [options]

  --model MODEL         model to use (default: best your account offers)
  --trusted-root PATH   auto-trust this dir and everything under it (repeatable)
  --no-yolo             keep approval prompts and the sandbox
  --yolo                default. no approval prompts, no sandbox
  --bin-dir DIR         where to install (default: ~/.cargo/bin)
  --skip-build          use an existing target/release/qudiks
  --skip-login          keep existing credentials
USAGE
}

while (( $# )); do
  case $1 in
    --model) model=$2; shift 2 ;;
    --trusted-root) trusted_roots+=("$2"); shift 2 ;;
    --yolo) yolo=1; shift ;;
    --no-yolo) yolo=""; shift ;;
    --bin-dir) bin_dir=$2; shift 2 ;;
    --skip-build) skip_build=1; shift ;;
    --skip-login) skip_login=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown option: $1" >&2; usage >&2; exit 1 ;;
  esac
done

command -v cargo >/dev/null || { echo "cargo not found; install rustup first" >&2; exit 1; }

# Ask when run interactively, so nobody has to read --help first.
if [[ -t 0 && ${#trusted_roots[@]} -eq 0 ]]; then
  echo "Trusted roots: directories qudiks should never ask permission in."
  echo "Space separated, blank to skip. Example: ~/code ~/dotfiles"
  read -r -p "> " -a answer || answer=()
  for root in ${answer+"${answer[@]}"}; do
    root=${root/#\~/$HOME}
    trusted_roots+=("$(cd "$root" 2>/dev/null && pwd -P || echo "$root")")
  done
  echo
fi

if [[ -n $yolo ]]; then
  echo "Yolo mode is on: no approval prompts, no sandbox. Pass --no-yolo to keep them."
  echo
fi

if [[ -z $skip_build ]]; then
  echo "==> building (first build takes ~10 min)"
  # --locked is required: without it cargo resolves rama-error to a version that
  # no longer has OpaqueError and rama-core fails to compile.
  (cd "$repo_root/codex-rs" && cargo build --release --locked -p codex-cli --bin codex)
fi

built=$repo_root/codex-rs/target/release/codex
[[ -x $built ]] || { echo "missing $built; drop --skip-build" >&2; exit 1; }

echo "==> installing to $bin_dir"
mkdir -p "$bin_dir"
install -m 755 "$built" "$bin_dir/qudiks"

if [[ -z $skip_login ]]; then
  echo "==> signing in to GitHub Copilot"
  login_args=(login github-copilot)
  [[ -n $model ]] && login_args+=(--model "$model")
  "$bin_dir/qudiks" "${login_args[@]}"
elif [[ -n $model ]]; then
  "$bin_dir/qudiks" login github-copilot --model "$model" setup
fi

if [[ -n $yolo ]]; then
  qudiks_home=${QUDIKS_HOME:-$HOME/.qudiks}
  config=$qudiks_home/config.toml
  echo "==> enabling yolo mode in $config"
  mkdir -p "$qudiks_home"
  touch "$config"
  grep -q '^approval_policy' "$config" \
    || printf 'approval_policy = "never"\n' >> "$config"
  grep -q '^sandbox_mode' "$config" \
    || printf 'sandbox_mode = "danger-full-access"\n' >> "$config"
fi

echo "==> writing $wrapper"
{
  cat <<'HEAD'
#!/usr/bin/env bash
# Marks configured roots as trusted before handing off to qudiks.
set -euo pipefail

real_qudiks=${QUDIKS_BIN:-BIN_DIR_PLACEHOLDER/qudiks}

trusted_roots=(
HEAD
  for root in "${trusted_roots[@]}"; do printf '  "%s"\n' "$root"; done
  cat <<'TAIL'
)

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

is_under_trusted_root() {
  local candidate=$1 root
  for root in ${trusted_roots+"${trusted_roots[@]}"}; do
    case "$candidate" in
      "$root"|"$root"/*) return 0 ;;
    esac
  done
  return 1
}

overrides=()
seen_paths=":"

add_trust_override() {
  local path=$1 escaped
  case "$seen_paths" in
    *:"$path":*) return ;;
  esac
  seen_paths+="$path:"
  escaped=${path//\\/\\\\}
  escaped=${escaped//\"/\\\"}
  overrides+=(-c "projects.\"$escaped\".trust_level=\"trusted\"")
}

if is_under_trusted_root "$workdir_abs"; then
  add_trust_override "$workdir_abs"
  if git_root=$(git -C "$workdir_abs" rev-parse --show-toplevel 2>/dev/null); then
    if git_root_abs=$(cd "$git_root" 2>/dev/null && pwd -P) && is_under_trusted_root "$git_root_abs"; then
      add_trust_override "$git_root_abs"
    fi
  fi
fi

exec "$real_qudiks" ${overrides+"${overrides[@]}"} "$@"
TAIL
} | sed "s|BIN_DIR_PLACEHOLDER|$bin_dir|" > "$wrapper"
chmod +x "$wrapper"

cat <<DONE

Done. qudiks is at $bin_dir/qudiks

  qudiks                  start a session
  qudiks login github-copilot models   list models your account can use

A wrapper was written to:

  $wrapper

DONE

if (( ${#trusted_roots[@]} )); then
  cat <<DONE
It auto-trusts these roots so you are not prompted inside them:
$(printf '  %s\n' "${trusted_roots[@]}")

Move it somewhere on your PATH *before* $bin_dir, then use "qudiks" as normal:

  mv $wrapper ~/bin/qudiks

DONE
else
  cat <<DONE
No trusted roots were configured, so it just forwards to qudiks. Edit the
trusted_roots array in it, or re-run with --trusted-root PATH.

Move it somewhere on your PATH *before* $bin_dir to use it.

DONE
fi

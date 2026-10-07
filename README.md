# Qudiks

A small fork of [Codex](https://github.com/openai/codex) that adds GitHub
Copilot as a native provider, so one binary can use your Copilot seat instead of
an OpenAI subscription.

No proxy, no Node sidecar. Copilot auth, model discovery and the Copilot-specific
request quirks are handled in Rust inside the normal Codex request path.

## Install

The binary install is recommended for Linux x86_64 on a compatible glibc-based
distribution, no Rust toolchain needed
(requires Git, curl, tar, and a published binary release):

```shell
git clone --depth 1 --filter=blob:none --sparse --single-branch --branch fork-off https://github.com/Viterkim/qudiks.git qudiks && cd qudiks && ./setup-qudiks-bin.sh
```

Or build the release binary yourself (requires rustup, a C toolchain, OpenSSL
headers and pkg-config):

```shell
git clone --depth 1 --single-branch --branch fork-off https://github.com/Viterkim/qudiks.git qudiks && cd qudiks && ./setup-qudiks.sh
```

Both install `qudiks` into `~/.cargo/bin`, sign you in via GitHub's device
flow, and select GPT-6.1 Sol with high reasoning when your Copilot seat
offers it. If not available, Qudiks picks another Responses-capable model.
Already signed-in updates keep the selected model.
Both installers enable YOLO mode by default; pass `--no-yolo` to
`setup-qudiks.sh` to keep confirmation prompts and the sandbox.
The launcher automatically trusts the working tree you start it in, so there is
no folder setup. It runs with the embedded server because this single binary
install does not include Codex's shared background server. You need your own
Copilot seat.

If `~/.cargo/bin` is not already on `PATH`, the installer prints the directory
to add using whichever shell configuration you prefer.

Source builds use the release profile by default; `--debug` is available for
local iteration. The first release build takes longer and consumes more disk.

```shell
qudiks                                        # start a session
qudiks login github-copilot models            # what your account can use
qudiks login github-copilot --model X setup   # switch model
```

## Updating

Run `/update` inside Qudiks, or `qudiks update` from any directory. After
confirmation, `/update` exits and installs the latest published Linux x86_64
binary and launcher beside the running binary. It keeps your credentials,
settings and launcher trust mode; no clone is needed, and your checkout is
untouched. Relaunch `qudiks` afterward. This also replaces installed source
builds with the published binary; use the source updater below to keep building
from source. The release archive must be repackaged with `qudiks-prepare-release.sh`
to include the launcher installer; older binary-only archives cannot self-update.

For the recommended binary install:

```shell
./install-qudiks-bin.sh
```

For a source build:

```shell
./install-qudiks.sh
```

Source builds automatically expand sparse checkouts made by the binary install.
Run `./login-qudiks.sh` to re-auth if credentials break.

Both update commands fetch `fork-off` and reset to it, even after a force-push.
Both save tracked and untracked changes in Git stash and local-only commits on
a backup branch before resetting. They print the exact stash restore command;
backups are not automatically reapplied. Both refuse to update during a merge
or rebase. To install your current work without resetting it, use
`./install-qudiks.sh --keep-local`, or run `./qudiks-prepare-release.sh` to build,
install, and package it for upload. Until a binary release exists, use the source
installer.

## Prepare a Linux binary

On Linux x86_64:

```shell
./qudiks-prepare-release.sh
```

This builds the release binary, packages it, and installs that same binary and
launcher locally without rebuilding or signing in again. Pass `--skip-install`
to only prepare the archive, or `--skip-build` to reuse an existing build.
The install directory defaults to `~/.cargo/bin`; `--bin-dir` or `QUDIKS_BIN_DIR`
overrides it. Existing launcher trust mode is kept. The script prints the full path to
`qudiks-linux-x86_64.tar.gz`. Create a GitHub release once, drag the archive
onto it, publish it, and mark it as **Latest**. For later updates, delete the
old asset from that release and drag in the new archive with the same name.
No new tag or CLI is needed; the binary installer downloads the Latest asset.
The release's original tag still points at its original commit, even after
replacing the binary.
Releases built on newer glibc distributions may not run on older ones.

## Notes

Config lives in `~/.qudiks`, not `~/.codex`, so this cannot disturb a real Codex
install. `QUDIKS_HOME` overrides it.

Codex only speaks the Responses API. Most of a Copilot catalog is chat-only and
unusable; model selection filters to what actually works.

See `plan.txt` for the Copilot quirks worth knowing about and what is still
unfinished.

## Inspired by

- [Codex](https://github.com/openai/codex)
- [hk-vk/codexpilot](https://github.com/hk-vk/codexpilot)
- [GaussianGuaicai/Codex-For-Copilot](https://github.com/GaussianGuaicai/Codex-For-Copilot)

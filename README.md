# Qudiks

A small fork of [Codex](https://github.com/openai/codex) that adds GitHub
Copilot as a native provider, so one binary can use your Copilot seat instead of
an OpenAI subscription.

No proxy, no Node sidecar. Copilot auth, model discovery and the Copilot-specific
request quirks are handled in Rust inside the normal Codex request path.

## Install

Linux x86_64 on a compatible glibc-based distribution, no Rust toolchain needed
(requires Git, curl, tar, sha256sum, and a published binary release):

```shell
git clone --depth 1 --filter=blob:none --sparse --single-branch --branch fork-off https://github.com/Viterkim/qudiks.git qudiks && cd qudiks && ./setup-qudiks-bin.sh
```

Or build the release binary yourself (requires rustup, a C toolchain, OpenSSL
headers and pkg-config):

```shell
git clone --depth 1 --single-branch --branch fork-off https://github.com/Viterkim/qudiks.git qudiks && cd qudiks && ./setup-qudiks.sh
```

Both install `qudiks` into `~/.cargo/bin`, sign you in via GitHub's device
flow, and select GPT-6 Sol with medium reasoning when your Copilot seat
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

```shell
./install-qudiks-bin.sh # reset to fork-off, download and install latest Linux binary
./install-qudiks.sh     # reset to fork-off, rebuild release and reinstall
./login-qudiks.sh       # re-auth if credentials break
```

Both update commands fetch `fork-off` and reset to it, even after a force-push.
**They discard local changes and local commits in this checkout.** For local
changes, use `./install-qudiks.sh --keep-local`, or run either `./setup-qudiks.sh`
or `./setup-qudiks-bin.sh` without updating. Until a binary release exists, use
the source installer.

## Prepare a Linux binary

On Linux x86_64, after building the release binary:

```shell
./qudiks-prepare-release.sh              # package existing binary
```

Pass `--build` to build it first. The script prints the full paths to
`qudiks-linux-x86_64.tar.gz` and `qudiks-linux-x86_64.tar.gz.sha256`.
Create a GitHub release targeting your pushed `fork-off` commit, drag both
files onto the release, publish it, and mark it as **Latest**. The binary
installer downloads that release and checks its SHA-256 before installing.
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

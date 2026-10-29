# Qudiks

A small fork of [Codex](https://github.com/openai/codex) that lets you use your
GitHub Copilot seat instead of an OpenAI subscription. No proxy or Node sidecar.

## Install

On Linux x86_64 with glibc, install the published binary with Git, curl and tar:

```shell
git clone --depth 1 --filter=blob:none --sparse --single-branch --branch fork-off https://github.com/Viterkim/qudiks.git qudiks && cd qudiks && ./setup-qudiks-bin.sh
```

Or build from source with rustup, a C toolchain, OpenSSL headers and pkg-config:

```shell
git clone --depth 1 --single-branch --branch fork-off https://github.com/Viterkim/qudiks.git qudiks && cd qudiks && ./setup-qudiks.sh
```

Both install into `~/.cargo/bin` and sign you in through GitHub. New installs
pick GPT-6.1 Sol with high reasoning when available.

YOLO mode is on by default, with no approval prompts or sandbox, and the launcher
trusts the working tree you start it in. Pass `--no-yolo` to either setup script
to keep those protections. Add `~/.cargo/bin` to `PATH` if your shell needs it.

Source builds use release mode. Pass `--debug` for faster local builds.

```shell
qudiks                                        # start a session
qudiks login github-copilot models            # what your account can use
qudiks login github-copilot --model X setup    # switch model
```

Config, credentials and sessions live in `~/.qudiks`, apart from regular Codex.
Home overrides are `QUDIKS_HOME`, then `CODEX_HOME`.

Use the OpenAI API with your own key, or try a local llama.cpp server:

```shell
qudiks --openai-api
qudiks --codex
qudiks resume --last --openai-api
qudiks --endpoint http://127.0.0.1:8080
qudiks --endpoint http://127.0.0.1:8080 -m your-model
qudiks --github
```

OpenAI mode asks for a key on first launch, shows where to create one, and saves
it in your active Qudiks config. You can also set `OPENAI_API_KEY`. The model
picker shows supported models available to that key. `/usage` shows session token
counts and links to API usage and credit balances; `/status` identifies API mode.

`--codex` uses ChatGPT sign-in and your normal Codex subscription allowance.
It opens the sign-in screen when needed and keeps the login in your Qudiks home.
You can also sign in first with `qudiks login`. This mode uses Codex's model list
and subscription usage display; `--openai-api` uses your API credits.

`--endpoint` discovers models from `/v1/models` and uses streaming Chat Completions
with function tools. The server and model need tool-calling support. A base URL
without a path gets `/v1` appended. An endpoint key, when needed, goes in config:

```toml
[model_providers.endpoint]
name = "Custom endpoint"
experimental_bearer_token = "your-endpoint-key"
```

Plain `qudiks` keeps your configured provider, normally GitHub Copilot. These
flags select a provider for that invocation and also work with `exec` and `fork`.

## Updating

Run `/update` inside Qudiks, or `qudiks update` in a terminal, to install the
latest published Linux binary. Your settings, credentials and launcher trust
mode stay, and your checkout is untouched. Relaunch afterward. This also
replaces source builds with the published binary.

To update a checkout instead:

```shell
./install-qudiks-bin.sh   # download the published binary
./install-qudiks.sh       # build from source
```

Both scripts fetch `fork-off` and reset to it, saving tracked and untracked
changes in Git stash and local commits on a backup branch first. Backups are
not reapplied automatically. Use `./install-qudiks.sh --keep-local` to build your
current work without resetting it.

Run `./login-qudiks.sh` to sign in again when credentials stop working.

## Prepare a Linux binary

On Linux x86_64:

```shell
./qudiks-prepare-release.sh
```

This builds, packages and installs Qudiks with its code mode helper and launcher.
Pass `--skip-install` to only package it, or `--skip-build` to reuse a build.
Existing credentials and launcher trust mode stay.

Upload `dist/qudiks-linux-x86_64.tar.gz` to a GitHub release marked Latest.
Later, replace that asset with an archive of the same name. The installer picks
it up without a new tag, though the release tag still points at its original
commit. Builds from newer glibc distributions may not run on older ones.

## Inspired by

- [Codex](https://github.com/openai/codex)
- [hk-vk/codexpilot](https://github.com/hk-vk/codexpilot)
- [GaussianGuaicai/Codex-For-Copilot](https://github.com/GaussianGuaicai/Codex-For-Copilot)

# Qudiks

A fork of [Codex](https://github.com/openai/codex) with GitHub Copilot support.
You can also use a ChatGPT subscription, OpenAI API credits, or a local model server.

## Install

For Linux x86_64 with glibc. Needs Bash, Git, curl and tar.

```shell
bash -c 'cd "$(mktemp -d)" && git clone --depth 1 --filter=blob:none --sparse -b fork-off https://github.com/Viterkim/qudiks.git . && ./qudiks-install.sh --download'
```

Installs the latest release into `~/.cargo/bin` and opens GitHub sign-in.
New Copilot installs use GPT-6.2 Sol at `xhigh` when available.
Add `~/.cargo/bin` to your `PATH` if needed.

YOLO is on by default, so commands run without approval prompts or a sandbox.
Add `--no-yolo` to the install command to keep those protections.

## Choose a provider

```shell
qudiks --github                         # GitHub Copilot
qudiks --codex                          # ChatGPT subscription
qudiks --openai-api                     # OpenAI API credits
qudiks --claude-api                     # Claude API, defaults to Opus 5.5
qudiks --endpoint http://localhost:8080  # llama.cpp or another local server
```

Plain `qudiks` uses your configured provider, normally GitHub Copilot.
These flags select a provider for that launch and also work with `resume`, `fork`
and `exec`.

GitHub Copilot needs a Copilot seat. List your available models or refresh the
catalog after new ones become available:

```shell
qudiks login github-copilot models
qudiks login github-copilot setup
```

`--codex` opens ChatGPT sign-in when needed. `--openai-api` asks for an API key
and saves it, or reads `OPENAI_API_KEY` from your environment.
`--claude-api` does the same with `ANTHROPIC_API_KEY`. Use `-m claude-sonnet-5-5`
to select another Claude model. Pick a reasoning level in `/model`, or start
with `qudiks --claude-api -c model_reasoning_effort=xhigh`.

`--endpoint` uses an OpenAI-compatible server with streaming Chat Completions
and tool calling. It discovers models from `/v1/models`, and a URL without a path
gets `/v1` appended. For servers that require a key, add this to your config:

```toml
[model_providers.endpoint]
experimental_bearer_token = "your-endpoint-key"
```

## Use it

```shell
qudiks -C ~/project                     # start in a directory
qudiks -m gemini-3.8-flash               # choose a model for this session
qudiks -c model_reasoning_effort=high    # choose reasoning effort
qudiks resume --last                    # continue the last session
qudiks fork --last                      # branch off the last session
qudiks exec "explain this project"       # run without the interactive UI
qudiks --help
```

Inside Qudiks:

- `/model` changes the model and reasoning effort.
- `/config` changes appearance, notifications and input settings. Changes save automatically.
- `/usage` shows usage for the active provider.
- `/usage toggle` shows or hides Copilot credits in the bottom right.
  Copilot setup turns it on by default. Use `/usage off` to hide it.
- `/status` shows the current session configuration.
- `/update` installs the latest release.

Tips are off by default. You can turn them on in `/config`.

Config, credentials, skills and sessions live in `~/.qudiks`.
Set `QUDIKS_HOME` to use another directory. `CODEX_HOME` is also accepted,
with `QUDIKS_HOME` taking precedence.

## Update

Run `/update` inside Qudiks or this in a terminal, then relaunch:

```shell
qudiks update
```

This installs the latest published Linux binary and enables YOLO.
Your credentials and settings stay. It also replaces an installed source build.

## Build from source

Needs rustup, a C toolchain, OpenSSL headers and pkg-config.

```shell
git clone --depth 1 --single-branch --branch fork-off https://github.com/Viterkim/qudiks.git
cd qudiks
./qudiks-build.sh            # build a release from this checkout
./qudiks-install.sh          # build and install from this checkout
./qudiks-install.sh --debug  # build and install a debug binary
```

For a release archive ready to upload:

```shell
./qudiks-release.sh
```

The build uses the files in your working tree, including uncommitted edits. After
checking a build, use `--skip-build` with `qudiks-install.sh` or `qudiks-release.sh`
to reuse it. Amending a commit with the same files can reuse the build too.

`qudiks-release.sh` puts the archive and checksum in `dist/`. The version comes
from `codex-rs/tui/assets/qudiks-version`.

## Inspired by

- [Codex](https://github.com/openai/codex)
- [hk-vk/codexpilot](https://github.com/hk-vk/codexpilot)
- [GaussianGuaicai/Codex-For-Copilot](https://github.com/GaussianGuaicai/Codex-For-Copilot)

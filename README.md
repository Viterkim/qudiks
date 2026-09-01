# Qudiks

A small fork of [Codex](https://github.com/openai/codex) that adds GitHub
Copilot as a native provider, so one binary can use your Copilot seat instead of
an OpenAI subscription.

No proxy, no Node sidecar. Copilot auth, model discovery and the Copilot-specific
request quirks are handled in Rust inside the normal Codex request path.

## Install

```shell
git clone https://github.com/Viterkim/qudiks
cd qudiks
./setup-qudiks.sh
```

That builds it, signs you in to GitHub with the device flow, picks a model your
account can actually use, writes the config, and generates a wrapper you can put
on your PATH. You need your own Copilot seat.

Requirements: rustup, a C toolchain and linker, openssl + pkg-config. The first
build takes about ten minutes.

```shell
qudiks                                        # start a session
qudiks login github-copilot models            # what your account can use
qudiks login github-copilot --model X setup   # switch model
```

## Updating

```shell
./install-qudiks.sh    # pull, rebuild, reinstall
./login-qudiks.sh      # re-auth if credentials break
```

`install-qudiks.sh` resets to the remote rather than merging, because this fork
is rebased onto upstream and force-pushed.

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

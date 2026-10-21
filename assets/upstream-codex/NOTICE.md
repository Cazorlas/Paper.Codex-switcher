# Upstream Codex model instructions

OpenAI Codex. Copyright 2025 OpenAI.

`model-instructions.md` is copied from `codex-rs/models-manager/prompt.md` in
[`openai/codex`](https://github.com/openai/codex), commit
`ff6aec96948b70d94983af2641a6b67c94faeff5` (`rust-v0.159.2`). It is byte-identical
to the cross-checked main snapshot `8c3612fb638d356579c446de68421288164484dc`.
Its SHA-256 is `ac8ae107a0d72fe3476b430afb161ea4e67da2e446d778aefc44828160559807`.
Rechecked on 2026-10-14 against `rust-v0.162.1`, commit
`092d3acd6bec3e3a14bdc7e7a2810ab628ab759d`, and on 2026-10-21 against
`rust-v0.163.0`, commit `83086188ae0614bd882818ff1aa23f3194962012`; the prompt is unchanged.
The upstream repository distributes this file under the
Apache License, Version 2.0; see [`LICENSE`](LICENSE). The instruction text is
included unchanged and supplies the default template for generated provider
catalog entries that do not have model-specific instructions.

# paper-codex-switch

Multi-account switcher for [OpenAI Codex CLI](https://github.com/openai/codex), with **automatic swap before you hit a usage limit**.

Forked from [xjoker/codex-switch](https://github.com/xjoker/codex-switch) (MIT, see `LICENSE`); the auto-swap design follows [claude-swap](https://github.com/realiti4/claude-swap).

> Manages local auth files. Never publish profiles, `auth.json` or tokens. Codex must use the file credential store (`cli_auth_credentials_store = "file"` in `~/.codex/config.toml`).

## Install

```bash
npm i -g paper-codex-switch                 # prebuilt binary (Windows / macOS / Linux x64, macOS arm64)
# or build from source (Rust 1.88+):
cargo install --git https://github.com/Cazorlas/Paper.Codex-switcher
```

Data lives in `~/.paper-codex-switch` (override with `PAPER_CODEX_SWITCH_HOME`). `self-update` is disabled in this fork.

## Use

```bash
paper-codex-switch login        # add an account (--device on headless machines)
paper-codex-switch list         # usage dashboard
paper-codex-switch use          # switch to the best account (or `use <alias>`)
paper-codex-switch tui          # interactive dashboard
paper-codex-switch launch       # start Codex with the best account
paper-codex-switch launch --auto-swap   # ...and, at the usage threshold, restart the session
                                        # on a better account with `codex resume --last`
```

## Automatic switching

```bash
paper-codex-switch auto                        # foreground loop, checks every 60s
paper-codex-switch auto --threshold 80         # switch earlier (default 90)
paper-codex-switch auto --dry-run              # log what it would do
paper-codex-switch auto --once --json          # single check, for cron / Task Scheduler
```

When the active account's 5h or 7d window reaches `--threshold`, it switches to the eligible account with the most headroom (scored by the same algorithm as `use`). A target must itself be under the threshold and at least `--margin` points (default 10) below the current account, and `--cooldown` (default 300s) prevents flip-flopping. The switch is a compare-and-swap on the active marker, so it never clobbers a change made by another process, and a running Codex app-server daemon is restarted afterwards like after `use`. If every account is exhausted it backs off to a 10-minute cadence. Usage-check errors keep the current account and retry.

`--once` exit codes: `0` switched, `1` error, `2` nothing to do, `3` blocked (no viable target).

Already-running Codex sessions keep the credentials they started with; the swap applies to new sessions.

## Development

```bash
cargo build && cargo test
```

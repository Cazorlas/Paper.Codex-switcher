# Codex compatibility

Checked 2026-10-14. The latest stable OpenAI Codex CLI release and npm
`@openai/codex@latest` both resolve to **0.162.1**; the local PATH CLI is also
0.162.1. The switcher's minimum remains **0.159.2**.

## Relevant upstream updates

- [0.160.0](https://github.com/openai/codex/releases/tag/rust-v0.160.0): explicit provider model catalogs are authoritative; failed refreshes must not reuse stale entries. Provider credential documentation clarifies storage backends.
- [0.161.0](https://github.com/openai/codex/releases/tag/rust-v0.161.0): GPT-6.1 Sol becomes the bundled default; authentication documentation accounts for keyring storage. This switcher still requires file credentials.
- [0.162.0](https://github.com/openai/codex/releases/tag/rust-v0.162.0): custom Responses providers can configure live web access and remote compaction; retries honor server advice; Windows installer and filesystem fixes.
- [0.162.1](https://github.com/openai/codex/releases/tag/rust-v0.162.1): fixes multiline asynchronous-question crashes and startup feature mismatches with a running background server.
- [0.163.0](https://github.com/openai/codex/releases/tag/rust-v0.163.0): `/config` and `/daemon` restart in the TUI, instant interrupts, parallel read-only tools; resumed threads restore their permissions. No auth, token or usage-client contract change.
- The [official product changelog](https://learn.chatgpt.com/docs/changelog) announces GPT-6.1 Sol Ultrafast on 2026-10-08. Availability depends on plan and workspace settings; this update does not force a model or service tier.

## Repository changes

The transport fallback and `doctor.aligned_version` use 0.163.0. A detected
CLI version still supplies the HTTP client version; the fallback applies only
when the version cannot be determined. Launch still rejects versions below
0.159.2 and unknown versions.

`doctor` distinguishes the exact baseline (`aligned`), supported older
versions (`below_baseline_unverified`) and future versions
(`above_baseline_unverified`). Both unverified statuses satisfy the minimum;
neither certifies a complete live account-switching workflow.

Source comparison between the release tags found the vendored
`codex-rs/models-manager/prompt.md` and `codex-rs/login/src/lib.rs` unchanged.
The catalog's nested `ModelMessages` type adds optional, serde-defaulted
`content_filter_guidance`; generated catalogs do not need to populate it.
The existing catalog instruction template remains compatible with the upstream
removal of personality variables. The prompt's original attribution and hash
remain in [NOTICE.md](../assets/upstream-codex/NOTICE.md).

The token-data, auth-header, OAuth client, device-code login, backend usage
client, rate-limit parser and rate-limit payload/window source files are
unchanged between these tags. Auth storage changes add keyring diagnostics
and telemetry; no changed serialized `auth.json` token fields were found.
The weekly health workflow now watches for releases beyond minor 163.

### 0.162.1 -> 0.163.0 (2026-10-21)

`codex-rs/models-manager/prompt.md` and `codex-rs/login/src/lib.rs` are
unchanged; the login crate only restyles its browser success/error pages. No
non-test source file about auth, token data, keyring or rate limits changed.
The backend client also falls back to its alternate route on request errors,
and incomplete responses now carry their token usage. The catalog's
`ToolMessages` gains optional, serde-defaulted `incremental_tools` and
`functions_namespace_functions_description_prefixes`; generated catalogs do
not need to populate them.

## Validation

Verification results are recorded in the repository's
[progress board](progress/board.html). Static upstream review, deterministic
tests and a read-only live version probe are separate evidence. Real OAuth
login, quota refresh, warmup, switching accounts and desktop-server restart
have not been exercised for this update.

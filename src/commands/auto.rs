//! `auto`: watch the active account's quota and swap to a better one before
//! Codex hits its usage limit (the claude-swap `cswap auto` model).

use super::profile::{report_daemon_restart, score_profile_candidates};
use crate::{app_server, auth, cache, color, config, output, profile, usage};
use anyhow::{Context, Result};
use std::time::{Duration, Instant};

/// Exit codes for `auto --once`, so cron and scripts can branch on the result.
pub(crate) const EXIT_SWITCHED: i32 = 0;
pub(crate) const EXIT_NOTHING_TO_DO: i32 = 2;
pub(crate) const EXIT_BLOCKED: i32 = 3;

#[derive(Debug, Clone)]
pub(crate) struct AutoOptions {
    /// Switch away once the active account's 5h or 7d window reaches this percent.
    pub threshold: f64,
    /// A target must be at least this many points below the active account.
    pub margin: f64,
    pub interval: Duration,
    pub cooldown: Duration,
    pub once: bool,
    pub dry_run: bool,
    pub json: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Decision {
    /// Active account is under the threshold.
    Stay { pressure: f64 },
    /// Switch to `alias`.
    Switch {
        alias: String,
        from_pressure: f64,
        to_pressure: f64,
    },
    /// Active account is over the threshold and no account is a viable target.
    Blocked { pressure: f64 },
}

/// The busier of the two quota windows, as a used percent.
pub(crate) fn pressure(c: &usage::Candidate) -> f64 {
    let five = if c.has_5h_data { c.used_5h } else { 0.0 };
    let seven = if c.has_7d_data { c.used_7d } else { 0.0 };
    five.max(seven)
}

/// Pure decision. `ranked` is the scored pool, best first (see
/// `score_profile_candidates`), and may include the active account.
pub(crate) fn decide(
    current: &str,
    ranked: &[(usage::Candidate, f64)],
    limited_now: bool,
    opts: &AutoOptions,
    safety_7d: f64,
) -> Decision {
    let cur_pressure = ranked
        .iter()
        .find(|(c, _)| c.alias == current)
        .map(|(c, _)| pressure(c))
        .unwrap_or(0.0);
    let cur_pressure = if limited_now {
        cur_pressure.max(100.0)
    } else {
        cur_pressure
    };
    if cur_pressure < opts.threshold {
        return Decision::Stay {
            pressure: cur_pressure,
        };
    }
    let target = ranked.iter().find(|(c, _)| {
        c.alias != current
            && usage::is_candidate_eligible(c, safety_7d)
            && pressure(c) < opts.threshold
            && pressure(c) + opts.margin <= cur_pressure
    });
    match target {
        Some((c, _)) => Decision::Switch {
            alias: c.alias.clone(),
            from_pressure: cur_pressure,
            to_pressure: pressure(c),
        },
        None => Decision::Blocked {
            pressure: cur_pressure,
        },
    }
}

static LAST_EVENT: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

fn log_path() -> Result<std::path::PathBuf> {
    Ok(auth::app_home()?.join("auto.log"))
}

fn status_path() -> Result<std::path::PathBuf> {
    Ok(auth::app_home()?.join("auto-status.json"))
}

/// Append one line to `auto.log` (rotated to `auto.log.1` past 256 KiB) so a
/// hidden background `auto` leaves a trail.
fn append_log(line: &str) {
    use std::io::Write;
    let Ok(path) = log_path() else { return };
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 256 * 1024) {
        let _ = std::fs::rename(&path, path.with_extension("log.1"));
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{} {line}", auth::now_unix_secs());
    }
}

/// Heartbeat read by `auto --status`.
fn write_status(wait: Duration) {
    let last = LAST_EVENT.lock().map(|g| g.clone()).unwrap_or_default();
    let body = serde_json::json!({
        "pid": std::process::id(),
        "at": auth::now_unix_secs(),
        "next_check_in_secs": wait.as_secs(),
        "last": last,
    });
    if let Ok(path) = status_path() {
        let _ = std::fs::write(path, body.to_string());
    }
}

fn read_status() -> Option<serde_json::Value> {
    status_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
}

/// A background `auto` is alive when its heartbeat is newer than two check cycles.
fn running_pid() -> Option<u64> {
    let v = read_status()?;
    let age = auth::now_unix_secs() - v["at"].as_i64()?;
    let next = v["next_check_in_secs"].as_i64().unwrap_or(60);
    (age <= next * 2 + 120).then(|| v["pid"].as_u64()).flatten()
}

/// `auto --start`: launch a detached, windowless `auto` and return at once.
pub(crate) fn start_cmd(opts: &AutoOptions, at_login: bool) -> Result<()> {
    if let Some(pid) = running_pid() {
        println!("auto is already running (pid {pid})");
        if at_login {
            let _ = startup_cmd(true, opts);
        }
        return Ok(());
    }
    let exe = std::env::current_exe().context("locating the paper-codex-switch executable")?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args([
        "auto",
        "--threshold",
        &opts.threshold.to_string(),
        "--margin",
        &opts.margin.to_string(),
        "--interval",
        &opts.interval.as_secs().to_string(),
        "--cooldown",
        &opts.cooldown.as_secs().to_string(),
    ])
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // DETACHED_PROCESS | CREATE_NO_WINDOW
        cmd.creation_flags(0x0000_0008 | 0x0800_0000);
        // The child would otherwise inherit our stdout/stderr pipe, and a caller
        // reading `... | tail` would wait for the background process to exit.
        unsafe extern "system" {
            fn GetStdHandle(n: u32) -> *mut core::ffi::c_void;
            fn SetHandleInformation(h: *mut core::ffi::c_void, mask: u32, flags: u32) -> i32;
        }
        for std_handle in [-11i32 as u32, -12i32 as u32] {
            // SAFETY: plain Win32 calls on this process's own std handles.
            unsafe {
                let h = GetStdHandle(std_handle);
                if !h.is_null() && h as isize != -1 {
                    SetHandleInformation(h, 1, 0); // clear HANDLE_FLAG_INHERIT
                }
            }
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    let child = cmd.spawn().context("starting the background auto")?;
    println!(
        "{}",
        color::success(&format!("auto started in the background (pid {})", child.id()))
    );
    // Start at login is opt-in: writing to the Startup folder is a persistence
    // behaviour that antivirus heuristics dislike, so it is never done silently.
    if at_login && let Err(e) = startup_cmd(true, opts) {
        println!("  (not set to start at login: {e})");
    }
    println!("  check with `paper-codex-switch auto --status`, stop with `paper-codex-switch auto --stop`");
    Ok(())
}

/// `auto --stop`: end the background `auto`.
pub(crate) fn stop_cmd(opts: &AutoOptions) -> Result<()> {
    // Also remove a login entry created earlier with `--start --at-login`.
    let _ = startup_cmd(false, opts);
    let Some(pid) = running_pid() else {
        println!("auto is not running");
        return Ok(());
    };
    #[cfg(windows)]
    let status = std::process::Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    #[cfg(not(windows))]
    let status = std::process::Command::new("kill")
        .arg(pid.to_string())
        .status();
    if !status.is_ok_and(|s| s.success()) {
        anyhow::bail!("could not stop pid {pid}; end it manually");
    }
    if let Ok(path) = status_path() {
        let _ = std::fs::remove_file(path);
    }
    append_log(&format!("stopped by user (pid {pid})"));
    println!("{}", color::success(&format!("auto stopped (pid {pid})")));
    Ok(())
}

/// `auto --startup on|off`: start `auto` at every Windows login (Startup folder, no admin).
pub(crate) fn startup_cmd(on: bool, opts: &AutoOptions) -> Result<()> {
    #[cfg(windows)]
    {
        let dir = std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .context("APPDATA is not set")?
            .join(r"Microsoft\Windows\Start Menu\Programs\Startup");
        let file = dir.join("paper-codex-switch-auto.vbs");
        if on {
            let line = format!(
                "CreateObject(\"Wscript.Shell\").Run \"cmd /c paper-codex-switch auto --threshold {} --margin {} --interval {} --cooldown {}\", 0, False
",
                opts.threshold,
                opts.margin,
                opts.interval.as_secs(),
                opts.cooldown.as_secs()
            );
            std::fs::write(&file, line).with_context(|| format!("writing {}", file.display()))?;
            println!("  auto will also start at every login");
        } else if file.exists() {
            std::fs::remove_file(&file)?;
            println!("  auto will no longer start at login");
        } else {
            
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (on, opts);
        anyhow::bail!(
            "--startup is Windows-only; on macOS/Linux use cron (`@reboot paper-codex-switch auto --start`) or a systemd user unit"
        )
    }
}

/// `auto --status`: is a background `auto` alive, and what did it do last?
pub(crate) fn status_cmd(json: bool) -> Result<()> {
    let now = auth::now_unix_secs();
    let status: Option<serde_json::Value> = status_path()
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok());
    let (running, age, last, pid) = match &status {
        Some(v) => {
            let at = v["at"].as_i64().unwrap_or(0);
            let next = v["next_check_in_secs"].as_i64().unwrap_or(60);
            let age = now - at;
            (
                age <= next * 2 + 120,
                Some(age),
                v["last"].as_str().unwrap_or("").to_string(),
                v["pid"].as_u64(),
            )
        }
        None => (false, None, String::new(), None),
    };
    if json {
        output::print_json(&serde_json::json!({
            "running": running, "pid": pid, "last_heartbeat_secs_ago": age, "last": last,
        }));
    } else if running {
        println!(
            "{}",
            color::success(&format!(
                "auto is running (pid {}, last check {}s ago)",
                pid.unwrap_or(0),
                age.unwrap_or(0)
            ))
        );
        println!("  last: {last}");
    } else {
        println!("{}", color::warn("auto is not running"));
        if age.is_some() {
            println!("  last: {last}");
        }
    }
    if let Ok(p) = log_path() {
        println!("  log: {}", p.display());
    }
    Ok(())
}

fn emit(opts: &AutoOptions, event: &str, detail: serde_json::Value, text: String) {
    if let Ok(mut last) = LAST_EVENT.lock() {
        *last = format!("{event}: {}", strip_ansi(&text));
    }
    append_log(&format!("{event} {}", strip_ansi(&text)));
    if opts.json {
        let mut obj = serde_json::json!({ "event": event, "at": auth::now_unix_secs() });
        if let (Some(o), Some(d)) = (obj.as_object_mut(), detail.as_object()) {
            o.extend(d.clone());
        }
        println!("{obj}");
    } else {
        println!("{text}");
    }
}

/// Fetch usage for every profile (forced: a switch decision must not run on a
/// stale cache) and rank them.
async fn rank_pool(current: &str) -> Result<Vec<(usage::Candidate, usage::UsageInfo, f64)>> {
    let profiles = profile::list_profiles()?;
    let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(
        config::get().network.max_concurrent,
    ));
    let mut tasks = tokio::task::JoinSet::new();
    for alias in profiles {
        let sem = semaphore.clone();
        let current = current.to_string();
        tasks.spawn(async move {
            let _permit = sem.acquire_owned().await.ok()?;
            let path = profile::profile_auth_path(&alias).ok()?;
            match usage::fetch_usage_retried_force(&alias, &path, &current).await {
                Ok(u) => Some((alias, u)),
                Err(e) => {
                    tracing::warn!("[{alias}] usage fetch failed during auto-swap: {e}");
                    None
                }
            }
        });
    }
    let mut fetched = Vec::new();
    while let Some(done) = tasks.join_next().await {
        if let Some(pair) = done.context("usage worker failed")? {
            fetched.push(pair);
        }
    }
    let cfg = config::get();
    Ok(score_profile_candidates(
        fetched,
        auth::now_unix_secs(),
        cfg.use_cfg.safety_margin_7d,
        cfg.use_cfg.team_priority,
    ))
}

/// One check-and-maybe-switch decision (nothing is switched here).
/// `current` is the account being watched; `marker` is the account whose
/// credentials are live in auth.json (they differ during a `launch` session).
pub(crate) async fn tick(opts: &AutoOptions, current: &str, marker: &str) -> Result<Decision> {
    let cfg = config::get();
    let safety_7d = cfg.use_cfg.safety_margin_7d;
    let path = profile::profile_auth_path(current)?;
    let live = usage::fetch_usage_retried_force(current, &path, marker)
        .await
        .map_err(|e| anyhow::anyhow!("usage check for '{current}' failed: {}", e.summary))?;
    let limited_now = live.account_limited;

    // Cheap pre-check on the active account alone: only fan out to the whole
    // pool when a switch is actually on the table.
    let single = score_profile_candidates(
        vec![(current.to_string(), live)],
        auth::now_unix_secs(),
        safety_7d,
        cfg.use_cfg.team_priority,
    );
    let pre: Vec<_> = single.iter().map(|(c, _, s)| (c.clone(), *s)).collect();
    let first = decide(current, &pre, limited_now, opts, safety_7d);
    if matches!(first, Decision::Stay { .. }) {
        return Ok(first);
    }

    let pool = rank_pool(marker).await?;
    let ranked: Vec<_> = pool.iter().map(|(c, _, s)| (c.clone(), *s)).collect();
    Ok(decide(current, &ranked, limited_now, opts, safety_7d))
}

fn apply(current: &str, alias: &str) -> Result<bool> {
    let before = app_server::snapshot_live_auth();
    // Compare-and-swap: bail if another process moved the active marker.
    if !profile::switch_profile_if_current(current, alias)? {
        return Ok(false);
    }
    cache::set_last_used(alias)?;
    tracing::info!(
        action = "switch",
        alias,
        selection = "auto-swap",
        "account switched"
    );
    report_daemon_restart(alias, &before);
    Ok(true)
}

pub(crate) async fn auto_cmd(opts: AutoOptions) -> Result<()> {
    auth::ensure_file_credentials_store()?;
    let mut last_switch: Option<Instant> = None;
    let mut last_blocked = false;

    loop {
        let current = profile::read_current();
        if current.is_empty() {
            anyhow::bail!("no active profile; run `paper-codex-switch use <alias>` first");
        }
        let in_cooldown = last_switch.is_some_and(|t| t.elapsed() < opts.cooldown);

        let code = match tick(&opts, &current, &current).await {
            Err(e) => {
                // Fail safe: keep the current account and retry next tick.
                emit(
                    &opts,
                    "error",
                    serde_json::json!({ "message": format!("{e:#}") }),
                    color::error(&format!("auto: {e:#}")),
                );
                if opts.once {
                    return Err(e);
                }
                None
            }
            Ok(Decision::Stay { pressure }) => {
                last_blocked = false;
                emit(
                    &opts,
                    "ok",
                    serde_json::json!({ "alias": current, "pressure": pressure }),
                    color::dim(&format!("auto: '{current}' at {pressure:.0}%, staying")),
                );
                Some(EXIT_NOTHING_TO_DO)
            }
            Ok(Decision::Blocked { pressure }) => {
                if !last_blocked || opts.once {
                    emit(
                        &opts,
                        "blocked",
                        serde_json::json!({ "alias": current, "pressure": pressure }),
                        color::warn(&format!(
                            "auto: '{current}' at {pressure:.0}% and no account is a viable target"
                        )),
                    );
                }
                last_blocked = true;
                Some(EXIT_BLOCKED)
            }
            Ok(Decision::Switch {
                alias,
                from_pressure,
                to_pressure,
            }) => {
                last_blocked = false;
                let detail = serde_json::json!({
                    "from": current, "to": alias,
                    "from_pressure": from_pressure, "to_pressure": to_pressure,
                    "dry_run": opts.dry_run,
                });
                let summary = format!(
                    "'{current}' ({from_pressure:.0}%) -> '{alias}' ({to_pressure:.0}%)"
                );
                if in_cooldown && !opts.once {
                    emit(
                        &opts,
                        "cooldown",
                        detail,
                        color::dim(&format!("auto: want {summary} but in cooldown")),
                    );
                    None
                } else if opts.dry_run {
                    emit(
                        &opts,
                        "would_switch",
                        detail,
                        format!("auto: would switch {summary}"),
                    );
                    Some(EXIT_NOTHING_TO_DO)
                } else if apply(&current, &alias)? {
                    last_switch = Some(Instant::now());
                    emit(
                        &opts,
                        "switched",
                        detail,
                        color::success(&format!("auto: switched {summary}")),
                    );
                    Some(EXIT_SWITCHED)
                } else {
                    emit(
                        &opts,
                        "raced",
                        detail,
                        color::dim("auto: active profile changed under us; re-checking"),
                    );
                    None
                }
            }
        };

        if opts.once {
            std::process::exit(code.unwrap_or(1));
        }

        // Exhausted pool: back off to a slower cadence instead of hammering the API.
        let wait = if last_blocked {
            opts.interval.max(Duration::from_secs(600))
        } else {
            opts.interval
        };
        write_status(wait);
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = tokio::signal::ctrl_c() => return Ok(()),
        }
    }
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '' && chars.peek() == Some(&'[') {
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> AutoOptions {
        AutoOptions {
            threshold: 90.0,
            margin: 10.0,
            interval: Duration::from_secs(60),
            cooldown: Duration::from_secs(300),
            once: true,
            dry_run: false,
            json: false,
        }
    }

    fn cand(alias: &str, used_5h: f64, used_7d: f64) -> usage::Candidate {
        usage::Candidate {
            alias: alias.into(),
            used_5h,
            resets_at_5h: None,
            used_7d,
            resets_at_7d: None,
            has_5h_data: true,
            has_7d_data: true,
            is_team: false,
            is_free: false,
            last_used: 0,
            now: 0,
            pool_size: 2,
            pool_exhausted: 0,
            team_priority: false,
        }
    }

    #[test]
    fn stays_under_threshold() {
        let r = vec![(cand("a", 50.0, 10.0), 1.0), (cand("b", 0.0, 0.0), 2.0)];
        assert!(matches!(
            decide("a", &r, false, &opts(), 20.0),
            Decision::Stay { .. }
        ));
    }

    #[test]
    fn switches_to_best_when_over_threshold() {
        let r = vec![(cand("b", 10.0, 10.0), 2.0), (cand("a", 95.0, 10.0), 1.0)];
        match decide("a", &r, false, &opts(), 20.0) {
            Decision::Switch { alias, .. } => assert_eq!(alias, "b"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn blocked_when_every_other_account_is_also_hot() {
        let r = vec![(cand("a", 95.0, 10.0), 1.0), (cand("b", 92.0, 10.0), 2.0)];
        assert!(matches!(
            decide("a", &r, false, &opts(), 20.0),
            Decision::Blocked { .. }
        ));
    }

    #[test]
    fn hysteresis_blocks_marginal_candidates() {
        let mut o = opts();
        o.threshold = 80.0;
        let r = vec![(cand("a", 82.0, 0.0), 1.0), (cand("b", 75.0, 0.0), 2.0)];
        assert!(matches!(
            decide("a", &r, false, &o, 20.0),
            Decision::Blocked { .. }
        ));
    }

    #[test]
    fn seven_day_window_also_triggers() {
        let r = vec![(cand("a", 5.0, 93.0), 1.0), (cand("b", 5.0, 5.0), 2.0)];
        assert!(matches!(
            decide("a", &r, false, &opts(), 20.0),
            Decision::Switch { .. }
        ));
    }
}

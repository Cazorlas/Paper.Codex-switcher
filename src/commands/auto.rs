//! `auto`: watch the active account's quota and swap to a better one before
//! Codex hits its usage limit (the claude-swap `cswap auto` model).

use super::profile::{report_daemon_restart, score_profile_candidates};
use crate::{app_server, auth, cache, color, config, profile, usage};
use anyhow::{Context, Result};
use std::collections::HashMap;
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
    /// Also show a Windows toast notification (the terminal bell is always sent).
    pub toast: bool,
    /// Prefer usable quota on a plan ending within this many days.
    pub prefer_expiring_days: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SwitchReason {
    Threshold,
    PlanEnding,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Decision {
    /// Active account is under the threshold.
    Stay {
        pressure: f64,
        used_5h: f64,
        used_7d: f64,
    },
    /// Switch to `alias`.
    Switch {
        alias: String,
        from_pressure: f64,
        to_pressure: f64,
        reason: SwitchReason,
    },
    /// Active account is over the threshold and no account is a viable target.
    Blocked { pressure: f64 },
}

/// The busier of the two quota windows, as a used percent. A window whose reset
/// time has already passed counts as empty (`effective_used_*`), so an old reading
/// cannot make an account look busy after its quota came back.
pub(crate) fn pressure(c: &usage::Candidate) -> f64 {
    let five = if c.has_5h_data { c.effective_used_5h() } else { 0.0 };
    let seven = if c.has_7d_data { c.effective_used_7d() } else { 0.0 };
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
    let current_candidate = ranked
        .iter()
        .find(|(c, _)| c.alias == current)
        .map(|(c, _)| c);
    let cur_pressure = current_candidate.map(pressure).unwrap_or(0.0);
    let cur_pressure = if limited_now {
        cur_pressure.max(100.0)
    } else {
        cur_pressure
    };
    if cur_pressure < opts.threshold {
        return Decision::Stay {
            pressure: cur_pressure,
            used_5h: current_candidate
                .filter(|c| c.has_5h_data)
                .map_or(0.0, |c| c.effective_used_5h()),
            used_7d: current_candidate
                .filter(|c| c.has_7d_data)
                .map_or(0.0, |c| c.effective_used_7d()),
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
            reason: SwitchReason::Threshold,
        },
        None => Decision::Blocked {
            pressure: cur_pressure,
        },
    }
}

/// Check local plan dates before fetching usage for the pool.
pub(crate) fn plan_ending_gate(
    current: &str,
    plan_until: &HashMap<String, i64>,
    now: i64,
    days: u32,
) -> bool {
    let latest = now.saturating_add(i64::from(days) * 86_400);
    plan_until.iter().any(|(alias, &until)| {
        alias != current
            && now < until
            && until <= latest
            && plan_until.get(current).is_none_or(|&end| until < end)
    })
}

/// Prefer the earliest eligible plan end; equal dates preserve pool ranking.
pub(crate) fn decide_with_plans(
    current: &str,
    ranked: &[(usage::Candidate, f64)],
    plan_until: &HashMap<String, i64>,
    now: i64,
    limited_now: bool,
    opts: &AutoOptions,
    safety_7d: f64,
) -> Decision {
    let fallback = decide(current, ranked, limited_now, opts, safety_7d);
    let Some(days) = opts.prefer_expiring_days else {
        return fallback;
    };
    let cur_pressure = ranked
        .iter()
        .find(|(c, _)| c.alias == current)
        .map_or(0.0, |(c, _)| pressure(c));
    let cur_pressure = if limited_now { 100.0 } else { cur_pressure };
    let latest = now.saturating_add(i64::from(days) * 86_400);
    let target = ranked
        .iter()
        .filter_map(|(c, _)| {
            let &until = plan_until.get(&c.alias)?;
            (c.alias != current
                && usage::is_candidate_eligible(c, safety_7d)
                && now < until
                && until <= latest
                && pressure(c) <= opts.threshold - opts.margin
                && (cur_pressure >= opts.threshold
                    || plan_until.get(current).is_none_or(|&end| until < end)))
            .then_some((c, until))
        })
        .min_by_key(|(_, until)| *until);
    match target {
        Some((c, _)) => Decision::Switch {
            alias: c.alias.clone(),
            from_pressure: cur_pressure,
            to_pressure: pressure(c),
            reason: SwitchReason::PlanEnding,
        },
        None => fallback,
    }
}

pub(crate) fn stay_line(alias: &str, used_5h: f64, used_7d: f64) -> String {
    format!("auto: '{alias}' at 5h {used_5h:.0}%, 7d {used_7d:.0}%, staying")
}

/// Tell the person who minimized the window that something happened: the terminal
/// bell flashes the taskbar entry, and `--toast` adds a Windows notification.
fn notify(opts: &AutoOptions, title: &str, body: &str) {
    if opts.json || opts.once || opts.dry_run {
        return;
    }
    print!("\x07");
    #[cfg(windows)]
    if opts.toast {
        // Text goes through the environment, never into the script, so account names
        // cannot be interpreted as PowerShell.
        let script = "$ErrorActionPreference='Stop';\
[void][Windows.UI.Notifications.ToastNotificationManager,Windows.UI.Notifications,ContentType=WindowsRuntime];\
$x=[Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02);\
$t=$x.GetElementsByTagName('text');\
[void]$t.Item(0).AppendChild($x.CreateTextNode($env:PCS_TOAST_TITLE));\
[void]$t.Item(1).AppendChild($x.CreateTextNode($env:PCS_TOAST_BODY));\
[Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('paper-codex-switch').Show([Windows.UI.Notifications.ToastNotification]::new($x))";
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("PCS_TOAST_TITLE", title)
            .env("PCS_TOAST_BODY", body)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    #[cfg(not(windows))]
    let _ = (title, body);
}

fn emit(opts: &AutoOptions, event: &str, detail: serde_json::Value, text: String) {
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
    let mut plan_until = HashMap::new();
    if opts.prefer_expiring_days.is_some() {
        for alias in profile::list_profiles()? {
            if let Some(until) =
                auth::read_account_info(&profile::profile_auth_path(&alias)?).subscription_until
            {
                plan_until.insert(alias, until);
            }
        }
    }
    let now = auth::now_unix_secs();
    if matches!(first, Decision::Stay { .. })
        && opts
            .prefer_expiring_days
            .is_none_or(|days| !plan_ending_gate(current, &plan_until, now, days))
    {
        return Ok(first);
    }

    let pool = rank_pool(marker).await?;
    let ranked: Vec<_> = pool.iter().map(|(c, _, s)| (c.clone(), *s)).collect();
    Ok(decide_with_plans(
        current, &ranked, &plan_until, now, limited_now, opts, safety_7d,
    ))
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
    if !opts.once && !opts.json {
        println!(
            "{}",
            color::success(&format!(
                "auto is running: switches accounts at {:.0}% (5h or 7d), checks every {}s.",
                opts.threshold,
                opts.interval.as_secs()
            ))
        );
        println!(
            "{}",
            color::dim("Leave this window open (you can minimize it). Stop with Ctrl+C or by closing the window.")
        );
        if let Some(days) = opts.prefer_expiring_days {
            println!(
                "{}",
                color::dim(&format!(
                    "Prefers an account whose plan ends within {days} days."
                ))
            );
        }
    }
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
            Ok(Decision::Stay {
                pressure,
                used_5h,
                used_7d,
            }) => {
                last_blocked = false;
                emit(
                    &opts,
                    "ok",
                    serde_json::json!({ "alias": current, "pressure": pressure, "used_5h": used_5h, "used_7d": used_7d }),
                    color::dim(&stay_line(&current, used_5h, used_7d)),
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
                if !last_blocked {
                    notify(
                        &opts,
                        "paper-codex-switch: no account left",
                        &format!("'{current}' is at {pressure:.0}% and every other account is over the limit"),
                    );
                }
                last_blocked = true;
                Some(EXIT_BLOCKED)
            }
            Ok(Decision::Switch {
                alias,
                from_pressure,
                to_pressure,
                reason,
            }) => {
                last_blocked = false;
                let (reason, suffix) = match reason {
                    SwitchReason::Threshold => ("threshold", ""),
                    SwitchReason::PlanEnding => (
                        "plan_ending",
                        ": its plan ends soonest, using it first",
                    ),
                };
                let detail = serde_json::json!({
                    "from": current, "to": alias,
                    "from_pressure": from_pressure, "to_pressure": to_pressure,
                    "dry_run": opts.dry_run,
                    "reason": reason,
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
                        format!("auto: would switch {summary}{suffix}"),
                    );
                    Some(EXIT_NOTHING_TO_DO)
                } else if apply(&current, &alias)? {
                    last_switch = Some(Instant::now());
                    notify(
                        &opts,
                        "paper-codex-switch: switched account",
                        &format!("{current} ({from_pressure:.0}%) -> {alias} ({to_pressure:.0}%){suffix}"),
                    );
                    emit(
                        &opts,
                        "switched",
                        detail,
                        color::success(&format!("auto: switched {summary}{suffix}")),
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
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = tokio::signal::ctrl_c() => return Ok(()),
        }
    }
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
            toast: false,
            prefer_expiring_days: None,
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
    fn a_window_that_already_reset_does_not_count_as_busy() {
        // Reading says 97% used, but that window reset 10 minutes ago (now = 1000).
        let mut stale = cand("a", 97.0, 20.0);
        stale.now = 1_000;
        stale.resets_at_5h = Some(400);
        assert_eq!(pressure(&stale), 20.0);

        // Same reading with the reset still ahead is busy.
        stale.resets_at_5h = Some(1_600);
        assert_eq!(pressure(&stale), 97.0);

        // The weekly window follows the same rule.
        let mut week = cand("b", 5.0, 95.0);
        week.now = 1_000;
        week.resets_at_7d = Some(999);
        assert_eq!(pressure(&week), 5.0);
    }

    #[test]
    fn auto_stays_when_the_busy_window_has_already_reset() {
        let mut a = cand("a", 97.0, 10.0);
        a.now = 1_000;
        a.resets_at_5h = Some(900);
        let r = vec![(a, 1.0), (cand("b", 0.0, 0.0), 2.0)];
        assert!(matches!(
            decide("a", &r, false, &opts(), 20.0),
            Decision::Stay { .. }
        ));
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
#[cfg(test)]
#[path = "auto_expiring_tests.rs"]
mod expiring_tests;

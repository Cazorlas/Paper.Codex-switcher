use super::*;

const N: i64 = 1_000_000;
const R: i64 = N + 2_640;

fn cand(
    alias: &str,
    used_5h: f64,
    resets_at_5h: Option<i64>,
    used_7d: f64,
    resets_at_7d: Option<i64>,
) -> usage::Candidate {
    usage::Candidate {
        alias: alias.into(),
        used_5h,
        resets_at_5h,
        used_7d,
        resets_at_7d,
        has_5h_data: true,
        has_7d_data: true,
        is_team: false,
        is_free: false,
        last_used: 0,
        now: N,
        pool_size: 2,
        pool_exhausted: 0,
        team_priority: false,
        limit_windows: None,
    }
}

/// A candidate the backend marked as limited, built the way `auto` builds it.
fn limited(alias: &str, w5h: (f64, Option<i64>), w7d: (f64, Option<i64>)) -> usage::Candidate {
    let window = |(used, resets_at): (f64, Option<i64>)| {
        Some(usage::WindowUsage {
            used_percent: Some(used),
            resets_at,
            window_minutes: None,
        })
    };
    let u = usage::UsageInfo {
        primary: window(w5h),
        secondary: window(w7d),
        account_limited: true,
        ..Default::default()
    };
    let mut c = usage::Candidate::from_usage(alias.into(), &u, false, false, 0, N);
    c.pool_size = 2;
    c
}

const R7: i64 = N + 45_660;

fn blocked_by(other: usage::Candidate) -> Decision {
    let ranked = vec![(cand("p07", 93.0, None, 96.0, None), 1.0), (other, 2.0)];
    decide("p07", &ranked, false, &opts(), 20.0)
}

#[test]
fn limited_account_with_full_weekly_window_waits_for_its_real_reset() {
    let queh = limited("queh", (0.0, Some(N + 18_000)), (100.0, Some(R7)));
    // Scoring still sees the forced exhaustion.
    assert_eq!((queh.used_5h, queh.used_7d), (100.0, 100.0));
    assert_eq!((queh.resets_at_5h, queh.resets_at_7d), (None, None));
    assert_eq!(pressure(&queh), 100.0);
    assert_eq!(
        blocked_by(queh),
        Decision::Blocked {
            pressure: 96.0,
            waiting: vec![Waiting {
                alias: "queh".into(),
                used_5h: None,
                used_7d: Some(100.0),
                free_at: Some(R7),
            }],
        }
    );
}

#[test]
fn limited_account_blocked_line_names_the_weekly_reset() {
    let Decision::Blocked { pressure, waiting } =
        blocked_by(limited("queh", (0.0, Some(N + 18_000)), (100.0, Some(R7))))
    else {
        panic!("expected Blocked");
    };
    assert_eq!(
        blocked_line("p07", pressure, &waiting),
        format!(
            "auto: 'p07' at 96% and no account is a viable target; 'queh' 7d 100% free at {}",
            crate::output::format_local_time(R7)
        )
    );
}

#[test]
fn limited_account_with_both_windows_full_waits_for_the_later_reset() {
    assert_eq!(
        blocked_by(limited("queh", (100.0, Some(N + 600)), (100.0, Some(R7)))),
        Decision::Blocked {
            pressure: 96.0,
            waiting: vec![Waiting {
                alias: "queh".into(),
                used_5h: Some(100.0),
                used_7d: Some(100.0),
                free_at: Some(R7),
            }],
        }
    );
    // One of the two resets unknown: no free time.
    assert_eq!(
        blocked_by(limited("queh", (100.0, None), (100.0, Some(R7)))),
        Decision::Blocked {
            pressure: 96.0,
            waiting: vec![Waiting {
                alias: "queh".into(),
                used_5h: Some(100.0),
                used_7d: Some(100.0),
                free_at: None,
            }],
        }
    );
}

#[test]
fn limited_account_without_a_full_window_keeps_the_forced_report() {
    assert_eq!(
        blocked_by(limited("queh", (10.0, Some(N + 600)), (40.0, Some(R7)))),
        Decision::Blocked {
            pressure: 96.0,
            waiting: vec![Waiting {
                alias: "queh".into(),
                used_5h: Some(100.0),
                used_7d: Some(100.0),
                free_at: None,
            }],
        }
    );
}

fn opts() -> AutoOptions {
    AutoOptions {
        threshold: 95.0,
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

#[test]
fn blocked_waiting_reports_only_the_window_that_blocks_today_case() {
    let ranked = vec![
        (cand("p07", 93.0, None, 96.0, None), 1.0),
        (cand("queh", 95.0, Some(R), 73.0, Some(N + 500_000)), 2.0),
    ];
    assert_eq!(
        decide("p07", &ranked, false, &opts(), 20.0),
        Decision::Blocked {
            pressure: 96.0,
            waiting: vec![Waiting {
                alias: "queh".into(),
                used_5h: Some(95.0),
                used_7d: None,
                free_at: Some(R),
            }],
        }
    );
}

#[test]
fn blocked_waiting_uses_latest_reset_when_both_windows_block() {
    let ranked = vec![
        (cand("p07", 96.0, None, 10.0, None), 1.0),
        (cand("queh", 97.0, Some(N + 600), 99.0, Some(N + 90_000)), 2.0),
    ];
    assert_eq!(
        decide("p07", &ranked, false, &opts(), 20.0),
        Decision::Blocked {
            pressure: 96.0,
            waiting: vec![Waiting {
                alias: "queh".into(),
                used_5h: Some(97.0),
                used_7d: Some(99.0),
                free_at: Some(N + 90_000),
            }],
        }
    );
}

#[test]
fn blocked_waiting_without_reset_time_is_unavailable() {
    let ranked = vec![
        (cand("p07", 96.0, None, 10.0, None), 1.0),
        (cand("queh", 96.0, None, 10.0, None), 2.0),
    ];
    assert_eq!(
        decide("p07", &ranked, false, &opts(), 20.0),
        Decision::Blocked {
            pressure: 96.0,
            waiting: vec![Waiting {
                alias: "queh".into(),
                used_5h: Some(96.0),
                used_7d: None,
                free_at: None,
            }],
        }
    );
}

#[test]
fn blocked_waiting_reports_margin_blocked_window() {
    let ranked = vec![
        (cand("p07", 96.0, None, 10.0, None), 1.0),
        (cand("queh", 90.0, Some(R), 20.0, Some(N + 500_000)), 2.0),
    ];
    assert_eq!(
        decide("p07", &ranked, false, &opts(), 20.0),
        Decision::Blocked {
            pressure: 96.0,
            waiting: vec![Waiting {
                alias: "queh".into(),
                used_5h: Some(90.0),
                used_7d: None,
                free_at: Some(R),
            }],
        }
    );
}

#[test]
fn blocked_wait_uses_earliest_reset_and_clamps_to_interval_and_ten_minutes() {
    let waiting = |free_at| Waiting {
        alias: "queh".into(),
        used_5h: Some(95.0),
        used_7d: None,
        free_at,
    };
    let interval = Duration::from_secs(60);
    assert_eq!(blocked_wait(N, &[waiting(Some(N + 300))], interval), Duration::from_secs(330));
    assert_eq!(blocked_wait(N, &[waiting(Some(N + 7_200))], interval), Duration::from_secs(600));
    assert_eq!(blocked_wait(N, &[waiting(Some(N - 10))], interval), Duration::from_secs(60));
    assert_eq!(blocked_wait(N, &[waiting(None)], interval), Duration::from_secs(600));
    assert_eq!(
        blocked_wait(N, &[waiting(Some(N + 7_200)), waiting(Some(N + 300))], interval),
        Duration::from_secs(330)
    );
    assert_eq!(
        blocked_wait(N, &[waiting(Some(N + 300))], Duration::from_secs(900)),
        Duration::from_secs(900)
    );
}

#[test]
fn blocked_line_shows_blocking_windows_and_reset_state() {
    let base = "auto: 'p07' at 96% and no account is a viable target";
    assert_eq!(
        blocked_line(
            "p07",
            96.0,
            &[Waiting {
                alias: "queh".into(),
                used_5h: Some(95.0),
                used_7d: None,
                free_at: Some(R),
            }]
        ),
        format!("{base}; 'queh' 5h 95% free at {}", crate::output::format_local_time(R))
    );
    assert_eq!(
        blocked_line(
            "p07",
            96.0,
            &[Waiting {
                alias: "queh".into(),
                used_5h: Some(97.0),
                used_7d: Some(99.0),
                free_at: Some(N + 90_000),
            }]
        ),
        format!("{base}; 'queh' 5h 97%, 7d 99% free at {}", crate::output::format_local_time(N + 90_000))
    );
    assert_eq!(
        blocked_line(
            "p07",
            96.0,
            &[Waiting {
                alias: "queh".into(),
                used_5h: Some(96.0),
                used_7d: None,
                free_at: None,
            }]
        ),
        format!("{base}; 'queh' 5h 96% no reset time")
    );
}

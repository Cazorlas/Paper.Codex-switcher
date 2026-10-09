use super::*;
use std::collections::HashMap;

const NOW: i64 = 1_000_000;
const DAY: i64 = 86_400;

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
        now: NOW,
        pool_size: 2,
        pool_exhausted: 0,
        team_priority: false,
    }
}

fn opts(threshold: f64, margin: f64, days: Option<u32>) -> AutoOptions {
    AutoOptions {
        threshold,
        margin,
        interval: Duration::from_secs(60),
        cooldown: Duration::from_secs(300),
        once: true,
        dry_run: false,
        json: false,
        toast: false,
        prefer_expiring_days: days,
    }
}

fn pool() -> Vec<(usage::Candidate, f64)> {
    vec![
        (cand("p05", 26.0, 4.0), 3.0),
        (cand("q", 0.0, 33.0), 2.0),
        (cand("p07", 5.0, 35.0), 1.0),
    ]
}

fn plans() -> HashMap<String, i64> {
    HashMap::from([
        ("p07".into(), NOW + 26 * DAY),
        ("p05".into(), NOW + 3 * DAY),
        ("q".into(), NOW + 14 * DAY),
    ])
}

fn p07_stay() -> Decision {
    Decision::Stay {
        pressure: 35.0,
        used_5h: 5.0,
        used_7d: 35.0,
    }
}

#[test]
fn d10_expiring_plan_overrides_the_higher_ranked_threshold_target() {
    let ranked = vec![
        (cand("q", 0.0, 33.0), 3.0),
        (cand("p05", 26.0, 4.0), 2.0),
        (cand("p07", 96.0, 35.0), 1.0),
    ];
    assert_eq!(
        decide_with_plans("p07", &ranked, &plans(), NOW, false, &opts(95.0, 10.0, Some(7)), 20.0),
        Decision::Switch {
            alias: "p05".into(),
            from_pressure: 96.0,
            to_pressure: 26.0,
            reason: SwitchReason::PlanEnding,
        }
    );
}

#[test]
fn d11_without_expiring_preference_switches_by_threshold_ranking() {
    let ranked = vec![
        (cand("q", 0.0, 33.0), 3.0),
        (cand("p05", 26.0, 4.0), 2.0),
        (cand("p07", 96.0, 35.0), 1.0),
    ];
    let options = opts(95.0, 10.0, None);
    let result = decide_with_plans("p07", &ranked, &plans(), NOW, false, &options, 20.0);
    assert_eq!(result, decide("p07", &ranked, false, &options, 20.0));
    assert_eq!(result, Decision::Switch {
        alias: "q".into(),
        from_pressure: 96.0,
        to_pressure: 33.0,
        reason: SwitchReason::Threshold,
    });
}

#[test]
fn d1_expiring_plan_switches_even_under_threshold() {
    assert_eq!(
        decide_with_plans("p07", &pool(), &plans(), NOW, false, &opts(95.0, 10.0, Some(7)), 20.0),
        Decision::Switch {
            alias: "p05".into(),
            from_pressure: 35.0,
            to_pressure: 26.0,
            reason: SwitchReason::PlanEnding,
        }
    );
}

#[test]
fn d2_without_expiring_preference_stays_under_threshold() {
    let ranked = pool();
    let options = opts(95.0, 10.0, None);
    let result = decide_with_plans("p07", &ranked, &plans(), NOW, false, &options, 20.0);
    assert_eq!(result, decide("p07", &ranked, false, &options, 20.0));
    assert_eq!(result, p07_stay());
}

#[test]
fn d3_lapsed_plan_is_not_a_target() {
    let mut ends = plans();
    ends.insert("p05".into(), NOW - 1);
    assert_eq!(decide_with_plans("p07", &pool(), &ends, NOW, false, &opts(95.0, 10.0, Some(7)), 20.0), p07_stay());
}

#[test]
fn d4_current_expiring_account_stays() {
    assert_eq!(
        decide_with_plans("p05", &pool(), &plans(), NOW, false, &opts(95.0, 10.0, Some(7)), 20.0),
        Decision::Stay { pressure: 26.0, used_5h: 26.0, used_7d: 4.0 }
    );
}

#[test]
fn d5_unknown_plan_dates_stay() {
    assert_eq!(decide_with_plans("p07", &pool(), &HashMap::new(), NOW, false, &opts(95.0, 10.0, Some(7)), 20.0), p07_stay());
}

#[test]
fn d6_plan_outside_the_expiring_window_stays() {
    let mut ends = plans();
    ends.insert("p05".into(), NOW + 8 * DAY);
    assert_eq!(decide_with_plans("p07", &pool(), &ends, NOW, false, &opts(95.0, 10.0, Some(7)), 20.0), p07_stay());
}

#[test]
fn d7_expiring_target_above_threshold_minus_margin_is_rejected() {
    let mut ranked = pool();
    ranked[0].0 = cand("p05", 90.0, 4.0);
    assert_eq!(decide_with_plans("p07", &ranked, &plans(), NOW, false, &opts(95.0, 10.0, Some(7)), 20.0), p07_stay());
}

#[test]
fn d8_ineligible_expiring_target_is_rejected() {
    let mut ranked = pool();
    ranked[0].0 = cand("p05", 100.0, 4.0);
    assert_eq!(decide_with_plans("p07", &ranked, &plans(), NOW, false, &opts(95.0, 10.0, Some(7)), 20.0), p07_stay());
}

#[test]
fn d9_unknown_current_plan_counts_as_never_ending() {
    let mut ends = plans();
    ends.remove("p07");
    assert_eq!(
        decide_with_plans("p07", &pool(), &ends, NOW, false, &opts(95.0, 10.0, Some(7)), 20.0),
        Decision::Switch { alias: "p05".into(), from_pressure: 35.0, to_pressure: 26.0, reason: SwitchReason::PlanEnding }
    );
}

#[test]
fn d12_current_plan_ending_first_stays_under_threshold() {
    let mut ends = plans();
    ends.insert("p07".into(), NOW + 2 * DAY);
    assert_eq!(decide_with_plans("p07", &pool(), &ends, NOW, false, &opts(95.0, 10.0, Some(7)), 20.0), p07_stay());
}

#[test]
fn d13_earliest_plan_end_wins_over_ranking() {
    let mut ends = plans();
    ends.insert("p05x".into(), NOW + DAY);
    let mut ranked = pool();
    ranked.push((cand("p05x", 20.0, 10.0), 0.5));
    assert_eq!(
        decide_with_plans("p07", &ranked, &ends, NOW, false, &opts(95.0, 10.0, Some(7)), 20.0),
        Decision::Switch { alias: "p05x".into(), from_pressure: 35.0, to_pressure: 20.0, reason: SwitchReason::PlanEnding }
    );
}

#[test]
fn g1_expiring_gate_opens_for_an_earlier_other_plan() {
    assert!(plan_ending_gate("p07", &plans(), NOW, 7));
}

#[test]
fn g2_expiring_gate_excludes_the_current_plan() {
    assert!(!plan_ending_gate("p05", &plans(), NOW, 7));
}

#[test]
fn g3_expiring_gate_stays_closed_without_plan_dates() {
    assert!(!plan_ending_gate("p07", &HashMap::new(), NOW, 7));
}

#[test]
fn g4_expiring_gate_excludes_lapsed_plans() {
    let mut ends = plans();
    ends.insert("p05".into(), NOW - 1);
    assert!(!plan_ending_gate("p07", &ends, NOW, 7));
}

#[test]
fn s1_expiring_stay_line_shows_both_windows() {
    assert_eq!(stay_line("paperengineer07", 5.0, 35.0), "auto: 'paperengineer07' at 5h 5%, 7d 35%, staying");
}

#[test]
fn s2_expiring_stay_line_rounds_usage_to_whole_percent() {
    assert_eq!(stay_line("a", 4.6, 0.0), "auto: 'a' at 5h 5%, 7d 0%, staying");
}

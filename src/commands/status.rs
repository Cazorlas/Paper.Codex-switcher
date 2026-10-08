//! `status`: one line about the active account, cheap enough for a shell prompt.

use crate::output::{self, print_json};
use crate::{auth, cache, profile, usage};
use anyhow::Result;

fn used(w: &Option<usage::WindowUsage>) -> Option<f64> {
    w.as_ref().and_then(|w| w.used_percent)
}

/// `alias 5h 22% 7d 47%`; a window the account does not have is left out.
pub(crate) fn short_line(alias: &str, u: &usage::UsageInfo) -> String {
    let mut line = alias.to_string();
    if let Some(p) = used(&u.primary) {
        line.push_str(&format!(" 5h {p:.0}%"));
    }
    if let Some(p) = used(&u.secondary) {
        line.push_str(&format!(" 7d {p:.0}%"));
    }
    line
}

pub(crate) async fn status_cmd(short: bool, force: bool, json: bool) -> Result<()> {
    let alias = profile::read_current();
    if alias.is_empty() {
        anyhow::bail!("no active profile; run `paper-codex-switch use <alias>` first");
    }
    let cached = if force { None } else { cache::get(&alias) };
    let usage = match cached {
        Some(u) => u,
        None => {
            let path = profile::profile_auth_path(&alias)?;
            usage::fetch_usage_retried_force(&alias, &path, &alias)
                .await
                .map_err(|e| anyhow::anyhow!("usage for '{alias}' unavailable: {}", e.summary))?
        }
    };
    if json {
        let info = auth::read_account_info(&profile::profile_auth_path(&alias)?);
        print_json(&serde_json::json!({
            "alias": alias,
            "account": output::account_to_json(&info, usage.plan_type.as_deref()),
            "usage": output::usage_to_json(Ok(&usage)),
        }));
    } else if short {
        println!("{}", short_line(&alias, &usage));
    } else {
        println!("{alias}");
        crate::commands::render::print_usage_line(&usage);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(p: f64) -> Option<usage::WindowUsage> {
        Some(usage::WindowUsage {
            used_percent: Some(p),
            resets_at: None,
            window_minutes: None,
        })
    }

    #[test]
    fn short_line_shows_used_percent_of_each_window() {
        let u = usage::UsageInfo {
            primary: window(21.6),
            secondary: window(47.0),
            ..Default::default()
        };
        assert_eq!(short_line("work", &u), "work 5h 22% 7d 47%");
    }

    #[test]
    fn short_line_leaves_out_a_missing_window() {
        let u = usage::UsageInfo {
            secondary: window(5.0),
            ..Default::default()
        };
        assert_eq!(short_line("free", &u), "free 7d 5%");
        assert_eq!(short_line("none", &usage::UsageInfo::default()), "none");
    }
}

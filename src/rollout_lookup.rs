//! Find the Codex thread an interactive `launch --auto-swap` child was running,
//! so the swap can relaunch it with `resume <thread-id>`.
//!
//! Codex 0.168 scopes `resume --last` to the signed-in ChatGPT account, so a
//! thread started on account A is invisible to `--last` on account B. Direct
//! access by thread id still works across accounts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Rollout files under `<codex home>/sessions`, keyed by path, with their mtime.
pub(crate) type RolloutSnapshot = HashMap<PathBuf, SystemTime>;

/// The thread id a `resume <id>` argv names, if any. `resume --last`, a bare
/// `resume`, and positionals after `--` (a prompt) name no id.
pub(crate) fn thread_from_args(args: &[String]) -> Option<String> {
    let resume_at = crate::launch::codex_subcommand_index(args)?;
    if args[resume_at] != "resume" {
        return None;
    }
    if crate::launch::resume_flag_before_separator(args, resume_at, "--last") {
        return None;
    }
    let separator = args[resume_at + 1..]
        .iter()
        .position(|arg| arg == "--")
        .map_or(args.len(), |offset| resume_at + 1 + offset);
    crate::launch::resume_positional_indices(args, resume_at)
        .into_iter()
        .find(|index| *index < separator)
        .map(|index| args[index].clone())
        .filter(|id| !id.trim().is_empty())
}

/// Every `rollout-*.jsonl` under `<codex_home>/sessions` with its mtime. A
/// missing or unreadable directory yields an empty snapshot.
pub(crate) fn snapshot_rollouts(codex_home: &Path) -> RolloutSnapshot {
    let mut snapshot = RolloutSnapshot::new();
    let mut pending = vec![codex_home.join("sessions")];
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(path);
                continue;
            }
            if !kind.is_file() || !is_rollout_name(&path) {
                continue;
            }
            if let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) {
                snapshot.insert(path, modified);
            }
        }
    }
    snapshot
}

fn is_rollout_name(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("rollout-") && name.ends_with(".jsonl"))
}

/// The thread id of the rollout the child wrote to since `before` was taken:
/// rollouts new since the snapshot win over ones whose mtime grew; within each
/// group the newest mtime wins. A candidate whose first record is unreadable
/// or not `session_meta` is skipped, and so is one whose `payload.cwd` differs
/// from `cwd` (another Codex session sharing this codex home). A candidate
/// without `payload.cwd` stays eligible.
pub(crate) fn thread_since(
    before: &RolloutSnapshot,
    after: &RolloutSnapshot,
    cwd: Option<&Path>,
) -> Option<String> {
    let mut created = Vec::new();
    let mut modified = Vec::new();
    for (path, mtime) in after {
        match before.get(path) {
            None => created.push((path, *mtime)),
            Some(old) if mtime > old => modified.push((path, *mtime)),
            Some(_) => {}
        }
    }
    created.sort_by(|a, b| b.1.cmp(&a.1));
    modified.sort_by(|a, b| b.1.cmp(&a.1));
    created
        .into_iter()
        .chain(modified)
        .find_map(|(path, _)| rollout_thread_id(path, cwd))
}

/// Snapshot `codex_home` now and compare it against `before`; `cwd` is the
/// child's working directory.
pub(crate) fn find_thread_since(
    codex_home: &Path,
    before: &RolloutSnapshot,
    cwd: Option<&Path>,
) -> Option<String> {
    thread_since(before, &snapshot_rollouts(codex_home), cwd)
}

/// `payload.id` (else `payload.session_id`) of a rollout's first record when
/// that record is `session_meta` and its `payload.cwd`, if any, is `cwd`.
fn rollout_thread_id(path: &Path, cwd: Option<&Path>) -> Option<String> {
    let value = crate::provider::read_rollout_metadata(path).ok()?;
    if value.get("type").and_then(serde_json::Value::as_str) != Some("session_meta") {
        return None;
    }
    let payload = value.get("payload")?;
    if let (Some(recorded), Some(cwd)) = (
        payload.get("cwd").and_then(serde_json::Value::as_str),
        cwd.and_then(Path::to_str),
    ) && !same_cwd(recorded, cwd, cfg!(windows))
    {
        return None;
    }
    ["id", "session_id"]
        .iter()
        .filter_map(|key| payload.get(*key).and_then(serde_json::Value::as_str))
        .find(|id| !id.trim().is_empty())
        .map(str::to_string)
}

/// Whether two working-directory strings name the same directory. One trailing
/// separator is ignored; with `windows`, the comparison is case-insensitive,
/// treats `\` and `/` alike and ignores a `\\?\` verbatim prefix.
fn same_cwd(a: &str, b: &str, windows: bool) -> bool {
    normalize_cwd(a, windows) == normalize_cwd(b, windows)
}

fn normalize_cwd(text: &str, windows: bool) -> String {
    let mut text = if windows {
        let text = text.strip_prefix(r"\\?\").unwrap_or(text);
        text.replace('\\', "/").to_lowercase()
    } else {
        text.to_string()
    };
    if text.len() > 1 && text.ends_with('/') {
        text.pop();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::{snapshot_rollouts, thread_from_args, thread_since};
    use std::fs::File;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    fn argv(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    fn meta_line(key: &str, id: &str) -> String {
        format!(
            "{{\"timestamp\":\"2026-11-19T00:00:00Z\",\"ordinal\":0,\"type\":\"session_meta\",\"payload\":{{\"creator_account_id\":\"acct\",\"{key}\":\"{id}\"}}}}\n{{\"type\":\"event\"}}\n"
        )
    }

    fn write_rollout(home: &Path, name: &str, body: &str, mtime_secs: u64) -> PathBuf {
        let dir = home.join("sessions").join("2026").join("11").join("19");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        set_mtime(&path, mtime_secs);
        path
    }

    fn set_mtime(path: &Path, secs: u64) {
        let file = File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + secs))
            .unwrap();
    }

    #[test]
    fn id_comes_from_resume_args() {
        assert_eq!(
            thread_from_args(&argv(&["resume", "01a1f460-bb66"])).as_deref(),
            Some("01a1f460-bb66")
        );
        assert_eq!(
            thread_from_args(&argv(&["--model", "gpt", "resume", "abc", "prompt"])).as_deref(),
            Some("abc")
        );
        assert_eq!(thread_from_args(&argv(&["resume", "--last"])), None);
        assert_eq!(thread_from_args(&argv(&["resume"])), None);
        assert_eq!(thread_from_args(&argv(&["resume", "--", "prompt"])), None);
        assert_eq!(thread_from_args(&argv(&["exec", "do it"])), None);
        assert_eq!(thread_from_args(&[]), None);
    }

    #[test]
    fn new_rollout_beats_an_older_modified_one() {
        let home = tempfile::tempdir().unwrap();
        let old = write_rollout(home.path(), "rollout-old.jsonl", &meta_line("id", "old"), 10);
        let before = snapshot_rollouts(home.path());
        write_rollout(home.path(), "rollout-new.jsonl", &meta_line("id", "new"), 20);
        set_mtime(&old, 30);
        let after = snapshot_rollouts(home.path());
        assert_eq!(thread_since(&before, &after, None).as_deref(), Some("new"));
    }

    #[test]
    fn modified_rollout_is_chosen_when_none_is_new() {
        let home = tempfile::tempdir().unwrap();
        let resumed = write_rollout(home.path(), "rollout-a.jsonl", &meta_line("id", "a"), 10);
        write_rollout(home.path(), "rollout-b.jsonl", &meta_line("id", "b"), 15);
        let before = snapshot_rollouts(home.path());
        set_mtime(&resumed, 40);
        let after = snapshot_rollouts(home.path());
        assert_eq!(thread_since(&before, &after, None).as_deref(), Some("a"));
    }

    #[test]
    fn unchanged_rollouts_yield_none() {
        let home = tempfile::tempdir().unwrap();
        write_rollout(home.path(), "rollout-a.jsonl", &meta_line("id", "a"), 10);
        let before = snapshot_rollouts(home.path());
        let after = snapshot_rollouts(home.path());
        assert_eq!(thread_since(&before, &after, None), None);
    }

    #[test]
    fn newest_mtime_wins_among_several() {
        let home = tempfile::tempdir().unwrap();
        let before = snapshot_rollouts(home.path());
        write_rollout(home.path(), "rollout-1.jsonl", &meta_line("id", "one"), 10);
        write_rollout(home.path(), "rollout-3.jsonl", &meta_line("id", "three"), 30);
        write_rollout(home.path(), "rollout-2.jsonl", &meta_line("id", "two"), 20);
        let after = snapshot_rollouts(home.path());
        assert_eq!(thread_since(&before, &after, None).as_deref(), Some("three"));
    }

    #[test]
    fn session_id_is_the_fallback() {
        let home = tempfile::tempdir().unwrap();
        let before = snapshot_rollouts(home.path());
        write_rollout(
            home.path(),
            "rollout-s.jsonl",
            &meta_line("session_id", "from-session-id"),
            10,
        );
        let after = snapshot_rollouts(home.path());
        assert_eq!(thread_since(&before, &after, None).as_deref(), Some("from-session-id"));
    }

    #[test]
    fn malformed_first_line_is_skipped() {
        let home = tempfile::tempdir().unwrap();
        let before = snapshot_rollouts(home.path());
        write_rollout(home.path(), "rollout-good.jsonl", &meta_line("id", "good"), 10);
        write_rollout(home.path(), "rollout-bad.jsonl", "{not json\n", 50);
        write_rollout(
            home.path(),
            "rollout-other.jsonl",
            "{\"type\":\"event\",\"payload\":{\"id\":\"wrong\"}}\n",
            40,
        );
        let after = snapshot_rollouts(home.path());
        assert_eq!(thread_since(&before, &after, None).as_deref(), Some("good"));
    }

    #[test]
    fn non_rollout_files_are_ignored() {
        let home = tempfile::tempdir().unwrap();
        let before = snapshot_rollouts(home.path());
        write_rollout(home.path(), "notes.jsonl", &meta_line("id", "x"), 10);
        write_rollout(home.path(), "rollout-x.jsonl.zst", &meta_line("id", "y"), 10);
        let after = snapshot_rollouts(home.path());
        assert!(after.is_empty());
        assert_eq!(thread_since(&before, &after, None), None);
    }

    #[test]
    fn empty_or_missing_sessions_dir_yields_none() {
        let home = tempfile::tempdir().unwrap();
        let missing = snapshot_rollouts(home.path());
        assert!(missing.is_empty());
        assert_eq!(thread_since(&missing, &snapshot_rollouts(home.path()), None), None);
        std::fs::create_dir_all(home.path().join("sessions")).unwrap();
        let empty = snapshot_rollouts(home.path());
        assert!(empty.is_empty());
        assert_eq!(super::find_thread_since(home.path(), &empty, None), None);
    }

    fn cwd_meta_line(id: &str, cwd: &str) -> String {
        let record = serde_json::json!({
            "timestamp": "2026-11-19T00:00:00Z",
            "type": "session_meta",
            "payload": { "id": id, "cwd": cwd },
        });
        format!("{record}\n")
    }

    #[test]
    fn newer_rollout_in_another_cwd_loses_to_ours() {
        let home = tempfile::tempdir().unwrap();
        let ours = home.path().join("ours");
        let theirs = home.path().join("theirs");
        let before = snapshot_rollouts(home.path());
        write_rollout(
            home.path(),
            "rollout-ours.jsonl",
            &cwd_meta_line("ours", ours.to_str().unwrap()),
            10,
        );
        write_rollout(
            home.path(),
            "rollout-theirs.jsonl",
            &cwd_meta_line("theirs", theirs.to_str().unwrap()),
            50,
        );
        let after = snapshot_rollouts(home.path());
        assert_eq!(
            thread_since(&before, &after, Some(&ours)).as_deref(),
            Some("ours")
        );
        assert_eq!(
            thread_since(&before, &after, Some(&theirs)).as_deref(),
            Some("theirs")
        );
        let elsewhere = home.path().join("elsewhere");
        assert_eq!(thread_since(&before, &after, Some(&elsewhere)), None);
    }

    #[test]
    fn rollout_without_cwd_stays_eligible() {
        let home = tempfile::tempdir().unwrap();
        let ours = home.path().join("ours");
        let before = snapshot_rollouts(home.path());
        write_rollout(home.path(), "rollout-plain.jsonl", &meta_line("id", "plain"), 10);
        let after = snapshot_rollouts(home.path());
        assert_eq!(
            thread_since(&before, &after, Some(&ours)).as_deref(),
            Some("plain")
        );
    }

    #[test]
    fn cwd_normalisation_matches_windows_case_and_separators() {
        use super::same_cwd;
        assert!(same_cwd(r"D:\Repo\Work", "d:/repo/work/", true));
        assert!(same_cwd(r"\\?\D:\Repo\Work\", r"D:\REPO\WORK", true));
        assert!(!same_cwd(r"D:\Repo\Work", r"D:\Repo\Other", true));
        assert!(same_cwd("/home/u/work/", "/home/u/work", false));
        assert!(!same_cwd("/home/u/Work", "/home/u/work", false));
        assert!(same_cwd("/", "/", false));
    }

    #[cfg(windows)]
    #[test]
    fn windows_cwd_differing_in_case_and_separator_is_found() {
        let home = tempfile::tempdir().unwrap();
        let ours = home.path().join("Ours");
        let recorded = format!(
            "{}/",
            ours.to_str().unwrap().replace('\\', "/").to_uppercase()
        );
        let before = snapshot_rollouts(home.path());
        write_rollout(
            home.path(),
            "rollout-ours.jsonl",
            &cwd_meta_line("ours", &recorded),
            10,
        );
        let after = snapshot_rollouts(home.path());
        assert_eq!(
            thread_since(&before, &after, Some(&ours)).as_deref(),
            Some("ours")
        );
    }
}

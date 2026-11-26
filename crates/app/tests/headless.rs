//! #69: the headless driver through the built binary.
use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::Value;

fn cadence() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cadence"))
}

/// Runs the binary with requests on stdin and returns the response lines.
fn run(args: &[&str], requests: &str) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut child = cadence()
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("stdin")?
        .write_all(requests.as_bytes())?;
    let output = child.wait_with_output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)?
        .lines()
        .map(|line| serde_json::from_str(line).map_err(Into::into))
        .collect()
}

fn by_id<'a>(responses: &'a [Value], id: &str) -> Option<&'a Value> {
    responses.iter().find(|r| r["id"] == id)
}

#[test]
fn the_scoring_cat_transcript_counts_exactly() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = include_str!("../../../fixtures/headless-session.jsonl");
    let responses = run(&["headless", "--stdio", "--ephemeral-profile"], fixture)?;
    assert!(responses.iter().all(|r| r["ok"] == true), "{responses:?}");
    let snapshot = by_id(&responses, "7").ok_or("snapshot")?;
    let words = &snapshot["result"]["tracks"][0];
    assert_eq!(words["first_pass"]["correct"], "2");
    assert_eq!(words["first_pass"]["attempted"], "3");
    assert_eq!(words["attempt"]["correct"], "3");
    assert_eq!(words["attempt"]["attempted"], "4");
    assert_eq!(words["deletions"], "1");
    let capture = by_id(&responses, "8").ok_or("capture")?;
    assert!(
        capture["result"]["text"]
            .as_str()
            .is_some_and(|t| t.contains("cat"))
    );
    Ok(())
}

#[test]
fn a_stale_revision_changes_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let requests = concat!(
        r#"{"v":1,"id":"1","op":"hello","args":{"scenario":"scoring-cat"}}"#,
        "\n",
        r#"{"v":1,"id":"2","op":"snapshot","args":{}}"#,
        "\n",
        r#"{"v":1,"id":"3","op":"action","args":{"name":"StartDrill"},"expected_state_revision":"0"}"#,
        "\n",
        r#"{"v":1,"id":"4","op":"snapshot","args":{}}"#,
        "\n",
        r#"{"v":1,"id":"5","op":"quit","args":{"flush":true}}"#,
        "\n",
    );
    let responses = run(&["headless", "--stdio", "--ephemeral-profile"], requests)?;
    let before = &by_id(&responses, "2").ok_or("before")?["result"];
    let stale = by_id(&responses, "3").ok_or("stale")?;
    assert_eq!(stale["error"]["code"], "StaleState");
    let after = &by_id(&responses, "4").ok_or("after")?["result"];
    assert_eq!(before["state_revision"], after["state_revision"]);
    assert_eq!(after["screen"], "today", "the action never ran");
    Ok(())
}

#[test]
fn headless_without_a_profile_creates_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let home = tempfile::tempdir()?;
    let output = cadence()
        .args(["headless", "--stdio"])
        .env("HOME", home.path())
        .env_remove("XDG_DATA_HOME")
        .stdin(Stdio::null())
        .output()?;
    assert!(!output.status.success());
    assert_eq!(
        std::fs::read_dir(home.path())?.count(),
        0,
        "nothing under HOME"
    );
    Ok(())
}

#[test]
fn a_synthetic_session_never_reaches_the_live_recap() -> Result<(), Box<dyn std::error::Error>> {
    let profile = tempfile::tempdir()?;
    let dir = profile.path().to_str().ok_or("path")?;
    let requests = concat!(
        r#"{"v":1,"id":"1","op":"hello","args":{"scenario":"scoring-cat"}}"#,
        "\n",
        r#"{"v":1,"id":"2","op":"action","args":{"name":"StartDrill"}}"#,
        "\n",
        r#"{"v":1,"id":"3","op":"text","args":{"text":"cat","inter_key_microseconds":300000}}"#,
        "\n",
        r#"{"v":1,"id":"4","op":"quit","args":{"flush":true}}"#,
        "\n",
    );
    let responses = run(&["headless", "--stdio", "--profile-dir", dir], requests)?;
    let quit = by_id(&responses, "4").ok_or("quit")?;
    assert_eq!(quit["result"]["saved"], true, "{quit}");
    let store = cadence_data::ProgressStore::open_in(profile.path())?;
    assert_eq!(store.session_history()?.len(), 1, "the session is kept");
    let bootstrap = store.bootstrap(10)?;
    assert!(bootstrap.history.recent.is_empty());
    assert_eq!(
        bootstrap.history.completed_sessions + bootstrap.history.interrupted_sessions,
        0
    );
    assert!(bootstrap.learner.sessions.is_empty());
    assert_eq!(bootstrap.next_session.get(), 2, "its id stays reserved");
    Ok(())
}

#[test]
fn doctor_reports_json() -> Result<(), Box<dyn std::error::Error>> {
    let profile = tempfile::tempdir()?;
    let output = cadence()
        .args([
            "doctor",
            "--json",
            "--profile-dir",
            profile.path().to_str().ok_or("path")?,
        ])
        .output()?;
    assert!(output.status.success());
    let report: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report["version"], env!("CARGO_PKG_VERSION"));
    assert!(report["schema"].as_i64().is_some());
    Ok(())
}

fn snapshot_of<'a>(responses: &'a [Value], id: &str) -> Result<&'a Value, String> {
    by_id(responses, id)
        .map(|r| &r["result"])
        .ok_or_else(|| format!("no response {id}"))
}

/// #71 journey 8: a consumed acknowledgement dispatches a follow-up
/// checkpoint in its parent's causal group; the causal barrier does not
/// return until the follow-up is drained, failing at its watchdog with the
/// pending job ids meanwhile.
#[test]
fn a_causal_barrier_waits_for_the_follow_up() -> Result<(), Box<dyn std::error::Error>> {
    let mut requests = vec![
        r#"{"v":1,"id":"1","op":"hello","args":{"scenario":"scoring-cat","fixture_delivery":"manual"}}"#.to_string(),
        r#"{"v":1,"id":"2","op":"action","args":{"name":"StartDrill"}}"#.to_string(),
        // Completing "cat" dispatches a checkpoint (held: manual delivery);
        // completing it again makes another due while the first is pending.
        r#"{"v":1,"id":"3","op":"text","args":{"text":"cat","inter_key_microseconds":300000}}"#.to_string(),
        r#"{"v":1,"id":"4","op":"text","args":{"text":"cat","inter_key_microseconds":300000}}"#.to_string(),
        r#"{"v":1,"id":"5","op":"snapshot","args":{}}"#.to_string(),
    ];
    let first = run(
        &["headless", "--stdio", "--ephemeral-profile"],
        &(requests.join("\n") + "\n"),
    )?;
    let snapshot = snapshot_of(&first, "5")?;
    let pending: Vec<String> = snapshot["pending_jobs"]
        .as_array()
        .ok_or("pending")?
        .iter()
        .filter_map(|j| j.as_str().map(str::to_string))
        .collect();
    assert_eq!(pending.len(), 1, "{snapshot}");
    let parent = pending[0].clone();
    let through = snapshot["event_seq"].as_str().ok_or("seq")?.to_string();
    requests.push(format!(
        r#"{{"v":1,"id":"6","op":"barrier","args":{{"scope":"causal","through_event_seq":"{through}","watchdog_milliseconds":100}}}}"#
    ));
    requests.push(format!(
        r#"{{"v":1,"id":"7","op":"deliver","args":{{"job_id":"{parent}","variant":"success"}}}}"#
    ));
    requests.push(r#"{"v":1,"id":"8","op":"snapshot","args":{}}"#.to_string());
    requests.push(format!(
        r#"{{"v":1,"id":"9","op":"barrier","args":{{"scope":"causal","through_event_seq":"{through}","watchdog_milliseconds":100}}}}"#
    ));
    let second = run(
        &["headless", "--stdio", "--ephemeral-profile"],
        &(requests.join("\n") + "\n"),
    )?;
    let waiting = by_id(&second, "6").ok_or("barrier 6")?;
    assert_eq!(waiting["error"]["code"], "BarrierTimeout");
    assert!(
        waiting["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains(&parent)
    );
    let children = snapshot_of(&second, "8")?["pending_jobs"].clone();
    let child = children[0]
        .as_str()
        .ok_or("a follow-up was dispatched")?
        .to_string();
    assert_ne!(child, parent);
    let still = by_id(&second, "9").ok_or("barrier 9")?;
    assert_eq!(
        still["error"]["code"], "BarrierTimeout",
        "the child is in the parent's scope"
    );
    assert!(
        still["error"]["message"]
            .as_str()
            .unwrap_or("")
            .contains(&child)
    );
    requests.push(format!(
        r#"{{"v":1,"id":"10","op":"deliver","args":{{"job_id":"{child}","variant":"success"}}}}"#
    ));
    requests.push(format!(
        r#"{{"v":1,"id":"11","op":"barrier","args":{{"scope":"causal","through_event_seq":"{through}","watchdog_milliseconds":100}}}}"#
    ));
    let third = run(
        &["headless", "--stdio", "--ephemeral-profile"],
        &(requests.join("\n") + "\n"),
    )?;
    assert_eq!(by_id(&third, "11").ok_or("barrier 11")?["ok"], true);
    Ok(())
}

/// #71: a durable barrier returns the watermark it covered; a key typed
/// afterwards is not reported persisted; barriers do not move practice
/// time; a barrier through a future event is out of scope.
#[test]
fn a_durable_barrier_covers_its_watermark_only() -> Result<(), Box<dyn std::error::Error>> {
    let requests = concat!(
        r#"{"v":1,"id":"1","op":"hello","args":{"scenario":"scoring-cat"}}"#,
        "\n",
        r#"{"v":1,"id":"2","op":"action","args":{"name":"StartDrill"}}"#,
        "\n",
        r#"{"v":1,"id":"3","op":"text","args":{"text":"ca","inter_key_microseconds":300000}}"#,
        "\n",
        r#"{"v":1,"id":"4","op":"snapshot","args":{}}"#,
        "\n",
        r#"{"v":1,"id":"5","op":"barrier","args":{"scope":"durable","through_event_seq":"5","watchdog_milliseconds":100}}"#,
        "\n",
        r#"{"v":1,"id":"6","op":"snapshot","args":{}}"#,
        "\n",
        r#"{"v":1,"id":"7","op":"key","args":{"key":"x","modifiers":[],"kind":"press","source":"unverified"}}"#,
        "\n",
        r#"{"v":1,"id":"8","op":"snapshot","args":{}}"#,
        "\n",
        r#"{"v":1,"id":"9","op":"barrier","args":{"scope":"causal","through_event_seq":"999","watchdog_milliseconds":100}}"#,
        "\n",
    );
    let responses = run(&["headless", "--stdio", "--ephemeral-profile"], requests)?;
    let barrier = by_id(&responses, "5").ok_or("barrier")?;
    assert_eq!(barrier["ok"], true, "{barrier}");
    let watermark = barrier["result"]["watermark"]
        .as_str()
        .ok_or("watermark")?
        .to_string();
    assert_eq!(
        barrier["result"]["durable_commit_seq"].as_str(),
        Some(watermark.as_str())
    );
    let before = snapshot_of(&responses, "4")?;
    let after = snapshot_of(&responses, "6")?;
    assert_eq!(
        before["active_micros"], after["active_micros"],
        "no practice time"
    );
    assert_eq!(before["elapsed_micros"], after["elapsed_micros"]);
    let later = snapshot_of(&responses, "8")?;
    assert_eq!(
        later["durable_commit_seq"].as_str(),
        Some(watermark.as_str()),
        "the later key is not persisted"
    );
    assert_eq!(
        by_id(&responses, "9").ok_or("future")?["error"]["code"],
        "BarrierOutOfScope"
    );
    Ok(())
}

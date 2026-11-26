//! #70: golden frames from the built binary's `capture` (cells, styles and
//! cursor, not ANSI bytes), the frame invariants, and that capture changes
//! no state. `CADENCE_UPDATE_GOLDEN=1` rewrites the goldens.
#![expect(
    clippy::arithmetic_side_effects,
    reason = "test fixtures count small bounded numbers of cells and requests"
)]
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::Value;

fn run(requests: &[String]) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cadence"))
        .args(["headless", "--stdio", "--ephemeral-profile"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("stdin")?
        .write_all(requests.join("\n").as_bytes())?;
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

fn by_id<'a>(responses: &'a [Value], id: &str) -> Result<&'a Value, String> {
    responses
        .iter()
        .find(|r| r["id"] == id)
        .ok_or_else(|| format!("no response {id}"))
}

fn request(id: u32, op: &str, args: &str) -> String {
    format!(r#"{{"v":1,"id":"{id}","op":"{op}","args":{args}}}"#)
}

/// The golden form of a frame (same as `cadence_driver::capture::Frame::golden`).
fn golden(frame: &Value) -> Result<String, Box<dyn std::error::Error>> {
    let columns = usize::try_from(frame["columns"].as_u64().ok_or("columns")?)?;
    let cells = frame["cells"].as_array().ok_or("cells")?;
    let renditions = frame["row_renditions"].as_array().ok_or("renditions")?;
    let rows = usize::try_from(frame["rows"].as_u64().ok_or("rows")?)?;
    assert_eq!(cells.len(), columns * rows, "exactly columns x rows cells");
    let mut out = String::new();
    for (index, row) in cells.chunks(columns).enumerate() {
        let text: String = row
            .iter()
            .filter(|c| c["continuation"] != true)
            .filter_map(|c| c["symbol"].as_str())
            .collect();
        // The version would change every golden on a release.
        let text = text.replace(env!("CARGO_PKG_VERSION"), "<version>");
        out.push_str(&format!(
            "{index:02} {} |{}\n",
            renditions
                .get(index)
                .and_then(Value::as_str)
                .unwrap_or("normal"),
            text
        ));
        let mut runs: Vec<(String, usize)> = Vec::new();
        for cell in row {
            let modifiers: Vec<&str> = cell["modifiers"]
                .as_array()
                .ok_or("modifiers")?
                .iter()
                .filter_map(Value::as_str)
                .collect();
            let style = format!(
                "{}/{}/{}",
                cell["foreground"].as_str().unwrap_or(""),
                cell["background"].as_str().unwrap_or(""),
                modifiers.join("+")
            );
            match runs.last_mut() {
                Some((last, count)) if *last == style => *count += 1,
                Some(_) | None => runs.push((style, 1)),
            }
        }
        let runs: Vec<String> = runs
            .iter()
            .map(|(style, count)| format!("{count}×{style}"))
            .collect();
        out.push_str(&format!("   {}\n", runs.join(" ")));
    }
    let cursor = &frame["cursor"];
    out.push_str(&format!(
        "cursor visible={} column={} row={}\n",
        cursor["visible"], cursor["column"], cursor["row"]
    ));
    Ok(out)
}

fn check(name: &str, frame: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.txt"));
    let actual = golden(frame)?;
    if std::env::var_os("CADENCE_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().ok_or("parent")?)?;
        std::fs::write(&path, &actual)?;
        return Ok(());
    }
    let expected =
        std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    assert_eq!(actual, expected, "golden frame {name} differs");
    Ok(())
}

/// The requests that bring a scenario to a screen, then a capture.
fn screen(scenario: &str, steps: &[(&str, &str)], columns: u16, rows: u16) -> Vec<String> {
    let mut requests = vec![
        request(1, "hello", &format!(r#"{{"scenario":"{scenario}"}}"#)),
        request(
            2,
            "resize",
            &format!(r#"{{"columns":{columns},"rows":{rows}}}"#),
        ),
    ];
    for (index, (op, args)) in steps.iter().enumerate() {
        requests.push(request(u32::try_from(index).unwrap_or(0) + 3, op, args));
    }
    requests.push(request(90, "capture", r#"{"format":"cells"}"#));
    requests
}

const START: &[(&str, &str)] = &[];
const PRACTICE: &[(&str, &str)] = &[
    ("action", r#"{"name":"StartDrill"}"#),
    ("text", r#"{"text":"cx","inter_key_microseconds":300000}"#),
];
const NUMPAD: &[(&str, &str)] = &[
    ("action", r#"{"name":"StartDrill"}"#),
    ("text", r#"{"text":"01","inter_key_microseconds":300000}"#),
];
const RESULTS: &[(&str, &str)] = &[
    ("action", r#"{"name":"StartDrill"}"#),
    ("text", r#"{"text":"cat","inter_key_microseconds":300000}"#),
    (
        "key",
        r#"{"key":"c","modifiers":["control"],"kind":"press","source":"unverified"}"#,
    ),
];
const SAVE_ERROR: &[(&str, &str)] = &[
    ("action", r#"{"name":"StartDrill"}"#),
    ("text", r#"{"text":"cat","inter_key_microseconds":300000}"#),
];

#[test]
fn golden_frames_at_60x18_and_100x32() -> Result<(), Box<dyn std::error::Error>> {
    for (columns, rows) in [(60_u16, 18_u16), (100, 32)] {
        for (name, scenario, steps) in [
            ("start", "scoring-cat", START),
            ("practice", "scoring-cat", PRACTICE),
            ("numpad", "numpad-small", NUMPAD),
            ("results", "scoring-cat", RESULTS),
            ("save-error", "save-error", SAVE_ERROR),
        ] {
            let responses = run(&screen(scenario, steps, columns, rows))?;
            let capture = by_id(&responses, "90")?;
            assert_eq!(capture["ok"], true, "{name}: {capture}");
            check(
                &format!("{name}-{columns}x{rows}"),
                &capture["result"]["frame"],
            )?;
        }
    }
    // Too small: the practice minimum is not met.
    let responses = run(&screen("scoring-cat", PRACTICE, 40, 12))?;
    check(
        "too-small-40x12",
        &by_id(&responses, "90")?["result"]["frame"],
    )?;
    Ok(())
}

#[test]
fn capture_changes_no_state() -> Result<(), Box<dyn std::error::Error>> {
    let mut requests = screen("scoring-cat", PRACTICE, 100, 32);
    requests.push(request(91, "capture", r#"{"format":"both"}"#));
    let responses = run(&requests)?;
    let first = &by_id(&responses, "90")?["result"];
    let second = &by_id(&responses, "91")?["result"];
    assert_eq!(first["state_revision"], second["state_revision"]);
    assert_eq!(first["frame"], second["frame"]);
    Ok(())
}

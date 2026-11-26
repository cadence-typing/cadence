//! #80: headless `action` uses the command table: an action the current
//! state does not offer is refused and changes nothing.
use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::Value;

fn run(requests: &[&str]) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
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
    String::from_utf8(output.stdout)?
        .lines()
        .map(|line| serde_json::from_str(line).map_err(Into::into))
        .collect()
}

#[test]
fn an_action_not_offered_now_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let responses = run(&[
        r#"{"v":1,"id":"1","op":"hello","args":{"scenario":"scoring-cat"}}"#,
        r#"{"v":1,"id":"2","op":"action","args":{"name":"Continue"}}"#,
        r#"{"v":1,"id":"3","op":"action","args":{"name":"NoSuchThing"}}"#,
        r#"{"v":1,"id":"4","op":"action","args":{"name":"Help"}}"#,
        r#"{"v":1,"id":"5","op":"action","args":{"name":"Begin"}}"#,
        r#"{"v":1,"id":"6","op":"action","args":{"name":"Help"}}"#,
        r#"{"v":1,"id":"7","op":"action","args":{"name":"Begin"}}"#,
    ])?;
    let by = |id: &str| {
        responses
            .iter()
            .find(|r| r["id"] == id)
            .cloned()
            .unwrap_or(Value::Null)
    };
    assert_eq!(
        by("2")["ok"],
        false,
        "Continue is not offered on the start screen"
    );
    assert_eq!(by("2")["error"]["code"], "InvalidArgument");
    assert_eq!(by("3")["ok"], false);
    assert_eq!(by("4")["ok"], true, "help opens");
    assert_eq!(
        by("5")["ok"],
        false,
        "help is open: only closing it is offered"
    );
    assert_eq!(by("6")["ok"], true, "help closes");
    assert_eq!(by("7")["ok"], true, "now practice begins");
    Ok(())
}

//! #68: the strict envelope, stable codes and the bounded response cache.
#![expect(
    clippy::arithmetic_side_effects,
    reason = "test fixtures count small bounded numbers of requests"
)]
use cadence_driver::envelope::{Code, Request, parse};
use cadence_driver::protocol::{CACHE_CAPACITY, Host, Protocol};
use serde_json::{Map, Value, json};

/// A host that counts applications and answers with its revision.
#[derive(Default)]
struct Counter {
    applied: u64,
}

impl Host for Counter {
    fn state_revision(&self) -> u64 {
        self.applied
    }
    fn apply(&mut self, _request: &Request) -> Result<Map<String, Value>, (Code, String)> {
        self.applied += 1;
        let mut result = Map::new();
        result.insert("state_revision".into(), json!(self.applied.to_string()));
        result.insert("event_seq".into(), json!(self.applied.to_string()));
        Ok(result)
    }
}

fn code(response: &str) -> Option<String> {
    let value: Value = serde_json::from_str(response).ok()?;
    value["error"]["code"].as_str().map(str::to_string)
}

fn hello(protocol: &mut Protocol, host: &mut Counter) {
    let response = protocol.handle(br#"{"v":1,"id":"1","op":"hello","args":{}}"#, host);
    assert!(response.contains(r#""ok":true"#), "{response}");
}

#[test]
fn every_fixture_request_parses() {
    let fixture = include_str!("../../../fixtures/headless-session.jsonl");
    for line in fixture.lines() {
        assert!(parse(line.as_bytes()).is_ok(), "{line}");
    }
}

#[test]
fn each_rejection_class_has_its_code_and_mutates_nothing() {
    let cases: [(&[u8], &str); 11] = [
        (b"{not json", "MalformedJson"),
        (b"[1,2]", "MalformedJson"),
        (
            br#"{"v":2,"id":"5","op":"snapshot","args":{}}"#,
            "UnsupportedVersion",
        ),
        (
            br#"{"v":1,"id":"05","op":"snapshot","args":{}}"#,
            "InvalidId",
        ),
        (
            br#"{"v":1,"id":"99999999999999999999","op":"snapshot","args":{}}"#,
            "InvalidId",
        ),
        (
            br#"{"v":1,"id":"5","op":"snapshot","args":{},"extra":1}"#,
            "UnknownField",
        ),
        (
            br#"{"v":1,"id":"5","op":"teleport","args":{}}"#,
            "UnknownOperation",
        ),
        (
            br#"{"v":1,"id":"5","op":"capture","args":{"format":"pdf"}}"#,
            "InvalidArgument",
        ),
        (
            br#"{"v":1,"id":"5","op":"snapshot","args":{"zoom":1}}"#,
            "UnknownField",
        ),
        (
            br#"{"v":1,"id":"5","op":"resize","args":{"columns":0,"rows":10}}"#,
            "InvalidArgument",
        ),
        (
            br#"{"v":1,"id":"5","op":"text","args":{"text":"a\tb","inter_key_microseconds":1}}"#,
            "InvalidArgument",
        ),
    ];
    let mut protocol = Protocol::new();
    let mut host = Counter::default();
    hello(&mut protocol, &mut host);
    for (line, expected) in cases {
        let response = protocol.handle(line, &mut host);
        assert_eq!(
            code(&response).as_deref(),
            Some(expected),
            "{}",
            String::from_utf8_lossy(line)
        );
    }
    let long = vec![b' '; 1024 * 1024 + 1];
    assert_eq!(
        code(&protocol.handle(&long, &mut host)).as_deref(),
        Some("LineTooLong")
    );
    assert_eq!(host.applied, 1, "only hello applied");
    assert_eq!(protocol.cached(), 1);
}

#[test]
fn hello_comes_first() {
    let mut protocol = Protocol::new();
    let mut host = Counter::default();
    let response = protocol.handle(br#"{"v":1,"id":"1","op":"snapshot","args":{}}"#, &mut host);
    assert_eq!(code(&response).as_deref(), Some("HelloRequired"));
    // The refused id was not consumed: hello may use it.
    hello(&mut protocol, &mut host);
}

#[test]
fn repeats_replay_conflict_or_expire() {
    let mut protocol = Protocol::new();
    let mut host = Counter::default();
    hello(&mut protocol, &mut host);
    let snapshot = br#"{"v":1,"id":"2","op":"snapshot","args":{"include_target_text":true}}"#;
    let first = protocol.handle(snapshot, &mut host);
    let again = protocol.handle(snapshot, &mut host);
    assert_eq!(first, again, "byte for byte");
    assert_eq!(host.applied, 2, "replayed, not re-executed");
    let other = protocol.handle(br#"{"v":1,"id":"2","op":"snapshot","args":{}}"#, &mut host);
    assert_eq!(code(&other).as_deref(), Some("RequestIdConflict"));
    for id in 3..(3 + CACHE_CAPACITY) {
        let line = format!(r#"{{"v":1,"id":"{id}","op":"snapshot","args":{{}}}}"#);
        protocol.handle(line.as_bytes(), &mut host);
    }
    assert_eq!(protocol.cached(), CACHE_CAPACITY);
    let expired = protocol.handle(snapshot, &mut host);
    assert_eq!(code(&expired).as_deref(), Some("RequestExpired"));
    let applied = host.applied;
    let stale = protocol.handle(
        format!(
            r#"{{"v":1,"id":"5000","op":"snapshot","args":{{}},"expected_state_revision":"{}"}}"#,
            applied - 1
        )
        .as_bytes(),
        &mut host,
    );
    assert_eq!(code(&stale).as_deref(), Some("StaleState"));
    assert_eq!(host.applied, applied, "a stale request mutates nothing");
}

proptest::proptest! {
    /// Arbitrary bytes never panic and never answer ok for a line the
    /// strict parser rejects.
    #[test]
    fn arbitrary_bytes_never_panic_or_pass(bytes in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..256)) {
        let mut protocol = Protocol::new();
        let mut host = Counter::default();
        let response = protocol.handle(&bytes, &mut host);
        if response.contains(r#""ok":true"#) {
            proptest::prop_assert!(parse(&bytes).is_ok());
        }
    }
}

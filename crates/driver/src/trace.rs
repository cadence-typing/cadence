//! Trace recording and replay (H-04, VR-01, #72). A trace is NDJSON: a
//! header, then every request and its response. Without content recording
//! (D-02 opt-in), target text, capture text and frames are replaced by
//! digests, so a trace holds no practice text yet still pins it. Replay
//! re-executes the requests on a fresh host and reports the earliest line
//! whose response differs, or a missing, reordered or modified line.

use serde_json::{Map, Value, json};

use crate::protocol::{Host, Protocol};

/// The trace header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// The build that recorded the trace.
    pub version: String,
    /// Whether target text was recorded.
    pub record_content: bool,
}

impl Header {
    /// The header line.
    #[must_use]
    pub fn line(&self) -> String {
        json!({
            "type": "header",
            "trace_version": 1,
            "build": self.version,
            "clock": "virtual",
            "services": "fixture-or-profile",
            "capabilities": {"keyboard": "normalized", "terminal": "test-backend"},
            "record_content": self.record_content,
        })
        .to_string()
    }
}

fn digest(value: &Value) -> String {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A response as stored: practice text replaced by its digest unless
/// content is recorded.
#[must_use]
pub fn redact(response: &Value, record_content: bool) -> Value {
    if record_content {
        return response.clone();
    }
    let mut out = response.clone();
    if let Some(result) = out.get_mut("result").and_then(Value::as_object_mut) {
        for field in ["text", "frame", "target_text", "config_text"] {
            if let Some(value) = result.get(field) {
                let sealed = json!({"sha256": digest(value)});
                result.insert(field.to_string(), sealed);
            }
        }
    }
    out
}

/// Records every request and response of a session.
#[derive(Debug)]
pub struct Recorder {
    lines: Vec<String>,
    record_content: bool,
}

impl Recorder {
    /// Starts a trace with its header.
    #[must_use]
    pub fn new(header: &Header) -> Self {
        Self {
            lines: vec![header.line()],
            record_content: header.record_content,
        }
    }

    /// Records one request line and its response line.
    pub fn record(&mut self, request: &[u8], response: &str) {
        let request: Value = serde_json::from_slice(request)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(request).into_owned()));
        let response: Value = serde_json::from_str(response).unwrap_or(Value::Null);
        self.lines
            .push(json!({"type": "request", "line": request}).to_string());
        self.lines.push(
            json!({"type": "response", "line": redact(&response, self.record_content)}).to_string(),
        );
    }

    /// The trace text.
    #[must_use]
    pub fn text(&self) -> String {
        let mut text = self.lines.join("\n");
        text.push('\n');
        text
    }
}

/// Where and how a replay diverged.
#[derive(Debug, Clone, PartialEq)]
pub struct Divergence {
    /// The 1-based trace line.
    pub line: usize,
    /// What went wrong.
    pub reason: String,
    /// The recorded record.
    pub expected: Value,
    /// What the replay produced.
    pub actual: Value,
}

impl Divergence {
    /// The report, one JSON object.
    #[must_use]
    pub fn report(&self) -> String {
        json!({
            "diverged_at_line": self.line,
            "reason": self.reason,
            "expected": self.expected,
            "actual": self.actual,
        })
        .to_string()
    }
}

fn divergence(line: usize, reason: &str, expected: Value, actual: Value) -> Divergence {
    Divergence {
        line,
        reason: reason.to_string(),
        expected,
        actual,
    }
}

/// The header shows the build's version right-aligned; a newer build
/// replaying an older trace differs only there. Both sides are normalised
/// before comparing: in `text`, a line ending in the version (after its
/// padding) ends in `<version>`; in a cell `frame`, the cells of the
/// padding, the version and the spaces after it become `#`.
fn normalise_version(response: &mut Value, version: &str) {
    if version.is_empty() {
        return;
    }
    let Some(result) = response.get_mut("result").and_then(Value::as_object_mut) else {
        return;
    };
    if let Some(Value::String(text)) = result.get_mut("text") {
        let lines: Vec<String> = text
            .split('\n')
            .map(|line| match line.trim_end().strip_suffix(version) {
                Some(before) => format!("{} <version>", before.trim_end()),
                None => line.to_string(),
            })
            .collect();
        *text = lines.join("\n");
    }
    let columns = result
        .get("frame")
        .and_then(|frame| frame.get("columns"))
        .and_then(Value::as_u64)
        .and_then(|c| usize::try_from(c).ok())
        .unwrap_or(0);
    let Some(cells) = result
        .get_mut("frame")
        .and_then(|frame| frame.get_mut("cells"))
        .and_then(Value::as_array_mut)
    else {
        return;
    };
    if columns == 0 {
        return;
    }
    for row in cells.chunks_mut(columns) {
        let symbols: Vec<String> = row
            .iter()
            .map(|cell| {
                cell.get("symbol")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        let joined: String = symbols.concat();
        // Only rows of single-cell symbols map string offsets to cells.
        if joined.chars().count() != symbols.len() {
            continue;
        }
        let chars: Vec<char> = joined.chars().collect();
        let needle: Vec<char> = version.chars().collect();
        let Some(at) = chars
            .windows(needle.len())
            .position(|w| w == needle.as_slice())
        else {
            continue;
        };
        let mut start = at;
        while start > 0 && chars.get(start.saturating_sub(1)) == Some(&' ') {
            start = start.saturating_sub(1);
        }
        for cell in row.iter_mut().skip(start) {
            if let Some(object) = cell.as_object_mut() {
                object.insert("symbol".into(), Value::String("#".into()));
            }
        }
    }
}

/// The first differing counter or field between two responses, as a path.
fn first_difference(expected: &Value, actual: &Value, path: &str) -> Option<String> {
    match (expected, actual) {
        (Value::Object(left), Value::Object(right)) => {
            let mut keys: Vec<&String> = left.keys().chain(right.keys()).collect();
            keys.sort();
            keys.dedup();
            keys.into_iter().find_map(|key| {
                first_difference(
                    left.get(key).unwrap_or(&Value::Null),
                    right.get(key).unwrap_or(&Value::Null),
                    &format!("{path}.{key}"),
                )
            })
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => left
            .iter()
            .zip(right)
            .enumerate()
            .find_map(|(index, (l, r))| first_difference(l, r, &format!("{path}[{index}]"))),
        (left, right) if left == right => None,
        _ => Some(path.to_string()),
    }
}

/// Replays a trace on `host`, returning each capture response for the
/// caller to keep, or the earliest divergence.
///
/// # Errors
/// The first missing, reordered, modified or diverging line.
pub fn replay(trace: &str, host: &mut impl Host) -> Result<Vec<Map<String, Value>>, Divergence> {
    let mut lines = trace
        .lines()
        .enumerate()
        .map(|(index, text)| (index.saturating_add(1), text));
    let (_, first) = lines
        .next()
        .ok_or_else(|| divergence(1, "empty trace", Value::Null, Value::Null))?;
    let header: Value = serde_json::from_str(first)
        .map_err(|_| divergence(1, "malformed header", Value::Null, Value::Null))?;
    if header["type"] != "header" || header["trace_version"] != 1 {
        return Err(divergence(
            1,
            "not a version-1 trace header",
            header,
            Value::Null,
        ));
    }
    let record_content = header["record_content"] == true;
    let recorded_build = header["build"].as_str().unwrap_or("").to_string();
    let mut protocol = Protocol::new();
    let mut captures = Vec::new();
    while let Some((number, text)) = lines.next() {
        let record: Value = serde_json::from_str(text)
            .map_err(|_| divergence(number, "malformed trace line", Value::Null, Value::Null))?;
        if record["type"] != "request" {
            return Err(divergence(
                number,
                "expected a request line",
                record,
                Value::Null,
            ));
        }
        let request = serde_json::to_vec(&record["line"]).unwrap_or_default();
        let actual: Value =
            serde_json::from_str(&protocol.handle(&request, host)).unwrap_or(Value::Null);
        let mut actual = redact(&actual, record_content);
        let Some((response_number, response_text)) = lines.next() else {
            return Err(divergence(
                number.saturating_add(1),
                "missing response line",
                Value::Null,
                actual,
            ));
        };
        let expected: Value = serde_json::from_str(response_text).map_err(|_| {
            divergence(
                response_number,
                "malformed trace line",
                Value::Null,
                actual.clone(),
            )
        })?;
        if expected["type"] != "response" {
            return Err(divergence(
                response_number,
                "expected a response line",
                expected,
                actual,
            ));
        }
        // The replaying build may be newer: its version is not a divergence
        // (the header records the recording build).
        if let (Some(recorded), Some(result)) = (
            expected["line"]["result"]["version"].as_str(),
            actual.get_mut("result").and_then(Value::as_object_mut),
        ) && result.contains_key("version")
        {
            result.insert("version".into(), Value::String(recorded.to_string()));
        }
        let mut expected = expected;
        if let Some(line) = expected.get_mut("line") {
            normalise_version(line, &recorded_build);
        }
        normalise_version(&mut actual, env!("CARGO_PKG_VERSION"));
        if expected["line"] != actual {
            let path = first_difference(&expected["line"], &actual, "response").unwrap_or_default();
            return Err(divergence(
                response_number,
                &format!("first difference at {path}"),
                expected["line"].clone(),
                actual,
            ));
        }
        if let Some(result) = actual["result"].as_object()
            && result.contains_key("frame")
        {
            captures.push(result.clone());
        }
    }
    Ok(captures)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::{Code, Request};

    /// A host whose snapshot reports a first-pass count.
    struct Counts {
        first_correct: u64,
        revision: u64,
    }

    impl Host for Counts {
        fn state_revision(&self) -> u64 {
            self.revision
        }
        fn apply(&mut self, _request: &Request) -> Result<Map<String, Value>, (Code, String)> {
            self.revision = self.revision.saturating_add(1);
            let mut result = Map::new();
            result.insert("state_revision".into(), json!(self.revision.to_string()));
            result.insert("event_seq".into(), json!(self.revision.to_string()));
            result.insert(
                "tracks".into(),
                json!([{"first_pass": {"correct": self.first_correct.to_string(), "attempted": "3"}}]),
            );
            Ok(result)
        }
    }

    /// A newer build differs from an older trace only in the header's
    /// version, of any length; any other change still shows.
    #[test]
    fn only_the_version_is_normalised() {
        let text = |version: &str, tail: &str| json!({"result": {"text": format!("  TODAY   Train{:>30}  \nbody {tail}", version)}});
        let mut old = text("0.1.0-beta.2", "same");
        let mut new = text("0.1.0-rc.1", "same");
        normalise_version(&mut old, "0.1.0-beta.2");
        normalise_version(&mut new, "0.1.0-rc.1");
        assert_eq!(old, new);
        let mut changed = text("0.1.0-rc.1", "changed");
        normalise_version(&mut changed, "0.1.0-rc.1");
        assert_ne!(old, changed);
    }

    /// A build whose scoring changed reports the first diverging counter.
    #[test]
    fn a_changed_counter_is_the_reported_divergence() {
        let requests = [
            br#"{"v":1,"id":"1","op":"hello","args":{}}"#.as_slice(),
            br#"{"v":1,"id":"2","op":"snapshot","args":{}}"#.as_slice(),
        ];
        let mut recorder = Recorder::new(&Header {
            version: "test".into(),
            record_content: false,
        });
        let mut protocol = Protocol::new();
        let mut original = Counts {
            first_correct: 2,
            revision: 0,
        };
        for request in requests {
            let response = protocol.handle(request, &mut original);
            recorder.record(request, &response);
        }
        assert!(
            replay(
                &recorder.text(),
                &mut Counts {
                    first_correct: 2,
                    revision: 0
                }
            )
            .is_ok()
        );
        let divergence = replay(
            &recorder.text(),
            &mut Counts {
                first_correct: 3,
                revision: 0,
            },
        )
        .err()
        .unwrap_or_else(|| Divergence {
            line: 0,
            reason: String::new(),
            expected: Value::Null,
            actual: Value::Null,
        });
        assert_eq!(divergence.line, 3);
        assert_eq!(
            divergence.reason,
            "first difference at response.result.tracks[0].first_pass.correct"
        );
    }
}

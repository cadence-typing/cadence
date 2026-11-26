//! The v1 headless request envelope (H-02, #68): one UTF-8 JSON object per
//! line, at most 1 MiB, parsed strictly into a typed request. Every
//! rejection carries a stable code (`contracts/headless-response.schema.json`)
//! and mutates nothing.

use serde_json::{Map, Value};

use crate::stream::RequestId;

/// Longest accepted request line, in bytes.
pub const MAXIMUM_LINE_BYTES: usize = 1024 * 1024;
/// Longest `text` expansion, in units.
pub const MAXIMUM_TEXT_UNITS: usize = 8192;

/// A stable error code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Code {
    /// The line is not one JSON object.
    MalformedJson,
    /// The line exceeds [`MAXIMUM_LINE_BYTES`].
    LineTooLong,
    /// `v` is not 1.
    UnsupportedVersion,
    /// `id` is missing, not a decimal string or out of range.
    InvalidId,
    /// A field the schema does not name.
    UnknownField,
    /// `op` is not an operation of the protocol.
    UnknownOperation,
    /// An argument is missing, mistyped, out of range or not in its enum.
    InvalidArgument,
    /// A request other than `hello` before the first accepted `hello`.
    HelloRequired,
    /// The id was used for a different request.
    RequestIdConflict,
    /// The id is old and its response was evicted from the cache.
    RequestExpired,
    /// `expected_state_revision` differs from the current revision.
    StaleState,
}

impl Code {
    /// The code as written on the wire.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::MalformedJson => "MalformedJson",
            Self::LineTooLong => "LineTooLong",
            Self::UnsupportedVersion => "UnsupportedVersion",
            Self::InvalidId => "InvalidId",
            Self::UnknownField => "UnknownField",
            Self::UnknownOperation => "UnknownOperation",
            Self::InvalidArgument => "InvalidArgument",
            Self::HelloRequired => "HelloRequired",
            Self::RequestIdConflict => "RequestIdConflict",
            Self::RequestExpired => "RequestExpired",
            Self::StaleState => "StaleState",
        }
    }
}

/// A rejected line: its code, a message, and the request id when one was
/// usable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejection {
    /// Stable code.
    pub code: Code,
    /// Human-readable detail.
    pub message: String,
    /// The request id, when the envelope had a usable one.
    pub id: Option<RequestId>,
}

/// Fixture completion variants for `deliver`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliverVariant {
    /// The real completion.
    Success,
    /// A failure.
    Failure,
    /// A completion for an outdated request.
    StaleSuccess,
    /// A failure for an outdated request.
    StaleFailure,
    /// A second copy of a completion.
    Duplicate,
}

/// A typed operation with its validated arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    /// Negotiate; optionally select a fixture scenario.
    Hello {
        /// The named fixture scenario.
        scenario: Option<String>,
        /// Whether fixture completions are delivered automatically.
        auto_delivery: Option<bool>,
    },
    /// One key event.
    Key {
        /// The key name.
        key: String,
        /// Modifiers, in request order.
        modifiers: Vec<String>,
        /// `press`, `repeat` or `release`.
        kind: String,
        /// `unverified`, `numpad` or `main`.
        source: String,
    },
    /// A synthetic content expansion, validated whole.
    Text {
        /// The units to type.
        text: String,
        /// Microseconds between units.
        inter_key_microseconds: u64,
    },
    /// Advance the virtual clock.
    Advance {
        /// Microseconds to advance.
        microseconds: u64,
    },
    /// A viewport change.
    Resize {
        /// Columns.
        columns: u16,
        /// Rows.
        rows: u16,
    },
    /// A documented user action.
    Action {
        /// The action name.
        name: String,
        /// The track, for actions that take one.
        track: Option<String>,
        /// The theme, for `SetTheme`.
        theme: Option<String>,
    },
    /// Await a causal or durable boundary.
    Barrier {
        /// `true` for the durable scope.
        durable: bool,
        /// Through this event sequence.
        through_event_seq: u64,
        /// Watchdog in milliseconds.
        watchdog_milliseconds: u64,
    },
    /// Observe the read model.
    Snapshot {
        /// Include the target text.
        include_target_text: bool,
    },
    /// Capture the shared renderer's frame.
    Capture {
        /// `cells`, `text` or `both`.
        format: String,
    },
    /// Release a pending fixture completion.
    Deliver {
        /// The pending job.
        job_id: u64,
        /// Which completion.
        variant: DeliverVariant,
    },
    /// Finish and flush.
    Quit,
}

/// A parsed request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// Its id.
    pub id: RequestId,
    /// The operation.
    pub op: Operation,
    /// The state revision the request was written against, if any.
    pub expected_state_revision: Option<u64>,
    /// The canonical request digest (sorted keys, no whitespace), for the
    /// response cache.
    pub digest: cadence_core::ContentDigest,
}

fn reject(code: Code, message: impl Into<String>, id: Option<RequestId>) -> Rejection {
    Rejection {
        code,
        message: message.into(),
        id,
    }
}

/// A decimal string for a `u64`, without sign, leading zeros or overflow.
fn decimal(value: &Value, allow_zero: bool) -> Option<u64> {
    let text = value.as_str()?;
    if text.is_empty() || text.len() > 20 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if text.len() > 1 && text.starts_with('0') {
        return None;
    }
    let number: u64 = text.parse().ok()?;
    (allow_zero || number > 0).then_some(number)
}

struct Args<'a> {
    map: &'a Map<String, Value>,
    id: RequestId,
}

impl Args<'_> {
    fn only(&self, allowed: &[&str]) -> Result<(), Rejection> {
        match self.map.keys().find(|key| !allowed.contains(&key.as_str())) {
            Some(key) => Err(reject(
                Code::UnknownField,
                format!("unknown argument {key}"),
                Some(self.id),
            )),
            None => Ok(()),
        }
    }

    fn invalid(&self, name: &str) -> Rejection {
        reject(
            Code::InvalidArgument,
            format!("invalid argument {name}"),
            Some(self.id),
        )
    }

    fn string(&self, name: &str, maximum: usize) -> Result<Option<String>, Rejection> {
        match self.map.get(name) {
            None => Ok(None),
            Some(value) => {
                let text = value.as_str().ok_or_else(|| self.invalid(name))?;
                if text.is_empty() || text.chars().count() > maximum {
                    return Err(self.invalid(name));
                }
                Ok(Some(text.to_string()))
            }
        }
    }

    fn required_string(&self, name: &str, maximum: usize) -> Result<String, Rejection> {
        self.string(name, maximum)?
            .ok_or_else(|| self.invalid(name))
    }

    fn choice(&self, name: &str, allowed: &[&str]) -> Result<Option<String>, Rejection> {
        match self.map.get(name) {
            None => Ok(None),
            Some(value) => match value.as_str() {
                Some(text) if allowed.contains(&text) => Ok(Some(text.to_string())),
                Some(_) | None => Err(self.invalid(name)),
            },
        }
    }

    fn required_choice(&self, name: &str, allowed: &[&str]) -> Result<String, Rejection> {
        self.choice(name, allowed)?
            .ok_or_else(|| self.invalid(name))
    }

    fn integer(&self, name: &str, minimum: u64, maximum: u64) -> Result<u64, Rejection> {
        self.map
            .get(name)
            .and_then(Value::as_u64)
            .filter(|n| (minimum..=maximum).contains(n))
            .ok_or_else(|| self.invalid(name))
    }
}

fn operation(op: &str, args: &Args<'_>) -> Result<Operation, Rejection> {
    match op {
        "hello" => {
            args.only(&["scenario", "fixture_delivery"])?;
            Ok(Operation::Hello {
                scenario: args.string("scenario", 80)?,
                auto_delivery: args
                    .choice("fixture_delivery", &["manual", "auto"])?
                    .map(|mode| mode == "auto"),
            })
        }
        "key" => {
            args.only(&["key", "modifiers", "kind", "source"])?;
            let modifiers = args
                .map
                .get("modifiers")
                .and_then(Value::as_array)
                .ok_or_else(|| args.invalid("modifiers"))?;
            let mut names: Vec<String> = Vec::new();
            for modifier in modifiers {
                match modifier.as_str() {
                    Some(name)
                        if ["shift", "control", "alt", "super"].contains(&name)
                            && !names.iter().any(|n| n == name) =>
                    {
                        names.push(name.to_string());
                    }
                    Some(_) | None => return Err(args.invalid("modifiers")),
                }
            }
            Ok(Operation::Key {
                key: args.required_string("key", 32)?,
                modifiers: names,
                kind: args.required_choice("kind", &["press", "repeat", "release"])?,
                source: args.required_choice("source", &["unverified", "numpad", "main"])?,
            })
        }
        "text" => {
            args.only(&["text", "inter_key_microseconds"])?;
            let text = args
                .map
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| args.invalid("text"))?;
            let gap = args.integer("inter_key_microseconds", 1, 60_000_000)?;
            // The whole expansion is checked before any key: every unit in
            // the training alphabet, the length bound, and its duration.
            let units = text.chars().count();
            let alphabet = text.chars().all(|c| {
                cadence_core::TrainingChar::new(c, cadence_core::LayoutSupportsPound::US).is_ok()
            });
            let span = u64::try_from(units).ok().and_then(|n| n.checked_mul(gap));
            if units > MAXIMUM_TEXT_UNITS || !alphabet || span.is_none() {
                return Err(args.invalid("text"));
            }
            Ok(Operation::Text {
                text: text.to_string(),
                inter_key_microseconds: gap,
            })
        }
        "advance" => {
            args.only(&["microseconds"])?;
            Ok(Operation::Advance {
                microseconds: args.integer("microseconds", 0, 1_800_000_000)?,
            })
        }
        "resize" => {
            args.only(&["columns", "rows"])?;
            let columns = args.integer("columns", 1, 240)?;
            let rows = args.integer("rows", 1, 80)?;
            Ok(Operation::Resize {
                columns: u16::try_from(columns).map_err(|_range| args.invalid("columns"))?,
                rows: u16::try_from(rows).map_err(|_range| args.invalid("rows"))?,
            })
        }
        "action" => {
            args.only(&["name", "track", "theme"])?;
            Ok(Operation::Action {
                name: args.required_choice(
                    "name",
                    &[
                        "StartDailyPlan",
                        "StartDrill",
                        "Pause",
                        "Resume",
                        "NextPhase",
                        "SetTheme",
                        "ReloadSettings",
                        "OpenSettings",
                        "CloseOverlay",
                    ],
                )?,
                track: args.choice(
                    "track",
                    &["words", "prose", "number_row", "numpad", "developer"],
                )?,
                theme: args.string("theme", 80)?,
            })
        }
        "barrier" => {
            args.only(&["scope", "through_event_seq", "watchdog_milliseconds"])?;
            let scope = args.required_choice("scope", &["causal", "durable"])?;
            let through = args
                .map
                .get("through_event_seq")
                .and_then(|value| decimal(value, true))
                .ok_or_else(|| args.invalid("through_event_seq"))?;
            Ok(Operation::Barrier {
                durable: scope == "durable",
                through_event_seq: through,
                watchdog_milliseconds: args.integer("watchdog_milliseconds", 1, 120_000)?,
            })
        }
        "snapshot" => {
            args.only(&["include_target_text"])?;
            let include = match args.map.get("include_target_text") {
                None => false,
                Some(value) => value
                    .as_bool()
                    .ok_or_else(|| args.invalid("include_target_text"))?,
            };
            Ok(Operation::Snapshot {
                include_target_text: include,
            })
        }
        "capture" => {
            args.only(&["format"])?;
            Ok(Operation::Capture {
                format: args.required_choice("format", &["cells", "text", "both"])?,
            })
        }
        "deliver" => {
            args.only(&["job_id", "variant"])?;
            let job_id = args
                .map
                .get("job_id")
                .and_then(|value| decimal(value, false))
                .ok_or_else(|| args.invalid("job_id"))?;
            let variant = match args
                .required_choice(
                    "variant",
                    &[
                        "success",
                        "failure",
                        "stale_success",
                        "stale_failure",
                        "duplicate",
                    ],
                )?
                .as_str()
            {
                "success" => DeliverVariant::Success,
                "failure" => DeliverVariant::Failure,
                "stale_success" => DeliverVariant::StaleSuccess,
                "stale_failure" => DeliverVariant::StaleFailure,
                _ => DeliverVariant::Duplicate,
            };
            Ok(Operation::Deliver { job_id, variant })
        }
        "quit" => {
            args.only(&["flush"])?;
            match args.map.get("flush") {
                Some(Value::Bool(true)) => Ok(Operation::Quit),
                Some(_) | None => Err(args.invalid("flush")),
            }
        }
        _ => Err(reject(
            Code::UnknownOperation,
            format!("unknown operation {op}"),
            Some(args.id),
        )),
    }
}

/// Parses one request line strictly.
///
/// # Errors
/// A [`Rejection`] with a stable code; the request id is set when the
/// envelope had a usable one.
pub fn parse(line: &[u8]) -> Result<Request, Rejection> {
    if line.len() > MAXIMUM_LINE_BYTES {
        return Err(reject(
            Code::LineTooLong,
            "request line exceeds 1 MiB",
            None,
        ));
    }
    let value: Value = serde_json::from_slice(line)
        .map_err(|error| reject(Code::MalformedJson, error.to_string(), None))?;
    let map = value
        .as_object()
        .ok_or_else(|| reject(Code::MalformedJson, "a request is one JSON object", None))?;
    let id = map
        .get("id")
        .and_then(|value| decimal(value, false))
        .map(RequestId::new)
        .ok_or_else(|| {
            reject(
                Code::InvalidId,
                "id must be a positive decimal u64 string",
                None,
            )
        })?;
    if map.get("v").and_then(Value::as_u64) != Some(1) {
        return Err(reject(
            Code::UnsupportedVersion,
            "only v 1 is supported",
            Some(id),
        ));
    }
    if let Some(key) = map
        .keys()
        .find(|key| !["v", "id", "op", "args", "expected_state_revision"].contains(&key.as_str()))
    {
        return Err(reject(
            Code::UnknownField,
            format!("unknown field {key}"),
            Some(id),
        ));
    }
    let expected_state_revision = match map.get("expected_state_revision") {
        None => None,
        Some(value) => Some(decimal(value, true).ok_or_else(|| {
            reject(
                Code::InvalidArgument,
                "invalid expected_state_revision",
                Some(id),
            )
        })?),
    };
    let op = map
        .get("op")
        .and_then(Value::as_str)
        .ok_or_else(|| reject(Code::UnknownOperation, "op must be a string", Some(id)))?;
    let args = map
        .get("args")
        .and_then(Value::as_object)
        .ok_or_else(|| reject(Code::InvalidArgument, "args must be an object", Some(id)))?;
    let op = operation(op, &Args { map: args, id })?;
    // serde_json's map is ordered by key, so this encoding is canonical.
    let canonical = serde_json::to_vec(&value)
        .map_err(|error| reject(Code::MalformedJson, error.to_string(), Some(id)))?;
    Ok(Request {
        id,
        op,
        expected_state_revision,
        digest: digest(&canonical),
    })
}

fn digest(bytes: &[u8]) -> cadence_core::ContentDigest {
    use sha2::{Digest, Sha256};
    cadence_core::ContentDigest::from_bytes(Sha256::digest(bytes).into())
}

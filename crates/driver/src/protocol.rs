//! The request loop (H-02, #68): strict parse, `hello` first, ordered
//! admission (`request::admit`), the expected-revision check
//! (`request::revision_ok`) and a bounded response cache of 1,024 completed
//! requests (`request::evict`). Only a `New`, current, valid request reaches
//! the host; everything else answers with a stable code and mutates nothing.

use std::collections::VecDeque;

use cadence_core::ContentDigest;
use cadence_core::request::{Admit, evict, revision_ok};
use serde_json::{Map, Value, json};

use crate::envelope::{Code, Operation, Request, parse};
use crate::stream::{RequestId, Stream};

/// Completed responses kept for replay.
pub const CACHE_CAPACITY: usize = 1024;

/// What executes an admitted request.
pub trait Host {
    /// The current state revision, for `expected_state_revision`.
    fn state_revision(&self) -> u64;
    /// Executes a request; the result object includes `state_revision` and
    /// `event_seq` as decimal strings.
    ///
    /// # Errors
    /// A stable code and message; the request still counts as completed.
    fn apply(&mut self, request: &Request) -> Result<Map<String, Value>, (Code, String)>;
}

/// One stream's protocol state.
#[derive(Debug, Default)]
pub struct Protocol {
    stream: Stream,
    cache: VecDeque<(RequestId, ContentDigest, String)>,
    hello: bool,
}

/// An error record.
#[must_use]
pub fn error_line(id: Option<RequestId>, code: Code, message: &str) -> String {
    json!({
        "v": 1,
        "id": id.map(|id| id.get().to_string()),
        "ok": false,
        "error": {"code": code.key(), "message": message, "details": {}},
    })
    .to_string()
}

impl Protocol {
    /// A fresh stream scope.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Completed responses currently cached.
    #[must_use]
    pub fn cached(&self) -> usize {
        self.cache.len()
    }

    /// Handles one request line and returns one response line.
    pub fn handle(&mut self, line: &[u8], host: &mut impl Host) -> String {
        let request = match parse(line) {
            Ok(request) => request,
            Err(rejection) => return error_line(rejection.id, rejection.code, &rejection.message),
        };
        if !self.hello && !matches!(request.op, Operation::Hello { .. }) {
            return error_line(
                Some(request.id),
                Code::HelloRequired,
                "hello must come first",
            );
        }
        let stored = self
            .cache
            .iter()
            .find(|(id, _, _)| *id == request.id)
            .map(|(_, digest, response)| (*digest, response.clone()));
        let stored_digest = stored.as_ref().map(|(digest, _)| *digest);
        match self
            .stream
            .admit(request.id, request.digest, |_| stored_digest)
        {
            Admit::Replay => {
                return stored.map_or_else(
                    || error_line(Some(request.id), Code::RequestExpired, "no cached response"),
                    |(_, response)| response,
                );
            }
            Admit::Conflict => {
                return error_line(
                    Some(request.id),
                    Code::RequestIdConflict,
                    "this id was used for a different request",
                );
            }
            Admit::Expired => {
                return error_line(
                    Some(request.id),
                    Code::RequestExpired,
                    "this id is older than the response cache",
                );
            }
            Admit::New => {}
        }
        let response = if revision_ok(
            request.expected_state_revision.unwrap_or(0),
            request.expected_state_revision.is_some(),
            host.state_revision(),
        ) {
            match host.apply(&request) {
                Ok(result) => {
                    if matches!(request.op, Operation::Hello { .. }) {
                        self.hello = true;
                    }
                    json!({
                        "v": 1,
                        "id": request.id.get().to_string(),
                        "ok": true,
                        "result": Value::Object(result),
                    })
                    .to_string()
                }
                Err((code, message)) => error_line(Some(request.id), code, &message),
            }
        } else {
            error_line(
                Some(request.id),
                Code::StaleState,
                "the state changed since that revision",
            )
        };
        if evict(self.cache.len(), CACHE_CAPACITY) {
            self.cache.pop_front();
        }
        self.cache
            .push_back((request.id, request.digest, response.clone()));
        response
    }
}

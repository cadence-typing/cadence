//! Gate registry and honest execution states (B-05).
//!
//! Every required gate is declared with its current implementation
//! status. Running an unimplemented gate fails with a specific error
//! naming the responsible work packet; gates are visibly pending, never
//! silently green (P0 exit criterion).

/// Implementation state of one required gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateState {
    /// Feedback checks exist, but do not cover the full qualification row.
    Feedback(&'static str),
    /// Gate has no implementation yet; the field names the packet.
    Pending(&'static str),
    /// A required environment/authority is absent (for example shared
    /// Buildbot controller); recorded, never converted to a pass.
    Blocked(&'static str),
}

/// One required qualification gate (B-05 inventory).
pub struct Gate {
    /// Stable gate ID used by `./ci gate --gate <id>`.
    pub id: &'static str,
    /// Human description.
    pub description: &'static str,
    /// Current state.
    pub state: GateState,
}

/// The complete required inventory. Admission to a qualified bundle is an
/// exact set comparison over these IDs (B-04); nothing here may be
/// quietly removed.
pub const GATES: &[Gate] = &[
    Gate {
        id: "quality",
        description: "fmt, Clippy warnings denied, dependency/license audit, boundary/no-placeholder checks",
        state: GateState::Feedback(
            "dependency/security audit and complete product assurance remain pending",
        ),
    },
    Gate {
        id: "unit-property",
        description: "core/trainer/engine/data/parser invariants and deterministic properties",
        state: GateState::Pending("P1-P3 fill crate test suites; workspace test runner exists"),
    },
    Gate {
        id: "headless",
        description: "required behavioural journeys and canonical cell frames",
        state: GateState::Pending("P4"),
    },
    Gate {
        id: "pty-linux",
        description: "small actual binary/PTY suite, terminal restoration",
        state: GateState::Pending("P6"),
    },
    Gate {
        id: "persistence-linux",
        description: "native SQLite/process-crash/reopen/migration/lock tests",
        state: GateState::Pending("P3/P6"),
    },
    Gate {
        id: "model",
        description: "full TLA+ model inventory, stated bounds, named fault kills",
        state: GateState::Feedback(
            "models/claims.toml runs with mutants; the save protocol is mapped, the session flow is not yet (#31)",
        ),
    },
    Gate {
        id: "prove",
        description: "required TLAPS/general proof inventory and calibration",
        state: GateState::Pending("P7"),
    },
    Gate {
        id: "verify",
        description: "same-source Verus kernels, obligation inventory, production mappings",
        state: GateState::Pending("P1 begins kernels; P7 completes inventory"),
    },
    Gate {
        id: "correspondence",
        description: "models/kernels mapped to real paths; negative campaigns",
        state: GateState::Pending("P7"),
    },
    Gate {
        id: "performance",
        description: "bounded resources, latency distribution and recorded environment",
        state: GateState::Pending("P6"),
    },
    Gate {
        id: "package-install",
        description: "exact candidate bundle, static ELF, fresh-home install/run/replay/backup smoke",
        state: GateState::Pending("P8"),
    },
    Gate {
        id: "wsl-terminal",
        description: "recorded real Windows-terminal→WSL journey with honest keypad facts",
        state: GateState::Pending("P8 owner-assisted"),
    },
    Gate {
        id: "buildbot-recovery",
        description: "duplicate nomination, detach/reconnect, restart, no missing-gate admission",
        state: GateState::Blocked(
            "Buildbot recovery and authorized signing are not integrated or qualified for this bundle",
        ),
    },
    Gate {
        id: "corpus",
        description: "licensed pack manifests, count/quality/capacity, unseen benchmark inventory",
        state: GateState::Feedback(
            "alphabet/count checks are not vocabulary vetting or benchmark qualification",
        ),
    },
];

/// Lists gates and states.
pub fn list() -> Result<(), String> {
    for gate in GATES {
        let state = match gate.state {
            GateState::Feedback(reason) => format!("feedback only ({reason})"),
            GateState::Pending(packet) => format!("pending ({packet})"),
            GateState::Blocked(reason) => format!("BLOCKED ({reason})"),
        };
        println!("{:<20} {state:<12} {}", gate.id, gate.description);
    }
    Ok(())
}

/// Runs one gate by ID with honest state handling.
pub fn run(id: Option<&str>) -> Result<(), String> {
    let Some(id) = id else {
        return Err("usage: cargo xtask gate <gate-id>".to_string());
    };
    let Some(gate) = GATES.iter().find(|gate| gate.id == id) else {
        return Err(format!(
            "unknown gate {id:?}; run `cargo xtask gates` for the required inventory"
        ));
    };
    match gate.state {
        GateState::Feedback(reason) => {
            dispatch_implemented(gate.id)?;
            Err(format!(
                "feedback checks passed; qualification for {} remains incomplete: {reason}",
                gate.id
            ))
        }
        GateState::Pending(packet) => Err(format!(
            "gate {} is not implemented yet (work packet {packet}); refusing to fake success",
            gate.id
        )),
        GateState::Blocked(reason) => Err(format!(
            "gate {} is blocked: {reason}; a blocked gate never becomes a pass",
            gate.id
        )),
    }
}

/// Dispatch for gates whose commands exist today.
fn dispatch_implemented(id: &str) -> Result<(), String> {
    match id {
        "quality" => crate::run_quality(),
        "corpus" => crate::corpus::check(),
        "model" => crate::run_models(),
        other => Err(format!("gate {other} has no runner wired yet")),
    }
}

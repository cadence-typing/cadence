//! Architecture boundary enforcement (A-01).
//!
//! The allowed dependency edges are transcribed from the workspace table
//! in docs/03-architecture.md. Parsing each member's manifest and
//! asserting the edges keeps the architecture contract executable instead
//! of aspirational; `cargo xtask boundary` runs it in the `quality` gate.

use std::collections::BTreeMap;
use std::path::Path;

/// Allowed internal dependency edges, keyed by crate name. An edge not
/// listed here is a violation; absence of an edge is always allowed.
const ALLOWED_EDGES: &[(&str, &[&str])] = &[
    // core has no workspace dependency.
    ("cadence-core", &[]),
    ("cadence-trainer", &["cadence-core"]),
    ("cadence-engine", &["cadence-core", "cadence-trainer"]),
    // data may use engine port DTOs when necessary (A-01).
    (
        "cadence-data",
        &["cadence-core", "cadence-trainer", "cadence-engine"],
    ),
    ("cadence-tui", &["cadence-engine", "cadence-core"]),
    (
        "cadence-runtime",
        &[
            "cadence-engine",
            "cadence-data",
            "cadence-core",
            "cadence-trainer",
        ],
    ),
    (
        "cadence-driver",
        &[
            "cadence-engine",
            "cadence-runtime",
            "cadence-tui",
            "cadence-core",
        ],
    ),
    // Composition root may depend on every library crate (decision
    // 0004); ownership prohibitions from A-01 still apply.
    (
        "cadence",
        &[
            "cadence-core",
            "cadence-engine",
            "cadence-runtime",
            "cadence-driver",
            "cadence-tui",
            "cadence-data",
        ],
    ),
    // xtask owns gate/receipt tooling; it may read core/trainer types
    // to run the corpus/verify gates. It is never a runtime dependency
    // of the application (A-01 "must not own" column).
    ("xtask", &["cadence-core", "cadence-trainer"]),
];

/// Parses workspace member manifests and asserts every internal edge is
/// allowed. External dependencies are unconstrained here; licence and
/// pin audits are separate gates.
pub fn check() -> Result<(), String> {
    let root = workspace_root()?;
    let members = member_manifests(&root)?;
    let mut violations = Vec::new();
    for (name, deps) in &members {
        let allowed = ALLOWED_EDGES
            .iter()
            .find(|(allowed_name, _)| allowed_name == name)
            .map(|(_, allowed)| *allowed)
            .ok_or_else(|| {
                format!("unknown workspace crate {name:?}: register it in ALLOWED_EDGES")
            })?;
        for dep in deps {
            if dep.starts_with("cadence") && !allowed.contains(&dep.as_str()) {
                violations.push(format!("{name} -> {dep} is not an allowed edge"));
            }
        }
    }
    for (name, _) in ALLOWED_EDGES {
        if !members.contains_key(*name) {
            violations.push(format!("declared crate {name:?} is not a workspace member"));
        }
    }
    if violations.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "architecture boundary violations (docs/03-architecture.md A-01):\n  {}",
            violations.join("\n  ")
        ))
    }
}

/// Workspace root, located relative to this executable's manifest dir.
fn workspace_root() -> Result<PathBuf, String> {
    let mut dir = std::env::var("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .map_err(|_| "CARGO_MANIFEST_DIR not set; run via cargo xtask".to_string())?;
    dir.pop();
    Ok(dir)
}

use std::path::PathBuf;

/// Maps each member crate name to its internal dependency names.
fn member_manifests(root: &Path) -> Result<BTreeMap<String, Vec<String>>, String> {
    let workspace_path = root.join("Cargo.toml");
    let text = std::fs::read_to_string(&workspace_path)
        .map_err(|error| format!("cannot read {}: {error}", workspace_path.display()))?;
    let value: toml::Table = text
        .parse()
        .map_err(|error| format!("cannot parse {}: {error}", workspace_path.display()))?;
    let mut members: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let globs: Vec<String> = value
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.as_str().map(String::from))
                .collect()
        })
        .ok_or_else(|| "workspace members missing".to_string())?;
    for glob in globs {
        let pattern = root.join(&glob);
        let matched: Vec<PathBuf> = expand_member_pattern(&pattern)?;
        for manifest_dir in matched {
            let path = manifest_dir.join("Cargo.toml");
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            let value: toml::Table = text
                .parse()
                .map_err(|error| format!("cannot parse {}: {error}", path.display()))?;
            let name = value
                .get("package")
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .ok_or_else(|| format!("package name missing in {}", path.display()))?
                .to_string();
            let mut deps: Vec<String> = Vec::new();
            for section in ["dependencies", "dev-dependencies"] {
                if let Some(map) = value.get(section).and_then(|d| d.as_table()) {
                    for key in map.keys() {
                        if key.starts_with("cadence") {
                            deps.push(key.clone());
                        }
                    }
                }
            }
            if !deps.is_empty() {
                members.entry(name).or_default().extend(deps);
            } else {
                members.entry(name).or_default();
            }
        }
    }
    Ok(members)
}

/// Expands one workspace-member entry: a literal directory resolves to
/// itself; a trailing `crates/*` style pattern expands to child
/// directories containing a manifest. Unknown shapes resolve to nothing
/// and are caught by the membership check in [`check`].
fn expand_member_pattern(pattern: &Path) -> Result<Vec<PathBuf>, String> {
    if pattern.is_dir() {
        return Ok(vec![pattern.to_path_buf()]);
    }
    let Some(parent) = pattern.parent() else {
        return Ok(Vec::new());
    };
    if pattern.file_name().and_then(|name| name.to_str()) != Some("*") {
        return Ok(Vec::new());
    }
    let mut hits = Vec::new();
    let read = std::fs::read_dir(parent)
        .map_err(|error| format!("cannot read {}: {error}", parent.display()))?;
    for entry in read.flatten() {
        if entry.path().join("Cargo.toml").is_file() {
            hits.push(entry.path());
        }
    }
    Ok(hits)
}

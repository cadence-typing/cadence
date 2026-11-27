//! Candidates and receipts (B-03, B-04, B-05; decision 0003; #93). A
//! candidate freezes everything a qualification is about: the source commit
//! and tree (from a clean checkout), the version, the support profile, the
//! gate inventory, the lock, image and toolchain, the policies, corpus and
//! schema, and the expected artifacts. Its id is a hash of its canonical
//! serialization (sorted keys, no timestamps, no floats). Every gate attempt
//! writes a receipt, failed ones included; admission is an exact set
//! comparison of passed receipts against the inventory. There is no
//! scheduler here: gates run one at a time, and with no Buildbot controller
//! local receipts cannot promote a bundle.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The only support profile.
pub const PROFILE: &str = "linux-x86_64-v1";
/// Gates that never pass locally: the owner's real-terminal journey (#98)
/// and the shared Buildbot recovery (#14). Recorded with these outcomes.
const NEVER_LOCAL: [(&str, Outcome, &str); 2] = [
    ("wsl-terminal", Outcome::Pending, "owner-assisted (#98)"),
    (
        "buildbot-recovery",
        Outcome::Unsupported,
        "blocked: no Buildbot controller (#14)",
    ),
];

/// A frozen qualification candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    /// The record's format.
    pub format: String,
    /// The source commit.
    pub commit: String,
    /// Its tree.
    pub tree: String,
    /// The product version.
    pub version: String,
    /// The support profile.
    pub profile: String,
    /// Every required gate id.
    pub inventory: Vec<String>,
    /// Digest of `Cargo.lock`.
    pub lock_sha256: String,
    /// The gate image's id, as `./ci` reports it.
    pub image: String,
    /// The toolchain channel.
    pub toolchain: String,
    /// Digests of `policies/*`.
    pub policies: BTreeMap<String, String>,
    /// Digests of `corpus/*` files.
    pub corpus: BTreeMap<String, String>,
    /// The store schema the build supports.
    pub schema: i64,
    /// What the bundle step must produce.
    pub artifacts: Vec<String>,
}

impl Candidate {
    /// The canonical serialization: serde_json's maps are sorted, and no
    /// field is a timestamp or a float.
    ///
    /// # Errors
    /// Encoding failure.
    pub fn canonical(&self) -> Result<String, String> {
        let value = serde_json::to_value(self).map_err(|e| e.to_string())?;
        serde_json::to_string(&value).map_err(|e| e.to_string())
    }

    /// The candidate id: a named hash of the canonical record.
    ///
    /// # Errors
    /// Encoding failure.
    pub fn id(&self) -> Result<String, String> {
        let digest = hex(&Sha256::digest(self.canonical()?));
        Ok(format!("cand-{}", digest.get(..32).unwrap_or(&digest)))
    }
}

/// A gate attempt's outcome (B-04).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// Not run.
    Pending,
    /// Started and not finished (an interrupted attempt stays so).
    Running,
    /// Passed.
    Passed,
    /// Failed.
    Failed,
    /// Stopped on purpose.
    Cancelled,
    /// Exceeded its time.
    TimedOut,
    /// Cannot run here.
    Unsupported,
    /// Left out of this qualification.
    Excluded,
}

/// One gate attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Receipt {
    /// The candidate id.
    pub candidate: String,
    /// The gate id.
    pub gate: String,
    /// The attempt number, from 1.
    pub attempt: u32,
    /// The command run.
    pub argv: Vec<String>,
    /// Digest of the gate's recipe (its argv and inventory description).
    pub recipe_sha256: String,
    /// The candidate inputs the attempt ran against.
    pub inputs: BTreeMap<String, String>,
    /// Tool and environment facts.
    pub environment: BTreeMap<String, String>,
    /// Unix seconds.
    pub started: u64,
    /// Unix seconds; absent while running.
    pub ended: Option<u64>,
    /// The outcome.
    pub outcome: Outcome,
    /// Why, for an outcome that is not a run.
    pub note: Option<String>,
    /// Where the output was written.
    pub log: Option<String>,
    /// Digest of that output.
    pub log_sha256: Option<String>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_file(path: &Path) -> Result<String, String> {
    std::fs::read(path)
        .map(|bytes| hex(&Sha256::digest(bytes)))
        .map_err(|e| format!("{}: {e}", path.display()))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn digests(dir: &Path) -> Result<BTreeMap<String, String>, String> {
    let mut out = BTreeMap::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_file()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
        {
            out.insert(name.to_string(), sha256_file(&path)?);
        }
    }
    Ok(out)
}

fn schema_version(root: &Path) -> Result<i64, String> {
    let source = std::fs::read_to_string(root.join("crates/data/src/schema.rs"))
        .map_err(|e| e.to_string())?;
    source
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("pub const SUPPORTED_SCHEMA: i64 = ")?
                .strip_suffix(';')?
                .parse()
                .ok()
        })
        .ok_or_else(|| "no SUPPORTED_SCHEMA in crates/data/src/schema.rs".to_string())
}

fn toolchain(root: &Path) -> Result<String, String> {
    let text =
        std::fs::read_to_string(root.join("rust-toolchain.toml")).map_err(|e| e.to_string())?;
    let parsed: toml::Value = toml::from_str(&text).map_err(|e| e.to_string())?;
    parsed
        .get("toolchain")
        .and_then(|t| t.get("channel"))
        .and_then(toml::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "no toolchain channel".to_string())
}

/// Freezes the candidate for `commit` in `root`: the checkout must be
/// clean and at that commit.
///
/// # Errors
/// A dirty tree, another commit checked out, or unreadable inputs.
pub fn freeze(root: &Path, commit: &str, profile: &str) -> Result<Candidate, String> {
    if profile != PROFILE {
        return Err(format!(
            "unknown support profile {profile:?}; the only one is {PROFILE}"
        ));
    }
    let dirty = git(root, &["status", "--porcelain", "--untracked-files=normal"])?;
    if !dirty.is_empty() {
        return Err(format!("the tree is not clean; commit or remove:\n{dirty}"));
    }
    let head = git(root, &["rev-parse", "HEAD"])?;
    let wanted = git(root, &["rev-parse", &format!("{commit}^{{commit}}")])?;
    if head != wanted {
        return Err(format!(
            "HEAD is {head}, not {wanted}; check the commit out first"
        ));
    }
    let version = env!("CARGO_PKG_VERSION").to_string();
    let target = "x86_64-unknown-linux-musl";
    Ok(Candidate {
        format: "cadence-candidate-v1".into(),
        commit: wanted.clone(),
        tree: git(root, &["rev-parse", &format!("{wanted}^{{tree}}")])?,
        version: version.clone(),
        profile: profile.into(),
        inventory: crate::gates::GATES
            .iter()
            .map(|g| g.id.to_string())
            .collect(),
        lock_sha256: sha256_file(&root.join("Cargo.lock"))?,
        image: std::env::var("CADENCE_IMAGE_ID").unwrap_or_else(|_unset| "unrecorded".into()),
        toolchain: toolchain(root)?,
        policies: digests(&root.join("policies"))?,
        corpus: digests(&root.join("corpus"))?,
        schema: schema_version(root)?,
        artifacts: vec![
            format!("cadence-{version}-{target}.tar.gz"),
            "SHA256SUMS".into(),
            "BUNDLE.txt".into(),
        ],
    })
}

/// Where candidates and their receipts live.
pub struct Store {
    root: PathBuf,
}

impl Store {
    /// The store under `CADENCE_BUILD_DIR` (Compose: the build cache) or
    /// the repository's `.cadence-build/`.
    #[must_use]
    pub fn at(repository: &Path) -> Self {
        let base = std::env::var_os("CADENCE_BUILD_DIR")
            .map_or_else(|| repository.join(".cadence-build"), PathBuf::from);
        Self {
            root: base.join("candidates"),
        }
    }

    fn dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    /// Stores a candidate, or checks a stored one with the same id is the same.
    ///
    /// # Errors
    /// A filesystem failure, or a stored record that differs.
    pub fn save(&self, candidate: &Candidate) -> Result<String, String> {
        let id = candidate.id()?;
        let dir = self.dir(&id);
        std::fs::create_dir_all(dir.join("receipts")).map_err(|e| e.to_string())?;
        let path = dir.join("candidate.json");
        let text = candidate.canonical()?;
        match std::fs::read_to_string(&path) {
            Ok(stored) if stored == text => {}
            Ok(_) => {
                return Err(format!(
                    "{}: a different record under the same id",
                    path.display()
                ));
            }
            Err(_absent) => write_atomic(&path, &text)?,
        }
        Ok(id)
    }

    /// Loads a stored candidate and checks its id.
    ///
    /// # Errors
    /// No such candidate, or a record that does not hash to its id.
    pub fn load(&self, id: &str) -> Result<Candidate, String> {
        let path = self.dir(id).join("candidate.json");
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let candidate: Candidate = serde_json::from_str(&text).map_err(|e| e.to_string())?;
        if candidate.id()? != id {
            return Err(format!(
                "{}: the record does not hash to {id}",
                path.display()
            ));
        }
        Ok(candidate)
    }

    fn receipt_path(&self, id: &str, gate: &str, attempt: u32) -> PathBuf {
        self.dir(id)
            .join("receipts")
            .join(format!("{gate}.{attempt:03}.json"))
    }

    fn write(&self, receipt: &Receipt) -> Result<(), String> {
        let text = serde_json::to_string_pretty(receipt).map_err(|e| e.to_string())?;
        write_atomic(
            &self.receipt_path(&receipt.candidate, &receipt.gate, receipt.attempt),
            &text,
        )
    }

    /// Every receipt of a candidate, valid for it or not.
    ///
    /// # Errors
    /// An unreadable receipt.
    pub fn receipts(&self, id: &str) -> Result<Vec<Receipt>, String> {
        let dir = self.dir(id).join("receipts");
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Ok(out);
        };
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().is_some_and(|e| e == "json") {
                let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
                out.push(
                    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?,
                );
            }
        }
        out.sort_by(|a, b| (a.gate.as_str(), a.attempt).cmp(&(b.gate.as_str(), b.attempt)));
        Ok(out)
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let staged = path.with_extension("tmp");
    std::fs::write(&staged, text).map_err(|e| format!("{}: {e}", staged.display()))?;
    std::fs::rename(&staged, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The candidate inputs a receipt records and must match to be reused.
fn inputs(candidate: &Candidate) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("commit".to_string(), candidate.commit.clone()),
        ("tree".to_string(), candidate.tree.clone()),
        ("lock_sha256".to_string(), candidate.lock_sha256.clone()),
        ("image".to_string(), candidate.image.clone()),
        ("toolchain".to_string(), candidate.toolchain.clone()),
    ])
}

/// Whether a receipt is valid evidence for this candidate.
///
/// # Errors
/// Why it is not.
pub fn valid_for(receipt: &Receipt, id: &str, candidate: &Candidate) -> Result<(), String> {
    if receipt.candidate != id {
        return Err(format!(
            "{}: a receipt of candidate {}",
            receipt.gate, receipt.candidate
        ));
    }
    if receipt.inputs != inputs(candidate) {
        return Err(format!(
            "{}: its inputs differ from the candidate's",
            receipt.gate
        ));
    }
    if !candidate.inventory.contains(&receipt.gate) {
        return Err(format!("{}: not in the inventory", receipt.gate));
    }
    Ok(())
}

/// Each gate's newest valid receipt.
fn latest(receipts: &[Receipt], id: &str, candidate: &Candidate) -> BTreeMap<String, Receipt> {
    let mut out: BTreeMap<String, Receipt> = BTreeMap::new();
    for receipt in receipts
        .iter()
        .filter(|r| valid_for(r, id, candidate).is_ok())
    {
        let newer = out
            .get(&receipt.gate)
            .is_none_or(|seen| seen.attempt < receipt.attempt);
        if newer {
            out.insert(receipt.gate.clone(), receipt.clone());
        }
    }
    out
}

/// The admission decision: exactly the inventory has passed receipts.
#[derive(Debug, PartialEq, Eq)]
pub struct Admission {
    /// Gates without a passed receipt.
    pub missing: BTreeSet<String>,
    /// Whether the set of passed gates equals the inventory.
    pub admitted: bool,
}

/// Compares passed receipts with the inventory, as sets.
#[must_use]
pub fn admission(candidate: &Candidate, id: &str, receipts: &[Receipt]) -> Admission {
    let required: BTreeSet<String> = candidate.inventory.iter().cloned().collect();
    let passed: BTreeSet<String> = latest(receipts, id, candidate)
        .into_values()
        .filter(|r| r.outcome == Outcome::Passed)
        .map(|r| r.gate)
        .collect();
    let missing: BTreeSet<String> = required.difference(&passed).cloned().collect();
    Admission {
        admitted: passed == required,
        missing,
    }
}

/// Runs one gate and returns its outcome and log; the real runner is
/// `cargo run -p xtask -- gate ID`, tests inject their own.
pub type Runner<'a> = dyn FnMut(&str, &Path) -> Result<Outcome, String> + 'a;

fn environment() -> BTreeMap<String, String> {
    let run = |program: &str, args: &[&str]| {
        Command::new(program)
            .args(args)
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    };
    BTreeMap::from([
        ("rustc".to_string(), run("rustc", &["--version"])),
        ("cargo".to_string(), run("cargo", &["--version"])),
        ("kernel".to_string(), run("uname", &["-sr"])),
        (
            "container".to_string(),
            Path::new("/.dockerenv").exists().to_string(),
        ),
    ])
}

/// Runs one gate attempt for a candidate and writes its receipt (first as
/// `running`, so an interruption is visible, then with the outcome).
///
/// # Errors
/// A store failure.
pub fn attempt(
    store: &Store,
    id: &str,
    candidate: &Candidate,
    gate: &str,
    runner: &mut Runner<'_>,
) -> Result<Receipt, String> {
    let previous = store
        .receipts(id)?
        .iter()
        .filter(|r| r.gate == gate)
        .map(|r| r.attempt)
        .max()
        .unwrap_or(0);
    let argv = vec![
        "cargo".into(),
        "run".into(),
        "--locked".into(),
        "-p".into(),
        "xtask".into(),
        "--".into(),
        "gate".into(),
        gate.into(),
    ];
    let description = crate::gates::GATES
        .iter()
        .find(|g| g.id == gate)
        .map_or("", |g| g.description);
    let log = store
        .dir(id)
        .join("logs")
        .join(format!("{gate}.{:03}.log", previous.saturating_add(1)));
    let mut receipt = Receipt {
        candidate: id.into(),
        gate: gate.into(),
        attempt: previous.saturating_add(1),
        recipe_sha256: hex(&Sha256::digest(format!(
            "{}\n{description}",
            argv.join(" ")
        ))),
        argv,
        inputs: inputs(candidate),
        environment: environment(),
        started: now(),
        ended: None,
        outcome: Outcome::Running,
        note: None,
        log: Some(log.display().to_string()),
        log_sha256: None,
    };
    if let Some((_, outcome, note)) = NEVER_LOCAL.iter().find(|(g, _, _)| *g == gate) {
        receipt.outcome = *outcome;
        receipt.note = Some((*note).to_string());
        receipt.log = None;
        receipt.ended = Some(now());
        store.write(&receipt)?;
        return Ok(receipt);
    }
    std::fs::create_dir_all(log.parent().unwrap_or(&log)).map_err(|e| e.to_string())?;
    store.write(&receipt)?;
    let outcome = runner(gate, &log);
    receipt.ended = Some(now());
    match outcome {
        Ok(outcome) => receipt.outcome = outcome,
        Err(error) => {
            receipt.outcome = Outcome::Failed;
            receipt.note = Some(error);
        }
    }
    receipt.log_sha256 = sha256_file(&log).ok();
    store.write(&receipt)?;
    Ok(receipt)
}

/// Runs every inventory gate whose newest valid receipt is not `passed`
/// (`qualify` runs them all; `resume` reuses passed ones), one at a time.
///
/// # Errors
/// A store failure.
pub fn run_inventory(
    store: &Store,
    id: &str,
    candidate: &Candidate,
    reuse: bool,
    runner: &mut Runner<'_>,
) -> Result<Vec<Receipt>, String> {
    let done = latest(&store.receipts(id)?, id, candidate);
    let mut ran = Vec::new();
    for gate in &candidate.inventory {
        let passed = done.get(gate).is_some_and(|r| r.outcome == Outcome::Passed);
        let settled = done.get(gate).is_some_and(|r| {
            NEVER_LOCAL
                .iter()
                .any(|(g, o, _)| g == gate && *o == r.outcome)
        });
        if reuse && (passed || settled) {
            continue;
        }
        let receipt = attempt(store, id, candidate, gate, runner)?;
        println!("qualify: {gate}: {:?}", receipt.outcome);
        ran.push(receipt);
    }
    Ok(ran)
}

/// The real runner: the gate as `cargo xtask gate ID`, output to the log.
fn cargo_gate(root: &Path) -> impl FnMut(&str, &Path) -> Result<Outcome, String> + '_ {
    move |gate, log| {
        let file = std::fs::File::create(log).map_err(|e| e.to_string())?;
        let err = file.try_clone().map_err(|e| e.to_string())?;
        let status = Command::new(std::env::var("CARGO").unwrap_or_else(|_unset| "cargo".into()))
            .args(["run", "--locked", "-q", "-p", "xtask", "--", "gate", gate])
            .current_dir(root)
            .stdout(file)
            .stderr(err)
            .status()
            .map_err(|e| e.to_string())?;
        Ok(if status.success() {
            Outcome::Passed
        } else {
            Outcome::Failed
        })
    }
}

/// The bundle's `ACCEPTANCE.txt` (#94): each gate's newest receipt and its
/// digest for the candidate named by `CADENCE_CANDIDATE_ID`, the admission
/// decision, and the signature state.
///
/// # Errors
/// A named candidate that cannot be read.
pub fn acceptance_summary(root: &Path) -> Result<String, String> {
    let mut out = String::from("Cadence acceptance summary\n");
    let Some(id) = std::env::var("CADENCE_CANDIDATE_ID").ok().filter(|id| !id.is_empty()) else {
        out.push_str("candidate: none (a development bundle built outside `./ci qualify`)\n");
        out.push_str("admission: not qualified\n");
        out.push_str("signature: not available: unsigned development bundle (#13)\n");
        return Ok(out);
    };
    let store = Store::at(root);
    let candidate = store.load(&id)?;
    let receipts = store.receipts(&id)?;
    out.push_str(&format!(
        "candidate: {id} ({} at {})\n",
        candidate.version, candidate.commit
    ));
    let newest = latest(&receipts, &id, &candidate);
    for gate in &candidate.inventory {
        match newest.get(gate) {
            Some(receipt) => {
                let digest = serde_json::to_string(receipt)
                    .map(|text| hex(&Sha256::digest(text)))
                    .map_err(|e| e.to_string())?;
                out.push_str(&format!(
                    "{gate:<20} {:?} attempt {} receipt sha256 {digest}\n",
                    receipt.outcome, receipt.attempt
                ));
            }
            None => out.push_str(&format!("{gate:<20} no receipt\n")),
        }
    }
    let decision = admission(&candidate, &id, &receipts);
    out.push_str(&format!(
        "admission: {}\n",
        if decision.admitted {
            "every gate passed".to_string()
        } else {
            format!(
                "refused; missing {}",
                decision
                    .missing
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    ));
    out.push_str("signature: not available: unsigned development bundle (#13)\n");
    out.push_str("Local receipts cannot promote a bundle: no Buildbot controller is configured (decision 0003).\n");
    Ok(out)
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i.saturating_add(1)))
        .map(String::as_str)
}

fn report(store: &Store, id: &str, candidate: &Candidate) -> Result<Admission, String> {
    let receipts = store.receipts(id)?;
    let newest = latest(&receipts, id, candidate);
    println!(
        "candidate {id} ({} at {})",
        candidate.version, candidate.commit
    );
    for gate in &candidate.inventory {
        let line = newest.get(gate).map_or_else(
            || "no receipt".to_string(),
            |r| {
                format!(
                    "{:?} (attempt {}{})",
                    r.outcome,
                    r.attempt,
                    r.note.as_ref().map_or(String::new(), |n| format!("; {n}"))
                )
            },
        );
        println!("  {gate:<20} {line}");
    }
    let decision = admission(candidate, id, &receipts);
    if decision.admitted {
        println!("admission: every gate passed");
    } else {
        println!(
            "admission: refused; missing {}",
            decision
                .missing
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    println!(
        "No Buildbot controller is configured: local receipts cannot promote a bundle (decision 0003)."
    );
    Ok(decision)
}

/// `cargo xtask qualify --commit SHA --profile PROFILE`.
///
/// # Errors
/// A refused freeze or a store failure; gate failures are receipts.
pub fn qualify(args: &[String]) -> Result<(), String> {
    let root = crate::dist::root()?;
    let commit = flag(args, "--commit").unwrap_or("HEAD");
    let profile = flag(args, "--profile").unwrap_or(PROFILE);
    let candidate = freeze(&root, commit, profile)?;
    let store = Store::at(&root);
    let id = store.save(&candidate)?;
    println!("qualify: froze {id}");
    run_inventory(&store, &id, &candidate, false, &mut cargo_gate(&root))?;
    report(&store, &id, &candidate).map(|_| ())
}

/// `cargo xtask resume --candidate ID`: re-runs only unfinished or failed
/// gates, at gate boundaries, reusing this candidate's passed receipts.
///
/// # Errors
/// An unknown candidate, a checkout that is not the candidate, or a store
/// failure.
pub fn resume(args: &[String]) -> Result<(), String> {
    let root = crate::dist::root()?;
    let id = flag(args, "--candidate").ok_or("usage: resume --candidate ID")?;
    let store = Store::at(&root);
    let candidate = store.load(id)?;
    if freeze(&root, &candidate.commit, &candidate.profile)? != candidate {
        return Err(format!("the checkout is not candidate {id}'s inputs"));
    }
    run_inventory(&store, id, &candidate, true, &mut cargo_gate(&root))?;
    report(&store, id, &candidate).map(|_| ())
}

/// `cargo xtask status --candidate ID`.
///
/// # Errors
/// An unknown candidate.
pub fn status(args: &[String]) -> Result<(), String> {
    let root = crate::dist::root()?;
    let id = flag(args, "--candidate").ok_or("usage: status --candidate ID")?;
    let store = Store::at(&root);
    let candidate = store.load(id)?;
    report(&store, id, &candidate).map(|_| ())
}

/// `cargo xtask gate --candidate ID --gate GATE`: one attempt with a receipt.
///
/// # Errors
/// An unknown candidate or gate, or a checkout that is not the candidate.
pub fn gate(args: &[String]) -> Result<(), String> {
    let root = crate::dist::root()?;
    let id = flag(args, "--candidate").ok_or("usage: gate --candidate ID --gate GATE")?;
    let gate = flag(args, "--gate").ok_or("usage: gate --candidate ID --gate GATE")?;
    let store = Store::at(&root);
    let candidate = store.load(id)?;
    if !candidate.inventory.iter().any(|g| g == gate) {
        return Err(format!("{gate} is not in candidate {id}'s inventory"));
    }
    if freeze(&root, &candidate.commit, &candidate.profile)? != candidate {
        return Err(format!("the checkout is not candidate {id}'s inputs"));
    }
    let receipt = attempt(&store, id, &candidate, gate, &mut cargo_gate(&root))?;
    println!("{gate}: {:?}", receipt.outcome);
    if receipt.outcome == Outcome::Passed {
        Ok(())
    } else {
        Err(format!("{gate} did not pass"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate() -> Candidate {
        Candidate {
            format: "cadence-candidate-v1".into(),
            commit: "c0ffee".into(),
            tree: "7ree".into(),
            version: "0.1.0-rc.1".into(),
            profile: PROFILE.into(),
            inventory: vec!["quality".into(), "headless".into(), "wsl-terminal".into()],
            lock_sha256: "11".into(),
            image: "img".into(),
            toolchain: "1.97.1".into(),
            policies: BTreeMap::from([("trainer-v4.toml".into(), "aa".into())]),
            corpus: BTreeMap::new(),
            schema: 7,
            artifacts: vec!["SHA256SUMS".into()],
        }
    }

    #[test]
    fn the_same_inputs_give_the_same_id_and_any_change_another() -> Result<(), String> {
        let base = candidate();
        assert_eq!(base.id()?, candidate().id()?);
        let mut lock = candidate();
        lock.lock_sha256 = "12".into();
        let mut policy = candidate();
        policy
            .policies
            .insert("trainer-v4.toml".into(), "ab".into());
        let mut inventory = candidate();
        inventory.inventory.push("corpus".into());
        for changed in [lock, policy, inventory] {
            assert_ne!(changed.id()?, base.id()?);
        }
        Ok(())
    }

    #[test]
    fn a_dirty_tree_is_refused() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let root = dir.path();
        for args in [
            &["init", "-q"][..],
            &[
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@example.invalid",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "x",
            ][..],
        ] {
            git(root, args)?;
        }
        std::fs::write(root.join("untracked.txt"), "x").map_err(|e| e.to_string())?;
        let refused = freeze(root, "HEAD", PROFILE);
        assert!(refused.is_err_and(|e| e.contains("not clean")));
        Ok(())
    }

    fn store() -> Result<(tempfile::TempDir, Store), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let store = Store {
            root: dir.path().join("candidates"),
        };
        Ok((dir, store))
    }

    #[test]
    fn a_receipt_of_another_candidate_is_rejected() -> Result<(), String> {
        let (_dir, store) = store()?;
        let base = candidate();
        let id = store.save(&base)?;
        let mut other = candidate();
        other.lock_sha256 = "99".into();
        let other_id = store.save(&other)?;
        let receipt = attempt(&store, &other_id, &other, "quality", &mut |_, _| {
            Ok(Outcome::Passed)
        })?;
        assert!(valid_for(&receipt, &id, &base).is_err());
        // Even copied under this candidate's id, its inputs give it away.
        let copied = Receipt {
            candidate: id.clone(),
            ..receipt
        };
        assert!(valid_for(&copied, &id, &base).is_err());
        Ok(())
    }

    #[test]
    fn resume_reruns_only_the_unfinished_gate() -> Result<(), String> {
        let (_dir, store) = store()?;
        let base = candidate();
        let id = store.save(&base)?;
        // `qualify` passes quality, then is killed during headless: its
        // receipt stays `running`.
        attempt(&store, &id, &base, "quality", &mut |_, _| {
            Ok(Outcome::Passed)
        })?;
        let mut interrupted = attempt(&store, &id, &base, "headless", &mut |_, _| {
            Ok(Outcome::Passed)
        })?;
        interrupted.outcome = Outcome::Running;
        interrupted.ended = None;
        store.write(&interrupted)?;
        let mut ran = Vec::new();
        run_inventory(&store, &id, &base, true, &mut |gate, _| {
            ran.push(gate.to_string());
            Ok(Outcome::Passed)
        })?;
        assert_eq!(ran, vec!["headless"], "only the unfinished gate ran again");
        // The interrupted attempt is kept beside the new one.
        let headless: Vec<_> = store
            .receipts(&id)?
            .into_iter()
            .filter(|r| r.gate == "headless")
            .collect();
        assert_eq!(headless.len(), 2);
        Ok(())
    }

    #[test]
    fn admission_is_an_exact_set_comparison() -> Result<(), String> {
        let (_dir, store) = store()?;
        let mut base = candidate();
        base.inventory.retain(|g| g != "wsl-terminal");
        let id = store.save(&base)?;
        attempt(&store, &id, &base, "quality", &mut |_, _| {
            Ok(Outcome::Passed)
        })?;
        // Every present receipt passed, one required is missing: refused.
        let partial = admission(&base, &id, &store.receipts(&id)?);
        assert!(!partial.admitted);
        assert_eq!(partial.missing, BTreeSet::from(["headless".to_string()]));
        attempt(&store, &id, &base, "headless", &mut |_, _| {
            Ok(Outcome::Failed)
        })?;
        assert!(
            !admission(&base, &id, &store.receipts(&id)?).admitted,
            "a failure is not a pass"
        );
        attempt(&store, &id, &base, "headless", &mut |_, _| {
            Ok(Outcome::Passed)
        })?;
        assert!(admission(&base, &id, &store.receipts(&id)?).admitted);
        // The owner's and the blocked gate never pass locally.
        let full = candidate();
        let full_id = store.save(&full)?;
        let receipt = attempt(&store, &full_id, &full, "wsl-terminal", &mut |_, _| {
            Ok(Outcome::Passed)
        })?;
        assert_eq!(receipt.outcome, Outcome::Pending);
        Ok(())
    }
}

//! `cadence import` and `cadence packs` (D-03, #84): read the file once,
//! normalise it with the pure `cadence_trainer::import::normalize`, report
//! what was accepted and refused (by line and column), and unless
//! `--dry-run` write the pack atomically. Nothing in the file is executed,
//! and no line of it is logged.

use std::error::Error;
use std::io::Read;

use cadence_data::packs::{self, PackManifest, Written};
use cadence_trainer::import::{ImportOptions, NORMALIZER_VERSION, normalize};

use crate::cli::ImportRequest;

/// Rejections listed before the rest are summarised.
const REJECTIONS_SHOWN: usize = 20;

fn packs_dir(profile: Option<&std::path::Path>) -> Result<std::path::PathBuf, Box<dyn Error>> {
    Ok(packs::directory(
        &cadence_data::paths::data_dir(profile)?.data,
    ))
}

/// Imports one file.
pub(crate) fn run(
    request: &ImportRequest,
    profile: Option<&std::path::Path>,
    layout: cadence_core::Layout,
) -> Result<(), Box<dyn Error>> {
    if !packs::valid_name(&request.name) {
        return Err(format!("pack name {:?}: use a-z, 0-9 and -, 1 to 64", request.name).into());
    }
    // Read at most one byte past the limit, so an oversized input is
    // refused without reading it all.
    let limit = u64::try_from(cadence_trainer::import::MAX_BYTES)?.saturating_add(1);
    let mut bytes = Vec::new();
    match &request.file {
        Some(path) => {
            std::fs::File::open(path)?
                .take(limit)
                .read_to_end(&mut bytes)?;
        }
        None => {
            std::io::stdin()
                .lock()
                .take(limit)
                .read_to_end(&mut bytes)?;
        }
    }
    let report = normalize(
        &bytes,
        ImportOptions {
            normalize_typography: request.normalize_typography,
            layout,
        },
    )?;
    println!(
        "{} lines: {} accepted ({} passages), {} rejected",
        report.lines,
        report.accepted,
        report.passages.len(),
        report.rejected.len()
    );
    for rejection in report.rejected.iter().take(REJECTIONS_SHOWN) {
        println!(
            "  line {}, column {}: {}",
            rejection.line, rejection.column, rejection.refusal
        );
    }
    if let Some(more) = report
        .rejected
        .len()
        .checked_sub(REJECTIONS_SHOWN)
        .filter(|n| *n > 0)
    {
        println!("  … and {more} more");
    }
    let t = report.typography;
    if request.normalize_typography {
        println!(
            "typography: {} quotes, {} dashes, {} non-breaking spaces normalised",
            t.quotes, t.dashes, t.spaces
        );
    }
    if report.passages.is_empty() {
        return Err("nothing to import: no line was accepted".into());
    }
    if request.dry_run {
        println!("dry run: nothing written");
        return Ok(());
    }
    let passages: String = report
        .passages
        .iter()
        .map(|passage| {
            let mut line: String = passage.iter().map(|unit| unit.get()).collect();
            line.push('\n');
            line
        })
        .collect();
    let digest = {
        use sha2::{Digest, Sha256};
        Sha256::digest(passages.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    };
    let units: usize = report.passages.iter().map(Vec::len).sum();
    let manifest = PackManifest {
        name: request.name.clone(),
        digest,
        normalizer_version: NORMALIZER_VERSION,
        lines: u64::try_from(report.lines)?,
        passages: u64::try_from(report.passages.len())?,
        units: u64::try_from(units)?,
        rejected: u64::try_from(report.rejected.len())?,
        source: request
            .file
            .as_ref()
            .and_then(|path| path.file_name())
            .map_or_else(
                || "-".to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
    };
    let written = packs::write(
        &packs_dir(profile)?,
        &manifest,
        &passages,
        &bytes,
        request.replace,
    )?;
    println!(
        "pack {}: {}",
        manifest.name,
        match written {
            Written::Created => "created",
            Written::Replaced => "replaced",
            Written::Unchanged => "unchanged (same content)",
        }
    );
    Ok(())
}

/// Lists the imported packs.
pub(crate) fn list(profile: Option<&std::path::Path>) -> Result<(), Box<dyn Error>> {
    let manifests = packs::list(&packs_dir(profile)?)?;
    if manifests.is_empty() {
        println!("no packs imported");
    }
    for m in manifests {
        println!(
            "{}  {} passages, {} units, {} rejected lines  from {}  (normaliser v{})",
            m.name, m.passages, m.units, m.rejected, m.source, m.normalizer_version
        );
    }
    Ok(())
}

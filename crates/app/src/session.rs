//! Practice composition (#76–#79): read the config, open the store for the
//! chosen layout, build the plan and the engine. First run (#78), when
//! there is no config file, opens the store without adopting a layout and
//! shows setup; once setup's write is acknowledged the store and plan are
//! built again from the written file, before anything is typed.

use std::error::Error;
use std::time::{SystemTime, UNIX_EPOCH};

use cadence_core::calendar::{Day, local_day};
use cadence_core::settings::Setting;
use cadence_runtime::{ConfigWorker, SaveWorker};
use cadence_trainer::TrainerPolicy;

use crate::cli;
use crate::practice::{self, Prepared};

/// Setup is shown when there is a config location but no file there.
pub(crate) const fn needs_setup(
    digest: Option<cadence_core::settings::ConfigDigest>,
    location: bool,
) -> bool {
    digest.is_none() && location
}

/// The store, plan and engine for the config file as it is now.
fn prepare(
    options: &cli::Options,
    policy: &TrainerPolicy,
    day: i64,
    offset: (i32, bool),
    (first_run, track): (bool, Option<practice::Track>),
) -> Result<Prepared, Box<dyn Error>> {
    let loaded = crate::load_config(None)?;
    let mut options = crate::apply_config(options.clone_for_practice(), &loaded.config);
    // Train and Numpad (#80) are `--track` and `--minutes`, nothing else.
    if let Some((phase, minutes)) = track {
        options.track = Some(phase);
        options.minutes = Some(minutes);
    }
    // The overlay starts from the file's values with the flags laid over
    // them, so a flag keeps winning until the file is edited.
    let mut settings = cadence_runtime::config_worker::values(&loaded.config);
    if options.theme_set {
        for (key, value) in &mut settings {
            if *key == "display.theme" {
                *value = Setting::Text(options.theme.key().into());
            }
        }
    }
    // Before setup the layout is not chosen: nothing is adopted and nothing
    // is audited until it is.
    let (layout, audit) = if first_run {
        (None, Vec::new())
    } else {
        (Some(options.layout), crate::score_affecting(&loaded.config))
    };
    let (worker, bootstrap) = SaveWorker::open(
        options.profile.clone(),
        policy.learning.history_window(),
        layout,
        audit,
    )?;
    let engine = crate::build_engine(&options, policy, &bootstrap, day)?
        .with_layout(
            options.layout,
            cadence_core::progress::ContentClass::Standard,
        )
        .with_utc_offset(Some(offset.0.checked_div(60).ok_or("offset")?), offset.1)
        .with_settings(&settings, loaded.digest);
    let engine = if first_run {
        engine.with_first_run()
    } else {
        engine
    };
    Ok(Prepared {
        engine,
        worker,
        bootstrap,
    })
}

/// Runs practice.
pub(crate) fn run(options: &cli::Options) -> Result<(), Box<dyn Error>> {
    // The config file is read before the terminal starts; an invalid file
    // stops here with its diagnostics (#76).
    let loaded = crate::load_config(None)?;
    let startup = crate::apply_config(options.clone_for_practice(), &loaded.config);
    let location = crate::config_path(None);
    let first_run = needs_setup(loaded.digest, location.is_some());
    // The wall clock and the local offset are read once, here, at session
    // start; the proved calendar::local_day decides the day.
    let now = i64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())?;
    let offset = match startup.utc_offset {
        Some(seconds) => (seconds, true),
        None => crate::local_offset(),
    };
    let day = match local_day(now, offset.0) {
        Day::Number(day) => day,
        Day::OffsetOutOfRange | Day::Overflow => {
            return Err("the local date is outside the supported range".into());
        }
    };
    let policy = crate::parse_policy(crate::POLICY.as_bytes())?;
    let first = prepare(options, &policy, day, offset, (first_run, None))?;
    let config = ConfigWorker::spawn(location)?;
    practice::run(
        first,
        config,
        (day, startup.text_size.unwrap_or_default()),
        startup.theme.palette(),
        |track| prepare(options, &policy, day, offset, (false, track)),
    )
}

#[cfg(test)]
mod tests {
    use super::needs_setup;
    use cadence_core::settings::{ConfigDigest, Setting};

    /// #78: declining the numpad removes its phase; plan::allocate gives
    /// its seconds to the others, so the plan still fills the budget, and
    /// the start screen says "numpad off".
    #[test]
    fn no_numpad_reallocates_its_seconds() -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let store = cadence_data::ProgressStore::open_in(directory.path())?;
        let policy = crate::parse_policy(crate::POLICY.as_bytes())?;
        let bootstrap = store.bootstrap(policy.learning.history_window())?;
        for numpad in [true, false] {
            let mut options = crate::cli::Options::defaults();
            options.numpad = numpad;
            options.seed = Some(7);
            let engine = crate::build_engine(&options, &policy, &bootstrap, 20_000)?
                .with_settings(&[("keyboard.numpad", Setting::Boolean(numpad))], None);
            let view = engine.prepare(cadence_core::MonoTick::ORIGIN);
            let total: u64 = view.phases.iter().map(|phase| phase.seconds).sum();
            assert_eq!(
                total, policy.session.default_budget_seconds,
                "numpad {numpad}"
            );
            let has_numpad = view.phases.iter().any(|phase| phase.label == "numpad");
            assert_eq!(has_numpad, numpad);
            assert_eq!(view.numpad_off, !numpad);
        }
        Ok(())
    }

    /// #78: setup shows once: with no file at a known location, never once
    /// a file exists, and never without a location to write it.
    #[test]
    fn setup_shows_only_without_a_config_file() {
        assert!(needs_setup(None, true));
        assert!(!needs_setup(Some(ConfigDigest::of(b"")), true));
        assert!(!needs_setup(None, false));
    }
}

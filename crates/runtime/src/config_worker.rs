//! The config worker (U-04, #79): writes and reloads `config.toml` off the
//! input path. It returns facts only, each with the ticket it answers; the
//! reducer admits them through the proved `ticket::admit`. One request per
//! settings job kind can be outstanding, so the channels are bounded at two.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::thread::{self, JoinHandle};

use cadence_data::config::Config;
use cadence_data::config_write::{Loaded, reload, write};
use cadence_engine::jobs::{JobOutcome, JobTicket};
use cadence_engine::settings::SettingsRequest;

/// Outstanding requests: one write and one reload.
const SLOTS: usize = 2;

/// A config worker failure.
#[derive(Debug)]
pub enum ConfigWorkerError {
    /// The thread could not start.
    Spawn(std::io::Error),
    /// Both slots are taken (the engine never asks for more).
    Busy,
    /// The worker stopped.
    Disconnected,
}

impl std::fmt::Display for ConfigWorkerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Spawn(error) => write!(f, "cannot start the settings worker: {error}"),
            Self::Busy => f.write_str("the settings worker is busy"),
            Self::Disconnected => f.write_str("the settings worker stopped"),
        }
    }
}

impl std::error::Error for ConfigWorkerError {}

type Request = (JobTicket, SettingsRequest);
type Answer = (JobTicket, JobOutcome);

/// The running worker.
#[derive(Debug)]
pub struct ConfigWorker {
    requests: SyncSender<Request>,
    results: Receiver<Answer>,
    thread: JoinHandle<()>,
}

fn settings(loaded: Loaded) -> JobOutcome {
    JobOutcome::Settings {
        values: values(&loaded.config),
        digest: loaded.digest,
    }
}

/// Every key's effective value.
#[must_use]
pub fn values(config: &Config) -> Vec<(&'static str, cadence_core::settings::Setting)> {
    config
        .effective()
        .into_iter()
        .map(|(key, value, _)| (key, value.clone()))
        .collect()
}

fn answer(path: Option<&PathBuf>, request: SettingsRequest) -> JobOutcome {
    let Some(path) = path else {
        return JobOutcome::Failed("no settings location: set HOME or XDG_CONFIG_HOME".into());
    };
    match request {
        SettingsRequest::Write { based_on, edits } => match write(path, based_on, &edits) {
            Ok(loaded) => settings(loaded),
            Err(error) => JobOutcome::Failed(error.to_string()),
        },
        SettingsRequest::Reload => match reload(path) {
            Ok(loaded) => settings(loaded),
            Err(errors) => JobOutcome::Failed(match errors.as_slice() {
                [] => "the settings file is not valid".to_string(),
                [only] => only.to_string(),
                [first, rest @ ..] => format!("{first} (and {} more)", rest.len()),
            }),
        },
    }
}

impl ConfigWorker {
    /// Starts the worker for the config file at `path`.
    ///
    /// # Errors
    /// The thread could not start.
    pub fn spawn(path: Option<PathBuf>) -> Result<Self, ConfigWorkerError> {
        let (requests, incoming) = mpsc::sync_channel::<Request>(SLOTS);
        let (outgoing, results) = mpsc::sync_channel::<Answer>(SLOTS);
        let thread = thread::Builder::new()
            .name("cadence-settings".to_string())
            .spawn(move || {
                while let Ok((ticket, request)) = incoming.recv() {
                    if outgoing
                        .send((ticket, answer(path.as_ref(), request)))
                        .is_err()
                    {
                        return;
                    }
                }
            })
            .map_err(ConfigWorkerError::Spawn)?;
        Ok(Self {
            requests,
            results,
            thread,
        })
    }

    /// Hands a started job to the worker.
    ///
    /// # Errors
    /// Busy or disconnected.
    pub fn submit(
        &self,
        ticket: JobTicket,
        request: SettingsRequest,
    ) -> Result<(), ConfigWorkerError> {
        self.requests
            .try_send((ticket, request))
            .map_err(|error| match error {
                TrySendError::Full(_) => ConfigWorkerError::Busy,
                TrySendError::Disconnected(_) => ConfigWorkerError::Disconnected,
            })
    }

    /// One finished job, if any.
    ///
    /// # Errors
    /// The worker stopped.
    pub fn try_result(&self) -> Result<Option<Answer>, ConfigWorkerError> {
        match self.results.try_recv() {
            Ok(answer) => Ok(Some(answer)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(ConfigWorkerError::Disconnected),
        }
    }

    /// Stops the worker after its current request.
    ///
    /// # Errors
    /// The thread panicked.
    pub fn finish(self) -> Result<(), ConfigWorkerError> {
        drop(self.requests);
        drop(self.results);
        self.thread
            .join()
            .map_err(|_| ConfigWorkerError::Disconnected)
    }
}

#[cfg(test)]
mod tests {
    use super::{ConfigWorker, ConfigWorkerError};
    use cadence_core::settings::Setting;
    use cadence_engine::jobs::{JobKind, JobOutcome, JobTicket};
    use cadence_engine::settings::SettingsRequest;

    fn wait(worker: &ConfigWorker) -> Result<(JobTicket, JobOutcome), ConfigWorkerError> {
        loop {
            if let Some(answer) = worker.try_result()? {
                return Ok(answer);
            }
            std::thread::yield_now();
        }
    }

    #[test]
    fn a_write_answers_with_its_ticket_and_the_new_values() -> Result<(), Box<dyn std::error::Error>>
    {
        let dir = tempfile::tempdir()?;
        let worker = ConfigWorker::spawn(Some(dir.path().join("config.toml")))?;
        let ticket = JobTicket {
            kind: JobKind::ConfigWrite,
            epoch: 0,
            request: 1,
        };
        worker.submit(
            ticket,
            SettingsRequest::Write {
                based_on: None,
                edits: vec![("session.minutes", Setting::Integer(20))],
            },
        )?;
        let (answered, outcome) = wait(&worker)?;
        assert_eq!(answered, ticket);
        assert!(matches!(
            outcome,
            JobOutcome::Settings { values, digest: Some(_) }
                if values.contains(&("session.minutes", Setting::Integer(20)))
        ));
        // A second write still based on "no file" conflicts.
        worker.submit(
            ticket,
            SettingsRequest::Write {
                based_on: None,
                edits: vec![("session.minutes", Setting::Integer(10))],
            },
        )?;
        assert!(
            matches!(wait(&worker)?.1, JobOutcome::Failed(reason) if reason.contains("changed on disk"))
        );
        worker.finish()?;
        Ok(())
    }
}

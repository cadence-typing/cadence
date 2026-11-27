//! The PTY smoke suite (H-05 "Real PTY smoke", #92): the built binary in a
//! real pseudo-terminal on a temporary profile. Every wait is an output
//! predicate with a watchdog deadline; nothing sleeps and nothing measures
//! speed. `CADENCE_PTY_BINARY` names the binary (the gate passes the static
//! release build); otherwise the test build's own.
#![cfg(test)]
#![expect(
    clippy::arithmetic_side_effects,
    reason = "a test driver: offsets into its own output buffer and watchdog deadlines"
)]

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use nix::sys::termios::LocalFlags;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};

/// How long any one predicate may take before the test fails.
const WATCHDOG: Duration = Duration::from_secs(15);
/// The alternate screen, entered and left (DECSET / DECRST 1049).
const ENTER_ALTERNATE: &[u8] = b"\x1b[?1049h";
const LEAVE_ALTERNATE: &[u8] = b"\x1b[?1049l";

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn binary() -> PathBuf {
    std::env::var_os("CADENCE_PTY_BINARY").map_or_else(
        || PathBuf::from(env!("CARGO_BIN_EXE_cadence")),
        PathBuf::from,
    )
}

/// A running binary in a PTY, with everything it has written so far.
struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    output: Receiver<Vec<u8>>,
    seen: Vec<u8>,
    _home: tempfile::TempDir,
}

impl Session {
    fn start(columns: u16, rows: u16, args: &[&str]) -> Result<Self, Box<dyn std::error::Error>> {
        let binary = binary();
        if !binary.is_file() {
            return Err(format!("the cadence binary is missing at {}", binary.display()).into());
        }
        let home = tempfile::tempdir()?;
        let config = home.path().join("config/cadence");
        std::fs::create_dir_all(&config)?;
        // A settings file, so first-run setup does not intervene.
        std::fs::write(config.join("config.toml"), "[keyboard]\nlayout = \"us\"\n")?;
        let pty = native_pty_system().openpty(PtySize {
            rows,
            cols: columns,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut command = CommandBuilder::new(&binary);
        command.args(args);
        command.env_clear();
        command.env("HOME", home.path());
        command.env("XDG_CONFIG_HOME", home.path().join("config"));
        command.env("XDG_DATA_HOME", home.path().join("data"));
        command.env("TERM", "xterm-256color");
        command.env("PATH", "/usr/bin:/bin");
        for passed in ["CADENCE_FAULT_LEAVE_RAW"] {
            if let Some(value) = std::env::var_os(passed) {
                command.env(passed, value);
            }
        }
        let child = pty.slave.spawn_command(command)?;
        drop(pty.slave);
        let mut reader = pty.master.try_clone_reader()?;
        let (send, output) = channel();
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 4096];
            while let Ok(read) = reader.read(&mut buffer) {
                if read == 0 || send.send(buffer[..read].to_vec()).is_err() {
                    break;
                }
            }
        });
        let writer = pty.master.take_writer()?;
        Ok(Self {
            master: pty.master,
            writer,
            child,
            output,
            seen: Vec::new(),
            _home: home,
        })
    }

    /// Waits until everything written since `from` contains `needle`.
    fn expect(&mut self, from: usize, needle: &[u8]) -> Result<usize, Box<dyn std::error::Error>> {
        let deadline = Instant::now() + WATCHDOG;
        loop {
            if let Some(at) = self.seen[from.min(self.seen.len())..]
                .windows(needle.len())
                .position(|window| window == needle)
            {
                return Ok(from + at + needle.len());
            }
            let left = deadline.saturating_duration_since(Instant::now());
            match self.output.recv_timeout(left) {
                Ok(chunk) => self.seen.extend_from_slice(&chunk),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(format!(
                        "no {:?} within {WATCHDOG:?}; output ends {:?}",
                        String::from_utf8_lossy(needle),
                        String::from_utf8_lossy(&self.seen[self.seen.len().saturating_sub(400)..])
                    )
                    .into());
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(format!(
                        "the terminal closed before {:?}",
                        String::from_utf8_lossy(needle)
                    )
                    .into());
                }
            }
        }
    }

    fn send(&mut self, bytes: &[u8]) -> TestResult {
        self.writer.write_all(bytes)?;
        self.writer.flush()?;
        Ok(())
    }

    /// Waits for exit and checks the terminal was restored: the alternate
    /// screen left after it was entered, and canonical mode and echo back on.
    fn finish(mut self, from: usize) -> TestResult {
        self.expect(from, LEAVE_ALTERNATE)?;
        let deadline = Instant::now() + WATCHDOG;
        let status = loop {
            if let Some(status) = self.child.try_wait()? {
                break status;
            }
            if Instant::now() > deadline {
                self.child.kill()?;
                return Err("the binary did not exit within the watchdog".into());
            }
            // Drain output while waiting, so the child never blocks writing.
            if let Ok(chunk) = self.output.recv_timeout(Duration::from_millis(50)) {
                self.seen.extend_from_slice(&chunk);
            }
        };
        let termios = self.master.get_termios().ok_or("no termios on the PTY")?;
        assert!(
            termios
                .local_flags
                .contains(LocalFlags::ICANON | LocalFlags::ECHO),
            "the terminal was left in raw mode: {:?}",
            termios.local_flags
        );
        assert!(status.success(), "exit status {status:?}");
        Ok(())
    }

    fn resize(&self, columns: u16, rows: u16) -> TestResult {
        self.master.resize(PtySize {
            rows,
            cols: columns,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }
}

/// Launch, type, quit: the alternate screen and raw mode are entered for
/// practice and restored on exit.
#[test]
fn launch_type_and_quit_restore_the_terminal() -> TestResult {
    let mut session = Session::start(100, 32, &["--utc-offset", "+00:00"])?;
    let at = session.expect(0, ENTER_ALTERNATE)?;
    let at = session.expect(at, b"Make room for a little practice.")?;
    let raw = session.master.get_termios().ok_or("no termios")?;
    assert!(
        !raw.local_flags.contains(LocalFlags::ICANON),
        "practice runs in raw mode"
    );
    session.send(b"q")?;
    session.finish(at)
}

/// SIGWINCH: a resize brings a new, non-empty frame; below 60x18 practice
/// pauses and says so, and recovers when the terminal grows again.
#[test]
fn a_resize_redraws_and_too_small_recovers() -> TestResult {
    let mut session = Session::start(100, 32, &["--utc-offset", "+00:00"])?;
    let at = session.expect(0, b"Make room for a little practice.")?;
    session.resize(120, 40)?;
    let at = session.expect(at, b"Make room for a little practice.")?;
    session.resize(40, 12)?;
    let at = session.expect(at, b"Terminal too small")?;
    session.resize(100, 32)?;
    let at = session.expect(at, b"Make room for a little practice.")?;
    session.send(b"q")?;
    session.finish(at)
}

/// Ctrl-C and an abnormal exit (SIGTERM) restore the terminal; with no
/// answer to the native text-size query the legacy fallback is used.
#[test]
fn interrupt_and_termination_restore_the_terminal() -> TestResult {
    // Ctrl-C during practice ends it into the review; Enter saves and exits.
    let mut session = Session::start(100, 32, &["--utc-offset", "+00:00", "--minutes", "5"])?;
    let at = session.expect(0, b"Make room for a little practice.")?;
    session.send(b"\r")?;
    let at = session.expect(at, b"native scaling unavailable")?;
    session.send(b"\x03")?;
    let at = session.expect(at, b"Session review")?;
    session.send(b"\r")?;
    session.finish(at)?;
    // SIGTERM: the documented terminate path saves and restores.
    let mut session = Session::start(100, 32, &["--utc-offset", "+00:00"])?;
    let at = session.expect(0, b"Make room for a little practice.")?;
    let pid = session.child.process_id().ok_or("no child pid")?;
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(i32::try_from(pid)?),
        nix::sys::signal::Signal::SIGTERM,
    )?;
    session.finish(at)
}

/// #101, #104: by default the numpad phase is the whole keypad: the start
/// screen says so, operator keys and Backspace reach practice through the
/// terminal, and the session ends into its review and exits cleanly.
#[test]
fn the_numpad_phase_takes_the_whole_keypad() -> TestResult {
    let mut session = Session::start(
        100,
        32,
        &[
            "--utc-offset",
            "+00:00",
            "--minutes",
            "5",
            "--track",
            "numpad",
        ],
    )?;
    let at = session.expect(0, b"Make room for a little practice.")?;
    let at = session.expect(at, b"digits, decimals and + - * /")?;
    session.send(b"\r")?;
    let at = session.expect(at, b"Enter submits")?;
    // Operator keys and a correction reach practice through the terminal
    // (the status line redraws incrementally, so the frame is not matched).
    session.send(b"1+")?;
    session.send(b"\x7f")?;
    session.send(b"*")?;
    session.send(b"\x03")?;
    let at = session.expect(at, b"Session review")?;
    session.send(b"\r")?;
    session.finish(at)
}

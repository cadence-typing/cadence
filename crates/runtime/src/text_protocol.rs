//! Narrow DEC line-rendition adapter; input replies use Crossterm's existing parser.

use std::fmt;
use std::io::{self, Write};

use cadence_engine::view::{LineRendition, TextSupport};
use crossterm::Command;
use crossterm::cursor::MoveTo;
use crossterm::execute;
use crossterm::style::{Attribute, ResetColor, SetAttribute};
use crossterm::terminal::{Clear, ClearType};

/// DEC VT510 DECSWL/DECDHL: whole-row attributes, not embedded bitmap glyphs.
pub(super) struct SetLineRendition(pub LineRendition);

impl Command for SetLineRendition {
    fn write_ansi(&self, writer: &mut impl fmt::Write) -> fmt::Result {
        writer.write_str(match self.0 {
            LineRendition::Normal => "\x1b#5",
            LineRendition::DoubleHeightTop => "\x1b#3",
            LineRendition::DoubleHeightBottom => "\x1b#4",
        })
    }

    #[cfg(windows)]
    fn execute_winapi(&self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native Windows is not a supported Cadence target",
        ))
    }
}

/// Probe in the empty alternate screen, before admitting practice input.
///
/// A positive result requires normal width AND each half's logical-width clamp.
/// Environment variables are not evidence. Crossterm owns response parsing and
/// its two-second cursor-query timeout; unrelated semantic keys remain queued.
/// A missing reply leaves normal text available. Other I/O failures propagate.
pub(super) fn probe<W, Q>(writer: &mut W, columns: u16, mut position: Q) -> io::Result<TextSupport>
where
    W: Write,
    Q: FnMut() -> io::Result<(u16, u16)>,
{
    let Some(last_column) = columns.checked_sub(1) else {
        return Ok(TextSupport::NormalOnly);
    };
    let Some(half_last) = columns.checked_div(2).and_then(|half| half.checked_sub(1)) else {
        return Ok(TextSupport::NormalOnly);
    };
    let outcome = (|| {
        for (rendition, expected_column) in [
            (LineRendition::Normal, last_column),
            (LineRendition::DoubleHeightTop, half_last),
            (LineRendition::DoubleHeightBottom, half_last),
        ] {
            execute!(
                writer,
                MoveTo(0, 0),
                SetLineRendition(LineRendition::Normal),
                SetLineRendition(rendition),
                MoveTo(last_column, 0)
            )?;
            let reply = match position() {
                Ok(reply) => reply,
                Err(error)
                    if matches!(error.kind(), io::ErrorKind::Other | io::ErrorKind::TimedOut) =>
                {
                    return Ok(TextSupport::NormalOnly);
                }
                Err(error) => return Err(error),
            };
            if reply != (expected_column, 0) {
                return Ok(TextSupport::NormalOnly);
            }
        }
        Ok(TextSupport::DoubleHeight)
    })();
    // Restore even after a missing/malformed response or a failed output command.
    let restored = execute!(
        writer,
        MoveTo(0, 0),
        SetLineRendition(LineRendition::Normal),
        SetAttribute(Attribute::Reset),
        ResetColor,
        Clear(ClearType::CurrentLine),
        MoveTo(0, 0)
    );
    restored?;
    outcome
}

#[cfg(test)]
mod tests;

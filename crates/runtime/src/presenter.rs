//! Retained Ratatui cells plus DEC row metadata. No alternate product renderer.
//!
//! Line attributes change the terminal's column geometry. Changed attributes
//! clear/invalidate that row; ordinary frames retain Ratatui's wide-cell diff.

use std::io::{self, Write};

use cadence_engine::view::{LineRendition, TextSupport};
use crossterm::cursor::{Hide, MoveTo};
use crossterm::queue;
use crossterm::style::{Attribute, ResetColor, SetAttribute};
use crossterm::terminal::{Clear, ClearType};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::buffer::Buffer;
use ratatui::layout::{Rect, Size};

use crate::text_protocol::SetLineRendition;

pub(super) struct Presenter<W: Write> {
    backend: CrosstermBackend<W>,
    previous: Buffer,
    next: Buffer,
    previous_rows: Vec<LineRendition>,
    next_rows: Vec<LineRendition>,
    support: TextSupport,
    invalidated: bool,
}

impl<W: Write> Presenter<W> {
    pub(super) fn new(writer: W, size: Size, support: TextSupport) -> Self {
        let area = Rect::new(0, 0, size.width, size.height);
        Self {
            backend: CrosstermBackend::new(writer),
            previous: Buffer::empty(area),
            next: Buffer::empty(area),
            previous_rows: vec![LineRendition::Normal; usize::from(size.height)],
            next_rows: vec![LineRendition::Normal; usize::from(size.height)],
            support,
            invalidated: true,
        }
    }

    pub(super) const fn text_support(&self) -> TextSupport {
        self.support
    }

    pub(super) fn size(&self) -> io::Result<Size> {
        self.backend.size()
    }

    pub(super) fn draw<P>(&mut self, size: Size, painter: P) -> io::Result<()>
    where
        P: FnOnce(&mut Buffer, &mut [LineRendition]),
    {
        let area = Rect::new(0, 0, size.width, size.height);
        if self.next.area != area {
            self.previous.resize(area);
            self.next.resize(area);
            self.previous_rows
                .resize(usize::from(size.height), LineRendition::Normal);
            self.next_rows
                .resize(usize::from(size.height), LineRendition::Normal);
            self.invalidated = true;
        }
        if self.invalidated {
            // A resize can uncover rows whose attributes were outside the last viewport.
            self.reset_rows(size.height)?;
            queue!(
                self.backend,
                SetAttribute(Attribute::Reset),
                ResetColor,
                Clear(ClearType::All)
            )?;
            self.previous.reset();
            self.previous_rows.fill(LineRendition::Normal);
            self.invalidated = false;
        }
        self.next.reset();
        self.next_rows.fill(LineRendition::Normal);
        painter(&mut self.next, &mut self.next_rows);
        if self.support == TextSupport::NormalOnly
            && self
                .next_rows
                .iter()
                .any(|row| *row != LineRendition::Normal)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "native text was not negotiated",
            ));
        }
        for (index, (old, new)) in self.previous_rows.iter().zip(&self.next_rows).enumerate() {
            if old == new {
                continue;
            }
            let y = u16::try_from(index)
                .map_err(|_| io::Error::other("row index exceeds terminal geometry"))?;
            queue!(
                self.backend,
                MoveTo(0, y),
                SetAttribute(Attribute::Reset),
                ResetColor
            )?;
            if *old != LineRendition::Normal {
                queue!(self.backend, SetLineRendition(LineRendition::Normal))?;
            }
            queue!(self.backend, Clear(ClearType::CurrentLine))?;
            if *new != LineRendition::Normal {
                queue!(self.backend, SetLineRendition(*new))?;
            }
            for x in area.left()..area.right() {
                if let Some(cell) = self.previous.cell_mut((x, y)) {
                    cell.reset();
                }
            }
        }
        let updates = self.previous.diff(&self.next);
        let rows = &self.next_rows;
        // Never address the hidden right half of a double-width logical row:
        // terminals clamp CUP there and would overwrite its last real character.
        self.backend.draw(updates.into_iter().filter(|(x, y, _)| {
            let logical_width = match rows.get(usize::from(*y)) {
                Some(LineRendition::DoubleHeightTop | LineRendition::DoubleHeightBottom) => {
                    area.width.checked_div(2).unwrap_or(0)
                }
                _ => area.width,
            };
            *x < logical_width
        }))?;
        queue!(self.backend, Hide)?;
        Write::flush(&mut self.backend)?;
        std::mem::swap(&mut self.previous, &mut self.next);
        std::mem::swap(&mut self.previous_rows, &mut self.next_rows);
        Ok(())
    }

    pub(super) fn restore_rows(&mut self) -> io::Result<()> {
        self.reset_rows(self.previous.area.height)?;
        queue!(
            self.backend,
            MoveTo(0, 0),
            SetAttribute(Attribute::Reset),
            ResetColor
        )?;
        Write::flush(&mut self.backend)
    }

    fn reset_rows(&mut self, height: u16) -> io::Result<()> {
        if self.support == TextSupport::DoubleHeight {
            for y in 0..height {
                queue!(
                    self.backend,
                    MoveTo(0, y),
                    SetLineRendition(LineRendition::Normal)
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

//! Wire-level behavior: the real presenter and Crossterm backend write into a byte sink.

use super::*;
use ratatui::style::Style;

fn pair(buffer: &mut Buffer, rows: &mut [LineRendition]) {
    for (row, rendition) in rows.iter_mut().zip([
        LineRendition::DoubleHeightTop,
        LineRendition::DoubleHeightBottom,
    ]) {
        *row = rendition;
    }
    for y in 0..2 {
        buffer.set_string(0, y, "X", Style::default());
    }
}

#[test]
fn changing_rendition_repaints_unchanged_letters_after_clearing() -> io::Result<()> {
    let mut bytes = Vec::new();
    let size = Size::new(6, 4);
    {
        let mut presenter = Presenter::new(&mut bytes, size, TextSupport::DoubleHeight);
        presenter.draw(size, pair)?;
        for _ in 0..2 {
            presenter.draw(size, |buffer, _| {
                for y in 0..2 {
                    buffer.set_string(0, y, "X", Style::default());
                }
            })?;
        }
        presenter.restore_rows()?;
    }
    // Two letters initially, two after native→normal. An unchanged third frame
    // does not rewrite them. Omitting rendition invalidation loses both letters.
    assert_eq!(bytes.iter().filter(|byte| **byte == b'X').count(), 4);
    Ok(())
}

#[test]
fn double_height_output_does_not_overwrite_the_last_logical_column() -> io::Result<()> {
    let mut bytes = Vec::new();
    let size = Size::new(7, 4);
    {
        let mut presenter = Presenter::new(&mut bytes, size, TextSupport::DoubleHeight);
        presenter.draw(size, |buffer, rows| {
            pair(buffer, rows);
            for y in 0..2 {
                buffer.set_string(2, y, "L", Style::default());
                buffer.set_string(6, y, "z", Style::default());
            }
        })?;
    }
    // Native CUP clamps hidden columns to logical column 2, corrupting L if sent.
    assert_eq!(bytes.iter().filter(|byte| **byte == b'L').count(), 2);
    assert!(!bytes.contains(&b'z'));
    Ok(())
}

#[test]
fn resize_repaints_native_letters_even_when_their_coordinates_match() -> io::Result<()> {
    let mut bytes = Vec::new();
    {
        let mut presenter = Presenter::new(&mut bytes, Size::new(10, 4), TextSupport::DoubleHeight);
        presenter.draw(Size::new(10, 4), pair)?;
        presenter.draw(Size::new(8, 4), pair)?;
    }
    assert_eq!(bytes.iter().filter(|byte| **byte == b'X').count(), 4);
    Ok(())
}

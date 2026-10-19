//! Capability facts must be positive for both halves; every probe restores its row.

use super::*;

fn query(
    replies: impl IntoIterator<Item = io::Result<(u16, u16)>>,
) -> impl FnMut() -> io::Result<(u16, u16)> {
    let mut replies = replies.into_iter();
    move || {
        replies
            .next()
            .ok_or_else(|| io::Error::other("unexpected extra query"))?
    }
}

fn last_rendition(bytes: &[u8]) -> Option<&[u8]> {
    bytes.windows(3).rfind(|bytes| bytes.starts_with(b"\x1b#"))
}

#[test]
fn both_native_halves_must_clamp_even_with_an_odd_column_count() -> io::Result<()> {
    let mut bytes = Vec::new();
    let support = probe(
        &mut bytes,
        73,
        query([Ok((72, 0)), Ok((35, 0)), Ok((35, 0))]),
    )?;
    assert_eq!(support, TextSupport::DoubleHeight);
    assert_eq!(last_rendition(&bytes), Some(b"\x1b#5".as_slice()));

    bytes.clear();
    let support = probe(
        &mut bytes,
        73,
        query([Ok((72, 0)), Ok((35, 0)), Ok((72, 0))]),
    )?;
    assert_eq!(support, TextSupport::NormalOnly);
    assert_eq!(last_rendition(&bytes), Some(b"\x1b#5".as_slice()));
    Ok(())
}

#[test]
fn missing_reply_restores_the_row_and_never_claims_native_support() -> io::Result<()> {
    let mut bytes = Vec::new();
    let support = probe(
        &mut bytes,
        100,
        query([Ok((99, 0)), Err(io::Error::from(io::ErrorKind::TimedOut))]),
    )?;
    assert_eq!(support, TextSupport::NormalOnly);
    assert_eq!(last_rendition(&bytes), Some(b"\x1b#5".as_slice()));
    Ok(())
}

#[test]
fn input_failure_propagates_after_restoring_the_probe_row() {
    let mut bytes = Vec::new();
    let result = probe(
        &mut bytes,
        100,
        query([Err(io::Error::from(io::ErrorKind::BrokenPipe))]),
    );
    assert_eq!(
        result.err().map(|error| error.kind()),
        Some(io::ErrorKind::BrokenPipe)
    );
    assert_eq!(last_rendition(&bytes), Some(b"\x1b#5".as_slice()));
}

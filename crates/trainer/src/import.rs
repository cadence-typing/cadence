//! Local text imports (D-03, L-02, #84): pure, bounded normalisation of a
//! file the learner supplies. Limits are checked before allocation. A line
//! is accepted whole or rejected whole: a line holding ESC, another C0
//! control (except tab), DEL, a C1 control, a bidirectional control, invalid
//! UTF-8 or any character outside the training alphabet is rejected with its
//! line, column and reason. Tabs become four spaces; CRLF line ends are
//! accepted; with typography normalisation, smart quotes, dashes and
//! non-breaking spaces map to their ASCII forms and are counted. Nothing is
//! executed, rendered or logged here.

use cadence_core::alphabet::{Layout, TrainingChar};

/// The largest file accepted, in bytes.
pub const MAX_BYTES: usize = 10 * 1024 * 1024;
/// The most lines accepted.
pub const MAX_LINES: usize = 200_000;
/// The most training units on one line.
pub const MAX_LINE_UNITS: usize = 2_000;
/// The spaces a tab becomes.
const TAB_SPACES: usize = 4;
/// The normaliser's version, recorded in every pack's manifest.
pub const NORMALIZER_VERSION: u32 = 1;

/// How to normalise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImportOptions {
    /// Map smart quotes, dashes and non-breaking spaces to ASCII.
    pub normalize_typography: bool,
    /// The layout whose alphabet applies (UK admits `£`).
    pub layout: Layout,
}

/// Why a whole import is refused before any line is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportError {
    /// More than [`MAX_BYTES`].
    TooLarge {
        /// The input's size.
        bytes: usize,
    },
    /// More than [`MAX_LINES`].
    TooManyLines,
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge { bytes } => write!(
                f,
                "{bytes} bytes is more than the {MAX_BYTES}-byte import limit"
            ),
            Self::TooManyLines => write!(f, "more than {MAX_LINES} lines"),
        }
    }
}

impl std::error::Error for ImportError {}

/// Why one line is rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The escape character (terminal sequences).
    Escape,
    /// Another C0 control, or DEL.
    Control,
    /// A C1 control.
    C1Control,
    /// A bidirectional formatting control.
    Bidirectional,
    /// Not valid UTF-8.
    InvalidUtf8,
    /// A character outside the training alphabet.
    OutsideAlphabet(char),
    /// More than [`MAX_LINE_UNITS`] units.
    TooLong,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Escape => f.write_str("escape character (terminal sequence)"),
            Self::Control => f.write_str("control character"),
            Self::C1Control => f.write_str("C1 control character"),
            Self::Bidirectional => f.write_str("bidirectional control"),
            Self::InvalidUtf8 => f.write_str("not UTF-8"),
            Self::OutsideAlphabet(c) => write!(
                f,
                "U+{:04X} is outside the training alphabet",
                u32::from(*c)
            ),
            Self::TooLong => write!(f, "more than {MAX_LINE_UNITS} characters"),
        }
    }
}

/// One rejected line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rejection {
    /// 1-based line.
    pub line: usize,
    /// 1-based column of the first refused character.
    pub column: usize,
    /// Why.
    pub refusal: Refusal,
}

/// Typography replacements made, by kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Typography {
    /// Smart quotes made straight.
    pub quotes: u64,
    /// En and em dashes made hyphens.
    pub dashes: u64,
    /// Non-breaking spaces made spaces.
    pub spaces: u64,
}

/// What an import accepted and refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    /// Accepted non-blank lines, as training units, in input order.
    pub passages: Vec<Vec<TrainingChar>>,
    /// Lines read.
    pub lines: usize,
    /// Accepted lines, blank ones included.
    pub accepted: usize,
    /// Rejected lines.
    pub rejected: Vec<Rejection>,
    /// Typography replacements made.
    pub typography: Typography,
}

const fn is_bidi(c: char) -> bool {
    matches!(
        c,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

fn typography(c: char, counts: &mut Typography) -> char {
    match c {
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => {
            counts.quotes = counts.quotes.saturating_add(1);
            '\''
        }
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => {
            counts.quotes = counts.quotes.saturating_add(1);
            '"'
        }
        '\u{2013}' | '\u{2014}' => {
            counts.dashes = counts.dashes.saturating_add(1);
            '-'
        }
        '\u{00A0}' | '\u{202F}' => {
            counts.spaces = counts.spaces.saturating_add(1);
            ' '
        }
        other => other,
    }
}

/// One line's units, or why it is refused.
fn line(
    bytes: &[u8],
    options: ImportOptions,
    counts: &mut Typography,
) -> Result<Vec<TrainingChar>, (usize, Refusal)> {
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    let text = std::str::from_utf8(bytes).map_err(|error| {
        let column = std::str::from_utf8(bytes.get(..error.valid_up_to()).unwrap_or_default())
            .map_or(1, |valid| valid.chars().count().saturating_add(1));
        (column, Refusal::InvalidUtf8)
    })?;
    // Checked before allocating the line's units.
    let width = text
        .chars()
        .map(|c| if c == '\t' { TAB_SPACES } else { 1 })
        .fold(0_usize, usize::saturating_add);
    if width > MAX_LINE_UNITS {
        return Err((MAX_LINE_UNITS.saturating_add(1), Refusal::TooLong));
    }
    let mut units = Vec::with_capacity(width);
    let mut pending = Typography::default();
    for (index, c) in text.chars().enumerate() {
        let column = index.saturating_add(1);
        let c = if options.normalize_typography {
            typography(c, &mut pending)
        } else {
            c
        };
        let refusal = match c {
            '\u{1B}' => Some(Refusal::Escape),
            '\t' => None,
            '\u{0}'..='\u{1F}' | '\u{7F}' => Some(Refusal::Control),
            '\u{80}'..='\u{9F}' => Some(Refusal::C1Control),
            c if is_bidi(c) => Some(Refusal::Bidirectional),
            _ => None,
        };
        if let Some(refusal) = refusal {
            return Err((column, refusal));
        }
        if c == '\t' {
            let space = TrainingChar::new(' ', options.layout)
                .map_err(|_alphabet| (column, Refusal::OutsideAlphabet(' ')))?;
            units.extend(std::iter::repeat_n(space, TAB_SPACES));
            continue;
        }
        let unit = TrainingChar::new(c, options.layout)
            .map_err(|_alphabet| (column, Refusal::OutsideAlphabet(c)))?;
        units.push(unit);
    }
    // A rejected line's replacements are not counted.
    counts.quotes = counts.quotes.saturating_add(pending.quotes);
    counts.dashes = counts.dashes.saturating_add(pending.dashes);
    counts.spaces = counts.spaces.saturating_add(pending.spaces);
    Ok(units)
}

/// Normalises `bytes`.
///
/// # Errors
/// The input is over a limit; nothing is read further.
pub fn normalize(bytes: &[u8], options: ImportOptions) -> Result<ImportReport, ImportError> {
    if bytes.len() > MAX_BYTES {
        return Err(ImportError::TooLarge { bytes: bytes.len() });
    }
    let body = bytes.strip_suffix(b"\n").unwrap_or(bytes);
    let lines = if bytes.is_empty() {
        0
    } else {
        body.iter()
            .filter(|b| **b == b'\n')
            .count()
            .saturating_add(1)
    };
    if lines > MAX_LINES {
        return Err(ImportError::TooManyLines);
    }
    let mut report = ImportReport {
        passages: Vec::new(),
        lines,
        accepted: 0,
        rejected: Vec::new(),
        typography: Typography::default(),
    };
    if bytes.is_empty() {
        return Ok(report);
    }
    for (index, raw) in body.split(|b| *b == b'\n').enumerate() {
        match line(raw, options, &mut report.typography) {
            Ok(units) => {
                report.accepted = report.accepted.saturating_add(1);
                if units.iter().any(|unit| unit.get() != ' ') {
                    report.passages.push(units);
                }
            }
            Err((column, refusal)) => report.rejected.push(Rejection {
                line: index.saturating_add(1),
                column,
                refusal,
            }),
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const US: ImportOptions = ImportOptions {
        normalize_typography: false,
        layout: Layout::Us,
    };

    fn text(units: &[TrainingChar]) -> String {
        units.iter().map(|unit| unit.get()).collect()
    }

    fn refused(c: char) -> bool {
        c == '\u{1B}'
            || (c.is_control() && c != '\n')
            || is_bidi(c)
            || TrainingChar::new(c, Layout::Us).is_err()
    }

    proptest! {
        // Failing cases persist in proptest-regressions/ (the seeds).
        #[test]
        fn arbitrary_bytes_never_yield_a_refused_code_point(bytes in proptest::collection::vec(any::<u8>(), 0..4096)) {
            let report = normalize(&bytes, US).unwrap();
            prop_assert_eq!(report.accepted.checked_add(report.rejected.len()), Some(report.lines));
            for passage in &report.passages {
                prop_assert!(passage.iter().all(|unit| !refused(unit.get())));
            }
        }

        #[test]
        fn arbitrary_text_keeps_the_line_count(lines in proptest::collection::vec("\\PC{0,40}", 0..60)) {
            let input = lines.join("\n");
            let report = normalize(input.as_bytes(), US).unwrap();
            prop_assert_eq!(report.accepted.checked_add(report.rejected.len()), Some(report.lines));
        }
    }

    #[test]
    fn terminal_sequences_and_bidi_controls_are_refused_with_their_column() {
        let report = normalize(
            b"plain line\nred \x1b[31mtext\x1b[0m\ntitle \x1b]0;owned\x07\nok again",
            US,
        )
        .unwrap();
        assert_eq!(report.lines, 4);
        assert_eq!(report.accepted, 2);
        assert_eq!(
            report.rejected,
            vec![
                Rejection {
                    line: 2,
                    column: 5,
                    refusal: Refusal::Escape
                },
                Rejection {
                    line: 3,
                    column: 7,
                    refusal: Refusal::Escape
                },
            ]
        );
        let bidi = normalize("abc\u{202E}def".as_bytes(), US).unwrap();
        assert_eq!(
            bidi.rejected,
            vec![Rejection {
                line: 1,
                column: 4,
                refusal: Refusal::Bidirectional
            }]
        );
    }

    #[test]
    fn limits_are_checked_before_reading() {
        let big = vec![b'a'; MAX_BYTES + 1];
        assert_eq!(
            normalize(&big, US),
            Err(ImportError::TooLarge {
                bytes: MAX_BYTES + 1
            })
        );
        let long = "x".repeat(3_000);
        let report = normalize(long.as_bytes(), US).unwrap();
        assert_eq!(
            report.rejected.first().map(|r| r.refusal),
            Some(Refusal::TooLong)
        );
    }

    #[test]
    fn crlf_tabs_and_typography() {
        let report = normalize(b"one\r\n\ttwo\r\n", US).unwrap();
        assert_eq!(report.lines, 2);
        assert_eq!(
            report.passages.iter().map(|p| text(p)).collect::<Vec<_>>(),
            ["one", "    two"]
        );
        let smart = "\u{201C}quoted\u{201D} \u{2014} it\u{2019}s\u{00A0}fine";
        assert!(normalize(smart.as_bytes(), US).unwrap().passages.is_empty());
        let options = ImportOptions {
            normalize_typography: true,
            ..US
        };
        let report = normalize(smart.as_bytes(), options).unwrap();
        assert_eq!(
            report.passages.first().map(|p| text(p)).as_deref(),
            Some("\"quoted\" - it's fine")
        );
        assert_eq!(
            report.typography,
            Typography {
                quotes: 3,
                dashes: 1,
                spaces: 1
            }
        );
    }
}

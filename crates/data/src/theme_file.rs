//! User theme files (U-02, #82): `<config dir>/<name>.toml` defines the
//! eleven roles as `"#rrggbb"` or `"default"`. The whole palette is
//! validated before use: a missing or unknown role, a wrong type or a bad
//! colour is an error naming the file, the role and what is accepted, and
//! the caller keeps its current palette.

use std::path::Path;

use cadence_core::settings::{THEME_ROLES, ThemeColor, ThemeColors};
use toml_edit::ImDocument;

fn colour(value: &str) -> Option<ThemeColor> {
    if value == "default" {
        return Some(ThemeColor::Default);
    }
    let hex = value.strip_prefix('#').filter(|h| h.len() == 6)?;
    let byte = |range: std::ops::Range<usize>| {
        hex.get(range)
            .and_then(|digits| u8::from_str_radix(digits, 16).ok())
    };
    Some(ThemeColor::Rgb(byte(0..2)?, byte(2..4)?, byte(4..6)?))
}

/// Parses a theme file's text.
///
/// # Errors
/// The first problem, naming the file and the role.
pub fn parse(text: &str, file: &Path) -> Result<ThemeColors, String> {
    let at = file.display();
    let document: ImDocument<&str> = ImDocument::parse(text)
        .map_err(|error| format!("{at}: not valid TOML: {}", error.message()))?;
    if let Some((unknown, _)) = document.iter().find(|(key, _)| !THEME_ROLES.contains(key)) {
        return Err(format!("{at}: unknown role {unknown}"));
    }
    let mut colours = [ThemeColor::Default; 11];
    for (slot, role) in colours.iter_mut().zip(THEME_ROLES) {
        let value = document
            .get(role)
            .ok_or_else(|| format!("{at}: missing role {role}"))?;
        *slot = value
            .as_str()
            .and_then(colour)
            .ok_or_else(|| format!("{at}: {role}: expected \"#rrggbb\" or \"default\""))?;
    }
    Ok(colours)
}

/// Reads the theme file `name` (a bare `*.toml` name, validated by the
/// config) from `directory`.
///
/// # Errors
/// An unreadable or invalid file.
pub fn load(directory: &Path, name: &str) -> Result<ThemeColors, String> {
    let path = directory.join(name);
    let text = std::fs::read_to_string(&path)
        .map_err(|error| format!("{}: cannot read: {error}", path.display()))?;
    parse(&text, &path)
}

/// The theme a config names: `None` for a built-in name, else the file's
/// validated colours or why not.
#[must_use]
pub fn for_config(
    config: &crate::config::Config,
    directory: Option<&Path>,
) -> Option<Result<ThemeColors, String>> {
    let name = config.text("display.theme")?;
    if ["dark", "light", "terminal"].contains(&name) {
        return None;
    }
    Some(match directory {
        Some(directory) => load(directory, name),
        None => Err(format!("{name}: no settings directory to read it from")),
    })
}

#[cfg(test)]
mod tests {
    use super::{THEME_ROLES, parse};
    use cadence_core::settings::ThemeColor;
    use std::path::Path;

    fn full() -> String {
        THEME_ROLES
            .iter()
            .map(|role| format!("{role} = \"#102030\"\n"))
            .collect()
    }

    #[test]
    fn a_full_theme_parses_and_a_missing_role_is_named() {
        let file = Path::new("/cfg/ocean.toml");
        let colours = parse(&full(), file).unwrap_or([ThemeColor::Default; 11]);
        assert!(
            colours
                .iter()
                .all(|c| *c == ThemeColor::Rgb(0x10, 0x20, 0x30))
        );
        let without = full().replace("warning = \"#102030\"\n", "");
        assert_eq!(
            parse(&without, file),
            Err("/cfg/ocean.toml: missing role warning".into())
        );
        let unknown = format!("{}sparkle = \"#ffffff\"\n", full());
        assert_eq!(
            parse(&unknown, file),
            Err("/cfg/ocean.toml: unknown role sparkle".into())
        );
        let bad = full().replace("text = \"#102030\"", "text = \"blue\"");
        assert!(parse(&bad, file).is_err_and(|e| e.contains("text: expected")));
        let default = full().replace("text = \"#102030\"", "text = \"default\"");
        assert_eq!(parse(&default, file).map(|c| c[2]), Ok(ThemeColor::Default));
    }
}
